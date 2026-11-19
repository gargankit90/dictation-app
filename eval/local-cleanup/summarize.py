# /// script
# requires-python = ">=3.11"
# dependencies = ["numpy"]
# ///
"""Summarise judged cleanup runs: both judges, both halves, and the costs.

    uv run summarize.py runs/parakeet/raw runs/parakeet/phi4-mini__v1 ... [--md table.md] [--json out.json]

Per run: ZER and AMEND per judge (Claude Opus 5.5 = scored.csv, GPT-6 Astra = scored_codex.csv) on the tune
half, the check half and all 151 clips, with a 95% speaker-bootstrap interval on all; "both" = the share of
drafts both judges passed with zero edits; fallbacks; latency (median / p90 over clips, this Mac); load time;
memory; model size. Also writes harms.csv per run: every clip that the Claude judge passed as raw (zero
edits) but failed after cleanup, with both texts and the judge's correction.
"""
import argparse
import csv
import json
import pathlib
import sys

import numpy as np

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent / "voicearena"))
from report import bootstrap, zer_amend  # noqa: E402

SPLIT = {r["clip_id"]: r["half"] for r in csv.DictReader((HERE / "split.csv").open())}


def load(p: pathlib.Path):
    if not p.exists():
        return None
    rows = [r for r in csv.DictReader(p.open(newline="", encoding="utf-8")) if r.get("edits") not in ("", None)]
    return {r["clip_id"]: r for r in rows}


def judge_stats(rows: dict) -> dict:
    out = {}
    for half in ("tune", "check", "all"):
        sub = [r for k, r in rows.items() if half == "all" or SPLIT[k] == half]
        z, a = zer_amend(sub)
        out[half] = {"n": len(sub), "zer": z, "amend": a}
    zc, ac = bootstrap(list(rows.values()))
    out["all"]["zer_ci95"], out["all"]["amend_ci95"] = zc, ac
    return out


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("runs", nargs="+", type=pathlib.Path)
    ap.add_argument("--md", type=pathlib.Path)
    ap.add_argument("--json", type=pathlib.Path)
    a = ap.parse_args()
    table = {}
    for d in a.runs:
        meta = json.loads((d / "meta.json").read_text()) if (d / "meta.json").exists() else {}
        res = {r["clip_id"]: r for r in csv.DictReader((d / "results.csv").open(newline="", encoding="utf-8"))}
        cl, cx = load(d / "scored.csv"), load(d / "scored_codex.csv")
        s = {"meta": meta}
        if cl:
            s["claude"] = judge_stats(cl)
        if cx:
            s["codex"] = judge_stats(cx)
        if cl and cx:
            for half in ("tune", "check", "all"):
                keys = [k for k in cl if (half == "all" or SPLIT[k] == half) and k in cx]
                s.setdefault("both", {})[half] = 100 * sum(cl[k]["zero_edit"] == "1" and cx[k]["zero_edit"] == "1"
                                                           for k in keys) / len(keys)
        # harms against the raw engine (Claude judge)
        raw_dir = d.parent / "raw"
        raw = load(raw_dir / "scored.csv")
        if cl and raw and d.name != "raw":
            harms = []
            for k, r in cl.items():
                if raw[k]["zero_edit"] == "1" and r["zero_edit"] == "0":
                    harms.append({"clip_id": k, "half": SPLIT[k], "edits": r["edits"],
                                  "fallback": res[k].get("fallback", ""), "raw": raw[k]["draft"],
                                  "cleaned": r["draft"], "judge_fixed": r["fixed"]})
            with (d / "harms.csv").open("w", newline="", encoding="utf-8") as fh:
                w = csv.DictWriter(fh, ["clip_id", "half", "edits", "fallback", "raw", "cleaned", "judge_fixed"])
                w.writeheader()
                w.writerows(harms)
            s["harms"] = len(harms)
            s["fixed"] = sum(1 for k, r in cl.items() if raw[k]["zero_edit"] == "0" and r["zero_edit"] == "1")
        table[f"{d.parent.name}/{d.name}"] = s

    lines = ["| Run | Size | Load s | Peak RSS | Median / p90 s | Fallbacks | Claude ZER tune / check / all (95%) | "
             "Claude AMEND all | Codex ZER tune / check / all | Codex AMEND all | Both ZER all | Fixed / harmed |",
             "| --- | ---: | ---: | ---: | --- | ---: | --- | ---: | --- | ---: | ---: | --- |"]
    for name, s in table.items():
        m = s["meta"]
        lat = m.get("latency_s") or {}
        size = f'{m["model_bytes"] / 1e9:.2f} GB' if m.get("model_bytes") else "–"
        rss = f'{m["max_rss_bytes"] / 1e9:.1f} GB' if m.get("max_rss_bytes") else "–"
        def zs(j):
            if j not in s:
                return "–", "–"
            x = s[j]
            ci = x["all"]["zer_ci95"]
            return (f'{x["tune"]["zer"]:.1f} / {x["check"]["zer"]:.1f} / **{x["all"]["zer"]:.1f}** '
                    f'({ci[0]:.1f}–{ci[1]:.1f})', f'{x["all"]["amend"]:.2f}%')
        cz, ca = zs("claude")
        xz, xa = zs("codex")
        both = f'{s["both"]["all"]:.1f}' if "both" in s else "–"
        fb = f'{m.get("fallback_total", 0)}' if m.get("latency_s") else "–"
        lt = f'{lat["median"]:.2f} / {lat["p90"]:.2f}' if lat else "–"
        fh = f'{s["fixed"]} / {s["harms"]}' if "harms" in s else "–"
        lines.append(f'| {name} | {size} | {m.get("load_s", "–")} | {rss} | {lt} | {fb} | {cz} | {ca} | {xz} | {xa} | '
                     f'{both} | {fh} |')
    md = "\n".join(lines)
    print(md)
    if a.md:
        a.md.write_text(md + "\n")
    if a.json:
        a.json.write_text(json.dumps(table, indent=1, default=float))


if __name__ == "__main__":
    main()
