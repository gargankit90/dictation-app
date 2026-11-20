# /// script
# requires-python = ">=3.11"
# dependencies = ["amend-va==0.2.1", "jiwer"]
# ///
"""Score harness drafts with AMEND / ZER, using Gemini 4 Argon through the Antigravity CLI (`agy -p`).

A third judge next to score_claude_cli.py (Claude Opus 5.5) and score_codex_cli.py (GPT-6 Astra): same
interface, same output columns, same prompt, same parsing and scoring. UNOFFICIAL: Gemini 4 Argon via
Antigravity, public set only.

Why Gemini 4 Argon: it is the Google judge on Voice Arena's own five-judge panel, so with Claude and
Astra we can report a 2-of-3 majority, closer to the panel's 3-of-5. Antigravity offers it on the owner's
subscription (no API key) as `gemini-4-argon-high`, the highest effort it lists for Argon (it has no
xhigh/max variant; `--effort` is not passed because the effort is part of the model id).

What is the same as amend-va 0.2.1 / the other two judges:
  - the prompt: amend.prompts.build_prompt(intended, draft, spoken, "v1"), byte for byte;
  - the output schema {"fixed": string} (passed with --json-schema), parsed as structured_output["fixed"]
    .strip() (falling back to json.loads(response)["fixed"]); an empty answer for a non-empty intended
    text counts as a failure and is retried, as amend-va does;
  - the scoring: amend.metric.char_edits, pooled AMEND = sum(edits) / sum(chars), ZER = share of drafts
    with 0 edits.
What differs: the request goes through Antigravity's agent (its own system prompt and tool definitions,
about 10k input tokens, can't be replaced from the CLI), not the Gemini API, so the one-line judge
system prompt of the other two judges is not sent. agy reads the prompt only as an argument
(`-p=<prompt>`; stdin is ignored), so the prompt is passed in argv. It runs in `--sandbox --mode plan`
(terminal restricted, no edits) from an empty temporary directory. Each answer is checked to come from
the requested model (agy's own log line `Print mode: starting (... model="...")`) before it is cached.

    uv run score_gemini_cli.py results/vocet_groq/harness_results.csv clips/manifest.csv \\
        -o results/vocet_groq/scored_gemini.csv --label vocet_groq

Judge answers are cached in <output>.judge.jsonl, keyed by the prompt's sha256, so a rerun resumes and
identical drafts are judged once. If agy reports a usage or rate limit, the run stops (exit 3) and keeps
what it has. The output CSV leaves out the dataset's spoken/intended references (CC BY-NC 4.0).
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
import shutil
import subprocess
import sys
import tempfile
import threading
import time

from amend.metric import char_edits
from amend.prompts import BASE, LENIENT, build_prompt

PROMPT_VERSION = "v1"
MODEL = "gemini-4-argon-high"
EFFORT = "high"
SCHEMA = json.dumps({"type": "object", "properties": {"fixed": {"type": "string"}},
                     "required": ["fixed"], "additionalProperties": False})
FINGERPRINT = hashlib.sha256((BASE + LENIENT).encode()).hexdigest()[:12]
AMEND_VERSION = importlib.metadata.version("amend-va")
LABEL = "unofficial: third judge (Gemini 4 Argon via Antigravity), public set only"
AGY = os.environ.get("AGY_BIN") or shutil.which("agy") or os.path.expanduser("~/.local/bin/agy")
_lock = threading.Lock()
_stop = threading.Event()
USAGE_LIMIT = re.compile(r"usage limit|rate limit|rate-limit|quota|too many requests|resource.?exhausted|\b429\b",
                         re.I)


class UsageLimit(RuntimeError):
    pass


def agy_version() -> str:
    try:
        p = subprocess.run([AGY, "--version"], capture_output=True, text=True, timeout=30)
        out = (p.stdout or p.stderr).strip().splitlines()
        return out[0][:60] if p.returncode == 0 and out else "unknown"
    except Exception:  # noqa: BLE001
        return "unknown"


def agy_cmd(prompt: str, log: str) -> list[str]:
    return [AGY, "--model", MODEL, "--output-format", "json", "--sandbox", "--mode", "plan",
            "--json-schema", SCHEMA, "--log-file", log, f"-p={prompt}"]


def ask_gemini(prompt: str, intended: str, timeout: int = 1200) -> dict:
    # Orca's hooks report status when these are set; a judge call must not.
    env = {k: v for k, v in os.environ.items() if not k.startswith("ORCA_")}
    with tempfile.TemporaryDirectory() as workdir:
        log = os.path.join(workdir, "agy.log")
        p = subprocess.run(agy_cmd(prompt, log), stdin=subprocess.DEVNULL, capture_output=True, text=True,
                           cwd=workdir, timeout=timeout, env=env)
        logtext = pathlib.Path(log).read_text(encoding="utf-8", errors="replace") if os.path.exists(log) else ""
    try:
        d = json.loads(p.stdout)
    except ValueError:
        d = None
    # Only agy's own status/error text is matched, never the echoed prompt.
    status = str(d.get("status")) if isinstance(d, dict) else ""
    errtext = p.stderr if d is None else (p.stderr + " " + status + " " + str(d.get("error", "")))
    errs = "\n".join(l for l in logtext.splitlines() if l[:1] == "E" and "not logged into" not in l)
    if p.returncode != 0 or not isinstance(d, dict) or status != "SUCCESS":
        if USAGE_LIMIT.search(errtext) or USAGE_LIMIT.search(errs):
            raise UsageLimit((errtext.strip() or errs)[-300:])
        raise RuntimeError(f"agy exited {p.returncode} status={status!r}: {p.stderr[-300:]}")
    m = re.search(r'Print mode: starting \(.*?model="([^"]+)"', logtext)
    model = m.group(1) if m else None
    if model != MODEL:
        raise RuntimeError(f"agy ran model {model!r}, not {MODEL}")
    out = d.get("structured_output")
    if not isinstance(out, dict) or not isinstance(out.get("fixed"), str):
        out = json.loads(str(d.get("response", "")).strip())  # amend-va parses the text the same way
    fixed = out["fixed"].strip()
    if not fixed and intended.strip():
        raise RuntimeError("empty answer for a non-empty intended text (amend-va retries these)")
    u = d.get("usage") or {}
    return {"fixed": fixed, "model": model, "output_tokens": u.get("output_tokens"),
            "thinking_tokens": u.get("thinking_tokens"), "input_tokens": u.get("input_tokens"),
            "seconds": d.get("duration_seconds")}


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
    ap.add_argument("--workers", type=int, default=6)
    ap.add_argument("--attempts", type=int, default=3)
    ap.add_argument("--limit", type=int, help="judge only the first N clips of the manifest (smoke test)")
    ap.add_argument("--max-calls", type=int, default=400, help="stop after this many agy calls in this run")
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
    via = f"agy -p (Antigravity CLI {agy_version()}, --sandbox --mode plan, owner subscription)"
    print(f"{len(rows)} drafts, {len(rows) - len(todo)} judged already (cache), {len(todo)} to judge "
          f"with {MODEL} via {via}", file=sys.stderr)
    calls = [0]
    limit_hit = []

    def job(item):
        sha, (prompt, intended) = item
        for attempt in range(1, a.attempts + 1):
            with _lock:
                if _stop.is_set() or calls[0] >= a.max_calls:
                    return
                calls[0] += 1
            try:
                ans = ask_gemini(prompt, intended)
                rec = {"prompt_sha256": sha, **ans, "effort": EFFORT, "prompt_version": PROMPT_VERSION,
                       "prompt_fingerprint": FINGERPRINT, "amend_va": AMEND_VERSION, "via": via,
                       "judged_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())}
                with _lock, cache_path.open("a", encoding="utf-8") as fh:
                    fh.write(json.dumps(rec, ensure_ascii=False) + "\n")
                cache[sha] = rec
                return
            except UsageLimit as e:
                print(f"Gemini usage limit, stopping: {e}", file=sys.stderr)
                limit_hit.append(str(e))
                _stop.set()
                return
            except Exception as e:  # noqa: BLE001 - retried, then reported
                print(f"judge attempt {attempt} failed: {str(e)[:300]}", file=sys.stderr)
                time.sleep(10 * attempt)

    with cf.ThreadPoolExecutor(a.workers) as pool:
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
              f"over {n} drafts ({unjudged} unjudged)  judge {MODEL}  prompt {PROMPT_VERSION} "
              f"fp {FINGERPRINT}  amend-va {AMEND_VERSION}  agy calls this run {calls[0]}")
    if limit_hit:
        sys.exit(3)
    if unjudged:
        sys.exit(1)


if __name__ == "__main__":
    main()
