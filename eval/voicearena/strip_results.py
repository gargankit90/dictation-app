"""Copy a harness results CSV without the dataset's reference text (CC BY-NC 4.0; not republished in bulk).

    python3 -I strip_results.py results/<engine>/results.csv results/<engine>/harness_results.csv
"""
import csv
import sys

src, dst = sys.argv[1], sys.argv[2]
with open(src, newline="", encoding="utf-8") as f:
    rows = list(csv.DictReader(f))
fields = [k for k in (rows[0].keys() if rows else []) if k != "reference_text"]
with open(dst, "w", newline="", encoding="utf-8") as f:
    w = csv.DictWriter(f, fields, extrasaction="ignore")
    w.writeheader()
    w.writerows(rows)
print(f"{len(rows)} rows -> {dst}")
