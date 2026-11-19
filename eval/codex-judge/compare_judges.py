"""Compare the Claude (Opus 5.5) and Codex (GPT-6 Astra) AMEND judges on the same drafts.

Reads results/<engine>/scored.csv (Claude) and results/<engine>/scored_codex.csv (Codex) from
eval/voicearena and prints, per engine and pooled: each judge's ZER and AMEND, the agreement measures of
the whitepaper's judge similarity study (section 9.6: same verdict, Cohen's kappa, identical edit count,
Spearman rho of per-draft edit counts), a strict ZER (both judges: no edit) and a lenient ZER (either
judge: no edit), and a speaker-bootstrap interval for the Codex - Claude ZER gap. Writes the drafts on which
the two verdicts differ to disagreements.csv (no dataset reference text).

    python3 eval/codex-judge/compare_judges.py [--json summary.json]
"""
import argparse
import csv
import json
import pathlib
import random

HERE = pathlib.Path(__file__).resolve().parent
VA = HERE.parent / "voicearena"
ENGINES = ["vocet_assemblyai", "vocet_groq", "vocet_parakeet", "vocet_qwen3"]
# Extra runs: in the per-engine tables, not in "pooled" (pooled stays the four engines' first full runs).
EXTRA = ["vocet_assemblyai_run2", "vocet_assemblyai_run2_verbatim"]
# (label, before, after): the same clips under two conditions, compared per judge.
PAIRS = [("AssemblyAI run 1 vs run 2 (repeat)", "vocet_assemblyai", "vocet_assemblyai_run2"),
         ("AssemblyAI run 2: verbatim vs cleaned", "vocet_assemblyai_run2_verbatim", "vocet_assemblyai_run2")]


def load(engine: str) -> list[dict]:
    cl = {r["clip_id"]: r for r in csv.DictReader((VA / "results" / engine / "scored.csv").open(encoding="utf-8"))}
    cx = {r["clip_id"]: r for r in csv.DictReader((VA / "results" / engine / "scored_codex.csv").open(encoding="utf-8"))}
    assert set(cl) == set(cx), engine
    out = []
    for cid in cl:
        a, b = cl[cid], cx[cid]
        assert a["draft"] == b["draft"] and a["prompt_fingerprint"] == b["prompt_fingerprint"], cid
        out.append({"engine": engine, "clip_id": cid, "speaker_id": a["speaker_id"], "domain": a["domain"],
                    "draft": a["draft"], "fixed_claude": a["fixed"], "fixed_codex": b["fixed"],
                    "e_claude": int(a["edits"]), "e_codex": int(b["edits"]), "chars": int(a["chars"])})
    return out


def rank(xs: list[float]) -> list[float]:
    order = sorted(range(len(xs)), key=lambda i: xs[i])
    r = [0.0] * len(xs)
    i = 0
    while i < len(order):
        j = i
        while j + 1 < len(order) and xs[order[j + 1]] == xs[order[i]]:
            j += 1
        for k in range(i, j + 1):
            r[order[k]] = (i + j) / 2
        i = j + 1
    return r


def spearman(x: list[float], y: list[float]) -> float:
    rx, ry = rank(x), rank(y)
    mx, my = sum(rx) / len(rx), sum(ry) / len(ry)
    num = sum((a - mx) * (b - my) for a, b in zip(rx, ry))
    den = (sum((a - mx) ** 2 for a in rx) * sum((b - my) ** 2 for b in ry)) ** 0.5
    return num / den if den else float("nan")


def stats(rows: list[dict]) -> dict:
    n = len(rows)
    za = [r["e_claude"] == 0 for r in rows]
    zb = [r["e_codex"] == 0 for r in rows]
    chars = sum(r["chars"] for r in rows)
    po = sum(a == b for a, b in zip(za, zb)) / n
    pa, pb = sum(za) / n, sum(zb) / n
    pe = pa * pb + (1 - pa) * (1 - pb)
    return {
        "n": n,
        "zer_claude": 100 * pa, "zer_codex": 100 * pb,
        "amend_claude": 100 * sum(r["e_claude"] for r in rows) / chars,
        "amend_codex": 100 * sum(r["e_codex"] for r in rows) / chars,
        "mean_edits_claude": sum(r["e_claude"] for r in rows) / n,
        "mean_edits_codex": sum(r["e_codex"] for r in rows) / n,
        "same_verdict": 100 * po,
        "kappa": (po - pe) / (1 - pe) if pe < 1 else 1.0,
        "identical_edits": 100 * sum(r["e_claude"] == r["e_codex"] for r in rows) / n,
        "identical_text": 100 * sum(r["fixed_claude"] == r["fixed_codex"] for r in rows) / n,
        "edit_rho": spearman([r["e_claude"] for r in rows], [r["e_codex"] for r in rows]),
        "zer_strict": 100 * sum(a and b for a, b in zip(za, zb)) / n,
        "zer_lenient": 100 * sum(a or b for a, b in zip(za, zb)) / n,
        "only_claude_ok": sum(a and not b for a, b in zip(za, zb)),
        "only_codex_ok": sum(b and not a for a, b in zip(za, zb)),
    }


