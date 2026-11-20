#!/usr/bin/env python3
"""The regression check: run the app's current cleanup prompt on the raw transcripts of a fixed 40-clip
subset, judge the result, and compare ZER with the stored baseline.

    eval/regress.sh --dry-run          # the plan and a cost estimate; no network, no key needed
    OPENROUTER_API_KEY=... eval/regress.sh [--model z-ai/glm-5.3-flash] [--judge claude] [--threshold 5]

Costs API calls (40 speech + 40 cleanup requests) and judge calls, so it is not part of the gate. Required
before merging any prompt or model-default change (docs/ENGINEERING.md). The clips are Voice Arena's
(CC BY-NC), so neither their audio nor their transcripts are in the repo: the first run downloads the
dataset (`uv run eval/fetch_dataset.py eval/clips`, git-ignored) and transcribes the 40 clips once with
the baseline's speech model; the raw text is then saved in eval/regress/work/ (git-ignored) and reused,
so a rerun costs only the cleanup and the judge. What is committed: clips.txt (the ids), baseline.csv
(per clip: judged edits, characters; no text) and baseline.json (what the baseline is).

Steps: (1) transcribe the 40 clips if not saved; (2) offline_cleanup.py with the app's prompt
(evalcommon.app_prompt_path(); EVAL_APP_PROMPT overrides) and the app's guard; (3) judge.py on the
drafts; (4) compare with baseline.csv. Exit 0 if ZER dropped by no more than --threshold points
(default 5.0 = two clips of 40), 1 if it dropped more, 2 for a setup problem.

The baseline was judged on raw text from one speech run; transcribing again can differ by a word or
two, so treat a drop near the threshold as a prompt for a closer look, not a verdict.
"""
from __future__ import annotations

import argparse
import csv
import hashlib
import json
import os
import pathlib
import subprocess
import sys

HERE = pathlib.Path(__file__).resolve().parent
EVAL = HERE.parent
sys.path.insert(0, str(EVAL))
from evalcommon import app_prompt_path, load_split  # noqa: E402
from report import paired, percentile, read_csv, zer_amend  # noqa: E402

WORK = HERE / "work"
CLIPS = EVAL / "clips"
MANIFEST = CLIPS / "manifest.csv"
OFFLINE_CLEANUP = EVAL / "offline_cleanup.py"
SCRIPTS = [EVAL / "fetch_dataset.py", EVAL / "transcribe_openrouter.py", OFFLINE_CLEANUP, EVAL / "judge.py"]
SPEECH_MODEL = "openai/gpt-4o-mini-transcribe"  # what the baseline's raw text came from
RAW = WORK / "asr" / SPEECH_MODEL.replace("/", "__") / "harness_results.csv"
# Measured over these 40 clips in PR #332: $0.0217 for 40 speech requests.
SPEECH_COST_PER_CLIP = 0.00054
DEFAULT_MODEL = "z-ai/glm-5.3-flash"
# Measured in PR #332's cleanup log: 149 GLM 5.3 Flash calls with prompt v4 cost $0.0184 (about $0.000123
# per call). Other models cost differently: pass --cost-per-call.
GLM_COST_PER_CALL = 0.000123


def prompt_sha(text: str) -> str:
    return hashlib.sha256(text.strip().encode("utf-8")).hexdigest()[:12]


def plan(a, n: int) -> dict:
    path = app_prompt_path()
    text = path.read_text(encoding="utf-8")
    cost = n * (a.cost_per_call if a.cost_per_call is not None else GLM_COST_PER_CALL)
    speech = 0.0 if RAW.exists() else n * SPEECH_COST_PER_CLIP
    return {
        "clips": n, "speech_model": SPEECH_MODEL, "speech_requests": 0 if RAW.exists() else n,
        "estimated_speech_cost_usd": round(speech, 4),
        "raw_text": "saved in eval/regress/work (reused)" if RAW.exists() else "not saved yet: transcribed on the first run", "prompt": str(path), "prompt_sha": prompt_sha(text), "model": a.model, "judge": a.judge,
        "cleanup_requests": n, "judge_calls": n, "estimated_cleanup_cost_usd": round(cost, 4),
        "cost_basis": ("PR #332 logs: GLM 5.3 Flash + prompt v4, gpt-4o-mini-transcribe" if a.cost_per_call is None and a.model == DEFAULT_MODEL
                       else "per-call price given with --cost-per-call, or the GLM figure as a guide"),
        "baseline": json.loads((HERE / "baseline.json").read_text()),
        "threshold_points": a.threshold, "work_dir": str(WORK),
        "key": "OPENROUTER_API_KEY is " + ("set" if os.environ.get("OPENROUTER_API_KEY") else "NOT set"),
        "manifest": "found" if MANIFEST.exists() else "missing: the first run downloads it (eval/fetch_dataset.py)",
    }


def print_plan(p: dict) -> None:
    b = p["baseline"]
    print("regression check plan (nothing sent; --dry-run)")
    print(f"  clips            {p['clips']} (eval/regress/clips.txt, both halves, all domains)")
    print(f"  speech           {p['speech_model']}: {p['speech_requests']} requests; raw text {p['raw_text']}")
    print(f"  prompt           {p['prompt']}  sha {p['prompt_sha']}")
    print(f"  cleanup model    {p['model']}  ({p['cleanup_requests']} requests, app guard applied)")
    print(f"  judge            {p['judge']}  ({p['judge_calls']} calls, your CLI plan, no per-call API cost)")
    print(f"  estimated cost   ${p['estimated_speech_cost_usd']:.4f} speech + ${p['estimated_cleanup_cost_usd']:.4f} cleanup"
          f"  [{p['cost_basis']}]")
    print(f"  baseline         {b['cleanup_model']} + prompt {b['prompt_name']} ({b['prompt_sha']}): "
          f"ZER {b['zer_pct']}% on these clips, AMEND {b['amend_pct']}%")
    print(f"  fails when       ZER drops more than {p['threshold_points']} points below the baseline")
    print(f"  {p['key']};  dataset manifest: {p['manifest']}")


