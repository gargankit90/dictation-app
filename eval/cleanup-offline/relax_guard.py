"""Counterfactual, no model calls: re-apply guard.judge_relaxed to a finished run's replies.

    python3 eval/cleanup-offline/relax_guard.py <label>   # results/<label>/ -> results/<label>_relaxed/

The relaxed guard is NOT the app's: it also accepts a reply that drops a number or a capitalised name
when, in the raw text, that value is followed within 30 characters by a correction cue (sorry, wait,
no, actually, make that, I mean, rather, scratch that, correction), i.e. the model resolved a spoken
self-correction ("starting at 9, sorry, starting at 10" -> "starting at 10"). Everything else is the
app's guard. It shows what the strict guard costs; score the new folder with score.sh as usual.
"""
import csv
import pathlib
import shutil
import sys

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import guard  # noqa: E402

label = sys.argv[1]
src, dst = HERE / "results" / label, HERE / "results" / f"{label}_relaxed"
dst.mkdir(exist_ok=True)
prompts = {p.stem: p.read_text(encoding="utf-8").strip() for p in (HERE / "prompts").glob("*.txt")}
rows = list(csv.DictReader((src / "results.csv").open(newline="", encoding="utf-8")))
changed = 0
for r in rows:
    if r["reason"] in ("dropped_number", "dropped_name") and r["cleaned_text"]:
        reason = guard.judge_relaxed(r["raw_text"], r["cleaned_text"], prompts[r["prompt_name"]])
        if reason != r["reason"]:
            changed += 1
        r["reason"] = reason
        r["transcript"] = r["cleaned_text"] if reason == "cleaned" else r["raw_text"]
with (dst / "results.csv").open("w", newline="", encoding="utf-8") as fh:
    w = csv.DictWriter(fh, list(rows[0].keys()))
    w.writeheader()
    w.writerows(rows)
shutil.copy(src / "cleanup.jsonl", dst / "cleanup.jsonl")
print(f"{label}_relaxed: {changed} fallbacks now pasted the reply")