def gap_interval(rows: list[dict], reps: int = 4000, seed: int = 7) -> tuple[float, float]:
    """95% speaker-bootstrap interval of ZER(codex) - ZER(claude), in points."""
    by = {}
    for r in rows:
        by.setdefault(r["speaker_id"], []).append((r["e_codex"] == 0) - (r["e_claude"] == 0))
    spk = list(by)
    rnd = random.Random(seed)
    diffs = []
    for _ in range(reps):
        pick = [x for s in rnd.choices(spk, k=len(spk)) for x in by[s]]
        diffs.append(100 * sum(pick) / len(pick))
    diffs.sort()
    return diffs[int(0.025 * reps)], diffs[int(0.975 * reps) - 1]


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--json", type=pathlib.Path, help="write the numbers here too")
    a = ap.parse_args()
    allrows, summary = [], {}
    print("engine            n  ZER cl  ZER cx  gap (95% CI)        AMEND cl AMEND cx  same%  kappa  ident%  "
          "rho   strict lenient")
    loaded = {}
    for e in ENGINES + ["pooled"] + EXTRA:
        rows = allrows if e == "pooled" else load(e)
        if e in ENGINES:
            allrows += rows
        if e != "pooled":
            loaded[e] = rows
        s = stats(rows)
        lo, hi = gap_interval(rows)
        s["zer_gap"], s["zer_gap_ci"] = s["zer_codex"] - s["zer_claude"], [lo, hi]
        summary[e] = s
        print(f"{e:16} {s['n']:3}  {s['zer_claude']:5.1f}   {s['zer_codex']:5.1f}  "
              f"{s['zer_gap']:+5.1f} ({lo:+5.1f}..{hi:+5.1f})  {s['amend_claude']:6.2f}   {s['amend_codex']:6.2f}  "
              f"{s['same_verdict']:5.1f}  {s['kappa']:.3f}  {s['identical_edits']:5.1f}  {s['edit_rho']:.3f}  "
              f"{s['zer_strict']:5.1f}  {s['zer_lenient']:5.1f}   (only Claude ok {s['only_claude_ok']}, "
              f"only Codex ok {s['only_codex_ok']}, identical text {s['identical_text']:.1f}%)")
    print()
    print("pair                                    judge   ZER before  ZER after  change  verdict flips "
          "(ok->edit / edit->ok)  identical drafts")
    for label, b, c in PAIRS:
        before = {r["clip_id"]: r for r in loaded[b]}
        after = {r["clip_id"]: r for r in loaded[c]}
        same_draft = sum(before[k]["draft"] == after[k]["draft"] for k in after)
        summary.setdefault("pairs", {})[label] = {"before": b, "after": c, "identical_drafts": same_draft}
        for j in ("claude", "codex"):
            zb = {k: before[k][f"e_{j}"] == 0 for k in after}
            za = {k: after[k][f"e_{j}"] == 0 for k in after}
            lost = sum(zb[k] and not za[k] for k in after)
            won = sum(za[k] and not zb[k] for k in after)
            pb, pa = 100 * sum(zb.values()) / len(after), 100 * sum(za.values()) / len(after)
            summary["pairs"][label][j] = {"zer_before": pb, "zer_after": pa, "ok_to_edit": lost, "edit_to_ok": won}
            print(f"{label:39} {j:6}  {pb:6.1f}     {pa:6.1f}   {pa - pb:+5.1f}   {lost + won:3} ({lost} / {won})"
                  f"                  {same_draft}")
    dis = [r for r in allrows if (r["e_claude"] == 0) != (r["e_codex"] == 0)]
    with (HERE / "disagreements.csv").open("w", newline="", encoding="utf-8") as fh:
        w = csv.DictWriter(fh, ["engine", "clip_id", "domain", "e_claude", "e_codex", "draft",
                                "fixed_claude", "fixed_codex"])
        w.writeheader()
        for r in sorted(dis, key=lambda r: (r["clip_id"], r["engine"])):
            w.writerow({k: r[k] for k in w.fieldnames})
    print(f"{len(dis)} verdict disagreements in the four first runs -> {HERE / 'disagreements.csv'}")
    if a.json:
        a.json.write_text(json.dumps(summary, indent=2) + "\n")


if __name__ == "__main__":
    main()
