# /// script
# requires-python = ">=3.11"
# dependencies = ["whisper-normalizer==1.0.1"]
# ///
"""Harms audit: dropped and invented content, where it happens (clip end, 30 s split point, elsewhere).

    uv run audit.py            -> audit.csv (one row per finding) and a summary table on stdout

For each run, each clip's pasted text is aligned word by word (difflib, after the Whisper English
normaliser) with what the speaker said (the dataset's spoken text). A finding is:
  dropped   a run of 3 or more spoken words missing from the text, at least 2 of which are content words
            that are also in the intended text (so a removed filler or abandoned false start does not count)
  invented  a run of 3 or more words in the text that are in neither the spoken nor the intended text
  repeated  (split clips) the end of one part repeated at the start of the next (the 0.15 s overlap)
Where: "end" = the run touches the last 4 spoken words; "split" = within 4 words of a part boundary in the
text (Gemma runs only, from the stored part replies); "start" = touches the first 3 words; else "middle".
It is a mechanical screen: misheard words also show up as small drop + invent pairs, so read audit.csv.
"""
import csv
import difflib
import json
import pathlib
import re

from whisper_normalizer.english import EnglishTextNormalizer

HERE = pathlib.Path(__file__).resolve().parent
MANIFEST = {r["clip_id"]: r for r in csv.DictReader((HERE.parent / "voicearena/clips/manifest.csv").open(encoding="utf-8"))}
NORM = EnglishTextNormalizer()
OH = re.compile(r"\boh\b", re.I)
STOP = set("a an the and or but so to of in on at for with is are was were be been it it's this that i i'm you we "
           "they he she my your our their me us them do does did not no yes just like know mean okay oh well um uh "
           "kind sort really very can could would should will have has had actually wait sorry".split())


def toks(t: str) -> list[str]:
    return NORM(OH.sub(" ", t)).split()


def runs():
    out = {"Parakeet raw": HERE / "baselines/parakeet_raw",
           "Parakeet + E2B text cleanup": HERE / "baselines/parakeet_gemma4-e2b__v3"}
    for d in sorted((HERE / "runs").glob("*__*")):
        if (d / "results.csv").exists():
            out[d.name] = d
    return out


def boundaries(row: dict) -> list[int]:
    """Token offsets in the joined text where a new part starts."""
    if "part_replies" not in row or int(row.get("parts") or 1) < 2:
        return []
    parts = json.loads(row["part_replies"])
    # the guard may have swapped in the plain text; boundaries only apply when the text is the joined parts
    if " ".join(p for p in parts if p) != row["transcript"]:
        return []
    b, n = [], 0
    for p in parts[:-1]:
        n += len(toks(p))
        b.append(n)
    return b


def main() -> None:
    findings, summary = [], {}
    for label, d in runs().items():
        rows = {r["clip_id"]: r for r in csv.DictReader((d / "results.csv").open(newline="", encoding="utf-8"))}
        cnt = {"dropped": 0, "invented": 0, "repeated": 0, "end": 0, "split": 0, "clips": set()}
        for cid, r in rows.items():
            m = MANIFEST[cid]
            text = r["transcript"] if r["status"] == "ok" else ""
            s, o, intended = toks(m["spoken_text"]), toks(text), set(toks(m["intended_text"]))
            bnd = boundaries(r)
            sm = difflib.SequenceMatcher(None, s, o, autojunk=False)
            for op, i1, i2, j1, j2 in sm.get_opcodes():
                if op in ("delete", "replace") and i2 - i1 >= 3:
                    lost = [w for w in s[i1:i2] if w not in STOP and w in intended and w not in o[j1:j2]]
                    if len(lost) >= 2:
                        where = ("end" if i2 >= len(s) - 4 else
                                 "split" if any(abs(j1 - b) <= 4 for b in bnd) else
                                 "start" if i1 < 3 else "middle")
                        findings.append({"run": label, "clip_id": cid, "kind": "dropped", "where": where,
                                         "words": " ".join(s[i1:i2]), "replaced_by": " ".join(o[j1:j2])})
                        cnt["dropped"] += 1; cnt["clips"].add(cid)
                        cnt["end"] += where == "end"; cnt["split"] += where == "split"
                if op in ("insert", "replace") and j2 - j1 >= 3:
                    new = [w for w in o[j1:j2] if w not in s and w not in intended]
                    if len(new) >= 3:
                        where = ("end" if j2 >= len(o) - 4 else
                                 "split" if any(abs(j1 - b) <= 4 for b in bnd) else
                                 "start" if j1 < 3 else "middle")
                        findings.append({"run": label, "clip_id": cid, "kind": "invented", "where": where,
                                         "words": " ".join(o[j1:j2]), "replaced_by": " ".join(s[i1:i2])})
                        cnt["invented"] += 1; cnt["clips"].add(cid)
                        cnt["end"] += where == "end"; cnt["split"] += where == "split"
            for b in bnd:
                for k in (4, 3):
                    if b >= k and o[b - k:b] == o[b:b + k]:
                        findings.append({"run": label, "clip_id": cid, "kind": "repeated", "where": "split",
                                         "words": " ".join(o[b - k:b]), "replaced_by": ""})
                        cnt["repeated"] += 1; cnt["split"] += 1; cnt["clips"].add(cid)
                        break
        cnt["clips"] = len(cnt["clips"])
        summary[label] = cnt
    with (HERE / "audit.csv").open("w", newline="", encoding="utf-8") as fh:
        w = csv.DictWriter(fh, ["run", "clip_id", "kind", "where", "words", "replaced_by"])
        w.writeheader()
        w.writerows(findings)
    print("| Run | Dropped runs | Invented runs | Repeated at split | of which at clip end | at a split | Clips affected |")
    print("| --- | ---: | ---: | ---: | ---: | ---: | ---: |")
    for label, c in summary.items():
        print(f'| {label} | {c["dropped"]} | {c["invented"]} | {c["repeated"]} | {c["end"]} | {c["split"]} | {c["clips"]} |')


if __name__ == "__main__":
    main()
