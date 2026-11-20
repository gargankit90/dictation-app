# /// script
# requires-python = ">=3.11"
# dependencies = ["jiwer>=3.0", "whisper-normalizer==1.0.1", "amend-va==0.2.1"]
# ///
"""Summarise every run in this experiment: one row per pipeline.

    uv run eval/voxtral-omni/summarize.py [--json out.json] [--missing]

Per run: WER vs the spoken and the intended reference (Whisper English normaliser, pooled, as
eval/hosted-asr/analyze.py); ZER / AMEND per judge on tune / check / all (split eval/groq-cleanup/split.csv)
with the number of clips judged; paired zero-edit wins / losses / ties against Nemotron + GPT-6.2-sol v4
under the same judge (clips both have verdicts for); API latency median / p90 (two-pass: the engine's
seconds + the cleanup's, per clip; a guarded one-pass: the slower of its two calls); USD per 1,000
dictations (observed cost / clips x 1,000; on-device = 0); guard fallbacks; harm screens against the
intended text: short (< 80% of its words: dropped content), long (> 120%: invented content), numbers
(digits differ). --missing prints how many distinct prompts each run still needs per judge.
"""
from __future__ import annotations

import argparse
import csv
import glob
import hashlib
import json
import pathlib
import re
import statistics
import sys

import jiwer
from amend.prompts import build_prompt
from whisper_normalizer.english import EnglishTextNormalizer

HERE = pathlib.Path(__file__).resolve().parent
WS = HERE.parents[2]
VA = HERE.parent / "voicearena"
MANIFEST = {r["clip_id"]: r for r in csv.DictReader(open(VA / "clips" / "manifest.csv", newline="", encoding="utf-8"))}
SPLIT = {r["clip_id"]: r["half"] for r in csv.DictReader(open(HERE.parent / "groq-cleanup" / "split.csv", newline=""))}
NORM = EnglishTextNormalizer()
DIGITS = re.compile(r"\d+")
IN = HERE / "input"
ENGINE_DIR = {"nemotron": "nvidia__nemotron-3.5-transcribe", "voxtral_mini": "mistralai__voxtral-mini-3b-2507",
              "voxtral_mini_dev": "ondevice_voxtral3", "oruk_dev": "ondevice_oruk",
              "voxtral_small": "mistralai__voxtral-small-24b-2507-stt",
              "openai_transcribe": "../results/openai_transcribe_plain"}
ENGINE_NAME = {"nemotron": "Nemotron 3.5 (hosted)", "voxtral_mini": "Voxtral Mini 3B (hosted)",
               "voxtral_mini_dev": "Voxtral Mini 3B (on-device)", "oruk_dev": "OrukQwen-ASR 1.7B (on-device)",
               "voxtral_small": "Voxtral Small 24B (hosted)",
              "openai_transcribe": "OpenAI gpt-transcribe (closed model)"}
CLEAN_NAME = {"raw": "raw", "sol": "GPT-6.2-sol", "luna": "GPT-6-luna", "glm": "GLM-5.3 Flash",
              "oss": "gpt-oss-120b", "gemma": "Gemma 4 E2B v3 (on-device)"}
BASELINE = IN / "nemotron_sol_v4"


def read(path) -> dict:
    with open(path, newline="", encoding="utf-8") as f:
        return {r["clip_id"]: r for r in csv.DictReader(f)}


def drafts(rows: dict) -> dict:
    return {c: (r.get("transcript", "") if r.get("status") == "ok" else "") for c, r in rows.items()}


def wer(hyps: dict, key: str) -> float | None:
    pairs = [(NORM(MANIFEST[c][key]), NORM(h)) for c, h in sorted(hyps.items()) if c in MANIFEST]
    pairs = [(r, h if h.strip() else "<empty>") for r, h in pairs if r.strip()]
    return 100 * jiwer.wer([r for r, _ in pairs], [h for _, h in pairs]) if pairs else None


def verdicts(path) -> dict:
    if not path or not pathlib.Path(path).exists():
        return {}
    return {c: r for c, r in read(path).items() if r.get("edits", "") != ""}


def zer(rows: list) -> tuple:
    if not rows:
        return None, None
    z = 100 * sum(int(r["edits"]) == 0 for r in rows) / len(rows)
    ch = sum(int(r["chars"]) for r in rows)
    return z, (100 * sum(int(r["edits"]) for r in rows) / ch if ch else None)


