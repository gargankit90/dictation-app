"""Tables and harms for the offline cleanup runs.

    python3 eval/cleanup-offline/summarize.py [label ...]  > eval/cleanup-offline/summary.md

For each run (results/<label>/ here, or input/vm/<name>/ for the VM runs copied from the Groq cleanup
eval): ZER and AMEND from both judges (Claude Opus 5.5 scored.csv, GPT-6 Astra scored_codex.csv) on the
tune half, the check half and all 151 clips (split: eval/groq-cleanup/split.csv), with a 95% speaker
bootstrap interval for the Claude ZER on all clips; the fallback rate and reasons; the median model-call
time. Against the raw Whisper drafts (input/vm/groq_clean_v1_raw, same judges, same cache) it counts
per judge the clips the cleanup FIXED (raw had edits, cleaned has none) and BROKE (raw had none,
cleaned has edits), and writes results/<label>/harms.md: every clip either judge says it broke, with
the raw and the pasted text, for reading by hand (content dropped, meaning changed, or a judge quirk).
"""
from __future__ import annotations

import csv
import json
import pathlib
import random
import statistics
import sys
from collections import Counter

HERE = pathlib.Path(__file__).resolve().parent
SPLIT = {r["clip_id"]: r["half"] for r in csv.DictReader((HERE / ".." / "groq-cleanup" / "split.csv").open())}
RAW = HERE / "input" / "vm" / "groq_clean_v1_raw"
VM = ["groq_clean_v1_raw", "groq_clean_v1", "groq_clean_v2"]


def load(path: pathlib.Path) -> dict:
    if not path.exists():
        return {}
    return {r["clip_id"]: r for r in csv.DictReader(path.open(newline="", encoding="utf-8"))
            if r.get("edits") not in ("", None)}


def zer_amend(rows):
    if not rows:
        return None, None
    z = 100.0 * sum(int(r["edits"]) == 0 for r in rows) / len(rows)
    chars = sum(int(r["chars"]) for r in rows)
    return z, (100.0 * sum(int(r["edits"]) for r in rows) / chars if chars else None)


def bootstrap_zer(rows, n=2000, seed=7):
    by: dict[str, list] = {}
    for r in rows:
        by.setdefault(r["speaker_id"], []).append(r)
    speakers = sorted(by)
    rng = random.Random(seed)
    zs = sorted(zer_amend([r for s in rng.choices(speakers, k=len(speakers)) for r in by[s]])[0] for _ in range(n))
    return zs[int(0.025 * n)], zs[int(0.975 * n) - 1]


def run_dir(label: str) -> pathlib.Path:
    return HERE / "input" / "vm" / label if label in VM else HERE / "results" / label


def details(d: pathlib.Path) -> dict:
    """Per clip: raw, pasted, reason, model_ms (results.csv here; groq_replies.csv for the VM run)."""
    if (d / "results.csv").exists():
        return {r["clip_id"]: {"raw": r["raw_text"], "pasted": r["transcript"], "reason": r["reason"],
                               "model_ms": r["model_ms"], "reply": r["cleaned_text"]}
                for r in csv.DictReader((d / "results.csv").open(newline="", encoding="utf-8"))}
    if (d / "groq_replies.csv").exists():
        return {r["clip_id"]: {"raw": r["raw_text"], "pasted": r["pasted_text"], "reason": r["reason"],
                               "model_ms": r["cleanup_ms"], "reply": r["cleaned_text"]}
                for r in csv.DictReader((d / "groq_replies.csv").open(newline="", encoding="utf-8"))}
    return {}


def missing(label: str, det: dict) -> set:
    """Offline clips with no model reply because of the run, not the model (a rate limit, the cost cap,
    not run yet): left out of the scores. The VM run's failures stay in (they are what the app pasted)."""
    if label in VM:
        return set()
    return {cid for cid, x in det.items() if x["reason"] in ("not_run", "cost_cap") or x["reason"].startswith("http_")}


