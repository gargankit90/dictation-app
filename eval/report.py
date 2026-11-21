#!/usr/bin/env python3
"""One summary for eval runs: ZER, AMEND, the check half, paired wins/losses, cost per 1,000, latency.

    python3 eval/report.py eval/work/cleanup/glm_v4 eval/work/cleanup/other \\
        --baseline eval/work/asr/groq_raw [--judge codex] [--format json]

A RUN is a folder with the judge's scored CSV (scored.csv = Claude, scored_codex.csv, scored_gemini.csv;
pick with --judge) and, if you have them, the engine's own files for cost and latency:
  results.csv or harness_results.csv   cost_usd (speech), finalize_s (key-up to paste, seconds), model_ms
  cleanup.jsonl                        usage.cost of every cleanup reply (OpenRouter)
ZER = share of drafts with no edit, AMEND = pooled edits / pooled characters (both per amend-va), all
as percentages. Halves come from the fixed speaker split (eval/split.csv): tune is for editing prompts,
check for confirming. Wins/losses are per clip against --baseline: a win is a clip the baseline got wrong
and this run got right. Cost per 1,000 = (speech + cleanup cost) / clips * 1000, only if the run has
the cost files; latency p50/p95 over finalize_s, only if present. Stdlib only, no network.
"""
from __future__ import annotations

import argparse
import csv
import json
import math
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from evalcommon import load_split  # noqa: E402

SCORED = {"claude": "scored.csv", "codex": "scored_codex.csv", "gemini": "scored_gemini.csv"}


def read_csv(path: pathlib.Path) -> list[dict]:
    with path.open(newline="", encoding="utf-8") as fh:
        return list(csv.DictReader(fh))


def percentile(values: list[float], p: float) -> float | None:
    """Nearest-rank percentile (p in 0..100); None for no values."""
    if not values:
        return None
    s = sorted(values)
    return s[max(0, math.ceil(p / 100 * len(s)) - 1)]


def zer_amend(rows: list[dict]) -> tuple[float, float]:
    """(ZER %, AMEND %) over judged rows (those with an edits value)."""
    judged = [r for r in rows if r.get("edits", "") != ""]
    if not judged:
        return float("nan"), float("nan")
    zero = sum(1 for r in judged if int(r["edits"]) == 0)
    chars = sum(int(r["chars"]) for r in judged)
    edits = sum(int(r["edits"]) for r in judged)
    return 100 * zero / len(judged), 100 * edits / chars if chars else float("nan")


def paired(rows: dict[str, dict], base: dict[str, dict]) -> tuple[int, int, int]:
    """(wins, losses, ties) on the clips both runs judged: win = baseline wrong (edits > 0), run right."""
    w = l = t = 0
    for cid, r in rows.items():
        b = base.get(cid)
        if b is None or r.get("edits", "") == "" or b.get("edits", "") == "":
            continue
        rz, bz = int(r["edits"]) == 0, int(b["edits"]) == 0
        if rz and not bz:
            w += 1
        elif bz and not rz:
            l += 1
        else:
            t += 1
    return w, l, t


def _floats(rows: list[dict], key: str) -> list[float]:
    out = []
    for r in rows:
        try:
            if r.get(key, "") != "":
                out.append(float(r[key]))
        except ValueError:
            pass
    return out


def cost_latency(run: pathlib.Path, n: int) -> dict:
    """cost_per_1000_usd, latency_p50_s, latency_p95_s from the run's engine files, None where absent."""
    out = {"cost_per_1000_usd": None, "latency_p50_s": None, "latency_p95_s": None}
    engine = next((run / f for f in ("results.csv", "harness_results.csv") if (run / f).exists()), None)
    rows = read_csv(engine) if engine else []
    speech = _floats(rows, "cost_usd")
    cleanup = []
    jl = run / "cleanup.jsonl"
    if jl.exists():
        for line in jl.read_text(encoding="utf-8").splitlines():
            if line.strip():
                c = (json.loads(line).get("usage") or {}).get("cost")
                if isinstance(c, (int, float)):
                    cleanup.append(float(c))
    if (speech or cleanup) and n:
        out["cost_per_1000_usd"] = round(1000 * (sum(speech) + sum(cleanup)) / n, 3)
    fin = _floats(rows, "finalize_s")
    if fin:
        out["latency_p50_s"] = round(percentile(fin, 50), 3)
        out["latency_p95_s"] = round(percentile(fin, 95), 3)
    return out


def summarize(run: pathlib.Path, judge: str, split: dict[str, str], base: dict[str, dict] | None) -> dict:
    scored = read_csv(run / SCORED[judge])
    s = {"run": run.name, "judge": judge, "clips": len(scored)}
    s["zer"], s["amend"] = zer_amend(scored)
    for half in ("tune", "check"):
        s[f"zer_{half}"], s[f"amend_{half}"] = zer_amend([r for r in scored if split.get(r["clip_id"]) == half])
    if base is not None:
        s["wins"], s["losses"], s["ties"] = paired({r["clip_id"]: r for r in scored}, base)
    s.update(cost_latency(run, len(scored)))
    return s


def fmt(v, digits=1, none="-") -> str:
    return none if v is None or (isinstance(v, float) and math.isnan(v)) else f"{v:.{digits}f}"


def table(rows: list[dict]) -> str:
    paired_col = any("wins" in r for r in rows)
    head = ["run", "clips", "ZER", "AMEND", "ZER tune", "ZER check"] + (["W/L/T"] if paired_col else []) + \
           ["$/1000", "p50 s", "p95 s"]
    lines = ["| " + " | ".join(head) + " |", "|" + "|".join(["---"] * len(head)) + "|"]
    for r in rows:
        cells = [r["run"], str(r["clips"]), fmt(r["zer"]), fmt(r["amend"], 2), fmt(r["zer_tune"]), fmt(r["zer_check"])]
        if paired_col:
            cells.append(f'{r["wins"]}/{r["losses"]}/{r["ties"]}' if "wins" in r else "-")
        cells += [fmt(r["cost_per_1000_usd"], 2), fmt(r["latency_p50_s"], 2), fmt(r["latency_p95_s"], 2)]
        lines.append("| " + " | ".join(cells) + " |")
    return "\n".join(lines)


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("runs", nargs="+", type=pathlib.Path)
    ap.add_argument("--baseline", type=pathlib.Path, help="a run folder to pair every run against")
    ap.add_argument("--judge", choices=sorted(SCORED), default="claude")
    ap.add_argument("--format", choices=["md", "json", "csv"], default="md")
    a = ap.parse_args()
    split = load_split()
    base = {r["clip_id"]: r for r in read_csv(a.baseline / SCORED[a.judge])} if a.baseline else None
    rows = [summarize(r, a.judge, split, base) for r in a.runs]
    if a.format == "csv":
        w = csv.DictWriter(sys.stdout, list(rows[0]), lineterminator="\n")
        w.writeheader()
        w.writerows({k: ("" if v is None else v) for k, v in r.items()} for r in rows)
    else:
        print(json.dumps(rows, indent=2) if a.format == "json" else table(rows))


if __name__ == "__main__":
    main()
