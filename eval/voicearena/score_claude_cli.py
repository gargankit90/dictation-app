# /// script
# requires-python = ">=3.11"
# dependencies = ["amend-va==0.2.1", "jiwer"]
# ///
"""Score harness drafts with AMEND / ZER, using Claude Opus 5.5 through the `claude -p` CLI as the judge.

UNOFFICIAL: single judge (Claude Opus 5.5 via Claude Code), public set only.

What is the same as amend-va 0.2.1 (the version behind the whitepaper):
  - the prompt: amend.prompts.build_prompt(intended, draft, spoken, "v1"), imported from the pinned
    package, so the text is byte for byte what amend-va sends;
  - the model and effort: claude-opus-5-5 at effort "max" (amend-va's defaults);
  - the output schema {"fixed": string}, parsed and .strip()ped the way amend-va's judge does;
  - the scoring: amend.metric.char_edits (amend-va's normalisation + jiwer character edits), pooled
    AMEND = sum(edits) / sum(chars), ZER = share of drafts with 0 edits (amend-va's AMEND@0).
What differs: the request goes through Claude Code's CLI (the owner's subscription) instead of the
Anthropic API, so it carries a one-line system prompt and Claude Code's request plumbing; the API
call amend-va makes has no system prompt. The fallback model setting of amend-va's request is not
reproduced. The whitepaper's fingerprint d26616976626 is not reproducible because amend-va does not
publish how it is computed; this script records its own fingerprint (sha256 of the v1 prompt template,
first 12 hex) and amend-va's version on every row.

    uv run score_claude_cli.py results_vocet.csv clips/manifest.csv -o scored_vocet.csv --label vocet_groq

Judge answers are cached in <output>.judge.jsonl, keyed by the prompt's sha256, so a rerun resumes and
identical drafts are judged once. The cache holds the judge's corrected text only (no keys).
The output CSV leaves out the dataset's spoken/intended references (CC BY-NC 4.0, not ours to
republish in bulk); join on clip_id with clips/manifest.csv to see them.
"""
import argparse
import concurrent.futures as cf
import csv
import hashlib
import importlib.metadata
import json
import pathlib
import subprocess
import sys
import tempfile
import threading
import time

from amend.metric import char_edits
from amend.prompts import BASE, LENIENT, build_prompt

PROMPT_VERSION = "v1"
MODEL = "claude-opus-5-5"
EFFORT = "max"
SCHEMA = json.dumps({"type": "object", "properties": {"fixed": {"type": "string"}},
                     "required": ["fixed"], "additionalProperties": False})
SYSTEM = "You are evaluating dictation output. Follow the user's instructions exactly."
FINGERPRINT = hashlib.sha256((BASE + LENIENT).encode()).hexdigest()[:12]
AMEND_VERSION = importlib.metadata.version("amend-va")
_lock = threading.Lock()


def ask_claude(prompt: str, cwd: str, timeout: int = 900) -> dict:
    cmd = ["claude", "-p", "--model", MODEL, "--effort", EFFORT, "--tools", "", "--no-session-persistence",
           "--setting-sources", "", "--strict-mcp-config", "--system-prompt", SYSTEM,
           "--output-format", "json", "--json-schema", SCHEMA]
    p = subprocess.run(cmd, input=prompt, capture_output=True, text=True, cwd=cwd, timeout=timeout)
    if p.returncode != 0:
        raise RuntimeError(f"claude exited {p.returncode}: {p.stderr[-300:]}")
    d = json.loads(p.stdout)
    if d.get("is_error") or d.get("subtype") != "success":
        raise RuntimeError(f"claude error: {d.get('subtype')} {str(d.get('result'))[:200]}")
    out = d.get("structured_output")
    if not isinstance(out, dict) or not isinstance(out.get("fixed"), str):
        out = json.loads(d["result"])  # amend-va parses the text the same way
    usage = d.get("modelUsage") or {}
    if MODEL not in usage:
        raise RuntimeError(f"the answer did not come from {MODEL}: {list(usage)}")
    return {"fixed": out["fixed"].strip(), "model": MODEL,
            "output_tokens": usage[MODEL].get("outputTokens"), "cost_usd": d.get("total_cost_usd")}


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
    a = ap.parse_args()

    manifest = {r["clip_id"]: r for r in csv.DictReader(a.manifest.open(newline="", encoding="utf-8"))}
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
    todo = sorted({x["sha"]: x["prompt"] for x in rows if x["sha"] not in cache}.items())
    print(f"{len(rows)} drafts, {len(rows) - len(todo)} judged already (cache), {len(todo)} to judge "
          f"with {MODEL} effort={EFFORT} via claude -p", file=sys.stderr)

    with tempfile.TemporaryDirectory() as cwd, cf.ThreadPoolExecutor(a.workers) as pool:
        def job(item):
            sha, prompt = item
            for attempt in range(1, a.attempts + 1):
                try:
                    ans = ask_claude(prompt, cwd)
                    rec = {"prompt_sha256": sha, **ans, "effort": EFFORT, "prompt_version": PROMPT_VERSION,
                           "prompt_fingerprint": FINGERPRINT, "amend_va": AMEND_VERSION,
                           "judged_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())}
                    with _lock, cache_path.open("a", encoding="utf-8") as fh:
                        fh.write(json.dumps(rec, ensure_ascii=False) + "\n")
                    cache[sha] = rec
                    return
                except Exception as e:  # noqa: BLE001 - retried, then reported
                    print(f"judge attempt {attempt} failed: {e}", file=sys.stderr)
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
                   "judge_model": MODEL, "judge_effort": EFFORT, "judge_via": "claude -p (Claude Code CLI)",
                   "prompt_version": PROMPT_VERSION, "prompt_fingerprint": FINGERPRINT,
                   "amend_va": AMEND_VERSION,
                   "label": "unofficial: single judge (Claude Opus 5.5 via Claude Code), public set only"}
            if c is None:
                unjudged += 1
            else:
                e, ch = char_edits(x["draft"], c["fixed"])
                out.update(fixed=c["fixed"], edits=e, chars=ch, amend=f"{(e / ch if ch else float(e > 0)):.5f}",
                           zero_edit=int(e == 0))
                edits_sum, chars_sum, zero, n = edits_sum + e, chars_sum + ch, zero + (e == 0), n + 1
            w.writerow(out)
    print(f"{a.label}: ZER {100 * zero / n:.1f}%  AMEND {100 * edits_sum / chars_sum:.2f}%  "
          f"over {n} drafts ({unjudged} unjudged)  judge {MODEL}/{EFFORT}  prompt {PROMPT_VERSION} "
          f"fp {FINGERPRINT}  amend-va {AMEND_VERSION}")
    if unjudged:
        sys.exit(1)


if __name__ == "__main__":
    main()
