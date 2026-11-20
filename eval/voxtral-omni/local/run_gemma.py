"""Gemma 4 E2B (Q4_K_M) + prompt v3 on-device cleanup of the hosted engines' saved transcripts, exactly
as eval/local-cleanup does it (clean.py, guard.py, prompts.py and split.csv here are copied unchanged
from eval/local-cleanup on branch eval/local-cleanup; the ASR screen, PR #331, ran its on-device
engines the same way with /tmp/vocet-asr-screen/cleanup_screen.py). No network, no key.

    python3 -I eval/voxtral-omni/local/run_gemma.py nemotron voxtral_mini voxtral_small
Writes eval/voxtral-omni/results-local/<engine>__gemma_v3/{results.csv, meta.json, server.log}.
"""
import pathlib
import sys

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import clean  # noqa: E402

INPUT = HERE.parent / "input"
SOURCES = {"nemotron": "nvidia__nemotron-3.5-transcribe", "voxtral_mini": "mistralai__voxtral-mini-3b-2507",
           "voxtral_small": "mistralai__voxtral-small-24b-2507-stt"}
MODEL = pathlib.Path.home() / "vocet-cleanup-models" / "gemma-4-E2B-it-Q4_K_M.gguf"

for name in sys.argv[1:] or list(SOURCES):
    clean.SOURCES[name] = INPUT / SOURCES[name] / "harness_results.csv"
    out = HERE.parent / "results-local" / f"{name}__gemma_v3"
    if (out / "meta.json").exists():
        print(f"{name}: done already")
        continue
    sys.argv = ["clean", "--source", name, "--model", str(MODEL), "--name", "gemma4-e2b", "--prompt", "v3",
                "--out", str(out)]
    clean.main()