def pct(v, q):
    v = sorted(x for x in v if x is not None)
    if not v:
        return None
    k = (len(v) - 1) * q
    f = int(k)
    c = min(f + 1, len(v) - 1)
    return v[f] + (v[c] - v[f]) * (k - f)


def harm(text: str, ref: str) -> list:
    out = []
    d, s = NORM(text).split(), NORM(ref).split()
    if s and len(d) < 0.8 * len(s):
        out.append("short")
    if s and len(d) > 1.2 * len(s):
        out.append("long")
    if sorted(DIGITS.findall(NORM(text))) != sorted(DIGITS.findall(NORM(ref))):
        out.append("numbers")
    return out


def engine_timing(engine: str) -> tuple[dict, dict]:
    rows = read(IN / ENGINE_DIR[engine] / "harness_results.csv")
    secs = {c: float(r["finalize_s"]) for c, r in rows.items() if r.get("finalize_s")}
    cost = {c: float(r.get("cost_usd") or 0) for c, r in rows.items()}
    return secs, cost


def cleanup_costs(run: pathlib.Path) -> tuple[dict, dict, dict]:
    """Per clip: cleanup seconds, USD, provider (from cleanup.jsonl, by request)."""
    rows = read(run / "results.csv")
    cache = {}
    if (run / "cleanup.jsonl").exists():
        for line in (run / "cleanup.jsonl").read_text(encoding="utf-8").splitlines():
            if line.strip():
                r = json.loads(line)
                cache[r["reply"], r.get("prompt_sha")] = r  # not unique by raw; use model_ms in results.csv
    secs, cost, prov = {}, {}, {}
    by_reply = {}
    for line in (run / "cleanup.jsonl").read_text(encoding="utf-8").splitlines() if (run / "cleanup.jsonl").exists() else []:
        if line.strip():
            r = json.loads(line)
            by_reply.setdefault(r["reply"], r)
    for c, r in rows.items():
        if r.get("model_ms") not in ("", None):
            secs[c] = float(r["model_ms"]) / 1000
        if "cleanup_s" in r and r["cleanup_s"]:
            secs[c] = float(r["cleanup_s"])
        rec = by_reply.get(r.get("cleaned_text", ""))
        if rec:
            cost[c] = float((rec.get("usage") or {}).get("cost") or 0)
            prov[c] = rec.get("served_by", "")
    return secs, cost, prov


def describe(run: pathlib.Path) -> dict:
    """Pipeline name, kind and per-clip latency/cost for a run folder."""
    name = run.name
    info = {"run": name, "dir": str(run)}
    m = re.match(r"(.+?)__(sol|luna|glm|oss|gemma)_v[34]$", name)
    if name == "nemotron_sol_v4":
        m = re.match(r"(nemotron)_(sol)_v4", name)
    if m:
        engine, clean = m.group(1), m.group(2)
        info.update(kind="two-pass", engine=engine, cleanup=clean,
                    pipeline=f"{ENGINE_NAME[engine]} + {CLEAN_NAME[clean]}")
        asr_s, asr_c = engine_timing(engine)
        cs, cc, prov = cleanup_costs(run) if (run / "results.csv").exists() else ({}, {}, {})
        if clean == "gemma" and (run / "results.csv").exists():
            cs = {c: float(r["cleanup_s"]) for c, r in read(run / "results.csv").items() if r.get("cleanup_s")}
        info["lat"] = {c: asr_s.get(c, 0) + cs.get(c, 0) for c in asr_s}
        info["usd"] = {c: asr_c.get(c, 0) + cc.get(c, 0) for c in asr_c}
        info["providers"] = sorted({p for p in prov.values() if p})
        if engine.endswith("_dev"):
            info["note"] = "engine seconds are on-device (M-series Mac), cleanup is API time"
    elif name.startswith("raw__"):
        engine = name[5:]
        info.update(kind="raw", engine=engine, cleanup="raw", pipeline=f"{ENGINE_NAME[engine]}, raw")
        asr_s, asr_c = engine_timing(engine)
        info["lat"], info["usd"] = asr_s, asr_c
    else:  # part B
        rows = read(run / "harness_results.csv")
        base, _, mode = name.rpartition("_onepass") if "_onepass" in name else (name.rsplit("_plain", 1)[0], "", "plain")
        mode = {"": "one-pass", "_guard": "one-pass + guard"}.get(mode, mode)
        info.update(kind="audio-llm", engine=base, cleanup=mode, pipeline=f"{base} ({mode})")
        info["lat"] = {c: float(r["finalize_s"]) for c, r in rows.items() if r.get("finalize_s") not in ("", None)}
        info["usd"] = {c: float(r["cost_usd"] or 0) for c, r in rows.items() if r.get("cost_usd") not in ("", None)}
        info["providers"] = sorted({r["provider"] for r in rows.values() if r.get("provider")})
    return info


