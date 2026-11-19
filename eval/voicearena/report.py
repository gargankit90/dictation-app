# /// script
# requires-python = ">=3.11"
# dependencies = ["numpy"]
# ///
"""Summarise scored.csv files from score_claude_cli.py into the tables REPORT.md uses.

    uv run report.py results/vocet_groq [results/vocet_assemblyai ...] --manifest clips/manifest.csv

Writes <dir>/summary.json and <dir>/summary.md for each engine, plus a side-by-side table on stdout
when more than one engine is given. ZER = share of drafts with zero edits; AMEND = pooled character
edits / pooled reference characters (amend-va's definitions). 95% intervals: speaker-level bootstrap
(2,000 resamples), as in the whitepaper.
"""
import argparse
import csv
import json
import pathlib

import numpy as np

DOMAINS = ["personal_messaging", "work_chat", "email", "ai_prompt", "code", "docs_notes", "long_form"]
# The dataset has no length-band or difficulty column, so length bands are made here from the spoken
# word count (whitepaper: short 3-15 words, standard 15-80, long-form 60 s or more).
BANDS = [("short (<=15 words)", 0, 15), ("standard (16-80 words)", 16, 80), ("long (>80 words)", 81, 10**9)]


def zer_amend(rows):
    if not rows:
        return None, None
    e = sum(int(r["edits"]) for r in rows)
    c = sum(int(r["chars"]) for r in rows)
    return 100 * sum(int(r["zero_edit"]) for r in rows) / len(rows), 100 * e / c if c else 0.0


def bootstrap(rows, n=2000, seed=7):
    rng = np.random.default_rng(seed)
    by = {}
    for r in rows:
        by.setdefault(r["speaker_id"], []).append(r)
    spk = list(by)
    z, a = [], []
    for _ in range(n):
        pick = [x for s in rng.choice(spk, len(spk)) for x in by[s]]
        zz, aa = zer_amend(pick)
        z.append(zz)
        a.append(aa)
    return [float(np.percentile(z, 2.5)), float(np.percentile(z, 97.5))], \
        [float(np.percentile(a, 2.5)), float(np.percentile(a, 97.5))]


def summarise(d: pathlib.Path, manifest: dict) -> dict:
    rows = list(csv.DictReader((d / "scored.csv").open(newline="", encoding="utf-8")))
    rows = [r for r in rows if r.get("edits") not in ("", None)]
    for r in rows:
        m = manifest[r["clip_id"]]
        r["spoken_text"], r["intended_text"] = m["spoken_text"], m["intended_text"]
        r["words"] = len(m["spoken_text"].split())
    z, a = zer_amend(rows)
    zer_ci, amend_ci = bootstrap(rows)
    lat = [float(r["finalize_s"]) for r in rows if r.get("finalize_s")]
    out = {
        "engine": rows[0]["engine"], "n": len(rows), "zer": z, "zer_ci95": zer_ci, "amend": a, "amend_ci95": amend_ci,
        "judge": f'{rows[0]["judge_model"]} effort={rows[0]["judge_effort"]} via {rows[0]["judge_via"]}',
        "prompt": f'amend-va {rows[0]["amend_va"]} prompt {rows[0]["prompt_version"]} '
                  f'fingerprint {rows[0]["prompt_fingerprint"]}',
        "label": rows[0]["label"],
        "empty_drafts": sum(1 for r in rows if not r["draft"].strip()),
        "latency_s": {"n": len(lat), "median": float(np.median(lat)), "p90": float(np.percentile(lat, 90)),
                      "mean": float(np.mean(lat)), "max": float(np.max(lat))} if lat else None,
        "by_domain": {}, "by_length": {},
        "worst": [],
    }
    for dom in DOMAINS:
        sub = [r for r in rows if r["domain"] == dom]
        zz, aa = zer_amend(sub)
        out["by_domain"][dom] = {"n": len(sub), "zer": zz, "amend": aa}
    for name, lo, hi in BANDS:
        sub = [r for r in rows if lo <= r["words"] <= hi]
        zz, aa = zer_amend(sub)
        out["by_length"][name] = {"n": len(sub), "zer": zz, "amend": aa}
    for r in sorted(rows, key=lambda r: (-int(r["edits"]), r["clip_id"]))[:10]:
        out["worst"].append({k: r[k] for k in ("clip_id", "domain", "edits", "chars", "draft", "fixed",
                                                "intended_text", "spoken_text")})
    (d / "summary.json").write_text(json.dumps(out, indent=1, ensure_ascii=False))
    return out


def fmt(x, digits=1):
    return "-" if x is None else f"{x:.{digits}f}"


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("dirs", nargs="+", type=pathlib.Path)
    ap.add_argument("--manifest", type=pathlib.Path,
                    default=pathlib.Path(__file__).parent / "clips" / "manifest.csv")
    a = ap.parse_args()
    manifest = {r["clip_id"]: r for r in csv.DictReader(a.manifest.open(newline="", encoding="utf-8"))}
    sums = [summarise(d, manifest) for d in a.dirs]
    for s in sums:
        L = s["latency_s"] or {}
        print(f"## {s['engine']}  (n={s['n']}, {s['label']})")
        print(f"ZER {fmt(s['zer'])}% [{fmt(s['zer_ci95'][0])}-{fmt(s['zer_ci95'][1])}]  "
              f"AMEND {fmt(s['amend'], 2)}% [{fmt(s['amend_ci95'][0], 2)}-{fmt(s['amend_ci95'][1], 2)}]  "
              f"empty drafts {s['empty_drafts']}  latency median {fmt(L.get('median'), 2)} s "
              f"p90 {fmt(L.get('p90'), 2)} s")
        print(f"judge: {s['judge']}; {s['prompt']}")
        print("| domain | n | ZER % | AMEND % |\n|---|---:|---:|---:|")
        for k, v in s["by_domain"].items():
            print(f"| {k} | {v['n']} | {fmt(v['zer'])} | {fmt(v['amend'], 2)} |")
        print("| length | n | ZER % | AMEND % |\n|---|---:|---:|---:|")
        for k, v in s["by_length"].items():
            print(f"| {k} | {v['n']} | {fmt(v['zer'])} | {fmt(v['amend'], 2)} |")
        print()
    if len(sums) > 1:
        print("| | " + " | ".join(s["engine"] for s in sums) + " |")
        print("|---|" + "---:|" * len(sums))
        print("| ZER % | " + " | ".join(fmt(s["zer"]) for s in sums) + " |")
        print("| AMEND % | " + " | ".join(fmt(s["amend"], 2) for s in sums) + " |")
        for dom in DOMAINS:
            print(f"| ZER {dom} | " + " | ".join(fmt(s["by_domain"][dom]["zer"]) for s in sums) + " |")
        print("| latency median s | " + " | ".join(fmt((s["latency_s"] or {}).get("median"), 2) for s in sums) + " |")
        print("| latency p90 s | " + " | ".join(fmt((s["latency_s"] or {}).get("p90"), 2) for s in sums) + " |")


if __name__ == "__main__":
    main()
