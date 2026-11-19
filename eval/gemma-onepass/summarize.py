# /// script
# requires-python = ">=3.11"
# dependencies = ["numpy", "jiwer>=3.0", "whisper-normalizer==1.0.1"]
# ///
"""Tables for the Gemma one-pass eval: both judges, both halves, WER, the 30 s split clips, paired comparisons.

    uv run summarize.py [--md tables.md] [--json summary.json]

Runs (label -> folder): the two baselines copied from the local-cleanup eval (baselines/), then every
runs/<name>__<mode>[__guard] folder that has both judges' scores.

Per run: ZER and AMEND per judge (Claude Opus 5.5 = scored.csv, GPT-6 Astra = scored_codex.csv) on the tune
half, the check half and all 151 clips (95% speaker-bootstrap interval on all); the same on the 13 clips over
30 s and the 138 clips up to 30 s; WER against the spoken and the intended text (Whisper English normaliser,
minus its "oh" -> 0 artefact, pooled, as aai_split.py); and per judge, paired clip by clip against Parakeet
raw and against Parakeet + Gemma 4 E2B text cleanup (v3, guarded): wins (this run needs no edit, the baseline
does), losses (the reverse), ties.
"""
import argparse
import csv
import json
import pathlib
import re
import sys

import jiwer
from whisper_normalizer.english import EnglishTextNormalizer

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent / "voicearena"))
from report import bootstrap, zer_amend  # noqa: E402

MANIFEST = {r["clip_id"]: r for r in csv.DictReader((HERE.parent / "voicearena/clips/manifest.csv").open(encoding="utf-8"))}
SPLIT = {r["clip_id"]: r["half"] for r in csv.DictReader((HERE / "split.csv").open())}
LONG = {k for k, m in MANIFEST.items() if float(m["audio_duration_s"]) > 30}
NORM = EnglishTextNormalizer()
OH = re.compile(r"\boh\b", re.I)

RUNS = {
    "Parakeet raw": HERE / "baselines/parakeet_raw",
    "Parakeet + Gemma 4 E2B text cleanup (v3, guard)": HERE / "baselines/parakeet_gemma4-e2b__v3",
    "Parakeet + Gemma 4 E4B text cleanup (v3, guard)": HERE / "baselines/parakeet_gemma4-e4b__v3",
}
NICE = {"plain": "plain transcription", "onepass": "one pass, no guard", "onepass__guard": "one pass + guard"}
for d in sorted((HERE / "runs").glob("*__*")):
    if d.name.startswith("gemma4-") and (d / "results.csv").exists():
        name, mode = d.name.split("__", 1)
        RUNS[f"{name.replace('gemma4-', 'Gemma 4 ').upper().replace('GEMMA 4', 'Gemma 4')} {NICE.get(mode, mode)}"] = d
BASE_RAW, BASE_CLEAN = "Parakeet raw", "Parakeet + Gemma 4 E2B text cleanup (v3, guard)"


def load(p: pathlib.Path):
    if not p.exists():
        return None
    rows = [r for r in csv.DictReader(p.open(newline="", encoding="utf-8")) if r.get("edits") not in ("", None)]
    return {r["clip_id"]: r for r in rows}


def norm(t: str) -> str:
    return NORM(OH.sub(" ", t))


def wer(texts: dict, ref: str, keys=None) -> float:
    ids = sorted(keys or texts)
    refs = [norm(MANIFEST[c][ref]) for c in ids]
    hyp = [norm(texts[c]) or "<empty>" for c in ids]
    return 100 * jiwer.wer(refs, hyp)


def stats(rows: dict) -> dict:
    out = {}
    for sub, keep in (("tune", lambda k: SPLIT[k] == "tune"), ("check", lambda k: SPLIT[k] == "check"),
                      ("all", lambda k: True), ("over30", lambda k: k in LONG), ("upto30", lambda k: k not in LONG)):
        z, a = zer_amend([r for k, r in rows.items() if keep(k)])
        out[sub] = {"zer": z, "amend": a}
    out["all"]["zer_ci95"], out["all"]["amend_ci95"] = bootstrap(list(rows.values()))
    return out


