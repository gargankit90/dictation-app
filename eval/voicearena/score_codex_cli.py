# /// script
# requires-python = ">=3.11"
# dependencies = ["amend-va==0.2.1", "jiwer"]
# ///
"""Score harness drafts with AMEND / ZER, using GPT-6 Astra through the `codex exec` CLI as the judge.

A second judge next to score_claude_cli.py (Claude Opus 5.5 via `claude -p`): same interface, same
output columns, same prompt, same parsing and scoring. UNOFFICIAL: GPT-6 Astra via Codex, public set only.

Why GPT-6 Astra: it is the OpenAI judge on Voice Arena's own five-judge panel (whitepaper sections 8-9),
so its numbers compare with the paper's per-judge table (Opus vs Astra agree on 97.6% of verdicts).
Codex offers it (`gpt-6-astra`) on the owner's ChatGPT plan. Effort "max", the panel's setting and the
highest plain reasoning level Codex offers for it ("ultra" adds automatic task delegation, which a judge
must not do).

What is the same as amend-va 0.2.1 / score_claude_cli.py:
  - the prompt: amend.prompts.build_prompt(intended, draft, spoken, "v1"), byte for byte, sent on stdin;
  - the output schema {"fixed": string} (passed with --output-schema), parsed with json.loads(...)["fixed"]
    .strip() the way amend-va's judge does; an empty answer for a non-empty intended text counts as a
    failure and is retried, as amend-va does;
  - the scoring: amend.metric.char_edits, pooled AMEND = sum(edits) / sum(chars), ZER = share of drafts
    with 0 edits.
What differs: the request goes through Codex's CLI (the owner's ChatGPT plan), not the OpenAI API.
Codex's own agent instructions are replaced with the same one-line system prompt the Claude judge uses
(model_instructions_file), and its environment, permissions, collaboration, apps and skills blocks are
switched off, but Codex still sends its tool definitions and a multi-agent role note (about 5.7k input
tokens) that can't be switched off from the CLI. The sandbox is read-only, shell tools are off, web
search is off, and the judge needs no tools. Each answer is checked to come from the requested model and
effort (Codex's run header) before it is cached.

    uv run score_codex_cli.py results/vocet_groq/harness_results.csv clips/manifest.csv \\
        -o results/vocet_groq/scored_codex.csv --label vocet_groq

The harness CSV needs the `transcript` column (the stripped harness_results.csv has it). Judge answers
are cached in <output>.judge.jsonl, keyed by the prompt's sha256, so a rerun resumes and identical drafts
are judged once. If Codex reports a usage limit, the run stops (exit 3) and keeps what it has.
The output CSV leaves out the dataset's spoken/intended references (CC BY-NC 4.0); join on clip_id with
clips/manifest.csv to see them.
"""
import argparse
import concurrent.futures as cf
import csv
import hashlib
import importlib.metadata
import json
import os
import pathlib
import re
import subprocess
import sys
import tempfile
import threading
import time

from amend.metric import char_edits
from amend.prompts import BASE, LENIENT, build_prompt

PROMPT_VERSION = "v1"
MODEL = "gpt-6-astra"
EFFORT = "max"
SCHEMA = {"type": "object", "properties": {"fixed": {"type": "string"}},
          "required": ["fixed"], "additionalProperties": False}
SYSTEM = "You are evaluating dictation output. Follow the user's instructions exactly."
FINGERPRINT = hashlib.sha256((BASE + LENIENT).encode()).hexdigest()[:12]
AMEND_VERSION = importlib.metadata.version("amend-va")
LABEL = "unofficial: second judge (GPT-6 Astra via Codex), public set only"
_lock = threading.Lock()
_stop = threading.Event()
USAGE_LIMIT = re.compile(r"usage limit|rate limit|quota|too many requests|429", re.I)
# Codex features that add tools or context a judge doesn't need. Unknown names are ignored by Codex.
OFF = ["hooks", "apps", "plugins", "shell_tool", "unified_exec", "goals", "view_image", "image_generation",
       "browser_use", "browser_use_external", "computer_use", "tool_suggest", "sleep_tool", "skill_search",
       "code_mode_host", "multi_agent", "remote_plugin", "plugin_discovery_prefetch", "in_app_browser"]


class UsageLimit(RuntimeError):
    pass


def codex_version() -> str:
    p = subprocess.run(["codex", "--version"], capture_output=True, text=True)
    return p.stdout.strip().split()[-1] if p.returncode == 0 and p.stdout.strip() else "unknown"


