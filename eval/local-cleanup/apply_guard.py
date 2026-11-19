"""Re-apply a different guard to a finished run, from the model replies it stored (no model run needed).

    python3 apply_guard.py runs/parakeet/s1-mini__s1 --mode noguard     -> runs/parakeet/s1-mini__s1__noguard/
    python3 apply_guard.py runs/parakeet/s1-mini__s1 --mode strict      -> runs/parakeet/s1-mini__s1__strict/

noguard  paste the model's reply whatever it is (only the <transcript> tags or wrapping quotes stripped, and
         the raw text when the reply is empty), to measure what the guard is worth.
strict   the standard guard, plus a content check: fall back when the reply drops 4 or more content words
         of the raw text (words outside a list of function words and fillers), or adds 4 or more content
         words that are not in the raw text. Meant to catch dropped clauses and answered prompts that keep
         every number and name.
Judging the new folder is cheap: every pasted text it shares with an earlier run is cached.
"""
import argparse
import collections
import csv
import json
import pathlib
import re
import shutil
import sys

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from guard import check, norm, strip_wrapping  # noqa: E402
from prompts import PROMPTS  # noqa: E402

FUNCTION = set("""
a an the and or but so if then than that this these those there here it its it's i i'm i'll i've i'd me my mine
we we're we'll we've us our you you're you'll your he she him her his they they're them their is are was were be
been being am do does did done have has had having will would should could can may might must shall to of in on at
by for with from up down out over into onto off about as just like um uh er ah oh okay ok yeah yes no not well right
actually really kind sort know mean guess also too very what which who whom whose when where why how all any some
s t re ve ll d m going gonna wanna got get gets go goes went let let's
""".split())


def content(text: str) -> collections.Counter:
    return collections.Counter(w for w in norm(text).split() if w not in FUNCTION and not w.isdigit())


def strict(raw: str, reply: str, instruction: str) -> tuple[str, str]:
    final, reason = check(raw, reply, instruction)
    if reason:
        return final, reason
    dropped = sum((content(raw) - content(final)).values())
    added = sum((content(final) - content(raw)).values())
    if dropped >= 4:
        return raw, "dropped_words"
    if added >= 4:
        return raw, "added_words"
    return final, ""


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("run", type=pathlib.Path)
    ap.add_argument("--mode", choices=["noguard", "strict"], required=True)
    a = ap.parse_args()
    meta = json.loads((a.run / "meta.json").read_text())
    system = PROMPTS[meta["prompt"]][0]
    out = a.run.parent / f"{a.run.name}__{a.mode}"
    out.mkdir(exist_ok=True)
    rows = list(csv.DictReader((a.run / "results.csv").open(newline="", encoding="utf-8")))
    fb = collections.Counter()
    for r in rows:
        raw, reply = r["raw"], r["reply"]
        if not raw.strip():
            continue
        if a.mode == "noguard":
            t = strip_wrapping(reply)
            r["transcript"], r["fallback"] = (t, "") if norm(t) else (raw, "empty")
        else:
            r["transcript"], r["fallback"] = strict(raw, reply, system)
        if r["fallback"]:
            fb[r["fallback"]] += 1
    with (out / "results.csv").open("w", newline="", encoding="utf-8") as fh:
        w = csv.DictWriter(fh, list(rows[0]))
        w.writeheader()
        w.writerows(rows)
    meta.update(guard=a.mode, fallbacks=dict(fb), fallback_total=sum(fb.values()),
                unchanged=sum(1 for r in rows if r["transcript"] == r["raw"] and not r["fallback"]))
    (out / "meta.json").write_text(json.dumps(meta, indent=1))
    for f in ("server.log",):
        if (a.run / f).exists():
            shutil.copy(a.run / f, out / f)
    print(out, dict(fb))


if __name__ == "__main__":
    main()
