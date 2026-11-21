# Evaluation harness

How we measure speech engines and cleanup prompts, on Voice Arena's public Dictation Bench (151 clips,
7 domains). Python, run with `uv`. Not shipped; not part of the app.

**Metric.** ZER = share of dictations that need no edit; AMEND = pooled characters edited / characters (both from
Voice Arena's [amend-va](https://pypi.org/project/amend-va/) 0.2.1, prompt `v1`). Unofficial: single judges
(Claude Opus 5.5, GPT-6 Astra via Codex, Gemini via Antigravity), public set only. **tune / check** is a fixed
split by speaker, `sha256(speaker_id)` with an even first hex digit = tune (`split.csv`): edit prompts on tune,
confirm on check.

**Data licence.** The clips, reference texts and the engines' transcripts of them are CC BY-NC 4.0 (Voice Arena).
They are fetched locally into `eval/clips/` (git-ignored) and never committed; neither are judge caches or run
output (`eval/work/`, `eval/.judge-cache/`). `test_hygiene.py` fails if audio or a key-shaped string is tracked.
Keys come only from environment variables (`OPENROUTER_API_KEY`) and are never printed or stored.

## The loop

```sh
uv run eval/fetch_dataset.py eval/clips                       # 1. clips + references (pinned revision, sha256 checked)
OPENROUTER_API_KEY=... uv run eval/transcribe_openrouter.py \
    --model openai/gpt-4o-mini-transcribe                     # 2a. hosted speech -> eval/work/asr/<model>/harness_results.csv
python3 eval/local_transcribe.py --base-url http://127.0.0.1:8080/v1 --model m --label l   # 2b. on-device engine
OPENROUTER_API_KEY=... python3 eval/offline_cleanup.py --provider openrouter --model z-ai/glm-5.3-flash \
    --prompt eval/prompts/v4.txt --input <harness_results.csv> --label glm_v4    # 3. cleanup + the app's guard
uv run eval/judge.py --judge claude <results.csv> eval/clips/manifest.csv -o scored.csv --label l   # 4. judge
python3 eval/report.py <run folder> [--baseline <run folder>] # 5. ZER, AMEND, tune/check, wins/losses, $/1000, p50/p95
```

| File | What it does |
| --- | --- |
| `fetch_dataset.py` | the public set, audio and references, into the git-ignored `clips/` |
| `transcribe_openrouter.py` | hosted speech models on OpenRouter; retries 429/5xx, spend cap (`--max-usd`), key only in the header |
| `local_transcribe.py` | an on-device engine behind a loopback OpenAI-style endpoint (llama.cpp, whisper.cpp, ...) |
| `offline_cleanup.py`, `guard.py` | cleanup of saved raw text through `codex`, `groq` or `openrouter`, with the app's guard (a port of the Rust one); spend cap `--max-cost`, 429 retry |
| `judge.py`, `judges.py` | one interface, three judges (`claude`, `codex`, `gemini`), one cache per judge and model in `.judge-cache/`; `GEMINI_JUDGE_MODEL` picks the Gemini model (default a Flash model, not Argon) |
| `report.py` | one summary table (`--format csv`/`json` too); `summary.csv` was made with it |
| `regress.sh` | the prompt/model regression check below |
| `evalcommon.py` | the split rule, and where the app's cleanup prompt is |
| `prompts/` | `v1-blurt-smart.txt`, `v3.txt`, `v4.txt` (v4 is the app's cleanup prompt) |
| `app_bench.py`, `app_bench.sh` | BENCH1: the clips through whole dictation apps (Vocet and others) in a Lume VM, black box: hotkey, BlackHole microphone, TextEdit; `reports/bench1.md` |

**The app's prompt.** `regress.sh` and `evalcommon.app_prompt_path()` read the prompt the app ships so eval and
app can't drift: `crates/core/prompts/cleanup.txt` once Q2 puts it there, until then `prompts/v4.txt` (the same
text, byte for byte, sha `c530a8a410f1`). `EVAL_APP_PROMPT=<file>` tries a candidate before it is in the app.

## Regression check

`eval/regress.sh` runs the app's current cleanup prompt (default model `z-ai/glm-5.3-flash`) on a fixed list of
40 clips (`regress/clips.txt`: per domain and half, the clips the baseline got wrong first, so it's sensitive;
both halves, all 7 domains), judges the result and compares ZER with the stored baseline
(`regress/baseline.csv`, `baseline.json`: gpt-4o-mini-transcribe + GLM 5.3 Flash + prompt v4, 57.5% on these
clips, 88.1% on all 151). It exits 1 if ZER drops by more than 5 points. **Required before merging any prompt or
model-default change**; costs about $0.03 plus judge calls, so not in the gate. `--dry-run` shows the plan and the
estimate without network. The first run downloads the dataset and transcribes the 40 clips once; the raw text is
kept in `regress/work/` (git-ignored). The baseline was judged on one speech run, so a re-transcription can move
ZER by a clip or so.

## Tests

`eval/test.sh` runs the offline tests with the standard library only (the gate runs it, about 15 s); `uv run pytest
eval` runs them with every dependency. Nothing needs the network, a key or the dataset.

## Results

`summary.csv` holds one row per run and judge (ZER, AMEND, tune and check ZER, $/1,000 and key-up-to-paste p50/p95
where the engine logged them): no per-clip data. The write-ups are in `reports/`, from the experiment PRs
(#323, #327, #330, #331, #332); some quote short fragments of clip text. Per-clip CSVs, judge caches and the VM
dictation runner stayed on those branches.

| Product default | Supported by | Numbers (Claude judge, 151 clips) |
| --- | --- | --- |
| Run cleanup after transcription | `reports/cleanup.md`, `summary.csv` group `cleanup` | Groq raw 60.3 → Groq + GLM 5.3 Flash 83.0 (+22.7); +15 to +25 on every engine |
| Cloud speech `openai/gpt-4o-mini-transcribe` + cleanup `z-ai/glm-5.3-flash` | `reports/voxtral-omni.md`, `summary.csv` run `gpt4o_mini__glm_v4` | ZER 88.1 (tune 90.8, check 85.3), $0.68 per 1,000 dictations |
| Fallback speech `mistralai/voxtral-mini-3b-2507` + GLM | `reports/voxtral-omni.md`, `reports/hosted-asr.md`, run `voxtral_mini__glm_v4` | 84.1, $0.47 per 1,000 |
| Why not Groq / on-device engines alone | `reports/voicearena.md`, `summary.csv` group `voicearena` | Groq raw 60.3, Parakeet 53.0, Qwen3 49.0; AssemblyAI Smart 86.8 |
| Which on-device speech model | `reports/asr-screen.md` | screen of local recognizers; numbers in the report |
| Keep the guard on cleanup replies, but know its price | `reports/cleanup.md` | it stops dropped sentences, and refuses some good fixes: `sol_v4` 82.1 with it, 84.8 without (`sol_v4_noguard`) |

The judges agree closely but not exactly (Claude and Codex differ by a few ZER points per run in `summary.csv`).
Compare runs only under the same judge.
