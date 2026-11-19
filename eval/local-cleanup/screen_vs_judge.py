# /// script
# requires-python = ">=3.11"
# dependencies = ["amend-va==0.2.1", "jiwer", "scipy"]
# ///
"""How well did the cheap screen predict the judges? Spearman rank correlation, over every judged Parakeet run
(standard guard), between each screen measure and the judged ZER (Claude, Codex)."""
import csv
import json
import pathlib
import sys

from scipy.stats import spearmanr

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import screen  # noqa: E402

split = {r["clip_id"]: r["half"] for r in csv.DictReader((HERE / "split.csv").open())}
ref = {r["clip_id"]: r["intended_text"] for r in csv.DictReader((screen.VA / "clips/manifest.csv").open())}
scored = {r["clip_id"]: r for r in csv.DictReader((HERE / "runs/parakeet/raw/scored.csv").open())}
fix = {k: v["fixed"] for k, v in scored.items()}
rawzero = {k: v["zero_edit"] == "1" for k, v in scored.items()}
rawtext = {k: v["draft"] for k, v in scored.items()}
rows = []
for d in sorted((HERE / "runs/parakeet").iterdir()):
    if not (d / "scored.csv").exists() or not (d / "scored_codex.csv").exists() or "__noguard" in d.name or "__strict" in d.name:
        continue
    res = list(csv.DictReader((d / "results.csv").open(newline="", encoding="utf-8")))
    for r in res:
        r["half"] = split[r["clip_id"]]
    s = screen.stats(res, ref, fix, rawzero, rawtext)["all"]
    z = lambda p: 100 * sum(r["zero_edit"] == "1" for r in csv.DictReader(p.open())) / 151  # noqa: E731
    rows.append({"run": d.name, **{k: round(v, 2) for k, v in s.items() if k != "n"},
                 "claude_zer": round(z(d / "scored.csv"), 1), "codex_zer": round(z(d / "scored_codex.csv"), 1)})
print(f"{'run':28} {'mref':>5} {'mfix':>5} {'cer':>5} {'wer':>5} {'brk':>3} {'claude':>6} {'codex':>6}")
for r in sorted(rows, key=lambda r: -r["claude_zer"]):
    print(f"{r['run']:28} {r['match_ref']:5.1f} {r['match_fix']:5.1f} {r['cer']:5.2f} {r['wer']:5.2f} {r['broke']:>3} "
          f"{r['claude_zer']:6.1f} {r['codex_zer']:6.1f}")
out = {"n_runs": len(rows), "rows": rows, "spearman": {}}
for m, sign in (("match_ref", 1), ("match_fix", 1), ("cer", -1), ("wer", -1), ("broke", -1)):
    for j in ("claude_zer", "codex_zer"):
        rho, p = spearmanr([sign * r[m] for r in rows], [r[j] for r in rows])
        out["spearman"][f"{m} vs {j}"] = {"rho": round(float(rho), 2), "p": round(float(p), 4)}
rho, p = spearmanr([r["claude_zer"] for r in rows], [r["codex_zer"] for r in rows])
out["spearman"]["claude_zer vs codex_zer"] = {"rho": round(float(rho), 2), "p": round(float(p), 4)}
for k, v in out["spearman"].items():
    print(f"  {k:28} rho {v['rho']:+.2f}  p {v['p']}")
(HERE / "runs/parakeet/screen_vs_judge.json").write_text(json.dumps(out, indent=1))
