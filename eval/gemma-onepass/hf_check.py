# /// script
# requires-python = ">=3.11"
# dependencies = ["transformers>=5.10.1", "torch", "accelerate", "pillow", "torchvision", "numpy", "jiwer>=3.0", "whisper-normalizer==1.0.1"]
# ///
"""Reference check: is llama.cpp's (experimental) Gemma 4 audio path as good as Google's own route?

    uv run hf_check.py --model ~/vocet-onepass-models/hf/gemma-4-E2B-it --n 30

Runs google/gemma-4-E2B-it (bf16 safetensors, the documented transformers route, on MPS) in the plain and
one-pass modes over the first N clips of up to 30 s, with the same prompts and 16 kHz mono float input,
greedy decoding. Writes runs/hf_check/results_<mode>.csv and prints WER (vs the spoken and the intended
text) and filler counts next to the llama.cpp Q4_K_M runs on the same clips. trust_remote_code is off.
"""
import argparse
import csv
import pathlib
import re
import subprocess
import sys
import time

import jiwer
import numpy as np
import torch
from transformers import AutoModelForImageTextToText, AutoProcessor
from whisper_normalizer.english import EnglishTextNormalizer

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from prompts_onepass import MODES  # noqa: E402

CLIPS = HERE.parent / "voicearena/clips"
NORM = EnglishTextNormalizer()
OH = re.compile(r"\boh\b", re.I)
FILL = re.compile(r"\b(um+|uh+|you know|i mean|like|okay so|actually,? no|wait,? no|sorry)\b", re.I)


def audio(path):
    raw = subprocess.run(["ffmpeg", "-loglevel", "error", "-i", str(path), "-ac", "1", "-ar", "16000", "-f", "f32le", "-"],
                         capture_output=True, check=True).stdout
    return np.frombuffer(raw, dtype=np.float32).copy()


def wer(ids, texts, man, ref):
    return 100 * jiwer.wer([NORM(OH.sub(" ", man[c][ref])) for c in ids], [NORM(OH.sub(" ", texts[c])) or "<empty>" for c in ids])


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--model", type=pathlib.Path, required=True)
    ap.add_argument("--n", type=int, default=30)
    a = ap.parse_args()
    man = {r["clip_id"]: r for r in csv.DictReader((CLIPS / "manifest.csv").open(encoding="utf-8"))}
    ids = [c for c, m in man.items() if float(m["audio_duration_s"]) <= 30][:a.n]
    dev = "mps" if torch.backends.mps.is_available() else "cpu"
    t0 = time.monotonic()
    proc = AutoProcessor.from_pretrained(a.model)
    model = AutoModelForImageTextToText.from_pretrained(a.model, dtype=torch.bfloat16).to(dev).eval()
    print(f"loaded on {dev} in {time.monotonic() - t0:.1f} s; transformers via uv, torch {torch.__version__}")
    out = HERE / "runs/hf_check"
    out.mkdir(parents=True, exist_ok=True)
    for mode in ("plain", "onepass"):
        system, user = MODES[mode]
        texts, secs = {}, []
        for c in ids:
            msgs = ([{"role": "system", "content": [{"type": "text", "text": system}]}] if system else []) + [
                {"role": "user", "content": [{"type": "text", "text": user}, {"type": "audio", "audio": audio(CLIPS / man[c]["file"])}]}]
            inp = proc.apply_chat_template(msgs, add_generation_prompt=True, tokenize=True, return_dict=True,
                                           return_tensors="pt", enable_thinking=False).to(dev)
            if "input_features" in inp:
                inp["input_features"] = inp["input_features"].to(torch.bfloat16)
            t = time.monotonic()
            with torch.inference_mode():
                g = model.generate(**inp, max_new_tokens=400, do_sample=False)
            secs.append(time.monotonic() - t)
            texts[c] = proc.decode(g[0][inp["input_ids"].shape[-1]:], skip_special_tokens=True).strip()
        with (out / f"results_{mode}.csv").open("w", newline="", encoding="utf-8") as fh:
            w = csv.DictWriter(fh, ["clip_id", "status", "transcript"])
            w.writeheader()
            w.writerows({"clip_id": c, "status": "ok", "transcript": texts[c]} for c in ids)
        lc = {r["clip_id"]: r["transcript"] for r in csv.DictReader((HERE / f"runs/gemma4-e2b__{mode}/results.csv").open(encoding="utf-8"))}
        same = sum(1 for c in ids if texts[c] == lc[c])
        print(f"{mode}: transformers bf16  WER spoken {wer(ids, texts, man, 'spoken_text'):.2f}  intended "
              f"{wer(ids, texts, man, 'intended_text'):.2f}  fillers {sum(len(FILL.findall(texts[c])) for c in ids)}  "
              f"median {np.median(secs):.2f} s")
        print(f"{mode}: llama.cpp Q4_K_M   WER spoken {wer(ids, lc, man, 'spoken_text'):.2f}  intended "
              f"{wer(ids, lc, man, 'intended_text'):.2f}  fillers {sum(len(FILL.findall(lc[c])) for c in ids)}  "
              f"identical text {same}/{len(ids)}")


if __name__ == "__main__":
    main()