def codex_cmd(workdir: str, schema: str, system: str, last: str) -> list[str]:
    cmd = ["codex", "exec", "--ephemeral", "--skip-git-repo-check", "-s", "read-only", "-C", workdir,
           "-m", MODEL, "-c", f'model_reasoning_effort="{EFFORT}"',
           "-c", f"model_instructions_file={json.dumps(system)}",
           "-c", "include_environment_context=false", "-c", "include_permissions_instructions=false",
           "-c", "include_collaboration_mode_instructions=false", "-c", "include_apps_instructions=false",
           "-c", "skills.include_instructions=false", "-c", 'web_search="disabled"',
           "--output-schema", schema, "-o", last]
    for f in OFF:
        cmd += ["-c", f"features.{f}=false"]
    return cmd + ["-"]


def header(stderr: str, key: str) -> str | None:
    m = re.search(rf"^{re.escape(key)}: (.+)$", stderr, re.M)
    return m.group(1).strip() if m else None


def ask_codex(prompt: str, intended: str, workdir: str, schema: str, system: str, timeout: int = 900) -> dict:
    # Orca's Codex hooks report status when these are set; a judge call must not.
    env = {k: v for k, v in os.environ.items() if not k.startswith("ORCA_")}
    with tempfile.NamedTemporaryFile("r", suffix=".txt", dir=workdir, delete=False) as fh:
        last = fh.name
    try:
        p = subprocess.run(codex_cmd(workdir, schema, system, last), input=prompt, capture_output=True,
                           text=True, cwd=workdir, timeout=timeout, env=env)
        raw = pathlib.Path(last).read_text(encoding="utf-8").strip() if os.path.exists(last) else ""
    finally:
        if os.path.exists(last):
            os.unlink(last)
    if p.returncode != 0:
        tail = p.stderr[-400:]
        # The prompt is echoed in Codex's log, so only Codex's own error lines are matched.
        errs = "\n".join(line for line in p.stderr.splitlines() if re.match(r"^(ERROR|error|Error)", line))
        if USAGE_LIMIT.search(errs):
            raise UsageLimit(errs[-300:])
        raise RuntimeError(f"codex exited {p.returncode}: {tail}")
    model, effort, sandbox = header(p.stderr, "model"), header(p.stderr, "reasoning effort"), header(p.stderr, "sandbox")
    if model != MODEL or effort != EFFORT or sandbox != "read-only":
        raise RuntimeError(f"codex ran {model}/{effort}/{sandbox}, not {MODEL}/{EFFORT}/read-only")
    fixed = json.loads(raw)["fixed"].strip()  # amend-va's own parsing
    if not fixed and intended.strip():
        raise RuntimeError("empty answer for a non-empty intended text (amend-va retries these)")
    tok = re.search(r"tokens used\s*\n\s*([\d,]+)", p.stderr)
    return {"fixed": fixed, "model": model, "tokens_used": int(tok.group(1).replace(",", "")) if tok else None}