def compare(scored_path: pathlib.Path, threshold: float) -> int:
    base = {r["clip_id"]: r for r in read_csv(HERE / "baseline.csv")}
    rows = read_csv(scored_path)
    cur = {r["clip_id"]: r for r in rows}
    if len(rows) < len(base):
        print(f"{len(base) - len(rows)} of the {len(base)} clips are missing from {scored_path}", file=sys.stderr)
        return 2
    split = load_split()
    zb, ab = zer_amend(list(base.values()))
    zc, ac = zer_amend(rows)
    print(f"\nregression check on {len(rows)} clips ({scored_path})")
    print(f"  ZER    baseline {zb:5.1f}%  current {zc:5.1f}%  change {zc - zb:+.1f} points")
    print(f"  AMEND  baseline {ab:5.2f}%  current {ac:5.2f}%")
    for half in ("tune", "check"):
        zb_h, _ = zer_amend([r for r in base.values() if split[r["clip_id"]] == half])
        zc_h, _ = zer_amend([r for r in rows if split[r["clip_id"]] == half])
        print(f"  ZER {half:5} baseline {zb_h:5.1f}%  current {zc_h:5.1f}%")
    w, l, t = paired(cur, base)
    print(f"  paired vs baseline: {w} fixed, {l} regressed, {t} unchanged")
    broke = sorted(c for c, r in cur.items() if c in base and int(base[c]["zero_edit"]) == 1 and int(r["edits"]) > 0)
    if broke:
        print("  regressed clips: " + " ".join(broke) + "  (join with clips/manifest.csv to read them)")
    if zb - zc > threshold:
        print(f"FAIL: ZER fell {zb - zc:.1f} points, more than the {threshold} allowed")
        return 1
    print("PASS: within the allowed drop")
    return 0


def run(cmd: list[str]) -> None:
    print("+ " + " ".join(cmd), file=sys.stderr)
    subprocess.run(cmd, check=True)


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--dry-run", action="store_true", help="print the plan and the cost estimate; no network")
    ap.add_argument("--model", default=DEFAULT_MODEL, help="OpenRouter cleanup model (default: the app's)")
    ap.add_argument("--judge", choices=["claude", "codex", "gemini"], default="claude")
    ap.add_argument("--threshold", type=float, default=5.0, help="allowed ZER drop in points (default 5.0)")
    ap.add_argument("--max-cost", type=float, default=0.25, help="stop the cleanup run at this USD spend")
    ap.add_argument("--cost-per-call", type=float, help="USD per cleanup call, for the estimate only")
    ap.add_argument("--compare-only", type=pathlib.Path, metavar="SCORED_CSV",
                    help="skip the runs; compare an existing scored CSV with the baseline")
    a = ap.parse_args()
    ids = [x for x in (HERE / "clips.txt").read_text().split() if x]
    if a.compare_only:
        return compare(a.compare_only, a.threshold)
    p = plan(a, len(ids))
    if a.dry_run:
        print_plan(p)
        return 0
    if not os.environ.get("OPENROUTER_API_KEY"):
        print("OPENROUTER_API_KEY is not set in the environment", file=sys.stderr)
        return 2
    missing = [str(x) for x in SCRIPTS if not x.is_file()]
    if missing:  # before anything is spent
        print("missing scripts: " + ", ".join(missing), file=sys.stderr)
        return 2
    if not pathlib.Path(p["prompt"]).is_file():
        print(f"cleanup prompt not found: {p['prompt']}", file=sys.stderr)
        return 2
    print_plan(p)
    py = sys.executable
    if not MANIFEST.exists():
        run(["uv", "run", "--quiet", str(EVAL / "fetch_dataset.py"), str(CLIPS)])
    WORK.mkdir(exist_ok=True)
    sub = WORK / "manifest.csv"  # the judge needs only these clips' reference texts
    rows = [r for r in read_csv(MANIFEST) if r["clip_id"] in set(ids)]
    with sub.open("w", newline="", encoding="utf-8") as fh:
        w = csv.DictWriter(fh, list(rows[0].keys()), lineterminator="\n")
        w.writeheader()
        w.writerows(rows)
    if not RAW.exists():
        run(["uv", "run", "--quiet", str(EVAL / "transcribe_openrouter.py"), "--model", SPEECH_MODEL, "--clips", str(CLIPS),
             "--out", str(WORK / "asr"), "--ids-file", str(HERE / "clips.txt"), "--max-usd", "0.25"])
    py = sys.executable
    run([py, str(OFFLINE_CLEANUP), "--provider", "openrouter", "--model", a.model,
         "--prompt", p["prompt"], "--label", "current", "--input", str(RAW),
         "--results-dir", str(WORK), "--max-cost", str(a.max_cost)])
    run(["uv", "run", "--quiet", str(EVAL / "judge.py"), "--judge", a.judge, str(WORK / "current" / "results.csv"),
         str(sub), "-o", str(WORK / "scored.csv"), "--label", "regress_current"])
    return compare(WORK / "scored.csv", a.threshold)


if __name__ == "__main__":
    sys.exit(main())
