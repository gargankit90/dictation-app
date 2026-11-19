# /// script
# requires-python = ">=3.11"
# dependencies = ["jiwer>=3.0", "whisper-normalizer==1.0.1"]
# ///
"""AssemblyAI run 2: split transcription from cleanup.

Subcommands (run from eval/voicearena):

  join      match each line of the opt-in reply dump (exp/assemblyai 9911b0a, VOCET_EVAL_DUMP_DIR) to its clip
            and write results/<run>/replies.csv plus results/<run>_verbatim/results.csv, a harness-shaped
            CSV whose "transcript" is AssemblyAI's verbatim text, so score_claude_cli.py can judge it as if
            it had been pasted.
              uv run aai_split.py join results/vocet_assemblyai_run2
  wer       pooled WER of every engine's pasted text (and AssemblyAI's verbatim text) against the spoken
            and the intended reference, after the Whisper English normaliser.
              uv run aai_split.py wer
  cleanup   verbatim -> cleaned, per clip: fillers and self-correction cues removed, content lost
            (digits, emails, capitalised names, content words in the verbatim text and in the intended
            reference but missing from the cleaned text), judge verdict flips, and the digits/names guard.
              uv run aai_split.py cleanup
"""
import csv
import json
import re
import sys
from bisect import bisect_left
from datetime import datetime, timezone
from pathlib import Path

import jiwer
from whisper_normalizer.english import EnglishTextNormalizer

R = Path("results")
MANIFEST = {r["clip_id"]: r for r in csv.DictReader(open("clips/manifest.csv", encoding="utf-8"))}
NORM = EnglishTextNormalizer()


def ts(s: str) -> float:
    return datetime.fromisoformat(s.replace("Z", "+00:00")).astimezone(timezone.utc).timestamp()


def load(path):
    return {r["clip_id"]: r for r in csv.DictReader(open(path, newline="", encoding="utf-8"))}


def join(run: Path) -> None:
    rows = sorted(csv.DictReader(open(run / "results.csv", newline="", encoding="utf-8")), key=lambda r: r["run_at"])
    done = [ts(r["run_at"]) for r in rows]
    lines = [json.loads(x) for x in open(run / "assemblyai-replies.jsonl", encoding="utf-8") if x.strip()]
    if len(lines) != len(rows):
        raise SystemExit(f"{len(lines)} dump lines but {len(rows)} harness rows")
    out, used = [], set()
    for d in lines:
        i = bisect_left(done, ts(d["ts"]) - 1)  # the reply comes before the row is written
        if i >= len(rows) or i in used:
            raise SystemExit(f"can't place dump line at {d['ts']}")
        used.add(i)
        r = rows[i]
        pasted_matches = " ".join(d["pasted_text"].split()) == " ".join(r["transcript"].split())
        out.append({"clip_id": r["clip_id"], "dump_ts": d["ts"], "run_at": r["run_at"],
                    "http_status": d.get("http_status"), "rewrite_mode_sent": d.get("rewrite_mode_sent"),
                    "instruction": d.get("instruction"), "pasted": d.get("pasted"),
                    "pasted_reason": d.get("pasted_reason"), "llm_error": d.get("llm_error"),
                    "request_time_ms": d.get("request_time_ms"), "audio_duration_ms": d.get("audio_duration_ms"),
                    "pasted_matches_textedit": pasted_matches, "verbatim_text": d.get("verbatim_text") or "",
                    "llm_response": d.get("llm_response") or "", "pasted_text": d.get("pasted_text") or ""})
    out.sort(key=lambda o: o["clip_id"])
    with open(run / "replies.csv", "w", newline="", encoding="utf-8") as f:
        w = csv.DictWriter(f, list(out[0]))
        w.writeheader()
        w.writerows(out)
    vdir = run.parent / (run.name + "_verbatim")
    vdir.mkdir(exist_ok=True)
    harness = {r["clip_id"]: r for r in rows}
    with open(vdir / "results.csv", "w", newline="", encoding="utf-8") as f:
        w = csv.DictWriter(f, ["clip_id", "status", "transcript", "finalize_s"])
        w.writeheader()
        for o in out:
            w.writerow({"clip_id": o["clip_id"], "status": "ok", "transcript": o["verbatim_text"],
                        "finalize_s": harness[o["clip_id"]]["finalize_s"]})
    mism = sum(1 for o in out if not o["pasted_matches_textedit"])
    print(f"{len(out)} replies joined; pasted text differs from what TextEdit got in {mism}; "
          f"pasted: {dict((k, sum(o['pasted'] == k for o in out)) for k in {o['pasted'] for o in out})}")


