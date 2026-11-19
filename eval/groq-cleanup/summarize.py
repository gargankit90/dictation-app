"""Tune / check / all table for the Groq cleanup runs.

    python3 -I eval/groq-cleanup/summarize.py eval/voicearena/results/<engine> [...] > out.md

For each results folder (scored.csv from bench.sh score; groq_replies.csv from join_dump.py if
present): ZER and AMEND on the tune half, the check half and all 151 clips, each with a 95%
speaker-bootstrap interval (2000 resamples of speakers within the half, seed 7, as report.py does
for the whole set); the key-up-to-text latency (finalize_s); the cleanup's own latency (cleanup_ms);
and the fallback rate with its reasons. Also writes halves.json into each folder.
"""
import csv
import json
import pathlib
import random
import statistics
import sys
from collections import Counter

HERE = pathlib.Path(__file__).resolve().parent
SPLIT = {r["clip_id"]: r["half"] for r in csv.DictReader((HERE / "split.csv").open(newline=""))}


def zer_amend(rows):
    if not rows:
        return None, None
    z = 100.0 * sum(int(r["edits"]) == 0 for r in rows) / len(rows)
    chars = sum(int(r["chars"]) for r in rows)
    a = 100.0 * sum(int(r["edits"]) for r in rows) / chars if chars else None
    return z, a


def bootstrap(rows, n=2000, seed=7):
    by = {}
    for r in rows:
        by.setdefault(r["speaker_id"], []).append(r)
    speakers = sorted(by)
    rng = random.Random(seed)
    zs, as_ = [], []
    for _ in range(n):
        sample = [r for s in rng.choices(speakers, k=len(speakers)) for r in by[s]]
        z, a = zer_amend(sample)
        zs.append(z)
        as_.append(a)
    zs.sort()
    as_.sort()
    lo, hi = int(0.025 * n), int(0.975 * n) - 1
    return (zs[lo], zs[hi]), (as_[lo], as_[hi])


def pct(values, q):
    if not values:
        return None
    values = sorted(values)
    k = (len(values) - 1) * q
    f = int(k)
    c = min(f + 1, len(values) - 1)
    return values[f] + (values[c] - values[f]) * (k - f)


def summarise(d: pathlib.Path) -> dict:
    rows = [r for r in csv.DictReader((d / "scored.csv").open(newline="", encoding="utf-8"))
            if r.get("edits") not in ("", None)]
    replies = {}
    if (d / "groq_replies.csv").exists():
        replies = {r["clip_id"]: r for r in csv.DictReader((d / "groq_replies.csv").open(newline="", encoding="utf-8"))}
    out = {"engine": d.name, "halves": {}}
    for half in ("tune", "check", "all"):
        sub = [r for r in rows if half == "all" or SPLIT.get(r["clip_id"]) == half]
        z, a = zer_amend(sub)
        zer_ci, amend_ci = bootstrap(sub)
        # A _raw folder re-scores the uncleaned drafts of a cleanup run: its timings include the cleanup.
        lat = [] if d.name.endswith("_raw") else [float(r["finalize_s"]) for r in sub if r.get("finalize_s")]
        rep = [replies[r["clip_id"]] for r in sub if r["clip_id"] in replies and replies[r["clip_id"]].get("reason")]
        cms = [int(x["cleanup_ms"]) for x in rep if x.get("cleanup_ms") not in ("", None)]
        reasons = Counter(x["reason"] for x in rep)
        out["halves"][half] = {
            "n": len(sub), "zer": z, "zer_ci95": zer_ci, "amend": a, "amend_ci95": amend_ci,
            "latency_median_s": pct(lat, 0.5), "latency_p90_s": pct(lat, 0.9),
            "cleanup_median_ms": pct(cms, 0.5), "cleanup_p90_ms": pct(cms, 0.9),
            "cleaned": reasons.get("cleaned", 0), "fallbacks": sum(v for k, v in reasons.items() if k != "cleaned"),
            "reasons": dict(reasons),
        }
    codex_path = d / "scored_codex.csv"
    if codex_path.exists():
        codex = {r["clip_id"]: r for r in csv.DictReader(codex_path.open(newline="", encoding="utf-8"))
                 if r.get("edits") not in ("", None)}
        claude = {r["clip_id"]: r for r in rows}
        out["codex"] = {}
        for half in ("tune", "check", "all"):
            sub = [r for cid, r in codex.items() if half == "all" or SPLIT.get(cid) == half]
            z, a = zer_amend(sub)
            both = [cid for cid in codex if cid in claude and (half == "all" or SPLIT.get(cid) == half)]
            agree = [cid for cid in both if int(codex[cid]["edits"]) == 0 and int(claude[cid]["edits"]) == 0]
            out["codex"][half] = {"n": len(sub), "zer": z, "amend": a, "n_both": len(both),
                                  "both_zer": 100.0 * len(agree) / len(both) if both else None}
    (d / "halves.json").write_text(json.dumps(out, indent=1))
    return out


def f(x, digits=1):
    return "-" if x is None else f"{x:.{digits}f}"


def main() -> None:
    summaries = [summarise(pathlib.Path(arg)) for arg in sys.argv[1:]]
    print("| Run | Tune ZER % | Check ZER % | All ZER % (95% CI) | Tune AMEND % | Check AMEND % | All AMEND % "
          "| Latency median / p90 s | Cleanup median / p90 ms | Fallbacks (reasons) |")
    print("| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |")
    for s in summaries:
        t, c, a = (s["halves"][h] for h in ("tune", "check", "all"))
        reasons = ", ".join(f"{k} {v}" for k, v in sorted(a["reasons"].items()) if k != "cleaned")
        fb = "-" if not a["reasons"] else f'{a["fallbacks"]}/{a["fallbacks"] + a["cleaned"]}' + (
            f" ({reasons})" if reasons else "")
        print(f'| {s["engine"]} | {f(t["zer"])} | {f(c["zer"])} | {f(a["zer"])} ({f(a["zer_ci95"][0])}–'
              f'{f(a["zer_ci95"][1])}) | {f(t["amend"], 2)} | {f(c["amend"], 2)} | {f(a["amend"], 2)} | '
              f'{f(a["latency_median_s"], 2)} / {f(a["latency_p90_s"], 2)} | '
              f'{f(a["cleanup_median_ms"], 0)} / {f(a["cleanup_p90_ms"], 0)} | {fb} |')
    judges_table(summaries)




def judges_table(summaries) -> None:
    print()
    print("| Run | Claude ZER tune / check / all | Codex ZER tune / check / all | Both agree ZER tune / check / all "
          "| Claude AMEND all | Codex AMEND all |")
    print("| --- | ---: | ---: | ---: | ---: | ---: |")
    for s in summaries:
        if "codex" not in s:
            continue
        h, c = s["halves"], s["codex"]
        print(f'| {s["engine"]} | {f(h["tune"]["zer"])} / {f(h["check"]["zer"])} / {f(h["all"]["zer"])} | '
              f'{f(c["tune"]["zer"])} / {f(c["check"]["zer"])} / {f(c["all"]["zer"])} (n {c["all"]["n"]}) | '
              f'{f(c["tune"]["both_zer"])} / {f(c["check"]["both_zer"])} / {f(c["all"]["both_zer"])} | '
              f'{f(h["all"]["amend"], 2)} | {f(c["all"]["amend"], 2)} |')


if __name__ == "__main__":
    main()