def input_csv(run: pathlib.Path) -> pathlib.Path:
    return run / "results.csv" if (run / "results.csv").exists() else run / "harness_results.csv"


SCORED = {"claude": "scored.csv", "codex": "scored_codex.csv", "gemini": "scored_gemini.csv"}


def scored(run: pathlib.Path, judge: str) -> pathlib.Path | None:
    if run.name.startswith("raw__"):
        return run / f"{judge}.csv"
    p = run / SCORED[judge]
    return p if p.exists() else None


def summarise(run: pathlib.Path) -> dict:
    info = describe(run)
    rows = read(input_csv(run))
    d = drafts(rows)
    info["n_ok"] = sum(1 for r in rows.values() if r.get("status") == "ok")
    info["wer_spoken"], info["wer_intended"] = wer(d, "spoken_text"), wer(d, "intended_text")
    reasons = {}
    for r in rows.values():
        why = r.get("reason") or r.get("fallback") or ""
        if why and why not in ("cleaned", "empty"):
            reasons[why] = reasons.get(why, 0) + 1
    info["fallbacks"] = reasons
    hs = {"short": 0, "long": 0, "numbers": 0}
    for c, t in d.items():
        for h in harm(t, MANIFEST[c]["intended_text"]):
            hs[h] += 1
    info["harms"] = hs
    lat = list(info.pop("lat").values())
    info["lat_median"], info["lat_p90"] = pct(lat, 0.5), pct(lat, 0.9)
    usd = list(info.pop("usd").values())
    info["usd_per_1000"] = 1000 * sum(usd) / 151 if usd else None
    allv = {}
    for judge in ("claude", "codex", "gemini"):
        v = verdicts(scored(run, judge))
        allv[judge] = v
        base = verdicts(scored(BASELINE, judge))
        j = {"n": len(v)}
        for half in ("tune", "check", "all"):
            sel = [r for c, r in v.items() if half == "all" or SPLIT.get(c) == half]
            j[half], j[f"amend_{half}"] = zer(sel)
            j[f"n_{half}"] = len(sel)
        common = sorted(v.keys() & base.keys())
        w = sum(int(v[c]["edits"]) == 0 and int(base[c]["edits"]) > 0 for c in common)
        lo = sum(int(v[c]["edits"]) > 0 and int(base[c]["edits"]) == 0 for c in common)
        j["vs_base"] = (w, lo, len(common) - w - lo)
        info[judge] = j
    # 2-of-3 majority (Voice Arena's panel is 3-of-5): a clip is zero-edit if at least two of the three
    # judges found nothing to fix; only clips all three judged, and only when all three judged all 151.
    full = all(len(allv[j]) == len(MANIFEST) for j in allv)
    if full:
        maj = {c: sum(int(allv[j][c]["edits"]) == 0 for j in allv) >= 2 for c in MANIFEST}
        info["majority"] = {h: 100 * sum(maj[c] for c in maj if h == "all" or SPLIT.get(c) == h) /
                            sum(1 for c in maj if h == "all" or SPLIT.get(c) == h) for h in ("tune", "check", "all")}
    else:
        info["majority"] = None
    return info


def missing(run: pathlib.Path, judge: str, known: set) -> set:
    rows = read(input_csv(run))
    need = set()
    for c, m in MANIFEST.items():
        r = rows.get(c, {})
        draft = r.get("transcript", "") if r.get("status") == "ok" else ""
        h = hashlib.sha256(build_prompt(m["intended_text"], draft, m["spoken_text"], "v1").encode()).hexdigest()
        if h not in known:
            need.add(h)
    return need


