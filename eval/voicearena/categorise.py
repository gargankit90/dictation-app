"""Failure categories for the Groq run's 60 drafts that needed edits: assigned by hand from the
judge's corrections (diff of draft vs fixed), recorded here so the counts in REPORT.md can be
rechecked.  python3 -I categorise.py results/vocet_groq/scored.csv
Categories:
  thankyou  - a "Thank you." the speaker never said, added at the end (Whisper hallucination)
  filler    - kept fillers, hedges, discourse words or a stutter ("like", "you know", "Okay, so",
              "Oh, and", "I kind of", "Did you? Did you")
  symbols   - spoken symbols not written out in an email/URL/path ("at" for @, "slash" for /)
  code      - code identifiers, file names, package/product casing ("calculateProratedAmount",
              "date-fns", "Dockerfile", "react-query")
  name      - a person or place name spelled wrong ("Rahman" for "Raman", "Thomas" for "Tomas")
  misheard  - a wrong word or number from the speech engine ("get" for "git", "$850" for "$150")
  other     - slang unit ("89 bucks", "megs"), a dropped URL path
"""
import collections
import csv
import sys

CATS = {
    "dict_0008": "name symbols", "dict_0014": "misheard", "dict_0023": "thankyou",
    "dict_0024": "code thankyou", "dict_0053": "misheard", "dict_0077": "symbols thankyou",
    "dict_0087": "misheard", "dict_0097": "filler", "dict_0103": "filler", "dict_0108": "filler thankyou",
    "dict_0119": "misheard", "dict_0138": "filler", "dict_0152": "filler", "dict_0161": "misheard",
    "dict_0173": "filler symbols", "dict_0177": "misheard", "dict_0179": "filler", "dict_0215": "filler",
    "dict_0218": "code symbols", "dict_0228": "misheard", "dict_0229": "filler", "dict_0231": "filler",
    "dict_0232": "code misheard other", "dict_0239": "filler", "dict_0255": "filler thankyou misheard",
    "dict_0257": "thankyou", "dict_0258": "thankyou", "dict_0279": "misheard filler", "dict_0285": "filler",
    "dict_0310": "code", "dict_0334": "misheard", "dict_0351": "code misheard thankyou",
    "dict_0356": "misheard thankyou", "dict_0364": "misheard code", "dict_0388": "misheard",
    "dict_0389": "misheard", "dict_0399": "filler other", "dict_0407": "filler",
    "dict_0428": "misheard thankyou", "dict_0430": "name symbols thankyou", "dict_0461": "filler",
    "dict_0495": "filler", "dict_0496": "symbols", "dict_0497": "filler", "dict_0533": "filler name other",
    "dict_0536": "filler", "dict_0539": "misheard filler code", "dict_0544": "filler name symbols",
    "dict_0551": "filler misheard", "dict_0559": "filler", "dict_0565": "misheard", "dict_0578": "filler",
    "dict_0614": "misheard", "dict_0616": "misheard", "dict_0620": "misheard", "dict_0641": "filler",
    "dict_0663": "filler", "dict_0696": "misheard", "dict_0711": "name", "dict_0728": "code",
}

rows = [r for r in csv.DictReader(open(sys.argv[1], encoding="utf-8")) if int(r["edits"]) > 0]
missing = {r["clip_id"] for r in rows} - set(CATS)
assert not missing, f"uncategorised: {missing}"
n_all = sum(1 for _ in csv.DictReader(open(sys.argv[1], encoding="utf-8")))
hit, only = collections.Counter(), collections.Counter()
for r in rows:
    cats = set(CATS[r["clip_id"]].split())
    hit.update(cats)
    if len(cats) == 1:
        only.update(cats)
print(f"{len(rows)} of {n_all} drafts needed edits")
print("| category | drafts with it | drafts where it is the only problem |\n|---|---:|---:|")
for c, k in hit.most_common():
    print(f"| {c} | {k} | {only[c]} |")
app = {"thankyou", "filler", "symbols"}
fixable = [r for r in rows if set(CATS[r["clip_id"]].split()) <= app]
print(f"drafts whose every problem is thankyou/filler/symbols (app-side cleanup): {len(fixable)}"
      f" -> ZER would be {100 * (n_all - len(rows) + len(fixable)) / n_all:.1f}%")
app2 = app | {"code", "name"}
fixable2 = [r for r in rows if set(CATS[r["clip_id"]].split()) <= app2]
print(f"... adding code/name formatting (LLM cleanup with context): {len(fixable2)}"
      f" -> ZER would be {100 * (n_all - len(rows) + len(fixable2)) / n_all:.1f}%")