def wer_pooled(hyps: dict, ref_key: str) -> float:
    ids = sorted(hyps)
    refs = [NORM(MANIFEST[c][ref_key]) for c in ids]
    hy = [NORM(hyps[c]) for c in ids]
    keep = [(r, h) for r, h in zip(refs, hy) if r.strip()]
    return 100 * jiwer.wer([r for r, _ in keep], [h if h.strip() else "<empty>" for _, h in keep])


def wer() -> None:
    sources = {
        "Groq (pasted, no cleanup)": ("vocet_groq/scored.csv", "draft"),
        "Parakeet (pasted, no cleanup)": ("vocet_parakeet/scored.csv", "draft"),
        "Qwen3 (pasted, no cleanup)": ("vocet_qwen3/scored.csv", "draft"),
        "AssemblyAI verbatim (run 2)": ("vocet_assemblyai_run2/replies.csv", "verbatim_text"),
        "AssemblyAI cleaned (run 2, pasted)": ("vocet_assemblyai_run2/replies.csv", "pasted_text"),
        "AssemblyAI cleaned (run 1, pasted)": ("vocet_assemblyai/scored.csv", "draft"),
    }
    out = []
    for name, (path, col) in sources.items():
        rows = load(R / path)
        hyps = {c: r[col] for c, r in rows.items()}
        out.append({"engine": name, "n": len(hyps), "wer_vs_spoken": round(wer_pooled(hyps, "spoken_text"), 2),
                    "wer_vs_intended": round(wer_pooled(hyps, "intended_text"), 2)})
    with open(R / "wer_summary.csv", "w", newline="", encoding="utf-8") as f:
        w = csv.DictWriter(f, list(out[0]))
        w.writeheader()
        w.writerows(out)
    print("| engine | n | WER vs spoken % | WER vs intended % |\n|---|---:|---:|---:|")
    for o in out:
        print(f"| {o['engine']} | {o['n']} | {o['wer_vs_spoken']} | {o['wer_vs_intended']} |")


FILLERS = re.compile(r"\b(um+|uh+|like|you know|i mean|kind of|sort of|okay so|so|well|oh|basically|actually)\b", re.I)
CUES = re.compile(r"\b(wait,? no|no wait|sorry|actually,? no|scratch that|make it|i mean)\b", re.I)
EMAIL = re.compile(r"\S+@\S+|\b\w+ (?:at|dot) \w+", re.I)
STOP = set("a an the and or but so to of in on at for with is are was were be been it this that i you we they he she "
           "my your our their me us them do does did not no yes just like know mean okay oh well um uh kind sort "
           "really very can could would should will have has had".split())


OH = re.compile(r"\boh\b", re.I)


def norm(text: str) -> str:
    """The Whisper normaliser, minus one artefact: it writes the interjection "oh" as the digit 0."""
    return NORM(OH.sub(" ", text))


def norm_tokens(text: str) -> list[str]:
    return norm(text).split()


def lost_items(verbatim: str, cleaned: str, intended: str) -> list[str]:
    """Things in the verbatim text, also in the intended reference, but gone from the cleaned text."""
    v, c, i = norm_tokens(verbatim), set(norm_tokens(cleaned)), set(norm_tokens(intended))
    lost = []
    for t in dict.fromkeys(v):
        if t in c or t not in i or t in STOP:
            continue
        lost.append(t)
    return lost


def guard_triggers(verbatim: str, cleaned: str) -> list[str]:
    """The proposed guard: digit groups, emails and capitalised mid-sentence words in the verbatim text that
    the cleaned text lost (compared after normalisation, so "three" vs "3" counts as kept)."""
    why = []
    nv, nc = norm(verbatim), norm(cleaned)
    for d in set(re.findall(r"\d+", nv)):
        if d not in re.findall(r"\d+", nc):
            why.append(f"digits {d}")
    for e in set(re.findall(r"\S+@\S+", verbatim)):
        if e.lower() not in cleaned.lower():
            why.append(f"email {e}")
    words = re.findall(r"(?<![.!?]\s)(?<!^)\b([A-Z][a-z]+)\b", verbatim)
    for n in set(words):
        if n.lower() not in cleaned.lower():
            why.append(f"name {n}")
    return why


