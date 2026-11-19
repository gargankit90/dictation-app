# /// script
# requires-python = ">=3.11"
# dependencies = ["jiwer>=3.0", "whisper-normalizer==1.0.1"]
# ///
"""Summarise the hosted ASR runs next to Groq raw (eval/voicearena/results/vocet_groq).

    uv run eval/hosted-asr/analyze.py [results/<label> ...]   # default: every folder in results/

For each run: WER vs the spoken and the intended reference (Whisper English normaliser, pooled over
the clips, as eval/voicearena/aai_split.py does); if judged, ZER / AMEND under each judge on the tune
half, the check half and all clips (95% speaker bootstrap, 2000 resamples, seed 7, as
eval/groq-cleanup/summarize.py), and paired wins / losses / ties against Groq raw under the same judge;
API latency median / p90 (finalize_s = request sent to reply received); the providers OpenRouter
used; the summed cost; and automatic failure flags:
  thank_you   the draft ends with "thank you" (any case/punctuation) and the spoken text doesn't
  empty       no text
  short       normalised draft has < 80% of the spoken text's words (dropped content)
  long        normalised draft has > 120% of the spoken text's words (invented content)
  numbers     the digits in the normalised draft differ from those in the normalised spoken text
Writes summary.json in each folder and prints a markdown table.
"""
import csv
import json
import pathlib
import random
import re
import statistics
import sys
from collections import Counter

import jiwer
from whisper_normalizer.english import EnglishTextNormalizer

HERE = pathlib.Path(__file__).resolve().parent
VA = HERE.parent / "voicearena"
MANIFEST = {r["clip_id"]: r for r in csv.DictReader(open(VA / "clips" / "manifest.csv", newline=""))}
SPLIT = {r["clip_id"]: r["half"] for r in csv.DictReader(open(HERE.parent / "groq-cleanup" / "split.csv", newline=""))}
GROQ = VA / "results" / "vocet_groq"
NORM = EnglishTextNormalizer()
THANKS = re.compile(r"thank you[\s.!]*$", re.I)
DIGITS = re.compile(r"\d+")
JUDGES = {"claude": "scored.csv", "codex": "scored_codex.csv"}


def read(path: pathlib.Path) -> dict:
    with open(path, newline="", encoding="utf-8") as f:
        return {r["clip_id"]: r for r in csv.DictReader(f)}


def wer(hyps: dict, ref_key: str) -> float:
    ids = sorted(hyps)
    pairs = [(NORM(MANIFEST[c][ref_key]), NORM(hyps[c])) for c in ids]
    pairs = [(r, h) for r, h in pairs if r.strip()]
    return 100 * jiwer.wer([r for r, _ in pairs], [h if h.strip() else "<empty>" for _, h in pairs])


def zer_amend(rows):
    if not rows:
        return None, None
    z = 100.0 * sum(int(r["edits"]) == 0 for r in rows) / len(rows)
    chars = sum(int(r["chars"]) for r in rows)
    return z, (100.0 * sum(int(r["edits"]) for r in rows) / chars if chars else None)


def bootstrap(rows, n=2000, seed=7):
    by = {}
    for r in rows:
        by.setdefault(r["speaker_id"], []).append(r)
    speakers, rng, zs = sorted(by), random.Random(seed), []
    for _ in range(n):
        zs.append(zer_amend([r for s in rng.choices(speakers, k=len(speakers)) for r in by[s]])[0])
    zs.sort()
    return zs[int(0.025 * n)], zs[int(0.975 * n) - 1]


def pct(values, q):
    values = sorted(values)
    if not values:
        return None
    k = (len(values) - 1) * q
    f = int(k)
    c = min(f + 1, len(values) - 1)
    return values[f] + (values[c] - values[f]) * (k - f)


def flags(draft: str, spoken: str) -> list[str]:
    out = []
    if not draft.strip():
        return ["empty"]
    if THANKS.search(draft) and not THANKS.search(spoken):
        out.append("thank_you")
    d, s = NORM(draft).split(), NORM(spoken).split()
    if s and len(d) < 0.8 * len(s):
        out.append("short")
    if s and len(d) > 1.2 * len(s):
        out.append("long")
    if sorted(DIGITS.findall(NORM(draft))) != sorted(DIGITS.findall(NORM(spoken))):
        out.append("numbers")
    return out


def judged(d: pathlib.Path, judge: str) -> dict | None:
    p = d / JUDGES[judge]
    if not p.exists():
        return None
    scored = read(p)
    if scored.keys() != MANIFEST.keys() or any(r["edits"] == "" for r in scored.values()):
        return None
    rows = list(scored.values())
    for r in rows:
        r.setdefault("speaker_id", MANIFEST[r["clip_id"]]["speaker_id"])
    out = {}
    for half in ("tune", "check"):
        z, a = zer_amend([r for r in rows if SPLIT[r["clip_id"]] == half])
        out[half] = {"zer": z, "amend": a}
    z, a = zer_amend(rows)
    out["all"] = {"n": len(rows), "zer": z, "amend": a, "zer_ci95": bootstrap(rows)}
    groq = read(GROQ / JUDGES[judge])
    w = l = t0 = t1 = 0
    for r in rows:
        g = groq.get(r["clip_id"])
        if not g:
            continue
        mine, theirs = int(r["edits"]) == 0, int(g["edits"]) == 0
        if mine and not theirs:
            w += 1
        elif theirs and not mine:
            l += 1
        elif mine:
            t1 += 1
        else:
            t0 += 1
    out["vs_groq"] = {"wins": w, "losses": l, "ties_both_zero": t1, "ties_both_edit": t0}
    return out