def paired(a: dict, b: dict) -> dict:
    w = sum(1 for k in a if a[k]["zero_edit"] == "1" and b[k]["zero_edit"] == "0")
    l = sum(1 for k in a if a[k]["zero_edit"] == "0" and b[k]["zero_edit"] == "1")
    fewer = sum(1 for k in a if int(a[k]["edits"]) < int(b[k]["edits"]))
    more = sum(1 for k in a if int(a[k]["edits"]) > int(b[k]["edits"]))
    return {"wins": w, "losses": l, "ties": len(a) - w - l, "fewer_edits": fewer, "more_edits": more}


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--md", type=pathlib.Path)
    ap.add_argument("--json", type=pathlib.Path)
    a = ap.parse_args()
    data, scored = {}, {}
    for label, d in RUNS.items():
        cl, cx = load(d / "scored.csv"), load(d / "scored_codex.csv")
        res = {r["clip_id"]: r for r in csv.DictReader((d / "results.csv").open(newline="", encoding="utf-8"))}
        texts = {k: (r["transcript"] if r["status"] == "ok" else "") for k, r in res.items()}
        meta = json.loads((d / "meta.json").read_text()) if (d / "meta.json").exists() else {}
        s = {"dir": str(d.relative_to(HERE)), "meta": meta,
             "wer_spoken": wer(texts, "spoken_text"), "wer_intended": wer(texts, "intended_text"),
             "wer_spoken_over30": wer(texts, "spoken_text", LONG & set(texts))}
        s["halves"] = {half: wer(texts, "intended_text", {k for k in texts if SPLIT[k] == half}) for half in ("tune", "check")}
        scored[label] = {}
        for j, rows in (("claude", cl), ("codex", cx)):
            if rows and len(rows) == len(MANIFEST):
                s[j] = stats(rows)
                scored[label][j] = rows
        data[label] = s
    for label in data:
        for j in ("claude", "codex"):
            if j not in scored[label]:
                continue
            for base in (BASE_RAW, BASE_CLEAN):
                if base != label and j in scored[base]:
                    comparison = paired(scored[label][j], scored[base][j])
                    for half in ("tune", "check"):
                        subset = {k: r for k, r in scored[label][j].items() if SPLIT[k] == half}
                        comparison[half] = paired(subset, scored[base][j])
                    data[label].setdefault("paired", {}).setdefault(j, {})[base] = comparison
        if "claude" in scored[label] and "codex" in scored[label]:
            c, x = scored[label]["claude"], scored[label]["codex"]
            data[label]["both_zer"] = 100 * sum(c[k]["zero_edit"] == "1" and x[k]["zero_edit"] == "1" for k in c) / len(c)

    def f(v, d=1):
        return "–" if v is None else f"{v:.{d}f}"

    L = ["### ZER (drafts needing no edit, %) and AMEND (% of characters to fix)", "",
         "| Run | Claude ZER tune / check / **all** (95%) | Codex ZER tune / check / **all** (95%) | Claude AMEND | "
         "Codex AMEND | WER vs spoken | WER vs intended |",
         "| --- | --- | --- | ---: | ---: | ---: | ---: |"]
    for label, s in data.items():
        cells = []
        for j in ("claude", "codex"):
            if j in s:
                x = s[j]
                ci = x["all"]["zer_ci95"]
                cells.append(f'{f(x["tune"]["zer"])} / {f(x["check"]["zer"])} / **{f(x["all"]["zer"])}** '
                             f'({ci[0]:.0f}–{ci[1]:.0f})')
            else:
                cells.append("–")
        am = [f'{s[j]["all"]["amend"]:.2f}' if j in s else "–" for j in ("claude", "codex")]
        L.append(f'| {label} | {cells[0]} | {cells[1]} | {am[0]} | {am[1]} | {s["wer_spoken"]:.2f} | '
                 f'{s["wer_intended"]:.2f} |')
    L += ["", "### Clips over 30 s (13, split for Gemma) vs up to 30 s (138)", "",
          "| Run | Claude ZER ≤30 s / >30 s | Codex ZER ≤30 s / >30 s | Claude AMEND ≤30 s / >30 s | "
          "WER vs spoken >30 s |", "| --- | --- | --- | --- | ---: |"]
    for label, s in data.items():
        c = s.get("claude", {}); x = s.get("codex", {})
        L.append(f'| {label} | {f(c.get("upto30", {}).get("zer"))} / {f(c.get("over30", {}).get("zer"))} | '
                 f'{f(x.get("upto30", {}).get("zer"))} / {f(x.get("over30", {}).get("zer"))} | '
                 f'{f(c.get("upto30", {}).get("amend"), 2)} / {f(c.get("over30", {}).get("amend"), 2)} | '
                 f'{s["wer_spoken_over30"]:.2f} |')
    L += ["", "### Paired, clip by clip: wins / losses / ties (by \"needs no edit\")", "",
          f"| Run | vs {BASE_RAW}: Claude | Codex | vs Parakeet + E2B text cleanup: Claude | Codex |",
          "| --- | --- | --- | --- | --- |"]
    for label, s in data.items():
        p = s.get("paired", {})
        def g(j, b):
            v = p.get(j, {}).get(b)
            return f'{v["wins"]} / {v["losses"]} / {v["ties"]}' if v else "–"
        L.append(f"| {label} | {g('claude', BASE_RAW)} | {g('codex', BASE_RAW)} | {g('claude', BASE_CLEAN)} | "
                 f"{g('codex', BASE_CLEAN)} |")
    L += ["", "### AMEND and intended-text WER by half", "",
          "| Run | Claude AMEND tune / check | Codex AMEND tune / check | WER tune / check |",
          "| --- | --- | --- | --- |"]
    for label, s in data.items():
        cells = [" / ".join(f(s[j][h]["amend"], 2) for h in ("tune", "check")) if j in s else "–"
                 for j in ("claude", "codex")]
        cells.append(" / ".join(f(s["halves"][h], 2) for h in ("tune", "check")))
        L.append("| " + " | ".join([label, *cells]) + " |")
    md = "\n".join(L)
    print(md)
    if a.md:
        a.md.write_text(md + "\n")
    if a.json:
        a.json.write_text(json.dumps(data, indent=1, default=float))


if __name__ == "__main__":
    main()