def cleanup() -> None:
    rep = load(R / "vocet_assemblyai_run2/replies.csv")
    clean = load(R / "vocet_assemblyai_run2/scored.csv")
    verb = load(R / "vocet_assemblyai_run2_verbatim/scored.csv")
    run1 = load(R / "vocet_assemblyai/scored.csv")
    rows = []
    for c in sorted(rep):
        r, m = rep[c], MANIFEST[c]
        v, p = r["verbatim_text"], r["pasted_text"]
        vf, pf = len(FILLERS.findall(v)), len(FILLERS.findall(p))
        vc, pc = len(CUES.findall(v)), len(CUES.findall(p))
        g = guard_triggers(v, p)
        guarded_edits = int(verb[c]["edits"]) if g else int(clean[c]["edits"])
        rows.append({
            "clip_id": c, "domain": m["domain"],
            "verbatim_zero_edit": verb[c]["zero_edit"], "cleaned_zero_edit": clean[c]["zero_edit"],
            "verbatim_edits": verb[c]["edits"], "cleaned_edits": clean[c]["edits"],
            "run1_zero_edit": run1[c]["zero_edit"], "run1_same_text": " ".join(run1[c]["draft"].split()) == " ".join(p.split()),
            "fillers_verbatim": vf, "fillers_cleaned": pf, "cues_verbatim": vc, "cues_cleaned": pc,
            "lost_vs_reference": " ".join(lost_items(v, p, m["intended_text"])),
            "guard_would_fire": "; ".join(g), "guarded_edits": guarded_edits,
            "guarded_zero_edit": int(guarded_edits == 0),
            "verbatim_text": v, "cleaned_text": p, "intended_text": m["intended_text"],
        })
    with open(R / "vocet_assemblyai_run2/cleanup_analysis.csv", "w", newline="", encoding="utf-8") as f:
        w = csv.DictWriter(f, list(rows[0]))
        w.writeheader()
        w.writerows(rows)
    n = len(rows)
    z = lambda k: sum(int(r[k]) for r in rows)
    print(f"ZER verbatim {100 * z('verbatim_zero_edit') / n:.1f}%  cleaned {100 * z('cleaned_zero_edit') / n:.1f}%  "
          f"with guard {100 * z('guarded_zero_edit') / n:.1f}%")
    print("verbatim ok, cleaned not:", [r["clip_id"] for r in rows if r["verbatim_zero_edit"] == "1" and r["cleaned_zero_edit"] == "0"])
    print("cleaned ok, verbatim not:", sum(1 for r in rows if r["verbatim_zero_edit"] == "0" and r["cleaned_zero_edit"] == "1"))
    print("run1 vs run2 cleaned: same text", sum(r["run1_same_text"] for r in rows),
          "zero-edit flips", sum(1 for r in rows if r["run1_zero_edit"] != r["cleaned_zero_edit"]))
    print("fillers verbatim/cleaned:", sum(r["fillers_verbatim"] for r in rows), sum(r["fillers_cleaned"] for r in rows),
          " cue words:", sum(r["cues_verbatim"] for r in rows), sum(r["cues_cleaned"] for r in rows))
    fired = [r for r in rows if r["guard_would_fire"]]
    helped = [r["clip_id"] for r in fired if int(r["verbatim_edits"]) < int(r["cleaned_edits"])]
    hurt = [r["clip_id"] for r in fired if int(r["verbatim_edits"]) > int(r["cleaned_edits"])]
    print(f"guard fires on {len(fired)}: helped {helped}, hurt {hurt}, neutral {len(fired) - len(helped) - len(hurt)}")
    print("clips that lost reference words in cleanup:", [(r["clip_id"], r["lost_vs_reference"]) for r in rows if r["lost_vs_reference"]])


if __name__ == "__main__":
    cmd = sys.argv[1]
    if cmd == "join":
        join(Path(sys.argv[2]))
    elif cmd == "wer":
        wer()
    elif cmd == "cleanup":
        cleanup()
    else:
        raise SystemExit(__doc__)
