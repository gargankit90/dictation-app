"""Failure categories for the drafts that needed edits, per engine: assigned by hand from the
judge's corrections (diff of draft vs fixed), recorded here so the counts in REPORT.md can be
rechecked.  python3 -I categorise.py results/<engine>/scored.csv
Categories:
  thankyou  - a "Thank you." the speaker never said, added at the end (Whisper hallucination)
  filler    - kept fillers, hedges, discourse words or a stutter ("like", "you know", "Okay, so",
              "Oh, and", "I kind of", "Did you? Did you")
  symbols   - spoken symbols not written out in an email/URL/path ("at" for @, "slash" for /)
  code      - code identifiers, file names, package/product casing ("calculateProratedAmount",
              "date-fns", "Dockerfile", "react-query")
  name      - a person or place name spelled wrong ("Rahman" for "Raman", "Thomas" for "Tomas")
  misheard  - a wrong word or number from the speech engine ("get" for "git", "$850" for "$150")
  rewrite   - the engine's rewrite dropped content that was said (a phone number, the word "pot")
  other     - slang unit ("89 bucks", "megs"), a dropped URL path, "£3,900" for 3,900 pounds (weight)
"""
import collections
import csv
import sys

GROQ = {
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
ASSEMBLYAI = {
    "dict_0108": "misheard", "dict_0119": "rewrite", "dict_0177": "misheard", "dict_0229": "filler",
    "dict_0232": "other", "dict_0279": "misheard", "dict_0285": "filler", "dict_0335": "misheard",
    "dict_0351": "misheard", "dict_0356": "misheard", "dict_0389": "misheard", "dict_0430": "name",
    "dict_0496": "name", "dict_0533": "name", "dict_0536": "misheard", "dict_0544": "name",
    "dict_0620": "misheard", "dict_0621": "rewrite", "dict_0726": "filler", "dict_0728": "code",
}
PARAKEET = {
    "dict_0008": "name symbols", "dict_0014": "misheard", "dict_0024": "code misheard", "dict_0039": "name",
    "dict_0053": "misheard", "dict_0063": "misheard", "dict_0066": "misheard", "dict_0069": "misheard",
    "dict_0077": "name symbols", "dict_0097": "filler", "dict_0103": "filler", "dict_0108": "filler",
    "dict_0119": "misheard name", "dict_0124": "filler", "dict_0138": "filler", "dict_0152": "filler",
    "dict_0173": "filler symbols", "dict_0177": "misheard", "dict_0179": "filler", "dict_0215": "filler",
    "dict_0218": "code symbols", "dict_0229": "filler", "dict_0231": "filler", "dict_0232": "code misheard other",
    "dict_0239": "filler", "dict_0255": "filler misheard", "dict_0279": "misheard filler", "dict_0285": "filler",
    "dict_0310": "code", "dict_0329": "code misheard", "dict_0334": "misheard", "dict_0335": "code misheard",
    "dict_0346": "code misheard", "dict_0351": "code misheard", "dict_0356": "misheard", "dict_0364": "misheard",
    "dict_0381": "misheard", "dict_0388": "misheard", "dict_0389": "misheard", "dict_0391": "misheard",
    "dict_0392": "misheard", "dict_0399": "filler other", "dict_0407": "misheard filler", "dict_0428": "misheard",
    "dict_0430": "name symbols", "dict_0461": "filler", "dict_0487": "misheard", "dict_0495": "filler",
    "dict_0496": "symbols", "dict_0497": "filler", "dict_0506": "name", "dict_0533": "filler name symbols",
    "dict_0534": "misheard", "dict_0536": "filler", "dict_0539": "filler code name misheard",
    "dict_0544": "filler name symbols other", "dict_0551": "misheard filler", "dict_0559": "name misheard",
    "dict_0565": "misheard", "dict_0578": "filler", "dict_0614": "misheard", "dict_0616": "misheard",
    "dict_0620": "misheard", "dict_0629": "misheard", "dict_0641": "filler", "dict_0663": "filler",
    "dict_0711": "name", "dict_0728": "code", "dict_0729": "symbols", "dict_0737": "name", "dict_0749": "misheard",
}
QWEN3 = {
    "dict_0008": "symbols", "dict_0014": "misheard", "dict_0024": "code misheard", "dict_0039": "misheard name",
    "dict_0063": "misheard", "dict_0077": "misheard name symbols other", "dict_0097": "filler", "dict_0103": "filler",
    "dict_0108": "misheard filler", "dict_0119": "name", "dict_0124": "filler", "dict_0138": "filler",
    "dict_0152": "filler", "dict_0173": "filler", "dict_0177": "misheard", "dict_0179": "filler misheard",
    "dict_0215": "filler", "dict_0218": "symbols", "dict_0228": "misheard", "dict_0229": "filler",
    "dict_0231": "filler", "dict_0232": "code misheard other", "dict_0239": "filler misheard", "dict_0255": "filler",
    "dict_0279": "misheard filler", "dict_0285": "filler", "dict_0292": "misheard", "dict_0310": "code",
    "dict_0320": "misheard", "dict_0329": "code misheard", "dict_0335": "code misheard", "dict_0346": "code misheard",
    "dict_0351": "code misheard", "dict_0356": "misheard", "dict_0364": "misheard code", "dict_0371": "misheard",
    "dict_0388": "misheard", "dict_0389": "misheard", "dict_0392": "misheard", "dict_0399": "filler other",
    "dict_0407": "misheard filler", "dict_0428": "misheard", "dict_0430": "misheard name symbols",
    "dict_0461": "filler", "dict_0467": "filler", "dict_0471": "misheard", "dict_0487": "misheard",
    "dict_0495": "filler", "dict_0496": "symbols", "dict_0497": "misheard filler", "dict_0506": "name",
    "dict_0525": "name", "dict_0533": "filler name symbols misheard", "dict_0534": "misheard name",
    "dict_0536": "filler", "dict_0539": "filler name misheard code", "dict_0544": "filler name symbols misheard",
    "dict_0551": "misheard filler", "dict_0559": "misheard filler", "dict_0565": "name misheard",
    "dict_0578": "filler", "dict_0598": "misheard", "dict_0606": "misheard", "dict_0614": "misheard",
    "dict_0616": "misheard", "dict_0620": "misheard name", "dict_0629": "misheard", "dict_0641": "filler",
    "dict_0644": "misheard", "dict_0663": "filler", "dict_0680": "misheard", "dict_0711": "name",
    "dict_0725": "name", "dict_0728": "code", "dict_0729": "symbols", "dict_0737": "name", "dict_0749": "misheard",
}
BY_ENGINE = {"vocet_groq": GROQ, "vocet_assemblyai": ASSEMBLYAI, "vocet_parakeet": PARAKEET, "vocet_qwen3": QWEN3}

rows = [r for r in csv.DictReader(open(sys.argv[1], encoding="utf-8")) if int(r["edits"]) > 0]
CATS = BY_ENGINE[rows[0]["engine"]]
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
