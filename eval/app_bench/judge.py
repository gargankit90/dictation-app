#!/usr/bin/env python3
# /// script
# requires-python = ">=3.11"
# dependencies = ["amend-va==0.2.1", "jiwer"]
# ///
"""Score harness drafts with AMEND / ZER with one of three judges: claude, codex or gemini.

    uv run judge.py --judge claude work/<run>/results.csv clips/manifest.csv -o work/<run>/scored.csv --label <run>
    GEMINI_JUDGE_MODEL=gemini-3.8-flash uv run judge.py --judge gemini ...

UNOFFICIAL: single judges, public set only. The prompt is amend-va 0.2.1's v1 (imported from the pinned
package, byte for byte), the schema is {"fixed": string}, the score is amend.metric.char_edits:
pooled AMEND = sum(edits) / sum(chars), ZER = share of drafts with 0 edits. See judges.py for what
differs per CLI (system prompt, tools, sandbox).

Cache: one JSONL per judge and model in .judge-cache/ (git-ignored; `EVAL_JUDGE_CACHE` moves it),
keyed by the prompt's sha256, shared by every run: a rerun resumes and identical drafts are judged once.
It holds the judge's corrected text only, never a key. If the judge's plan reports a usage limit the run
stops (exit 3) and keeps what it has. The output CSV leaves out the dataset's spoken/intended references
(CC BY-NC 4.0); join on clip_id with clips/manifest.csv to see them.
"""
from __future__ import annotations

import argparse
import concurrent.futures as cf
import csv
import hashlib
import json
import os
import pathlib
import sys
import tempfile
import threading
import time

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from judges import NAMES, UsageLimit, make_judge  # noqa: E402
from judges import PROMPT_VERSION, slug  # noqa: E402

HERE = pathlib.Path(__file__).resolve().parent
FIELDS = ["engine", "clip_id", "domain", "speaker_id", "audio_length_s", "status", "finalize_s",
          "draft", "fixed", "edits", "chars", "amend", "zero_edit",
          "judge_model", "judge_effort", "judge_via", "prompt_version", "prompt_fingerprint", "amend_va",
          "label"]
_lock = threading.Lock()


def cache_path(judge) -> pathlib.Path:
    root = pathlib.Path(os.environ.get("EVAL_JUDGE_CACHE") or HERE / ".judge-cache")
    root.mkdir(parents=True, exist_ok=True)
    return root / f"{judge.name}-{slug(judge.model)}.jsonl"


def load_cache(path: pathlib.Path) -> dict:
    cache = {}
    if path.exists():
        for line in path.read_text(encoding="utf-8").splitlines():
            if line.strip():
                r = json.loads(line)
                cache[r["prompt_sha256"]] = r
    return cache


def amend_tools():
    """(build_prompt, char_edits, fingerprint, version) from the pinned amend-va; imported late so the
    cache, the CSV flow and the tests don't need the package."""
    import importlib.metadata
    from amend.metric import char_edits
    from amend.prompts import BASE, LENIENT, build_prompt
    return (build_prompt, char_edits, hashlib.sha256((BASE + LENIENT).encode()).hexdigest()[:12],
            importlib.metadata.version("amend-va"))


def read_csv(path: pathlib.Path) -> list[dict]:
    with path.open(newline="", encoding="utf-8") as fh:
        return list(csv.DictReader(fh))


