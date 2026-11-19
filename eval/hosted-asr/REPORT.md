# Hosted open ASR on the Voice Arena public set

Nemotron is the strongest fully Codex-judged challenger so far. It reduced spoken WER from 3.98% to 1.97%. Its 24 wins, 12 losses and 115 ties against Groq gave it a 7.9-point ZER advantage overall, but the advantage on the check half was only 1.3 points. Keep Groq as the default because the cleanup gain did not carry over to the check half and the second judge is pending. AssemblyAI verbatim reached 70.9/68.2 Claude/Codex ZER and 2.48% WER.

| Run | ZER Claude tune / check / **all** (95% CI) | ZER Codex tune / check / **all** | AMEND C / X | vs Groq W/L (Claude; Codex) | WER spoken % | WER intended % | API s median / p90 | thank-you / short / long / numbers / empty |
|---|---|---|---|---|---:|---:|---|---|
| vocet_groq (VM, key-up to text) | 55.3 / 65.3 / **60.3** (52.1–68.4) | 55.3 / 58.7 / **57.0** | 1.43 / 1.46 | –; – | 3.98 | 6.89 | 0.63 / 1.12 | 11 / 2 / 0 / 17 / 0 |
| mistralai__voxtral-small-24b-2507-stt | – | – | – / – | –; – | 3.14 | 6.34 | 2.20 / 4.71 | 0 / 1 / 0 / 15 / 0 |
| moondream__parakeet-ultra | – | – | – / – | –; – | 3.78 | 7.53 | 0.98 / 1.63 | 0 / 0 / 0 / 18 / 0 |
| nvidia__nemotron-3.5-transcribe | – | 69.7 / 60.0 / **64.9** | – / 1.35 | –; 24/12 | 1.97 | 6.64 | 1.27 / 3.23 | 0 / 1 / 0 / 2 / 0 |
| nvidia__parakeet-tdt-0.6b-v3 | – | 50.0 / 49.3 / **49.7** | – / 1.63 | –; 7/18 | 3.56 | 7.86 | 0.92 / 1.72 | 0 / 0 / 0 / 15 / 0 |
| openai__whisper-large-v3-turbo | – | – | – / – | –; – | 5.14 | 8.44 | 1.45 / 3.30 | 5 / 1 / 4 / 24 / 0 |
| qwen__qwen3-asr-0.6b | – | – | – / – | –; – | 5.92 | 10.25 | 1.02 / 1.72 | 0 / 1 / 4 / 19 / 0 |

- mistralai__voxtral-small-24b-2507-stt: 151/151 ok, errors -, providers {'DeepInfra': 94, '?': 57}, cost $0.1703 for 0.95 h sent ($0.180/h)
- moondream__parakeet-ultra: 151/151 ok, errors -, providers {'PrimeIntellect': 93, '?': 58}, cost $0.0568 for 0.95 h sent ($0.060/h)
- nvidia__nemotron-3.5-transcribe: 151/151 ok, errors -, providers {'DeepInfra': 13, '?': 67, 'PrimeIntellect': 71}, cost $0.0660 for 0.95 h sent ($0.070/h)
- nvidia__parakeet-tdt-0.6b-v3: 151/151 ok, errors -, providers {'Together': 28, 'PrimeIntellect': 64, '?': 59}, cost $0.0634 for 0.95 h sent ($0.067/h)
- openai__whisper-large-v3-turbo: 151/151 ok, errors -, providers {'DeepInfra': 71, 'Groq': 7, 'PrimeIntellect': 7, '?': 66}, cost $0.0154 for 0.95 h sent ($0.016/h)
- qwen__qwen3-asr-0.6b: 151/151 ok, errors -, providers {'PrimeIntellect': 48, 'DeepInfra': 38, '?': 65}, cost $0.0113 for 0.95 h sent ($0.012/h)

