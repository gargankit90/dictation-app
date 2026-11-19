"""Harm scan beyond the judges: clips where the pasted text lost content words of the raw text.

    python3 eval/cleanup-offline/word_loss.py <label> [min_lost=4]

Counts raw words (lowercased letters/digits) missing from the pasted text, as a multiset, ignoring
fillers and discourse openers the prompts allow deleting. Prints each clip that lost at least
min_lost such words with the lost words and both texts, for reading by hand.
"""
import csv
import pathlib
import sys
from collections import Counter

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import guard  # noqa: E402

ALLOWED = set("uh um er ah oh huh yeah well right okay ok so and but no like just you know i mean guess kind of "
              "actually sorry wait thank thanks".split())
label = sys.argv[1]
min_lost = int(sys.argv[2]) if len(sys.argv) > 2 else 4
n = 0
for r in csv.DictReader((HERE / "results" / label / "results.csv").open(newline="", encoding="utf-8")):
    lost = Counter(guard.word_parts(r["raw_text"])) - Counter(guard.word_parts(r["transcript"]))
    content = [w for w in lost.elements() if w not in ALLOWED]
    if len(content) >= min_lost:
        n += 1
        print(f"## {r['clip_id']} lost {len(content)}: {' '.join(content)}\n- raw: {r['raw_text']}\n- pasted: {r['transcript']}\n")
print(f"{label}: {n} clips lost >= {min_lost} content words")
