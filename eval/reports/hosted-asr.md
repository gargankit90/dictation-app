# Hosted open ASR on the Voice Arena public set

**Both judges are now complete for Nemotron 3.5, Whisper large v3, Voxtral Mini and both cleaned Nemotron runs.** Raw Nemotron is the best hosted transcript, with ZER of 66.2% (Claude) and 64.9% (GPT-6 Astra) against Groq's 60.3 / 57.0. It halves spoken WER (1.97% vs 3.98%). Its lead does not hold on the held-out check half (62.7 / 60.0 against Groq's 65.3 / 58.7). Voxtral Mini (3B) comes within 1.3–2.0 points of Nemotron overall, beats it on the check half under both judges (68.0 / 62.7) and has lower AMEND (0.94 / 0.97%). Both cost about $0.4 per 1,000 dictations. Whisper large v3 scores below Groq's Whisper turbo under both judges (53.6 / 51.7).

**Best pipeline:** Nemotron cleaned by GPT-6.2-sol + v4 through the OpenRouter API, with the app's guard, scores **84.8% (Claude) / 84.1% (Astra)**. On the held-out check half it scores **81.3 / 78.7**. AssemblyAI Smart scores 88.1 / 86.8, or 86.7 / 84.0 on the check half. The 3–5 point gap is inside the ±8-point speaker-bootstrap interval. That pairing lives in draft PR #327 (`eval/cleanup-offline/paired-results/nemotron_sol_v4`). Keep Groq as the shipped default until the pipeline's API latency is measured in the app. Claude scores for the six other round-1 raw transcripts are still pending.