def summarise(d: pathlib.Path) -> dict:
    res = read(d / "harness_results.csv")
    ok = {c: r for c, r in res.items() if r["status"] == "ok"}
    hyps = {c: (ok[c]["transcript"] if c in ok else "") for c in MANIFEST}
    lat = [float(r["finalize_s"]) for r in ok.values() if r.get("finalize_s")]
    fl = Counter()
    per_clip = {}
    for c, h in hyps.items():
        f = flags(h, MANIFEST[c]["spoken_text"])
        per_clip[c] = f
        fl.update(f)
    cost = sum(float(r["cost_usd"]) for r in ok.values() if r.get("cost_usd") not in ("", None))
    audio_h = sum(float(r["sent_s"]) for r in ok.values() if r.get("sent_s")) / 3600
    s = {
        "label": d.name, "model": next(iter(res.values()))["model"] if res else "",
        "ok": len(ok), "errors": Counter(r["error"] for r in res.values() if r["status"] != "ok"),
        "wer_vs_spoken": wer(hyps, "spoken_text"), "wer_vs_intended": wer(hyps, "intended_text"),
        "latency_median_s": statistics.median(lat) if lat else None, "latency_p90_s": pct(lat, 0.9),
        "providers": Counter(r.get("provider") or "?" for r in ok.values()),
        "cost_usd": cost, "audio_hours": audio_h, "usd_per_audio_hour": cost / audio_h if audio_h else None,
        "flags": fl, "flagged_clips": {c: f for c, f in per_clip.items() if f},
    }
    for j in JUDGES:
        s[j] = judged(d, j)
    (d / "summary.json").write_text(json.dumps(s, indent=1, default=list) + "\n")
    return s


def groq_row() -> dict:
    res = read(GROQ / "harness_results.csv")
    hyps = {c: res[c]["transcript"] for c in MANIFEST}
    fl = Counter(f for c, h in hyps.items() for f in flags(h, MANIFEST[c]["spoken_text"]))
    lat = [float(r["finalize_s"]) for r in res.values() if r.get("finalize_s")]
    s = {"label": "vocet_groq (VM, key-up to text)", "wer_vs_spoken": wer(hyps, "spoken_text"),
         "wer_vs_intended": wer(hyps, "intended_text"), "flags": fl,
         "latency_median_s": statistics.median(lat), "latency_p90_s": pct(lat, 0.9), "providers": {"Groq": 151}}
    for j, f in JUDGES.items():
        rows = list(read(GROQ / f).values())
        for r in rows:
            r.setdefault("speaker_id", MANIFEST[r["clip_id"]]["speaker_id"])
        s[j] = {h: dict(zip(("zer", "amend"), zer_amend([r for r in rows if SPLIT[r["clip_id"]] == h])))
                for h in ("tune", "check")}
        z, a = zer_amend(rows)
        s[j]["all"] = {"zer": z, "amend": a, "zer_ci95": bootstrap(rows)}
    return s


def fmt(x, n=1):
    return "–" if x is None else f"{x:.{n}f}"


def main() -> None:
    dirs = [pathlib.Path(a) for a in sys.argv[1:]] or sorted(p for p in (HERE / "results").iterdir()
                                                            if (p / "harness_results.csv").exists())
    rows = [groq_row()] + [summarise(d) for d in dirs]
    print("| Run | ZER Claude tune / check / **all** (95% CI) | ZER Codex tune / check / **all** | AMEND C / X "
          "| vs Groq W/L (Claude; Codex) | WER spoken % | WER intended % | API s median / p90 | thank-you / short / long / numbers / empty |")
    print("|---|---|---|---|---|---:|---:|---|---|")
    for s in rows:
        cells = []
        for j in JUDGES:
            v = s.get(j)
            cells.append("–" if not v else f"{fmt(v['tune']['zer'])} / {fmt(v['check']['zer'])} / **{fmt(v['all']['zer'])}**"
                         + (f" ({fmt(v['all']['zer_ci95'][0])}–{fmt(v['all']['zer_ci95'][1])})" if j == "claude" else ""))
        am = " / ".join(fmt((s.get(j) or {}).get("all", {}).get("amend"), 2) for j in JUDGES)
        vs = "; ".join(f"{v['vs_groq']['wins']}/{v['vs_groq']['losses']}" if (v := s.get(j)) and "vs_groq" in v else "–"
                       for j in JUDGES)
        f = s["flags"]
        print(f"| {s['label']} | {cells[0]} | {cells[1]} | {am} | {vs} | {fmt(s['wer_vs_spoken'], 2)} | "
              f"{fmt(s['wer_vs_intended'], 2)} | {fmt(s['latency_median_s'], 2)} / {fmt(s['latency_p90_s'], 2)} | "
              f"{f.get('thank_you', 0)} / {f.get('short', 0)} / {f.get('long', 0)} / {f.get('numbers', 0)} / {f.get('empty', 0)} |")
    print()
    for s in rows[1:]:
        print(f"- {s['label']}: {s['ok']}/151 ok, errors {dict(s['errors']) or '-'}, providers {dict(s['providers'])}, "
              f"cost ${s['cost_usd']:.4f} for {s['audio_hours']:.2f} h sent (${fmt(s['usd_per_audio_hour'], 3)}/h)")


if __name__ == "__main__":
    main()
