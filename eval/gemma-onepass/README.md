# Gemma 4 one-pass transcribe + cleanup: eval

An experiment, not product code and not for `main`. Can one on-device model take the audio and give
back cleaned text in a single call, instead of Parakeet (speech to text) followed by a small text model
(cleanup)? It runs Gemma 4 E2B (and E4B) with audio input over the Voice Arena public set (151 clips,
PR #323) on the owner's Mac, and scores the text with the same two judges as the other evals. Results
and the recommendation: [REPORT.md](REPORT.md).

No VM, no computer use and no keys. Model inference runs locally. The subscription CLI judges receive benchmark text.

## Files

| File | What it does |
| --- | --- |
| `prompts_onepass.py` | `plain`: Google's documented Gemma 4 speech-recognition prompt; `onepass`: the local-cleanup eval's prompt v3 with its frame and examples reworded for audio (rules unchanged) |
| `onepass.py` | runs one model in one mode over the clips through `llama-server` (audio in): splits clips over 30 s at silence, times each clip, records load time and peak memory |
| `apply_guard.py` | the local-cleanup guard on a one-pass run, against the same model's plain text (fallback to it) |
| `judge.sh` | both judges over run folders (Claude Opus 5.5 `score_claude_cli.py`, Codex GPT-6 Astra `score_codex_cli.py`), one shared answer cache per judge |
| `summarize.py` | the tables: both judges, tune / check halves, clips over and under 30 s, WER, paired wins / losses / ties against the two baselines (`summary.json`, `tables.md`) |
| `audit.py` | mechanical screen for dropped / invented content and where it happens (clip end, split point) (`audit.csv`) |
| `vendor/` | `guard.py` and `prompts.py` copied unchanged from the eval/local-cleanup worktree on 2026-11-19 (not yet pushed there) |
| `split.csv` | tune / check halves by speaker, from `origin/eval/local-cleanup` |
| `baselines/` | Parakeet raw and Parakeet + Gemma 4 E2B / E4B text cleanup (v3, guarded): results and both judges' scores, copied from the local-cleanup eval |
| `runs/<model>__<mode>[__guard]/` | `results.csv` (pasted text, the part replies of split clips, seconds), `meta.json` (files, sha256, load time, memory, latency), `server.log`, `scored.csv` (Claude), `scored_codex.csv` (Codex) |

The committed CSVs hold the models' text and the judges' corrections, never the dataset's reference
texts (CC BY-NC 4.0); join on `clip_id` with `../voicearena/clips/manifest.csv` after
`../voicearena/bench.sh fetch`.

## Reproducing

```sh
brew install llama.cpp ffmpeg                # llama.cpp 1.0.0 (build 12359) was used; it has Gemma 4 audio (mtmd "gemma4a")
eval/voicearena/bench.sh fetch               # the 151 clips + manifest
# models live outside the repo; check each sha256 against the Hugging Face LFS hash (each run records them in meta.json)
#   ~/vocet-cleanup-models/gemma-4-E2B-it-Q4_K_M.gguf     unsloth/gemma-4-E2B-it-GGUF
#   ~/vocet-onepass-models/e2b/mmproj-F16.gguf            unsloth/gemma-4-E2B-it-GGUF (audio + vision projector)
#   (E4B: the same names from unsloth/gemma-4-E4B-it-GGUF)
cd eval/gemma-onepass
M=~/vocet-cleanup-models/gemma-4-E2B-it-Q4_K_M.gguf P=~/vocet-onepass-models/e2b/mmproj-F16.gguf
uv run onepass.py --model $M --mmproj $P --name gemma4-e2b --mode plain
uv run onepass.py --model $M --mmproj $P --name gemma4-e2b --mode onepass
python3 -I apply_guard.py runs/gemma4-e2b__plain runs/gemma4-e2b__onepass
./judge.sh runs/gemma4-e2b__plain runs/gemma4-e2b__onepass runs/gemma4-e2b__onepass__guard   # claude -p and codex exec, signed in
uv run summarize.py --md tables.md --json summary.json
uv run audit.py
```

`llama-mtmd-cli` can't apply Gemma 4's chat template without `--jinja` support, so the runs use
`llama-server --jinja` and the OpenAI-style `input_audio` content part. Text goes before the audio in the
user turn (Google's examples do the same); with the audio first, E2B answered a dictated request instead of
transcribing it.