| Model | Claude ZER all / check | Astra ZER all / check | AMEND Claude / Astra % | Spoken WER % | API s median / p90 | USD/1,000 dictations |
|---|---|---|---|---:|---|---:|
| Groq whisper-large-v3-turbo (VM, key-up to text) | **60.3** / 65.3 | **57.0** / 58.7 | 1.43 / 1.46 | 3.98 | 0.63 / 1.12 | – |
| Nemotron 3.5 | **66.2** / 62.7 | **64.9** / 60.0 | 1.16 / 1.35 | 1.97 | 1.27 / 3.23 | 0.44 |
| Voxtral Mini 3B | **64.9** / 68.0 | **62.9** / 62.7 | 0.94 / 0.97 | 3.27 | 1.11 / 2.03 | 0.38 |
| Voxtral Small 24B | pending | **62.3** / 65.3 | – / 1.00 | 3.14 | 2.20 / 4.71 | 1.13 |
| Qwen3-ASR 1.7B | pending | **60.9** / 64.0 | – / 7.07 | 8.39 | 1.76 / 42.71 | 0.17 |
| Qwen3-ASR 0.6B | pending | **53.6** / 58.7 | – / 4.00 | 5.92 | 1.02 / 1.72 | 0.08 |
| Whisper large v3 | **53.6** / 57.3 | **51.7** / 53.3 | 2.30 / 2.30 | 4.66 | 1.84 / 4.75 | 0.24 |
| Parakeet TDT 0.6B v3 | pending | **49.7** / 49.3 | – / 1.63 | 3.56 | 0.92 / 1.72 | 0.42 |
| Parakeet Ultra | pending | **47.7** / 48.0 | – / 1.58 | 3.78 | 0.98 / 1.63 | 0.38 |
| Whisper large v3 turbo (OpenRouter) | pending | **45.7** / 50.7 | – / 2.49 | 5.14 | 1.45 / 3.30 | 0.10 |
| Nemotron + GPT-6.2-sol v4 via OpenRouter API, guarded (#327) | **84.8** / 81.3 | **84.1** / 78.7 | 0.34 / 0.41 | – | ASR + 0.97 / 2.13 cleanup | 0.44 + 1.94 |
| Nemotron + GPT-6.2-sol v4 via Codex CLI, guarded | **85.4** / 82.7 | **84.8** / 80.0 | 0.32 / 0.39 | – | ASR + 4.75 / 13.03 CLI | – |
| AssemblyAI Smart (VM run 2, pasted) | **88.1** / 86.7 | **86.8** / 84.0 | 0.26 / – | – | 1.15 median key-up to text | – |

Rows are sorted by Astra ZER and cover all 151 clips. "Pending" means some clips still lack a Claude Opus 5.5 verdict. The cleanup cost is OpenRouter's reported $0.29247 for 151 sol calls. The Codex CLI row is a subscription round trip, not API latency. The full generated table (`uv run eval/hosted-asr/analyze.py`) adds tune-half ZER, intended-text WER, confidence intervals and failure screens:

| Run | ZER Claude tune / check / **all** (95% CI) | ZER Codex tune / check / **all** | AMEND C / X | vs Groq wins/losses (Claude; Codex) | WER spoken % | WER intended % | API s median / p90 | thank-you / short / long / numbers / empty |
|---|---|---|---|---|---:|---:|---|---|
| vocet_groq (VM, key-up to text) | 55.3 / 65.3 / **60.3** (52.1–68.4) | 55.3 / 58.7 / **57.0** | 1.43 / 1.46 | –; – | 3.98 | 6.89 | 0.63 / 1.12 | 11 / 2 / 0 / 17 / 0 |
| mistralai__voxtral-mini-3b-2507 | 61.8 / 68.0 / **64.9** (57.2–72.5) | 63.2 / 62.7 / **62.9** | 0.94 / 0.97 | 12/5; 12/3 | 3.27 | 6.34 | 1.11 / 2.03 | 0 / 1 / 0 / 16 / 0 |
| mistralai__voxtral-small-24b-2507-stt | – | 59.2 / 65.3 / **62.3** | – / 1.00 | –; 11/3 | 3.14 | 6.34 | 2.20 / 4.71 | 0 / 1 / 0 / 15 / 0 |
| moondream__parakeet-ultra | – | 47.4 / 48.0 / **47.7** | – / 1.58 | –; 7/21 | 3.78 | 7.53 | 0.98 / 1.63 | 0 / 0 / 0 / 18 / 0 |
| nvidia__nemotron-3.5-transcribe | 69.7 / 62.7 / **66.2** (58.7–73.8) | 69.7 / 60.0 / **64.9** | 1.16 / 1.35 | 22/13; 24/12 | 1.97 | 6.64 | 1.27 / 3.23 | 0 / 1 / 0 / 2 / 0 |
| nvidia__parakeet-tdt-0.6b-v3 | – | 50.0 / 49.3 / **49.7** | – / 1.63 | –; 7/18 | 3.56 | 7.86 | 0.92 / 1.72 | 0 / 0 / 0 / 15 / 0 |
| openai__whisper-large-v3 | 50.0 / 57.3 / **53.6** (45.2–62.4) | 50.0 / 53.3 / **51.7** | 2.30 / 2.30 | 12/22; 13/21 | 4.66 | 8.17 | 1.84 / 4.75 | 0 / 1 / 3 / 22 / 0 |
| openai__whisper-large-v3-turbo | – | 40.8 / 50.7 / **45.7** | – / 2.49 | –; 3/20 | 5.14 | 8.44 | 1.45 / 3.30 | 5 / 1 / 4 / 24 / 0 |
| qwen__qwen3-asr-0.6b | – | 48.7 / 58.7 / **53.6** | – / 4.00 | –; 13/18 | 5.92 | 10.25 | 1.02 / 1.72 | 0 / 1 / 4 / 19 / 0 |
| qwen__qwen3-asr-1.7b | – | 57.9 / 64.0 / **60.9** | – / 7.07 | –; 16/10 | 8.39 | 12.19 | 1.76 / 42.71 | 0 / 0 / 6 / 19 / 0 |

- mistralai__voxtral-mini-3b-2507: 151/151 ok, errors -, providers {'DeepInfra': 144, '?': 7}, cost $0.0568 for 0.95 h sent ($0.060/h)
- mistralai__voxtral-small-24b-2507-stt: 151/151 ok, errors -, providers {'DeepInfra': 94, '?': 57}, cost $0.1703 for 0.95 h sent ($0.180/h)
- moondream__parakeet-ultra: 151/151 ok, errors -, providers {'PrimeIntellect': 93, '?': 58}, cost $0.0568 for 0.95 h sent ($0.060/h)
- nvidia__nemotron-3.5-transcribe: 151/151 ok, errors -, providers {'DeepInfra': 13, '?': 67, 'PrimeIntellect': 71}, cost $0.0660 for 0.95 h sent ($0.070/h)
- nvidia__parakeet-tdt-0.6b-v3: 151/151 ok, errors -, providers {'Together': 28, 'PrimeIntellect': 64, '?': 59}, cost $0.0634 for 0.95 h sent ($0.067/h)
- openai__whisper-large-v3: 151/151 ok, errors -, providers {'DeepInfra': 119, 'Together': 17, 'Groq': 10, '?': 5}, cost $0.0367 for 0.95 h sent ($0.039/h)
- openai__whisper-large-v3-turbo: 151/151 ok, errors -, providers {'DeepInfra': 71, 'Groq': 7, 'PrimeIntellect': 7, '?': 66}, cost $0.0154 for 0.95 h sent ($0.016/h)
- qwen__qwen3-asr-0.6b: 151/151 ok, errors -, providers {'PrimeIntellect': 48, 'DeepInfra': 38, '?': 65}, cost $0.0113 for 0.95 h sent ($0.012/h)
- qwen__qwen3-asr-1.7b: 151/151 ok, errors -, providers {'PrimeIntellect': 47, 'DeepInfra': 49, '?': 55}, cost $0.0255 for 0.95 h sent ($0.027/h)

Latency is wall time for successful transcription requests. The Groq baseline includes the app's key-up-to-text path. At this set's clip length, Nemotron cost $0.44 per 1,000 dictations (observed spend divided by 151, times 1,000). Listed audio pricing was $0.06/hour on Prime Intellect and $0.12/hour on DeepInfra. [Provider rates](https://openrouter.ai/nvidia/nemotron-3.5-transcribe/providers)

OpenRouter received every clip and routed audio to its inference providers. A question mark indicates an unattributed request. The counts above describe identifiable providers only. The cleanup stage sends transcript text. The benchmark includes silence padding and uses English, no prompt and 16 kHz mono audio.

Groq had 11 unmatched “thank you” endings. Nemotron had zero, two digit mismatches and one short draft. The short-draft flag means fewer than 80% of the spoken reference's normalized words. These are screens, not adjudicated error counts. Parakeet v3 won 7, lost 18 and tied 126 clips against Groq on zero-edit status.

The judges use amend-va 0.2.1 prompt v1 at max effort: Claude Opus 5.5 through `claude -p` and GPT-6 Astra through `codex exec`. Groq with GPT-6.2-sol v4 cleanup scored 82.1 / 80.8 ZER. Nemotron with the same cleanup through the Codex CLI scored **85.4 (Claude) / 84.8 (Astra) ZER and 0.32 / 0.39% AMEND**. It fixed 29 / 30 imperfect raw drafts and broke none. On the check half it scored 82.7 / 80.0. The guard accepted 147 cleanups and kept four raw drafts after detecting dropped names. CLI time was 4.75 s median and 13.03 s p90, on top of ASR. The OpenRouter API run of the same model and prompt (#327) scored 84.8 / 84.1, with 0.97 s median cleanup time.


| Hosted model | Open ASR WER | Highest listed USD/audio hour | Observed USD/1,000 dictations |
|---|---:|---:|---:|
| [Nemotron 3.5](https://openrouter.ai/nvidia/nemotron-3.5-transcribe/providers) | 6.13 | 0.120 | 0.44 |
| [Qwen3 1.7B](https://openrouter.ai/qwen/qwen3-asr-1.7b/providers) | 6.15 | 0.027 | 0.17 |
| [Parakeet TDT v3](https://openrouter.ai/nvidia/parakeet-tdt-0.6b-v3/providers) | 7.04 | 0.090 | 0.42 |
| [Qwen3 0.6B](https://openrouter.ai/qwen/qwen3-asr-0.6b/providers) | 7.07 | 0.012 | 0.07 |
| [Voxtral Small](https://openrouter.ai/mistralai/voxtral-small-24b-2507-stt/providers) | 7.44 | 0.180 | 1.13 |
| [Parakeet Ultra](https://openrouter.ai/moondream/parakeet-ultra/providers) | unranked | 0.060 | 0.38 |
| [Whisper Turbo](https://openrouter.ai/openai/whisper-large-v3-turbo/providers) | 8.75 | 0.040 | 0.10 |
| [Whisper large v3](https://openrouter.ai/openai/whisper-large-v3/providers) | 8.24 | 0.111 | 0.24 |
| [Voxtral Mini](https://openrouter.ai/mistralai/voxtral-mini-3b-2507/providers) | 8.32 | 0.060 | 0.38 |

The Open ASR WER column is the external leaderboard used for candidate selection. Parakeet Ultra was included as an exploratory candidate. Whisper Turbo controls for changes in hosting and upload path. [Open ASR Leaderboard](https://huggingface.co/spaces/hf-audio/open_asr_leaderboard) · [OpenRouter speech-to-text API](https://openrouter.ai/docs/guides/overview/multimodal/speech-to-text)

The catalog scan found no OpenRouter STT route for Canary-Qwen, Cohere Transcribe, Granite Speech, Musci or SOV-Transcribe-1. The prior provider scan found Cohere on fal and no managed SOV endpoint. SOV's documented BF16 self-hosting setup uses roughly 53 GB of weights plus runtime memory and reports about 0.15 seconds for a 10–30 second clip with speculative decoding on an RTX PRO 6000 Blackwell. That figure excludes network and queue time. A continuously rented $1–3.30/hour GPU would cost about $730–2,409 per 730-hour month. [SOV model card](https://huggingface.co/salesforce-sovereign-intelligence/SOV-Transcribe-1)


Voxtral Small had the lowest first-round raw Codex AMEND (1.00%) and higher check-half ZER (65.3%) than raw Nemotron (60.0%). Voxtral Mini later matched it (0.97%) at a third of the price. It required 2.20 seconds median and 4.71 seconds p90. Qwen 1.7B had six unusually long drafts and a 42.71-second p90. All seven first-round routes returned 151 transcripts.

## Coverage and limits

Qwen3-ASR 0.6B ranked fourth of seven hosted first-round models by Codex ZER both overall and on the held-out check half. It scored 53.6% overall (81/151 zero-edit drafts, 4.00% AMEND), 58.7% on check (44/75, 1.29% AMEND) and 48.7% on tune (37/76, 6.81% AMEND). Direct Groq scored 57.0% overall and tied Qwen on check. All 151 distinct scored clip IDs matched the manifest and the saved transcripts.

The resumed GPT-6 Astra/max judge used 71 new calls and reused verdicts for 80 clips by exact prompt match. Through round one, cumulative Codex use reached 869 of 900 calls, comprising 718 judge calls and 151 GPT-6.2 Sol/low cleanup calls.

Claude Opus 5.5 now covers raw Nemotron, both cleaned Nemotron runs, Whisper large v3 and Voxtral Mini on all 151 clips. The finishing pass used 346 fresh Claude verdicts: 102 raw Nemotron, 53 OpenRouter-cleaned Nemotron (#327), 69 Voxtral Mini, 111 Whisper large v3 and 11 Codex-cleaned Nemotron. All other prompts were exact matches from the shared caches of every eval worktree. Claude is still pending for six first-round raw runs. After cache matching they need 41 verdicts for Voxtral Small, 90 for Parakeet Ultra, 72 for Parakeet TDT, 95 for Whisper Turbo, 83 for Qwen 0.6B and 85 for Qwen 1.7B (447 distinct prompts). Every score in the tables covers the entire 151-clip public set.

The first round cost about $0.41 in transcription charges. Claude and Codex CLI runs used subscription access.

## Round 2 results

Both owner-run transcriptions returned all 151 unique manifest clips. WER uses the normalized spoken reference. Latency measures each successful transcription request. Charges cover the uploaded audio, including the same silence padding as in round one.

| Model | Spoken WER % | API s median / p90 | USD for 151 clips | USD/audio hour | USD/1,000 dictations |
|---|---:|---:|---:|---:|---:|
| Whisper large v3 | 4.66 | 1.84 / 4.75 | 0.0367 | 0.039 | 0.24 |
| Voxtral Mini | 3.27 | 1.11 / 2.03 | 0.0568 | 0.060 | 0.38 |

OpenRouter routed 119 Whisper large v3 requests to DeepInfra, 17 to Together and 10 to Groq. Five lacked provider attribution. DeepInfra served 144 Voxtral Mini requests, and seven lacked attribution. The two runs together cost $0.0935.

Both judges scored all 151 clips of each model (see the first table). Voxtral Mini beat direct Groq on 12 clips and lost 5 under Claude, or 12 and 3 under Astra. Whisper large v3 lost more than it won (12 wins / 22 losses under Claude, 13 / 21 under Astra). Under Astra, large v3 scored 6 points above the OpenRouter Whisper turbo route (51.7 vs 45.7) but stayed below Groq's turbo through the app (57.0). Its screens resemble turbo's: 22 digit mismatches (turbo had 24) and three long drafts (turbo had four).

The finishing pass spent **118 GPT-6 Astra/max calls** from a separate 150-call budget: 34 for Voxtral Mini and 84 for Whisper large v3. That was six more than the 112 the earlier coverage count estimated. No call hit a usage or rate limit. The 900-call budget above was already spent. The recommendation to keep Groq as the shipped default stands until a cleaned pipeline's latency is measured in the app.
