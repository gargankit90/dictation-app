# /// script
# requires-python = ">=3.11"
# dependencies = ["amend-va==0.2.1", "jiwer"]
# ///
"""Cheap screen before the judge: compare each pasted text with the dataset's intended text.

    uv run screen.py runs/parakeet/*__v1 [--raw parakeet]

Per run and per half (tune / check / all):
  match_ref   share of clips whose text equals the intended text after amend-va's normalisation;
  match_fix   share equal to the intended text OR to the judge's corrected text of the raw draft (a
              looser proxy for "the judge would pass it", since the judge accepts kept self-corrections
              and digits for spoken numbers);
  cer, wer    pooled character edits / word error rate against the intended text (amend-va's normalise);
  broke       clips the judge passed as raw (zero edits) that the cleanup changed away from the raw text
              (a risk count: each needs the judge to say whether it still passes).
The raw engine itself is printed first, as the baseline.
"""
import argparse
import csv
import json
import pathlib

import jiwer
from amend.metric import char_edits, normalize

HERE = pathlib.Path(__file__).resolve().parent
VA = HERE.parent / "voicearena"
RAW = {"parakeet": "vocet_parakeet", "qwen3": "vocet_qwen3", "groq": "vocet_groq"}


def stats(rows, ref, fix, rawzero, rawtext):
    out = {}
    for half in ("tune", "check", "all"):
        sub = [r for r in rows if half == "all" or r["half"] == half]
        if not sub:
            continue
        n = len(sub)
        mref = sum(char_edits(r["transcript"], ref[r["clip_id"]])[0] == 0 for r in sub)
        mfix = sum(char_edits(r["transcript"], ref[r["clip_id"]])[0] == 0
                   or char_edits(r["transcript"], fix[r["clip_id"]])[0] == 0 for r in sub)
        e = c = 0
        for r in sub:
            ee, cc = char_edits(r["transcript"], ref[r["clip_id"]])
            e, c = e + ee, c + cc
        wer = jiwer.wer([normalize(ref[r["clip_id"]]) for r in sub], [normalize(r["transcript"]) or "∅" for r in sub])
        broke = sum(1 for r in sub if rawzero[r["clip_id"]]
                    and normalize(r["transcript"]) != normalize(rawtext[r["clip_id"]]))
        out[half] = {"n": n, "match_ref": 100 * mref / n, "match_fix": 100 * mfix / n, "cer": 100 * e / c,
                     "wer": 100 * wer, "broke": broke}
    return out


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("runs", nargs="+", type=pathlib.Path)
    ap.add_argument("--raw", default="parakeet", choices=RAW)
    ap.add_argument("--json", type=pathlib.Path)
    a = ap.parse_args()
    split = {r["clip_id"]: r["half"] for r in csv.DictReader((HERE / "split.csv").open())}
    ref = {r["clip_id"]: r["intended_text"] for r in csv.DictReader((VA / "clips/manifest.csv").open())}
    scored = {r["clip_id"]: r for r in csv.DictReader((VA / "results" / RAW[a.raw] / "scored.csv").open())}
    fix = {k: v["fixed"] for k, v in scored.items()}
    rawzero = {k: v["zero_edit"] == "1" for k, v in scored.items()}
    rawtext = {k: v["draft"] for k, v in scored.items()}
    table = {}
    base = [{"clip_id": k, "transcript": v["draft"], "half": split[k]} for k, v in scored.items()]
    table[f"{a.raw} raw"] = stats(base, ref, fix, rawzero, rawtext)
    for d in a.runs:
        rows = list(csv.DictReader((d / "results.csv").open(newline="", encoding="utf-8")))
        for r in rows:
            r["half"] = split[r["clip_id"]]
        meta = json.loads((d / "meta.json").read_text())
        s = stats(rows, ref, fix, rawzero, rawtext)
        s["meta"] = {k: meta.get(k) for k in ("model_bytes", "load_s", "peak_memory_bytes", "max_rss_bytes",
                                              "latency_s", "fallback_total", "fallbacks", "unchanged")}
        table[d.name] = s
    hdr = f"{'run':38} {'half':5} {'n':>3} {'mref':>5} {'mfix':>5} {'cer':>5} {'wer':>5} {'brk':>3} {'fb':>3} {'unch':>4} {'med_s':>5} {'p90_s':>5}"
    print(hdr)
    for name, s in table.items():
        for half in ("tune", "check", "all"):
            if half not in s:
                continue
            x = s[half]
            m = s.get("meta") or {}
            lat = m.get("latency_s") or {}
            print(f"{name:38} {half:5} {x['n']:>3} {x['match_ref']:5.1f} {x['match_fix']:5.1f} {x['cer']:5.2f} "
                  f"{x['wer']:5.2f} {x['broke']:>3} {m.get('fallback_total', ''):>3} {m.get('unchanged', ''):>4} "
                  f"{lat.get('median', 0):5.2f} {lat.get('p90', 0):5.2f}")
    if a.json:
        a.json.write_text(json.dumps(table, indent=1))


if __name__ == "__main__":
    main()