def judge_run(judge, results: pathlib.Path, manifest: pathlib.Path, output: pathlib.Path, label: str, *,
              workers: int | None = None, attempts: int = 3, limit: int | None = None,
              max_calls: int = 400, tools=None, cache_file: pathlib.Path | None = None,
              backoff: float = 10.0, log=sys.stderr) -> int:
    """Judge every clip of the manifest, write the scored CSV, return the exit code (0 ok, 1 unjudged,
    3 usage limit)."""
    build_prompt, char_edits, fingerprint, amend_version = tools or amend_tools()
    man = {r["clip_id"]: r for r in read_csv(manifest)}
    if limit:
        man = dict(list(man.items())[:limit])
    res = {r["clip_id"]: r for r in read_csv(results)}
    missing = sorted(set(man) - set(res))
    if missing:
        print(f"warning: {len(missing)} clips have no harness row; scored as empty drafts: {missing[:5]}...",
              file=log)
    rows = []
    for cid, m in man.items():
        r = res.get(cid, {})
        draft = r.get("transcript", "") if r.get("status") == "ok" else ""
        prompt = build_prompt(m["intended_text"], draft, m["spoken_text"], PROMPT_VERSION)
        rows.append({"m": m, "r": r, "draft": draft, "sha": hashlib.sha256(prompt.encode()).hexdigest(),
                     "prompt": prompt})
    path = cache_file or cache_path(judge)
    cache = load_cache(path)
    todo = sorted({x["sha"]: (x["prompt"], x["m"]["intended_text"]) for x in rows
                   if x["sha"] not in cache}.items())
    print(f"{len(rows)} drafts, {len(rows) - len(todo)} judged already (cache), {len(todo)} to judge with "
          f"{judge.model} via {judge.via()}", file=log)
    calls, stop, limit_hit = [0], threading.Event(), []

    def job(item, workdir):
        sha, (prompt, intended) = item
        for attempt in range(1, attempts + 1):
            with _lock:
                if stop.is_set() or calls[0] >= max_calls:
                    return
                calls[0] += 1
            try:
                ans = judge.ask(prompt, intended, workdir)
                rec = {"prompt_sha256": sha, **ans, "effort": judge.effort, "prompt_version": PROMPT_VERSION,
                       "prompt_fingerprint": fingerprint, "amend_va": amend_version, "via": judge.via(),
                       "judged_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())}
                with _lock, path.open("a", encoding="utf-8") as fh:
                    fh.write(json.dumps(rec, ensure_ascii=False) + "\n")
                cache[sha] = rec
                return
            except UsageLimit as e:
                print(f"{judge.name} usage limit, stopping: {e}", file=log)
                limit_hit.append(str(e))
                stop.set()
                return
            except Exception as e:  # noqa: BLE001 - retried, then reported
                print(f"judge attempt {attempt} failed: {str(e)[:300]}", file=log)
                time.sleep(backoff * attempt)

    with tempfile.TemporaryDirectory() as workdir, cf.ThreadPoolExecutor(workers or judge.default_workers) as pool:
        done = 0
        for _ in pool.map(lambda it: job(it, workdir), todo):
            done += 1
            if done % 10 == 0:
                print(f"  judged {done}/{len(todo)}", file=log)

    edits_sum = chars_sum = zero = n = unjudged = 0
    output.parent.mkdir(parents=True, exist_ok=True)
    with output.open("w", newline="", encoding="utf-8") as fh:
        w = csv.DictWriter(fh, FIELDS)
        w.writeheader()
        for x in rows:
            m, r, c = x["m"], x["r"], cache.get(x["sha"])
            out = {"engine": label, "clip_id": m["clip_id"], "domain": m["domain"],
                   "speaker_id": m["speaker_id"], "audio_length_s": m["audio_length_s"],
                   "status": r.get("status", "missing"), "finalize_s": r.get("finalize_s", ""),
                   "draft": x["draft"], "judge_model": judge.model, "judge_effort": judge.effort,
                   "judge_via": c.get("via", judge.via()) if c else judge.via(),
                   "prompt_version": PROMPT_VERSION, "prompt_fingerprint": fingerprint,
                   "amend_va": amend_version, "label": judge.label}
            if c is None:
                unjudged += 1
            else:
                e, ch = char_edits(x["draft"], c["fixed"])
                out.update(fixed=c["fixed"], edits=e, chars=ch, amend=f"{(e / ch if ch else float(e > 0)):.5f}",
                           zero_edit=int(e == 0))
                edits_sum, chars_sum, zero, n = edits_sum + e, chars_sum + ch, zero + (e == 0), n + 1
            w.writerow(out)
    if n:
        print(f"{label}: ZER {100 * zero / n:.1f}%  AMEND {100 * edits_sum / max(chars_sum, 1):.2f}%  over {n} "
              f"drafts ({unjudged} unjudged)  judge {judge.model}  prompt {PROMPT_VERSION}  "
              f"fp {fingerprint}  amend-va {amend_version}  calls this run {calls[0]}", file=log)
    return 3 if limit_hit else (1 if unjudged else 0)


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("results", type=pathlib.Path, help="harness results CSV (needs the transcript column)")
    ap.add_argument("manifest", type=pathlib.Path, help="clips/manifest.csv from fetch_clips.py")
    ap.add_argument("-o", "--output", type=pathlib.Path, required=True)
    ap.add_argument("--label", required=True, help="run label stored on every row, e.g. handy_local")
    ap.add_argument("--judge", choices=NAMES, default="claude")
    ap.add_argument("--judge-model", help="override the judge's model (gemini: also GEMINI_JUDGE_MODEL)")
    ap.add_argument("--workers", type=int)
    ap.add_argument("--attempts", type=int, default=3)
    ap.add_argument("--limit", type=int, help="judge only the first N clips of the manifest (smoke test)")
    ap.add_argument("--max-calls", type=int, default=400, help="stop after this many judge calls in this run")
    a = ap.parse_args()
    judge = make_judge(a.judge, a.judge_model)
    try:
        code = judge_run(judge, a.results, a.manifest, a.output, a.label, workers=a.workers,
                         attempts=a.attempts, limit=a.limit, max_calls=a.max_calls)
    finally:
        judge.close()
    sys.exit(code)


if __name__ == "__main__":
    main()