def load_cache(path: pathlib.Path) -> dict:
    cache = {}
    if path.exists():
        for line in path.read_text().splitlines():
            if line.strip():
                r = json.loads(line)
                cache[r["prompt_sha256"]] = r
    return cache


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("results", type=pathlib.Path, help="harness results CSV (run_vocet_dictation.py -o)")
    ap.add_argument("manifest", type=pathlib.Path, help="clips/manifest.csv from fetch_dataset.py")
    ap.add_argument("-o", "--output", type=pathlib.Path, required=True)
    ap.add_argument("--label", required=True, help="engine label stored on every row, e.g. vocet_groq")
    ap.add_argument("--workers", type=int, default=4)
    ap.add_argument("--attempts", type=int, default=3)
    ap.add_argument("--limit", type=int, help="judge only the first N clips of the manifest (smoke test)")
    ap.add_argument("--max-calls", type=int, default=400, help="stop after this many Codex calls in this run")
    a = ap.parse_args()

    manifest = {r["clip_id"]: r for r in csv.DictReader(a.manifest.open(newline="", encoding="utf-8"))}
    if a.limit:
        manifest = dict(list(manifest.items())[:a.limit])
    results = {r["clip_id"]: r for r in csv.DictReader(a.results.open(newline="", encoding="utf-8"))}
    missing = sorted(set(manifest) - set(results))
    if missing:
        print(f"warning: {len(missing)} clips have no harness row; scored as empty drafts: {missing[:5]}...",
              file=sys.stderr)

    rows = []
    for cid, m in manifest.items():
        r = results.get(cid, {})
        draft = r.get("transcript", "") if r.get("status") == "ok" else ""
        prompt = build_prompt(m["intended_text"], draft, m["spoken_text"], PROMPT_VERSION)
        rows.append({"m": m, "r": r, "draft": draft, "prompt": prompt,
                     "sha": hashlib.sha256(prompt.encode()).hexdigest()})

    cache_path = a.output.with_suffix(".judge.jsonl")
    cache = load_cache(cache_path)
    todo = sorted({x["sha"]: (x["prompt"], x["m"]["intended_text"]) for x in rows if x["sha"] not in cache}.items())
    version = codex_version()
    via = f"codex exec (Codex CLI {version}, ChatGPT plan)"
    print(f"{len(rows)} drafts, {len(rows) - len(todo)} judged already (cache), {len(todo)} to judge "
          f"with {MODEL} effort={EFFORT} via {via}", file=sys.stderr)
    calls = [0]
    limit_hit = []

    with tempfile.TemporaryDirectory() as workdir, cf.ThreadPoolExecutor(a.workers) as pool:
        schema = os.path.join(workdir, "schema.json")
        system = os.path.join(workdir, "system.txt")
        pathlib.Path(schema).write_text(json.dumps(SCHEMA))
        pathlib.Path(system).write_text(SYSTEM)

        def job(item):
            sha, (prompt, intended) = item
            for attempt in range(1, a.attempts + 1):
                with _lock:
                    if _stop.is_set() or calls[0] >= a.max_calls:
                        return
                    calls[0] += 1
                try:
                    ans = ask_codex(prompt, intended, workdir, schema, system)
                    rec = {"prompt_sha256": sha, **ans, "effort": EFFORT, "prompt_version": PROMPT_VERSION,
                           "prompt_fingerprint": FINGERPRINT, "amend_va": AMEND_VERSION, "via": via,
                           "judged_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())}
                    with _lock, cache_path.open("a", encoding="utf-8") as fh:
                        fh.write(json.dumps(rec, ensure_ascii=False) + "\n")
                    cache[sha] = rec
                    return
                except UsageLimit as e:
                    print(f"Codex usage limit, stopping: {e}", file=sys.stderr)
                    limit_hit.append(str(e))
                    _stop.set()
                    return
                except Exception as e:  # noqa: BLE001 - retried, then reported
                    print(f"judge attempt {attempt} failed: {str(e)[:300]}", file=sys.stderr)
                    time.sleep(10 * attempt)
        done = 0
        for _ in pool.map(job, todo):
            done += 1
            if done % 10 == 0:
                print(f"  judged {done}/{len(todo)}", file=sys.stderr)

    fields = ["engine", "clip_id", "domain", "speaker_id", "audio_length_s", "status", "finalize_s",
              "draft", "fixed", "edits", "chars", "amend", "zero_edit",
              "judge_model", "judge_effort", "judge_via", "prompt_version", "prompt_fingerprint", "amend_va",
              "label"]
    edits_sum = chars_sum = zero = n = 0
    unjudged = 0
    with a.output.open("w", newline="", encoding="utf-8") as fh:
        w = csv.DictWriter(fh, fields)
        w.writeheader()
        for x in rows:
            m, r, c = x["m"], x["r"], cache.get(x["sha"])
            out = {"engine": a.label, "clip_id": m["clip_id"], "domain": m["domain"],
                   "speaker_id": m["speaker_id"], "audio_length_s": m["audio_length_s"],
                   "status": r.get("status", "missing"), "finalize_s": r.get("finalize_s", ""),
                   "draft": x["draft"],
                   "judge_model": MODEL, "judge_effort": EFFORT, "judge_via": c.get("via", via) if c else via,
                   "prompt_version": PROMPT_VERSION, "prompt_fingerprint": FINGERPRINT,
                   "amend_va": AMEND_VERSION, "label": LABEL}
            if c is None:
                unjudged += 1
            else:
                e, ch = char_edits(x["draft"], c["fixed"])
                out.update(fixed=c["fixed"], edits=e, chars=ch, amend=f"{(e / ch if ch else float(e > 0)):.5f}",
                           zero_edit=int(e == 0))
                edits_sum, chars_sum, zero, n = edits_sum + e, chars_sum + ch, zero + (e == 0), n + 1
            w.writerow(out)
    if n:
        print(f"{a.label}: ZER {100 * zero / n:.1f}%  AMEND {100 * edits_sum / chars_sum:.2f}%  "
              f"over {n} drafts ({unjudged} unjudged)  judge {MODEL}/{EFFORT}  prompt {PROMPT_VERSION} "
              f"fp {FINGERPRINT}  amend-va {AMEND_VERSION}  codex calls this run {calls[0]}")
    if limit_hit:
        sys.exit(3)
    if unjudged:
        sys.exit(1)


if __name__ == "__main__":
    main()