def summarise(label: str) -> dict:
    d = run_dir(label)
    det = details(d)
    gone = missing(label, det)
    judges = {j: {cid: r for cid, r in load(d / f).items() if cid not in gone}
              for j, f in (("claude", "scored.csv"), ("codex", "scored_codex.csv"))}
    raw = {"claude": load(RAW / "scored.csv"), "codex": load(RAW / "scored_codex.csv")}
    out: dict = {"label": label, "judges": {}, "missing": len(gone)}
    for j, rows in judges.items():
        if not rows:
            continue
        out["judges"][j] = {}
        for half in ("tune", "check", "all"):
            sub = [r for cid, r in rows.items() if half == "all" or SPLIT.get(cid) == half]
            z, a = zer_amend(sub)
            fixed = [cid for cid, r in rows.items() if (half == "all" or SPLIT.get(cid) == half)
                     and cid in raw[j] and int(raw[j][cid]["edits"]) > 0 and int(r["edits"]) == 0]
            broke = [cid for cid, r in rows.items() if (half == "all" or SPLIT.get(cid) == half)
                     and cid in raw[j] and int(raw[j][cid]["edits"]) == 0 and int(r["edits"]) > 0]
            out["judges"][j][half] = {"n": len(sub), "zer": z, "amend": a, "fixed": len(fixed), "broke": len(broke),
                                      "broke_ids": broke}
        out["judges"][j]["all"]["zer_ci95"] = bootstrap_zer(list(rows.values()))
    if det:
        reasons = Counter(x["reason"] for x in det.values())
        ms = [int(x["model_ms"]) for x in det.values() if str(x["model_ms"]).isdigit() and int(x["model_ms"]) > 0]
        out["reasons"] = dict(reasons)
        out["fallbacks"] = sum(v for k, v in reasons.items() if k not in ("cleaned", "not_run", "cost_cap") and not k.startswith("http_"))
        out["model_ms_median"] = statistics.median(ms) if ms else None
        out["changed"] = sum(x["pasted"] != x["raw"] for x in det.values())
    if label not in VM:
        write_harms(label, d, out, judges, raw, det)
    return out


def write_harms(label, d, out, judges, raw, det) -> None:
    ids = sorted({cid for j in out["judges"] for cid in out["judges"][j]["all"]["broke_ids"]})
    lines = [f"# {label}: clips the cleanup broke", "",
             "Raw Whisper draft judged perfect (0 edits) and the pasted text not, by the judge(s) named. "
             "Read each by hand: dropped content, changed meaning, or a judge quirk.", ""]
    for cid in ids:
        x = det.get(cid, {})
        who = [j for j in out["judges"] if cid in out["judges"][j]["all"]["broke_ids"]]
        lines += [f"## {cid} ({SPLIT.get(cid, '?')}) — broke for {', '.join(who)}; reason {x.get('reason')}", "",
                  f"- raw: {x.get('raw', '')}", f"- pasted: {x.get('pasted', '')}"]
        for j in who:
            lines.append(f"- {j} judge's fix of the pasted text: {judges[j][cid]['fixed']}")
        lines.append("")
    (d / "harms.md").write_text("\n".join(lines), encoding="utf-8")


def f(x, digits=1):
    return "-" if x is None else f"{x:.{digits}f}"


def main() -> None:
    labels = sys.argv[1:] or VM + sorted(p.name for p in (HERE / "results").iterdir()
                                          if (p / "scored.csv").exists() or (p / "scored_codex.csv").exists())
    sums = [summarise(lab) for lab in labels]
    print("| Run | n | Claude ZER tune / check / all (95% CI) | Codex ZER tune / check / all | AMEND all Claude / Codex "
          "| Fixed / broke vs raw (Claude; Codex) | Fallbacks (reasons) | Changed | Model ms median |")
    print("| --- | ---: | --- | --- | --- | --- | --- | ---: | ---: |")
    for s in sums:
        c, x = s["judges"].get("claude"), s["judges"].get("codex")

        def zz(j, ci=False):
            if not j:
                return "-"
            t = f'{f(j["tune"]["zer"])} / {f(j["check"]["zer"])} / **{f(j["all"]["zer"])}**'
            if ci:
                t += f' ({f(j["all"]["zer_ci95"][0])}–{f(j["all"]["zer_ci95"][1])})'
            return t
        fb = "-"
        if "reasons" in s:
            rs = ", ".join(f"{k} {v}" for k, v in sorted(s["reasons"].items()) if k not in ("cleaned", "not_run", "cost_cap") and not k.startswith("http_"))
            fb = f'{s["fallbacks"]}/{151 - s["missing"]}' + (f" ({rs})" if rs else "")
        fx = "; ".join(f'{j["all"]["fixed"]}/{j["all"]["broke"]}' if j else "-" for j in (c, x))
        print(f'| {s["label"]} | {151 - s["missing"]} | {zz(c, True)} | {zz(x)} | {f(c and c["all"]["amend"], 2)} / '
              f'{f(x and x["all"]["amend"], 2)} | {fx} | {fb} | {s.get("changed", "-")} | '
              f'{f(s.get("model_ms_median"), 0)} |')


if __name__ == "__main__":
    main()
