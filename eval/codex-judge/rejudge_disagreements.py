# /// script
# requires-python = ">=3.11"
# dependencies = ["amend-va==0.2.1", "jiwer"]
# ///
"""Re-ask Codex the drafts on which the two judges disagree, plus 20 random agreed drafts as a control.

Measures whether a Codex/Claude disagreement is stable (the same Codex verdict again) or judge noise.
    uv run eval/codex-judge/rejudge_disagreements.py eval/voicearena eval/codex-judge/rejudge.json
"""
import csv, json, os, random, sys, tempfile, pathlib, concurrent.futures as cf
VA = pathlib.Path(sys.argv[1]); sys.path.insert(0, str(VA))
import score_codex_cli as sc
from amend.metric import char_edits
from amend.prompts import build_prompt
m = {r["clip_id"]: r for r in csv.DictReader(open(VA / "clips/manifest.csv"))}
dis = {(r["engine"], r["clip_id"]) for r in csv.DictReader(open(VA.parent / "codex-judge/disagreements.csv"))}
rows = []
for e in ["vocet_assemblyai", "vocet_groq", "vocet_parakeet", "vocet_qwen3"]:
    cl = {r["clip_id"]: r for r in csv.DictReader(open(VA / f"results/{e}/scored.csv"))}
    for r in csv.DictReader(open(VA / f"results/{e}/scored_codex.csv")):
        rows.append({"engine": e, "clip_id": r["clip_id"], "draft": r["draft"], "e_codex": int(r["edits"]),
                     "e_claude": int(cl[r["clip_id"]]["edits"]), "dis": (e, r["clip_id"]) in dis})
rnd = random.Random(11)
pick = [r for r in rows if r["dis"]] + rnd.sample([r for r in rows if not r["dis"]], 20)
with tempfile.TemporaryDirectory() as wd:
    schema, system = os.path.join(wd, "s.json"), os.path.join(wd, "sys.txt")
    pathlib.Path(schema).write_text(json.dumps(sc.SCHEMA)); pathlib.Path(system).write_text(sc.SYSTEM)
    def job(r):
        mm = m[r["clip_id"]]
        p = build_prompt(mm["intended_text"], r["draft"], mm["spoken_text"], "v1")
        try:
            a = sc.ask_codex(p, mm["intended_text"], wd, schema, system)
        except sc.UsageLimit as ex:
            print("USAGE LIMIT", ex); return None
        e2, _ = char_edits(r["draft"], a["fixed"])
        return {**r, "e_codex2": e2}
    out = [x for x in cf.ThreadPoolExecutor(8).map(job, pick) if x]
json.dump([{k: v for k, v in x.items() if k != "draft"} for x in out], open(sys.argv[2], "w"), indent=1)
for grp in (True, False):
    g = [x for x in out if x["dis"] == grp]
    same_v = sum((x["e_codex"] == 0) == (x["e_codex2"] == 0) for x in g)
    same_e = sum(x["e_codex"] == x["e_codex2"] for x in g)
    print(f"{'disagreed' if grp else 'agreed (random)'}: n={len(g)} codex rerun same verdict {same_v}, same edits {same_e}")
for x in out:
    if x["dis"]: print(x["engine"], x["clip_id"], "claude", x["e_claude"], "codex", x["e_codex"], "codex rerun", x["e_codex2"])