def known_verdicts(judge: str) -> set:
    name = SCORED[judge][:-4]
    other = f"_{judge}"
    pats = [f"{VA}/results/*/{name}.judge.jsonl", f"{HERE}/results*/*/{name}.judge.jsonl", f"{HERE}/input/*/*.judge.jsonl",
            f"{WS}/hosted-asr-eval/eval/hosted-asr/results/*/{name}.judge.jsonl",
            f"{WS}/cleanup-offline-eval/eval/cleanup-offline/results/*/{name}.judge.jsonl",
            f"{WS}/cleanup-offline-eval/eval/cleanup-offline/paired-results/*/{name}.judge.jsonl",
            f"{WS}/local-cleanup-eval/eval/local-cleanup/judge_cache/{judge}.jsonl",
            f"/tmp/vocet-asr-screen/runs/*{other}.judge.jsonl", f"/tmp/vocet-asr-screen/allcache-{judge}"]
    want = {"claude": "claude", "codex": "gpt", "gemini": "gemini"}[judge]
    out = set()
    for p in pats:
        for f in glob.glob(p):
            for line in open(f, encoding="utf-8"):
                try:
                    r = json.loads(line)
                except ValueError:
                    continue
                if want in str(r.get("model", "")).lower():
                    out.add(r["prompt_sha256"])
    return out


def runs() -> list[pathlib.Path]:
    out = [BASELINE]
    out += sorted(p for p in (HERE / "baselines").glob("raw__*") if p.is_dir())
    for root in ("results", "results-local"):
        out += sorted(p for p in (HERE / root).glob("*") if p.is_dir() and (input_csv(p)).exists()
                      and not (p / "calls.jsonl").exists())
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--json", type=pathlib.Path)
    ap.add_argument("--missing", action="store_true")
    ap.add_argument("only", nargs="*")
    a = ap.parse_args()
    todo = [pathlib.Path(x).resolve() for x in a.only] or runs()
    if a.missing:
        for judge in ("codex", "claude", "gemini"):
            known = known_verdicts(judge)
            print(f"{judge}: {len(known)} known verdicts")
            for r in todo:
                print(f"  {r.name}: {len(missing(r, judge, known))} distinct prompts missing")
        return
    out = [summarise(r) for r in todo]
    f = lambda x, n=1: "–" if x is None else f"{x:.{n}f}"  # noqa: E731
    print("| Pipeline | Claude ZER tune / check / **all** | Codex ZER tune / check / **all** | Gemini ZER all (n) | "
          "2-of-3 ZER check / **all** | AMEND C / X | vs Nemotron+Sol W/L/T (Claude; Codex) | WER spoken % | "
          "s median / p90 | USD/1k | fallbacks | short / long / numbers |")
    print("|---|---|---|---|---|---|---|---:|---|---:|---|---|")

    def zz(j):
        return f"{f(j['tune'])} / {f(j['check'])} / **{f(j['all'])}**" + ("" if j["n"] in (0, 151) else f" ({j['n']})") \
            if j["n"] else "–"
    for s in out:
        x, c, g, m = s["codex"], s["claude"], s["gemini"], s["majority"]
        zg = f"{f(g['all'])} ({g['n']})" if g["n"] else "–"
        zm = f"{f(m['check'])} / **{f(m['all'])}**" if m else "–"
        vs = f"{'/'.join(map(str, c['vs_base'])) if c['n'] else '–'}; {'/'.join(map(str, x['vs_base'])) if x['n'] else '–'}"
        fb = ", ".join(f"{k} {v}" for k, v in sorted(s["fallbacks"].items())) or "–"
        h = s["harms"]
        print(f"| {s['pipeline']} | {zz(c)} | {zz(x)} | {zg} | {zm} | {f(c['amend_all'], 2)} / {f(x['amend_all'], 2)} | "
              f"{vs} | {f(s['wer_spoken'], 1)} | {f(s['lat_median'], 2)} / {f(s['lat_p90'], 2)} | "
              f"{f(s['usd_per_1000'], 2)} | {fb} | {h['short']} / {h['long']} / {h['numbers']} |")
    if a.json:
        a.json.write_text(json.dumps(out, indent=1, default=str))


if __name__ == "__main__":
    sys.exit(main())