Latency is wall time for successful transcription requests. The Groq baseline includes the app's key-up-to-text path. At this set's clip length, Nemotron cost $0.44 per 1,000 dictations (observed spend divided by 151, times 1,000). Listed audio pricing was $0.06/hour on Prime Intellect and $0.12/hour on DeepInfra. [Provider rates](https://openrouter.ai/nvidia/nemotron-3.5-transcribe/providers)

OpenRouter received every clip and routed audio to its inference providers. A question mark indicates an unattributed request. The counts above describe identifiable providers only. The cleanup stage sends transcript text. The benchmark includes silence padding and uses English, no prompt and 16 kHz mono audio.

Groq had 11 unmatched “thank you” endings. Nemotron had zero, two digit mismatches and one short draft. The short-draft flag means fewer than 80% of the spoken reference's normalized words. These are screens, not adjudicated error counts. Parakeet v3 won 7, lost 18 and tied 126 clips against Groq on zero-edit status.

The judge uses amend-va 0.2.1 prompt v1. Claude Opus 5.5 judgments are incomplete after repeated CLI failures. Dashes denote missing scores. Groq with GPT-6.2 Sol v4 cleanup scored 82.1/80.8 ZER. Nemotron with the same cleanup scored **84.8 Codex ZER and 0.39% AMEND**, versus 80.8 and 0.44% for cleaned Groq. Nemotron gained six zero-edit clips (12 wins, 6 losses, 133 ties). On tune it scored 89.5 versus 80.3, but on check it scored 80.0 versus 81.3. The guard accepted 147 cleanups and kept four raw drafts after detecting dropped names. Cleanup CLI time was 4.75 s median and 13.03 s p90, in addition to ASR. The Claude CLI confirmed a five-hour usage cap with a scheduled 15:10 Pacific reset.

Two Nemotron number errors changed meaning. In dict_0228 a $150 budget became $850. In dict_0721 it dropped one of three consecutive fives from a clinic fax number. Its only short-draft flag, dict_0578, removed a repeated “I'm so” while preserving the message.

| Hosted model | Open ASR WER | Highest listed USD/audio hour | Observed USD/1,000 dictations |
|---|---:|---:|---:|
| Nemotron 3.5 | 6.13 | 0.120 | 0.44 |
| Qwen3 1.7B | 6.15 | 0.027 | pending |
| Parakeet TDT v3 | 7.04 | 0.090 | 0.42 |
| Qwen3 0.6B | 7.07 | 0.012 | 0.07 |
| Voxtral Small | 7.44 | 0.180 | 1.13 |
| Parakeet Ultra | unranked | 0.060 | 0.38 |
| Whisper Turbo | 8.75 | 0.040 | 0.10 |
| Whisper large v3 | 8.24 | 0.111 | pending |
| Voxtral Mini | 8.32 | 0.060 | pending |

The Open ASR WER column is the external leaderboard used for candidate selection. Parakeet Ultra was included as an exploratory candidate. Whisper Turbo controls for changes in hosting and upload path. [Open ASR Leaderboard](https://huggingface.co/spaces/hf-audio/open_asr_leaderboard) · [OpenRouter speech-to-text API](https://openrouter.ai/docs/guides/overview/multimodal/speech-to-text)

The catalog scan found no OpenRouter STT route for Canary-Qwen, Cohere Transcribe, Granite Speech, Musci or SOV-Transcribe-1. The prior provider scan found Cohere on fal and no managed SOV endpoint. SOV's documented BF16 self-hosting setup uses roughly 53 GB of weights plus runtime memory and reports about 0.15 seconds for a 10–30 second clip with speculative decoding on an RTX PRO 6000 Blackwell. That figure excludes network and queue time. A continuously rented $1–3.30/hour GPU would cost about $730–2,409 per 730-hour month. [SOV model card](https://huggingface.co/Salesforce/SOV-Transcribe-1)

Names remain a failure mode. Nemotron changed Austin to “Austrian” in dict_0177, Tomas to “Thomas” in dict_0544 and Okafor to “Oakivore” in dict_0620. These are spot checks.
