# Voxtral + cleanup matrix, audio-LLM one-pass, closed-model comparison

Experiment, **do not merge** (draft PR #332). Voice Arena public set: 151 clips, CC BY-NC. Scores are
**unofficial and from the public set only**. ZER is the share of drafts the judge would not edit (amend-va 0.2.1,
prompt v1, fingerprint 8ed04caf4663). Judges: **Claude** Opus 5.5 (max), **Codex** GPT-6 Astra (max),
and, on the headline rows, **Gemini** 4 Argon (High) through Antigravity. Expect about ±3–4 points of judge noise
between runs and a ±8 point interval on 151 clips, so treat differences under ~4 points as ties.
The "check" half is the fixed held-out speaker split (`eval/groq-cleanup/split.csv`, 75 clips).

## Answers

**A. Does Voxtral + the best cleanup get close to Nemotron + GPT-6.2-sol? Yes. It ties.**

| | Claude / Codex ZER | check half | paired vs Nemotron+Sol (Claude W/L/T) | USD per 1,000 | median s |
|---|---|---|---|---:|---:|
| Nemotron 3.5 (hosted) + Sol (reference) | 84.8 / 84.1 (Gemini 85.4, 2-of-3 84.8) | 81.3 / 78.7 | – | 2.37 | 2.4 |
| **Voxtral Mini 3B on-device + Sol** | **84.8 / 83.4** (Gemini 85.4, 2-of-3 85.4) | 82.7 / 77.3 | 6 / 6 / 139 | 1.84 | 3.7 |
| Voxtral Mini 3B hosted + Sol | 84.1 / 82.1 | 81.3 / 76.0 | 8 / 9 / 134 | 2.21 | 2.5 |
| Voxtral Small 24B hosted + Sol | 86.1 / 84.1 | 85.3 / 80.0 | 8 / 6 / 137 | 2.75 | 3.6 |

So one ~3 GB model (Voxtral Mini 3B) can be both the on-device engine and the hosted engine. With
Sol on top, it lands within a point of Nemotron + Sol under all three judges (2-of-3 majority 85.4 vs 84.8), with 6 wins and 6 losses
on the same clips.

**B. Can a large audio-input model transcribe and clean up in one pass? Not well enough.** The best
is MiMo-V2.5 (MIT): 69.5 / 68.9 one-pass and 70.2 / 68.9 through the guard, at $0.24 per 1,000
dictations and 2.7 s median. That beats raw Groq Whisper (60.3 / 57.0) but sits below every two-pass cell in the
matrix (71.5–86.1 Claude) and 14–16 points below the Sol pairings. Against Nemotron + Sol it wins 6 clips and loses 29. Nemotron 3
Nano Omni reaches 64.9 (free route). Inkling Small ignores the clean-up instruction: 99 of 151
replies are identical to its plain transcript. Voxtral Small chat answered only 108 of 151 clips
(see Caveats).

## Recommendations (best three per deployment)

Cost is USD per 1,000 dictations at OpenRouter list prices, the API costs actually billed in this
run (on-device = $0). Latency is the median of the engine and clean-up seconds summed per clip.
On-device seconds come from the ASR screen's M-series Mac runs (PR #331).

| Deployment | 1st | 2nd | 3rd |
|---|---|---|---|
| **Fully on-device** | **OrukQwen-ASR 1.7B + Gemma 4 E2B v3**: 76.8 / 74.2, $0, 1.8 s; smaller and faster | Voxtral Mini 3B + Gemma 4 E2B v3: 76.8 / 75.5, $0, 4.9 s | Voxtral Mini 4B Realtime + Gemma 4 E2B v3: 74.2 / 72.8 (PR #331) |
| **Hybrid** (on-device ASR, cloud text clean-up) | **Voxtral Mini 3B + GPT-6.2-sol**: 84.8 / 83.4 / Gemini 85.4, $1.84, 3.7 s; ties the best hosted pair | Voxtral Mini 3B + GPT-6-luna: 80.8 / 78.8, **$0.12**, 3.7 s | Voxtral Mini 3B + GLM-5.3 Flash: 79.5 / 78.1, $0.10, 4.3 s (open model) |
| **Hosted, cheapest** (good quality under $1) | **Voxtral Mini 3B + GLM-5.3 Flash**: 84.1 / 81.5, **$0.47**, 3.1 s | Nemotron 3.5 + GLM-5.3 Flash ("combination 8"): 82.8 / 82.1, $0.55, 3.2 s | Nemotron 3.5 + GPT-6-luna: 82.8 / 82.8, $0.56, 2.8 s |
| **Hosted, best** | **Voxtral Small 24B + GPT-6.2-sol**: 86.1 / 84.1, $2.75, 3.6 s | Nemotron 3.5 + GPT-6.2-sol: 84.8 / 84.1 / Gemini 85.4, $2.37, 2.4 s | Voxtral Mini 3B + GPT-6.2-sol: 84.1 / 82.1, $2.21, 2.5 s |

Notes on the picks:

- **Hybrid looks strongest.** On-device Voxtral Mini + Sol matches the best hosted pairing.
  Audio never leaves the Mac and only text goes out. It costs 22% less than Nemotron + Sol,
  because Sol is the only paid call. With Luna it costs 15× less again for ~4 points.
- **Hosted, best.** The three picks are within noise of each other, and AssemblyAI Smart (88.1 / 86.8,
  closed) still leads. Voxtral Small + Sol has the best check half (85.3 / 80.0). Nemotron + Sol is the fastest
  and, with Voxtral Mini on-device + Sol, one of the two pipelines all three judges scored.
- **Hosted, cheapest.** The Voxtral Mini hosted transcription is ~$0.38 per 1,000 dictations against
  Nemotron's ~$0.44. GLM Flash and Luna are 4–5× cheaper than Sol for 0–3 points. MiMo one-pass is cheaper
  still ($0.24) but 13+ points worse.
- **On-device.** The two ASR engines tie with Gemma. #331 recommends OrukQwen when clean-up is on, since
  it saves 1.66 GB and is faster. Voxtral Mini is the better raw engine (64.9 vs 58.9 Claude) and the
  better input for a cloud clean-up (Sol: 84.8 vs 80.8).
- gpt-oss-120b is the weakest clean-up everywhere (71–79). Gemma 4 E2B on-device is a reasonable
  free clean-up even for hosted engines (76.8–79.5).

## Reference rows (earlier PRs, same set and judges)

| Pipeline | Claude / Codex ZER | Source |
|---|---|---|
| AssemblyAI Universal Smart (closed) | 88.1 / 86.8 | PR #322 |
| Nemotron 3.5 + GPT-6.2-sol low (this run's baseline, reused) | 84.8 / 84.1 (Gemini 85.4) | PR #327 |
| Groq Whisper + GPT-6.2-sol | 82.1 / 80.8 | PR #327 |
| OrukQwen-ASR 1.7B + Gemma 4 E2B v3 (on-device) | 76.8 / 74.2 | PR #331 |
| Voxtral Mini 3B on-device, raw | 64.9 / 64.2 | PR #331 |
| Groq Whisper raw (today's app) | 60.3 / 57.0 | PR #322 |

## Part A: engine × clean-up matrix

Each cell shows **Claude** / Codex ZER on all 151 clips (G = Gemini where judged), the check half
(Claude / Codex), the 2-of-3 majority where all three judges exist, USD per 1,000 dictations, and the
median of engine + clean-up seconds. Clean-ups use prompt v4 and the app's guard. They go through OpenRouter,
except Gemma 4 E2B v3, which runs locally via llama.cpp. On-device engine seconds were measured on an M-series Mac.

| Engine \ cleanup | GPT-6.2-sol low | GPT-6-luna | GLM-5.3 Flash | gpt-oss-120b | Gemma 4 E2B v3 (on-device) |
|---|---|---|---|---|---|
| Nemotron 3.5 Transcribe (hosted) | **84.8** / 84.1 / 85.4G<br>check 81.3 / 78.7<br>2-of-3 84.8<br>$2.37/1k · 2.4 s | **82.8** / 82.8<br>check 80.0 / 78.7<br>$0.56/1k · 2.8 s | **82.8** / 82.1<br>check 78.7 / 76.0<br>$0.55/1k · 3.2 s | **78.8** / 78.8<br>check 72.0 / 72.0<br>$0.52/1k · 2.1 s | **79.5** / 78.8<br>check 73.3 / 72.0<br>$0.44/1k · 1.8 s |
| Voxtral Mini 3B (hosted) | **84.1** / 82.1<br>check 81.3 / 76.0<br>$2.21/1k · 2.5 s | **82.1** / 80.1<br>check 81.3 / 76.0<br>$0.50/1k · 2.6 s | **84.1** / 81.5<br>check 82.7 / 77.3<br>$0.47/1k · 3.1 s | **74.2** / 73.5<br>check 69.3 / 66.7<br>$0.46/1k · 2.0 s | **76.8** / 75.5<br>check 76.0 / 72.0<br>$0.38/1k · 1.5 s |
| Voxtral Mini 3B (on-device) | **84.8** / 83.4 / 85.4G<br>check 82.7 / 77.3<br>2-of-3 85.4<br>$1.84/1k · 3.7 s | **80.8** / 78.8<br>check 80.0 / 74.7<br>$0.12/1k · 3.7 s | **79.5** / 78.1<br>check 78.7 / 74.7<br>$0.10/1k · 4.3 s | **71.5** / 72.2<br>check 70.7 / 68.0<br>$0.08/1k · 3.0 s | **76.8** / 75.5<br>check 77.3 / 73.3<br>$0/1k · 4.9 s (PR #331) |
| OrukQwen-ASR 1.7B (on-device) | **80.8** / 78.1<br>check 78.7 / 74.7<br>$2.07/1k · 2.1 s | **76.8** / 75.5<br>check 73.3 / 72.0<br>$0.13/1k · 2.8 s | **74.8** / 73.5<br>check 76.0 / 73.3<br>$0.11/1k · 2.7 s | **72.2** / 70.9<br>check 70.7 / 69.3<br>$0.08/1k · 1.4 s | **76.8** / 74.2<br>check 76.0 / 72.0<br>$0/1k · 1.8 s (PR #331) |
| Voxtral Small 24B (hosted) | **86.1** / 84.1<br>check 85.3 / 80.0<br>$2.75/1k · 3.6 s | **82.8** / 80.1<br>check 85.3 / 80.0<br>$1.25/1k · 3.8 s | **82.8** / 80.1<br>check 85.3 / 80.0<br>$1.23/1k · 4.1 s | **76.8** / 73.5<br>check 77.3 / 70.7<br>$1.24/1k · 2.9 s | **78.1** / 76.2<br>check 82.7 / 77.3<br>$1.13/1k · 2.6 s |

## Part B: audio-input chat models (OpenRouter), one request per clip

"one-pass + guard" pastes the model's own plain transcript whenever the app's guard rejects the
one-pass reply. Its cost and latency count both calls (run in parallel). Claude judged every row. Codex
judged MiMo, the best one-pass model; it needed 0 new calls, all cached. * = judged on fewer than 151 clips.

| Model (open weights unless noted) | Mode | Claude ZER tune / check / **all** | Codex ZER all | Gemini ZER all | WER spoken % | s median / p90 | USD/1k | guard fallbacks |
|---|---|---|---|---|---:|---|---:|---|
| MiMo-V2.5 (MIT) | plain | 63.2 / 72.0 / **67.5** | 66.2 | – | 1.8 | 3.19 / 6.37 | 0.19 | – |
| MiMo-V2.5 (MIT) | one-pass | 68.4 / 70.7 / **69.5** | 68.9 | – | 1.8 | 2.65 / 6.10 | 0.24 | – |
| MiMo-V2.5 (MIT) | one-pass + guard | 68.4 / 72.0 / **70.2** | 68.9 | 71.3* | 1.8 | 3.92 / 7.17 | 0.43 | dropped_name 3, dropped_number 2 |
| Nemotron 3 Nano Omni 30B-A3B (free route) | plain | 56.6 / 58.7 / **57.6** | – | – | 3.9 | 1.73 / 3.58 | 0.00 | – |
| Nemotron 3 Nano Omni 30B-A3B (free route) | one-pass | 61.8 / 68.0 / **64.9** | – | – | 5.0 | 1.76 / 3.20 | 0.00 | – |
| Nemotron 3 Nano Omni 30B-A3B (free route) | one-pass + guard | 61.8 / 66.7 / **64.2** | – | – | 4.9 | 1.96 / 3.85 | 0.00 | dropped_name 4, dropped_number 5 |
| Inkling Small (Apache-2.0) | plain | 59.2 / 62.7 / **60.9** | – | – | 2.9 | 1.03 / 1.56 | 0.33 | – |
| Inkling Small (Apache-2.0) | one-pass | 59.2 / 62.7 / **60.9** | – | – | 2.8 | 1.05 / 1.66 | 0.35 | – |
| Inkling Small (Apache-2.0) | one-pass + guard | 59.2 / 62.7 / **60.9** | – | – | 2.8 | 1.17 / 1.78 | 0.68 | dropped_link 1, dropped_name 3, dropped_number 1 |
| Voxtral Small 24B chat (Apache-2.0) | plain | 34.2 / 40.0 / **37.1** | – | – | 43.9 | 2.03 / 3.78 | 1.74 | – |
| Voxtral Small 24B chat (Apache-2.0) | one-pass | 40.8 / 54.7 / **47.7** | – | – | 42.8 | 1.84 / 3.28 | 1.72 | – |
| Voxtral Small 24B chat (Apache-2.0) | one-pass + guard | 39.5 / 49.3 / **44.4** | – | – | 44.4 | 2.24 / 3.89 | 3.47 | dropped_name 3, dropped_number 10, missing 43, too_long 3 |

## Closed models (Gemini, OpenAI): results (2026-11-20)

Rows labelled **closed model**. Claude Opus 5.5 judge only (amend-va 0.2.1 v1); differences of a few
points are within noise (±8 interval on 151 clips), so read the paired wins/losses.

**Speech engine alone** (OpenAI and Gemini 3.5 Transcribe via OpenRouter `audio/transcriptions`,
`transcribe_openrouter.py` into `results-or-closed/`; Flash-Lite direct via `closed_asr.py`):

| Engine (closed model) | ZER Claude | vs Groq W/L | vs Nemotron W/L | API s median | USD/1k |
|---|---:|---|---|---:|---:|
| OpenAI gpt-4o-transcribe | **74.8** | 26/4 | 19/6 | 1.52 | 1.11 |
| OpenAI gpt-4o-mini-transcribe | 72.8 | 24/5 | 19/9 | 1.27 | 0.56 |
| OpenAI gpt-transcribe | 68.2 | 19/7 | 11/8 | 1.28 | 1.73 |
| Gemini 3.5 Transcribe | 66.9 | 21/11 | 12/11 | 2.91 | 1.84 |
| Gemini 3.5 Flash-Lite plain / one-pass v4 / one-pass+guard | 58.9 / 62.9 / 58.9 | – | – | – | ~0.9 |
| *Nemotron 3.5 (open, reference)* | *66.2* | 22/13 | – | 1.27 | 0.44 |
| *Groq Whisper turbo (app)* | *60.3* | – | – | 0.63 key-up | – |

**With clean-up (prompt v4 + guard, OpenRouter):**

| Engine + clean-up | ZER Claude | vs Voxtral Small+Sol W/L | clean-up s median | USD/1k total |
|---|---:|---|---:|---:|
| gpt-4o-mini-transcribe + GPT-6.2 Sol | **88.7** | 8/4 | 1.50 | 2.48 |
| gpt-4o-mini-transcribe + GLM Flash | **88.1** | 10/7 | 0.99 | **0.68** |
| gpt-4o-mini-transcribe + GPT-6-luna | 87.4 | 11/9 | 1.28 | 0.68 |
| gpt-4o-transcribe + GPT-6-luna | 87.4 | 10/8 | 1.19 | 1.22 |
| gpt-4o-transcribe + GPT-6.2 Sol | 86.8 | 7/6 | 1.51 | 3.30 |
| gpt-4o-transcribe + GLM Flash | 85.4 | 9/10 | 0.93 | 1.23 |
| *Voxtral Small + Sol (best open)* | *86.1* | – | – | 2.75 |
| *AssemblyAI Smart* | *88.1* | – | – | ~3.90 |

**Caveat that decides it: OpenAI deprecated gpt-4o-transcribe and gpt-4o-mini-transcribe (announced
2026-08); they shut down 2027-02-26.** OpenAI's named replacements are gpt-transcribe (68.2 raw here)
and gpt-live-transcribe (not on OpenRouter; untested, needs a working OpenAI key). Not shippable as a
default; useful as evidence of what a strong engine + cheap clean-up reaches. Gemini 3.8 Flash and
most Gemini Transcribe clips hit Google 429s (free-tier quota) on the direct API; the OpenAI direct
key returned 401, so OpenAI ran through OpenRouter. Total spend: Gemini direct $0.23, OpenRouter
transcription $0.79, clean-up ~$0.69.

## Judges

- **Coverage:** Claude covers every Part A and Part B row. Codex covers every Part A row and MiMo.
  Gemini covers Nemotron + Sol (85.4), Voxtral Mini on-device + Sol (85.4).
- **Agreement on Nemotron + Sol** (zero-edit verdicts, 151 clips):
  - Claude–Gemini 99.3%
  - Claude–Codex 98.0%
  - Codex–Gemini 97.4%
  - The 2-of-3 majority is 84.8.
- **Gemini route:** Gemini 4 Argon runs through `agy -p --model gemini-4-argon-high --sandbox --mode plan`
  on the owner's Antigravity subscription. It uses the same prompt, schema and parsing as the other two judges
  (`eval/voicearena/score_gemini_cli.py`). Antigravity adds its own agent system prompt (~10k tokens) that
  can't be replaced. It is slow: a median of ~2.5 minutes per verdict, about 4–5 verdicts a minute with 8 workers.
  So it judged only the headline rows. The owner's Antigravity quota then ran out ("Individual quota reached", resets in ~7 days). Judging stopped there, leaving MiMo one-pass + guard at 101 of 151 clips and Voxtral Mini hosted + GLM Flash at 79 of 151 (from shared drafts). Partial rows are marked *.
- **Judge calls in this session:** Claude ~906 new verdicts (Part B 4 models × 3 rows, Part A's 22 un-Claude-judged cells, plus the smoke test). Codex 0 new calls (MiMo's verdicts were all cached; the previous worker's Codex pass judged Part A and MiMo). Gemini ~350 (333 verdicts kept; stopped at the quota message) calls.

## Caveats

- **Voxtral Small chat (Part B)** answered 108 of 151 clips. The last 41 clips by id hit HTTP 429 and the run's
  sub-cap stop, so its all-151 ZER counts them as failures. On the 108 answered clips, Claude ZER is
  51.9 plain, 66.7 one-pass and 62.0 one-pass + guard. That is still below MiMo.
- **Harm screens** (short / long / numbers against the intended text) are about the same for every
  two-pass row, including the baseline (0 short, 6–10 long, 28–33 numbers). The numbers screen counts
  spoken-vs-written number forms, so it flags a lot of acceptable output. Guard fallbacks (dropped
  name / number) are 1–9 per run.
- **On-device latencies** come from #331's Mac runs; API latencies come from one evening's OpenRouter
  routing. Providers varied per request (listed in `summarize.py --json`).
- **Gemma column for the on-device engines** comes from PR #331's guarded Gemma 4 E2B v3 runs (copied into
  `input/ondevice_*`). The hosted engines' Gemma cells were run here with the same `local/` code.
- Rows where only one judge exists should not be ranked against three-judge rows on fine margins.

## Reproduce

- `run_all.sh` is the owner's OpenRouter command for parts A and B ($2.03 of the $3.00 ledger used).
- `run_all_closed.sh` is the owner's command for the closed models (Gemini / OpenAI keys, separate $3 caps,
  plus OpenRouter clean-up of OpenAI's transcripts).
- `score.sh <run> claude|codex|gemini` judges one run with a shared cache. `judge_claude.sh` holds the Claude lanes
  used here.
- `uv run eval/voxtral-omni/summarize.py` prints the full table (below). `--missing` lists unjudged prompts.
- Tests: `python3 -m unittest eval/voxtral-omni/test_runners.py eval/voxtral-omni/test_closed_asr.py`,
  against fake servers with no keys.

<details><summary>Full table (summarize.py)</summary>

| Pipeline | Claude ZER tune / check / **all** | Codex ZER tune / check / **all** | Gemini ZER all (n) | 2-of-3 ZER check / **all** | AMEND C / X | vs Nemotron+Sol W/L/T (Claude; Codex) | WER spoken % | s median / p90 | USD/1k | fallbacks | short / long / numbers |
|---|---|---|---|---|---|---|---:|---|---:|---|---|
| Nemotron 3.5 (hosted) + GPT-6.2-sol | 88.2 / 81.3 / **84.8** | 89.5 / 78.7 / **84.1** | 85.4 (151) | 81.3 / **84.8** | 0.34 / 0.41 | 0/0/151; 0/0/151 | 3.9 | 2.44 / 4.74 | 2.37 | dropped_name 4 | 0 / 9 / 32 |
| inkling_small (one-pass) | 59.2 / 62.7 / **60.9** | – | – | – | 1.24 / – | 6/42/103; – | 2.8 | 1.05 / 1.66 | 0.35 | – | 0 / 15 / 30 |
| inkling_small (one-pass + guard) | 59.2 / 62.7 / **60.9** | – | – | – | 1.24 / – | 6/42/103; – | 2.8 | 1.17 / 1.78 | 0.68 | dropped_link 1, dropped_name 3, dropped_number 1 | 0 / 16 / 29 |
| inkling_small (plain) | 59.2 / 62.7 / **60.9** | – | – | – | 1.25 / – | 6/42/103; – | 2.9 | 1.03 / 1.56 | 0.33 | – | 0 / 17 / 29 |
| mimo_v25 (one-pass) | 68.4 / 70.7 / **69.5** | 67.1 / 70.7 / **68.9** | – | – | 0.99 / 1.09 | 6/29/116; 5/28/118 | 1.8 | 2.65 / 6.10 | 0.24 | – | 0 / 15 / 28 |
| mimo_v25 (one-pass + guard) | 68.4 / 72.0 / **70.2** | 67.1 / 70.7 / **68.9** | 71.3 (101) | – | 0.98 / 1.08 | 6/28/117; 5/28/118 | 1.8 | 3.92 / 7.17 | 0.43 | dropped_name 3, dropped_number 2 | 0 / 15 / 28 |
| mimo_v25 (plain) | 63.2 / 72.0 / **67.5** | 63.2 / 69.3 / **66.2** | – | – | 1.11 / 1.23 | 6/32/113; 5/32/114 | 1.8 | 3.19 / 6.37 | 0.19 | – | 0 / 15 / 28 |
| Nemotron 3.5 (hosted) + GLM-5.3 Flash | 86.8 / 78.7 / **82.8** | 88.2 / 76.0 / **82.1** | – | – | 0.27 / 0.38 | 4/7/140; 4/7/140 | 4.2 | 3.22 / 6.05 | 0.55 | dropped_name 5 | 0 / 9 / 31 |
| Nemotron 3.5 (hosted) + GPT-6-luna | 85.5 / 80.0 / **82.8** | 86.8 / 78.7 / **82.8** | – | – | 0.29 / 0.32 | 2/5/144; 2/4/145 | 4.0 | 2.76 / 6.05 | 0.56 | dropped_name 5 | 0 / 7 / 31 |
| Nemotron 3.5 (hosted) + gpt-oss-120b | 85.5 / 72.0 / **78.8** | 85.5 / 72.0 / **78.8** | – | – | 0.37 / 0.49 | 3/12/136; 3/11/137 | 4.2 | 2.07 / 4.83 | 0.52 | dropped_name 2 | 0 / 8 / 32 |
| nemotron_omni (one-pass) | 61.8 / 68.0 / **64.9** | – | – | – | 1.88 / – | 5/35/111; – | 5.0 | 1.76 / 3.20 | 0.00 | – | 4 / 7 / 25 |
| nemotron_omni (one-pass + guard) | 61.8 / 66.7 / **64.2** | – | – | – | 2.00 / – | 5/36/110; – | 4.9 | 1.96 / 3.85 | 0.00 | dropped_name 4, dropped_number 5 | 4 / 8 / 28 |
| nemotron_omni (plain) | 56.6 / 58.7 / **57.6** | – | – | – | 2.03 / – | 4/45/102; – | 3.9 | 1.73 / 3.58 | 0.00 | – | 1 / 13 / 29 |
| OrukQwen-ASR 1.7B (on-device) + GLM-5.3 Flash | 73.7 / 76.0 / **74.8** | 73.7 / 73.3 / **73.5** | – | – | 0.51 / 0.66 | 8/23/120; 7/23/121 | 4.4 | 2.68 / 5.66 | 0.11 | dropped_name 8 | 0 / 10 / 32 |
| OrukQwen-ASR 1.7B (on-device) + GPT-6-luna | 80.3 / 73.3 / **76.8** | 78.9 / 72.0 / **75.5** | – | – | 0.44 / 0.57 | 9/21/121; 8/21/122 | 4.7 | 2.83 / 5.82 | 0.13 | dropped_name 9 | 0 / 9 / 33 |
| OrukQwen-ASR 1.7B (on-device) + gpt-oss-120b | 73.7 / 70.7 / **72.2** | 72.4 / 69.3 / **70.9** | – | – | 0.64 / 0.75 | 9/28/114; 8/28/115 | 4.6 | 1.40 / 3.02 | 0.08 | dropped_name 1 | 0 / 11 / 33 |
| OrukQwen-ASR 1.7B (on-device) + GPT-6.2-sol | 82.9 / 78.7 / **80.8** | 81.6 / 74.7 / **78.1** | – | – | 0.46 / 0.59 | 6/12/133; 5/14/132 | 4.4 | 2.12 / 3.63 | 2.07 | dropped_name 6 | 0 / 10 / 32 |
| Voxtral Mini 3B (hosted) + GLM-5.3 Flash | 85.5 / 82.7 / **84.1** | 85.5 / 77.3 / **81.5** | – | – | 0.29 / 0.32 | 11/12/128; 10/14/127 | 4.7 | 3.14 / 4.97 | 0.47 | dropped_name 5, dropped_number 1 | 0 / 10 / 28 |
| Voxtral Mini 3B (hosted) + GPT-6-luna | 82.9 / 81.3 / **82.1** | 84.2 / 76.0 / **80.1** | – | – | 0.28 / 0.31 | 9/13/129; 9/15/127 | 5.0 | 2.64 / 5.70 | 0.50 | dropped_name 3 | 0 / 10 / 28 |
| Voxtral Mini 3B (hosted) + gpt-oss-120b | 78.9 / 69.3 / **74.2** | 80.3 / 66.7 / **73.5** | – | – | 0.55 / 0.56 | 7/23/121; 7/23/121 | 5.2 | 1.97 / 5.07 | 0.46 | – | 0 / 9 / 28 |
| Voxtral Mini 3B (hosted) + GPT-6.2-sol | 86.8 / 81.3 / **84.1** | 88.2 / 76.0 / **82.1** | – | – | 0.29 / 0.32 | 8/9/134; 8/11/132 | 4.9 | 2.54 / 4.27 | 2.21 | dropped_name 5 | 0 / 10 / 28 |
| Voxtral Mini 3B (on-device) + GLM-5.3 Flash | 80.3 / 78.7 / **79.5** | 81.6 / 74.7 / **78.1** | – | – | 0.39 / 0.39 | 10/18/123; 9/18/124 | 5.0 | 4.32 / 7.67 | 0.10 | dropped_name 3, dropped_number 1 | 0 / 8 / 28 |
| Voxtral Mini 3B (on-device) + GPT-6-luna | 81.6 / 80.0 / **80.8** | 82.9 / 74.7 / **78.8** | – | – | 0.32 / 0.36 | 8/14/129; 8/16/127 | 5.4 | 3.73 / 7.41 | 0.12 | dropped_name 2 | 0 / 8 / 28 |
| Voxtral Mini 3B (on-device) + gpt-oss-120b | 72.4 / 70.7 / **71.5** | 76.3 / 68.0 / **72.2** | – | – | 1.02 / 0.95 | 8/28/115; 8/26/117 | 5.8 | 2.95 / 5.21 | 0.08 | dropped_link 1 | 0 / 8 / 28 |
| Voxtral Mini 3B (on-device) + GPT-6.2-sol | 86.8 / 82.7 / **84.8** | 89.5 / 77.3 / **83.4** | 85.4 (151) | 82.7 / **85.4** | 0.27 / 0.29 | 6/6/139; 6/7/138 | 5.3 | 3.74 / 6.25 | 1.84 | dropped_name 3 | 0 / 9 / 28 |
| Voxtral Small 24B (hosted) + GLM-5.3 Flash | 80.3 / 85.3 / **82.8** | 80.3 / 80.0 / **80.1** | – | – | 0.37 / 0.43 | 11/14/126; 9/15/127 | 4.7 | 4.11 / 7.66 | 1.23 | dropped_name 1, dropped_number 2 | 0 / 9 / 31 |
| Voxtral Small 24B (hosted) + GPT-6-luna | 80.3 / 85.3 / **82.8** | 80.3 / 80.0 / **80.1** | – | – | 0.29 / 0.34 | 10/13/128; 8/14/129 | 5.0 | 3.84 / 7.32 | 1.25 | dropped_name 1 | 0 / 7 / 31 |
| Voxtral Small 24B (hosted) + gpt-oss-120b | 76.3 / 77.3 / **76.8** | 76.3 / 70.7 / **73.5** | – | – | 0.39 / 0.47 | 9/21/121; 7/23/121 | 5.0 | 2.92 / 5.88 | 1.24 | – | 0 / 7 / 31 |
| Voxtral Small 24B (hosted) + GPT-6.2-sol | 86.8 / 85.3 / **86.1** | 88.2 / 80.0 / **84.1** | – | – | 0.29 / 0.32 | 8/6/137; 6/6/139 | 4.7 | 3.57 / 5.97 | 2.75 | dropped_name 1, dropped_number 1 | 0 / 8 / 31 |
| voxtral_small_chat (one-pass) | 40.8 / 54.7 / **47.7** | – | – | – | 40.97 / – | 7/63/81; – | 42.8 | 1.84 / 3.28 | 1.72 | – | 47 / 13 / 45 |
| voxtral_small_chat (one-pass + guard) | 39.5 / 49.3 / **44.4** | – | – | – | 42.08 / – | 4/65/82; – | 44.4 | 2.24 / 3.89 | 3.47 | dropped_name 3, dropped_number 10, missing 43, too_long 3 | 47 / 15 / 51 |
| voxtral_small_chat (plain) | 34.2 / 40.0 / **37.1** | – | – | – | 42.12 / – | 4/76/71; – | 43.9 | 2.03 / 3.78 | 1.74 | – | 47 / 18 / 51 |
| Nemotron 3.5 (hosted) + Gemma 4 E2B v3 (on-device) | 85.5 / 73.3 / **79.5** | 85.5 / 72.0 / **78.8** | – | – | 0.38 / 0.43 | 3/11/137; 3/11/137 | 4.5 | 1.81 / 4.60 | 0.44 | name 3 | 0 / 6 / 29 |
| Voxtral Mini 3B (hosted) + Gemma 4 E2B v3 (on-device) | 77.6 / 76.0 / **76.8** | 78.9 / 72.0 / **75.5** | – | – | 0.54 / 0.56 | 9/21/121; 9/22/120 | 5.4 | 1.53 / 3.01 | 0.38 | name 1, number 1 | 1 / 7 / 28 |
| Voxtral Small 24B (hosted) + Gemma 4 E2B v3 (on-device) | 73.7 / 82.7 / **78.1** | 75.0 / 77.3 / **76.2** | – | – | 0.50 / 0.53 | 11/21/119; 9/21/121 | 5.4 | 2.63 / 5.76 | 1.13 | number 4 | 1 / 5 / 30 |

</details>
