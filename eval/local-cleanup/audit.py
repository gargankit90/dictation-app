"""Audit what a cleanup changed, independent of the judge: lost negations, changed identifiers, invented text.

    python3 audit.py runs/parakeet/s1-mini__s1 [more runs]   -> <run>/audit.csv and a count table

For each clip whose pasted text differs from the raw draft:
  negation   the raw text has more negations ("not", "no", "never", "n't" words, "nothing", "none") than the
             pasted text. Includes legitimate drops of a self-correction cue ("Actually, no, ..."); read the rows.
  identifier a code-like token of the raw text (camelCase, dotted, snake_case, a slash path, letters+digits,
             ALL CAPS of 2+ letters) is missing from the pasted text, compared case-sensitively.
  invented   a run of 3 or more consecutive words in the pasted text that never occur in the raw text.
"""
import csv
import difflib
import pathlib
import re
import sys

NEG = re.compile(r"\b(not|no|never|nothing|none|nobody|nowhere|cannot|\w+n't|\w+n’t)\b", re.I)
IDENT = re.compile(r"\b(?:[a-z]+[A-Z]\w*|\w+(?:[._/]\w+)+|[A-Za-z]+\d+\w*|[A-Z]{2,})\b")


def words(t: str) -> list[str]:
    return re.findall(r"[a-z0-9']+", t.lower().replace("’", "'"))


def audit(raw: str, final: str) -> dict:
    out = {"negation": "", "identifier": "", "invented": ""}
    if len(NEG.findall(raw)) > len(NEG.findall(final)):
        out["negation"] = f"{len(NEG.findall(raw))}->{len(NEG.findall(final))}"
    gone = [t for t in dict.fromkeys(IDENT.findall(raw)) if t not in final]
    if gone:
        out["identifier"] = " ".join(gone)
    a, b = words(raw), words(final)
    inv = []
    for op, i1, i2, j1, j2 in difflib.SequenceMatcher(None, a, b, autojunk=False).get_opcodes():
        new = [w for w in b[j1:j2] if w not in a]
        if op in ("insert", "replace") and len(new) >= 3:
            inv.append(" ".join(b[j1:j2]))
    out["invented"] = " | ".join(inv)
    return out


def main() -> None:
    print(f"{'run':40} {'changed':>7} {'negation':>8} {'ident':>5} {'invented':>8}")
    for d in map(pathlib.Path, sys.argv[1:]):
        rows = list(csv.DictReader((d / "results.csv").open(newline="", encoding="utf-8")))
        found = []
        changed = 0
        for r in rows:
            if r["transcript"] == r["raw"]:
                continue
            changed += 1
            x = audit(r["raw"], r["transcript"])
            if any(x.values()):
                found.append({"clip_id": r["clip_id"], **x, "raw": r["raw"], "pasted": r["transcript"]})
        with (d / "audit.csv").open("w", newline="", encoding="utf-8") as fh:
            w = csv.DictWriter(fh, ["clip_id", "negation", "identifier", "invented", "raw", "pasted"])
            w.writeheader()
            w.writerows(found)
        n = lambda k: sum(1 for f in found if f[k])  # noqa: E731
        print(f"{d.parent.name + '/' + d.name:40} {changed:>7} {n('negation'):>8} {n('identifier'):>5} {n('invented'):>8}")


if __name__ == "__main__":
    main()
