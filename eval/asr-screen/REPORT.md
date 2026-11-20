# Which on-device recognizer should Vocet ship?

**Validate Voxtral Mini 3B Q4_K_M as the default when recognition runs alone. Prefer Oruk Qwen 1.7B Q4_K_M when the user enables Gemma 4 E2B v3 cleanup.** Voxtral produced the most sendable raw drafts on the 151 public clips. Against direct-file Parakeet it gained 22 Claude passes and 23 Codex passes, with zero losses for either judge. Both judges counted nine gains and zero losses on the 75-clip check half.

**Does Voxtral + Gemma beat Oruk + Gemma?** Claude passed **116/151 for each**. Voxtral won 11 clips, lost 11 and tied 129. Codex passed **114/151 for Voxtral and 112/151 for Oruk** (12 wins, 10 losses, 129 ties). The two-clip Codex edge is unconfirmed. Oruk's cleaned AMEND was lower on both judges, and its ASR weights are 1.32 GB versus Voxtral's 2.98 GB. With Gemma, users download 4.43 GB for Oruk or 6.09 GB for Voxtral. Summing the separately measured stages gives 0.93 s median / 2.33 s p90 for Oruk versus 1.83 / 3.16 s for Voxtral.

| Direct-file recognizer | Raw ZER, Claude / Codex | With Gemma E2B v3, Claude / Codex | ASR download |
| --- | ---: | ---: | ---: |
| Voxtral Mini 3B | 64.9% / 64.2% | 76.8% / 75.5% | 2.98 GB |
| Oruk Qwen 1.7B | 58.9% / 57.0% | 76.8% / 74.2% | 1.32 GB |
| Qwen3-ASR 1.7B | 60.3% / 57.6% | 73.5% / 70.9% | 1.32 GB |
| Voxtral Mini 4B Realtime | 60.3% / 58.9% | 74.2% / 72.8% | 2.83 GB |
| Parakeet unified 0.6B | 50.3% / 49.0% | 64.9% / 62.9% | 0.48 GB |
| Qwen3-ASR 0.6B | 49.0% / 47.7% | 59.6% / 57.0% | 0.59 GB |

This experiment compares local engines on the 151 public Voice Arena Dictation Dataset clips (revision ab671bb2, CC BY-NC 4.0). Each file was decoded to 16 kHz mono float PCM. Vocet's local recognizer receives those samples directly with English selected. The Rust harness used the release build of transcribe-cpp and transcribe-cpp-sys 0.4.1 with Metal. The test machine was an M4 Pro with 24 GB of unified memory running macOS 15.7.4.

## Direct-file baseline validation

| Engine | Direct spoken WER | VM spoken WER | Direct intended WER | Direct intended normalized exact | Direct and VM drafts identical after normalization |
| --- | ---: | ---: | ---: | ---: | ---: |
| Parakeet unified 0.6B | 3.71% | 3.87% | 7.23% | 29.8% | 115/151 |
| Qwen3-ASR 0.6B | 4.04% | 4.33% | 9.35% | 23.2% | 99/151 |

WER pools word edits across the corpus after Whisper English normalization. The direct path has slightly lower spoken WER for both engines. Formatting affects how often users can paste a draft without edits. Raw strings matched the VM on 47 and 48 clips respectively, and each direct run was judged independently. On Claude, the direct-file gap was **−2.6 points for Parakeet and 0.0 for Qwen** using unrounded counts. On Codex it was −1.3 and +0.7 points. Both shortcuts landed within the prespecified four-point tolerance overall. The paired CSV retains tune and check differences.

## Prior runs on the same clips

| Engine | Claude ZER | Codex ZER | Claude AMEND |
| --- | ---: | ---: | ---: |
| Parakeet unified 0.6B, VM | 53.0% | 50.3% | 1.38% |
| Qwen3-ASR 0.6B, VM | 49.0% | 47.0% | 1.70% |
| Parakeet + Gemma 4 E2B v3 cleanup | 65.6% | 62.3% | 0.76% |
| Gemma 4 E2B one-pass, unguarded | 49.7% | 49.7% | 1.46% |
| Gemma 4 E4B one-pass, unguarded | 53.6% | 53.0% | 1.34% |
| Groq Whisper large v3 turbo, VM | 60.3% | 57.0% | 1.43% |
| Hosted Nemotron 3.5 Transcribe | 66.2% | 64.9% | 1.16% |
| Hosted Voxtral Mini 3B | 64.9% | 62.9% | 0.94% |
| Hosted Nemotron + guarded GPT-6.2-sol v4, OpenRouter | 84.8% | 84.1% | 0.34% |
| AssemblyAI, first VM run | 86.8% | 85.4% | 0.26% |
| AssemblyAI Smart, second VM run | 88.1% | 86.8% | 0.26% |

The hosted comparison uses the separate experiment's results. Nemotron's Codex advantage over Groq was 7.9 points overall and 1.3 on check. Gemma E2B one-pass lost 37 sendable drafts and gained 13 against Parakeet + E2B text cleanup by Claude's judgment. [Voice Arena VM results](../voicearena/REPORT.md) · [Local cleanup](https://github.com/gargankit90/dictation-app/blob/eval/local-cleanup/eval/local-cleanup/REPORT.md) · [Gemma one-pass](https://github.com/gargankit90/dictation-app/blob/eval/gemma-onepass/eval/gemma-onepass/REPORT.md) · [Hosted screen](https://github.com/gargankit90/dictation-app/blob/eval/hosted-asr/eval/hosted-asr/REPORT.md)

## Paired changes in sendability

Each cell gives wins/losses/ties for the first engine, followed by its percentage-point ZER difference and 95% speaker-bootstrap interval.

| Candidate vs baseline | Claude | Codex |
| --- | --- | --- |
| Parakeet 0.6B vs Parakeet VM | 2/6/143, -2.6 (-6.2 to +0.8) | 2/4/145, -1.3 (-4.0 to +1.3) |
| Qwen 0.6B vs Qwen 0.6B VM | 6/6/139, +0.0 (-5.0 to +4.7) | 6/5/140, +0.7 (-3.6 to +4.9) |
| Qwen 1.7B vs Parakeet 0.6B | 20/5/126, +9.9 (+3.6 to +16.0) | 19/6/126, +8.6 (+2.4 to +14.5) |
| Oruk 1.7B vs Parakeet 0.6B | 19/6/126, +8.6 (+2.6 to +14.2) | 19/7/125, +8.0 (+1.4 to +13.8) |
| Voxtral Mini 3B vs Parakeet 0.6B | 22/0/129, +14.6 (+9.5 to +19.9) | 23/0/128, +15.2 (+10.0 to +20.5) |
| Voxtral Mini 3B vs Qwen 0.6B | 26/2/123, +15.9 (+9.9 to +22.3) | 27/2/122, +16.6 (+10.7 to +23.0) |
| Voxtral Realtime 4B vs Parakeet 0.6B | 21/6/124, +9.9 (+4.1 to +15.5) | 20/5/126, +9.9 (+4.4 to +15.2) |
| Qwen 1.7B + E2B vs Parakeet 0.6B + E2B | 21/8/122, +8.6 (+2.7 to +14.8) | 21/9/121, +8.0 (+1.6 to +14.4) |
| Oruk 1.7B + E2B vs Parakeet 0.6B + E2B | 24/6/121, +11.9 (+4.9 to +19.1) | 24/7/120, +11.3 (+4.0 to +18.6) |
| Voxtral Mini 3B + E2B vs Parakeet 0.6B + E2B | 23/5/123, +11.9 (+5.9 to +17.9) | 24/5/122, +12.6 (+6.5 to +18.6) |
| Voxtral Realtime 4B + E2B vs Parakeet 0.6B + E2B | 24/10/117, +9.3 (+2.2 to +16.6) | 23/8/120, +9.9 (+3.5 to +16.8) |
| Voxtral Mini 3B + E2B vs Oruk 1.7B + E2B | 11/11/129, +0.0 (-6.2 to +5.9) | 12/10/129, +1.3 (-4.7 to +7.5) |

## Full quality screen

Values are **tune / check / all**, in percent. The same split contains 76 tune clips from 37 speakers and 75 check clips from 34 other speakers. Spoken WER measures recognition, intended WER measures distance from send-ready text, and normalized exact match disregards case and most punctuation. Lower WER and higher exact match are better.

| Recognizer and mode | Spoken WER | Intended WER | Intended exact match |
| --- | ---: | ---: | ---: |
| Canary Qwen 2.5B (`canary`) | 7.44/8.97/8.20 | 11.38/12.84/12.11 | 23.7/30.7/27.2 |
| Cohere Transcribe (`cohere`) | 14.34/15.82/15.08 | 17.33/18.75/18.04 | 23.7/33.3/28.5 |
| Fluid Speech Pro, clean (`fluidclean`) | 3.48/4.70/4.09 | 6.99/7.98/7.48 | 22.4/32.0/27.2 |
| Fluid Speech Pro, raw (`fluidraw`) | 2.73/4.06/3.39 | 7.32/8.43/7.87 | 23.7/28.0/25.8 |
| Granite Speech 4.1 2B (`granite41`) | 4.31/4.30/4.30 | 7.62/7.71/7.66 | 22.4/33.3/27.8 |
| Granite Speech 4.1 2B Plus (`granite41plus`) | 4.20/5.02/4.61 | 7.71/8.52/8.11 | 25.0/32.0/28.5 |
| Granite 5.0 2B, written + dictionary (`granitebias`) | 3.74/4.03/3.88 | 7.59/7.74/7.66 | 22.4/32.0/27.2 |
| Granite 5.0 2B, verbatim (`graniteverbatim`) | 3.76/4.64/4.20 | 7.20/7.56/7.38 | 23.7/32.0/27.8 |
| Granite 5.0 2B, written (`granitewritten`) | 3.88/4.67/4.27 | 6.60/7.62/7.11 | 26.3/30.7/28.5 |
| Oruk Qwen 1.7B (`oruk`) | 2.16/2.79/2.47 | 8.04/8.07/8.05 | 23.7/29.3/26.5 |
| Oruk Qwen 1.7B + Gemma E2B (`orukgemma`) | 5.03/5.37/5.20 | 6.48/7.17/6.83 | 23.7/36.0/29.8 |
| Parakeet unified 0.6B (`parakeet`) | 3.51/3.92/3.71 | 6.96/7.50/7.23 | 25.0/34.7/29.8 |
| Parakeet unified 0.6B + Gemma E2B (`parakeetgemma`) | 5.55/5.69/5.62 | 5.92/6.72/6.32 | 23.7/38.7/31.1 |
| Qwen3-ASR 0.6B (`qwen06`) | 3.53/4.56/4.04 | 8.96/9.75/9.35 | 19.7/26.7/23.2 |
| Qwen3-ASR 0.6B + Gemma E2B (`qwen06gemma`) | 6.87/6.65/6.76 | 7.53/9.24/8.38 | 19.7/28.0/23.8 |
| Qwen3-ASR 1.7B (`qwen17`) | 2.27/2.84/2.56 | 7.32/7.80/7.56 | 25.0/32.0/28.5 |
| Qwen3-ASR 1.7B + Gemma E2B (`qwen17gemma`) | 5.29/5.52/5.40 | 6.07/7.08/6.57 | 23.7/37.3/30.5 |
| Qwen 1.7B, generic vocabulary (`qwenbias`) | 2.33/2.76/2.54 | 7.17/7.68/7.42 | 26.3/33.3/29.8 |
| Qwen 1.7B, user dictionary (`qwenwords`) | 2.30/3.02/2.66 | 7.14/7.95/7.54 | 26.3/29.3/27.8 |
| Rumble-2-Flash (`rumble`) | 3.36/3.89/3.62 | 7.05/7.95/7.50 | 26.3/30.7/28.5 |
| Parakeet TDT 1.1B (`tdt11`) | 4.05/4.47/4.26 | 8.78/9.69/9.24 | 25.0/25.3/25.2 |
| Granite 470M TurboCTC (`turbo`) | 3.68/4.47/4.07 | 7.80/8.43/8.11 | 19.7/24.0/21.9 |
| Voxtral Mini 3B (`voxtral3`) | 3.33/3.77/3.55 | 5.38/7.17/6.27 | 32.9/32.0/32.5 |
| Voxtral Mini 3B + Gemma E2B (`voxtral3gemma`) | 5.89/5.54/5.72 | 5.17/7.08/6.12 | 31.6/34.7/33.1 |
| Voxtral Mini 4B Realtime (`voxtral4`) | 3.39/4.09/3.74 | 6.42/7.08/6.75 | 25.0/34.7/29.8 |
| Voxtral Mini 4B Realtime + Gemma E2B (`voxtral4gemma`) | 6.01/5.81/5.91 | 5.74/7.20/6.47 | 27.6/34.7/31.1 |

### Judge scores

ZER gives the percentage of drafts the judge would paste unchanged. AMEND pools character edits over character counts. Dashes identify runs screened without that judge. Both judges used amend-va 0.2.1 v1 at max effort through their respective CLIs. The four finalist recognizers, their Gemma combinations and both direct baseline controls received both judges. Exact prompt hashes reused the same judge’s earlier answer. Across the study the scorers requested 710 new Claude judgments and 1,448 new Codex judgments; all reported scores cover 151 clips.

| Recognizer and mode | Claude ZER | Claude AMEND | Codex ZER | Codex AMEND |
| --- | ---: | ---: | ---: | ---: |
| Fluid Speech Pro, clean (`fluidclean`) | — | — | 52.6/53.3/53.0 | 1.49/1.29/1.39 |
| Fluid Speech Pro, raw (`fluidraw`) | — | — | 51.3/50.7/51.0 | 1.62/1.47/1.54 |
| Granite 5.0 2B, written + dictionary (`granitebias`) | — | — | 51.3/58.7/55.0 | 1.47/1.36/1.42 |
| Granite 5.0 2B, verbatim (`graniteverbatim`) | — | — | 48.7/61.3/55.0 | 1.50/1.24/1.37 |
| Granite 5.0 2B, written (`granitewritten`) | — | — | 51.3/57.3/54.3 | 1.27/1.31/1.29 |
| Oruk Qwen 1.7B (`oruk`) | 55.3/62.7/58.9 | 1.58/1.39/1.49 | 53.9/60.0/57.0 | 1.57/1.59/1.58 |
| Oruk Qwen 1.7B + Gemma E2B (`orukgemma`) | 77.6/76.0/76.8 | 0.49/0.54/0.51 | 76.3/72.0/74.2 | 0.47/0.71/0.59 |
| Parakeet unified 0.6B (`parakeet`) | 46.1/54.7/50.3 | 1.61/1.15/1.37 | 46.1/52.0/49.0 | 1.42/1.24/1.33 |
| Parakeet unified 0.6B + Gemma E2B (`parakeetgemma`) | 64.5/65.3/64.9 | 0.76/0.55/0.65 | 64.5/61.3/62.9 | 0.66/0.64/0.65 |
| Qwen3-ASR 0.6B (`qwen06`) | 48.7/49.3/49.0 | 1.85/1.82/1.83 | 48.7/46.7/47.7 | 1.80/1.98/1.89 |
| Qwen3-ASR 0.6B + Gemma E2B (`qwen06gemma`) | 60.5/58.7/59.6 | 1.03/1.10/1.07 | 57.9/56.0/57.0 | 1.04/1.35/1.20 |
| Qwen3-ASR 1.7B (`qwen17`) | 57.9/62.7/60.3 | 1.43/1.34/1.39 | 56.6/58.7/57.6 | 1.41/1.47/1.44 |
| Qwen3-ASR 1.7B + Gemma E2B (`qwen17gemma`) | 72.4/74.7/73.5 | 0.52/0.52/0.52 | 71.1/70.7/70.9 | 0.51/0.74/0.63 |
| Qwen 1.7B, user dictionary (`qwenwords`) | — | — | 56.6/56.0/56.3 | 1.39/1.50/1.45 |
| Rumble-2-Flash (`rumble`) | — | — | 52.6/50.7/51.7 | 1.58/1.41/1.49 |
| Granite 470M TurboCTC (`turbo`) | — | — | 44.7/34.7/39.7 | 1.79/1.89/1.84 |
| Voxtral Mini 3B (`voxtral3`) | 63.2/66.7/64.9 | 1.00/0.97/0.99 | 64.5/64.0/64.2 | 0.98/1.09/1.04 |
| Voxtral Mini 3B + Gemma E2B (`voxtral3gemma`) | 76.3/77.3/76.8 | 0.74/0.46/0.60 | 77.6/73.3/75.5 | 0.73/0.53/0.63 |
| Voxtral Mini 4B Realtime (`voxtral4`) | 53.9/66.7/60.3 | 1.27/0.86/1.06 | 53.9/64.0/58.9 | 1.24/0.96/1.09 |
| Voxtral Mini 4B Realtime + Gemma E2B (`voxtral4gemma`) | 73.7/74.7/74.2 | 0.81/0.40/0.60 | 73.7/72.0/72.8 | 0.77/0.46/0.61 |

## What users download and wait for

A separate single-model replay timed all 151 clips in each mode. The native models use Metal, the two MLX models use their model-card loaders, and Granite 5.0 2B uses llama.cpp. A new process loaded each model; Granite's three prompts shared one loaded server. Load times include initialization and depend on filesystem and Metal shader caches. Parakeet and Voxtral Mini 3B each took about 11 seconds to initialize in their reported runs. After detecting unrelated CPU activity, we repeated four complete runs in a reserved GPU window. One five-second CPU sample during the Oruk replay recorded 64% of one core from Android Studio. No other monitored build process crossed 50% during those repeats.

| Recognizer | Download GB | Max RSS / peak footprint GB | Load s | Per clip median / p90 s |
| --- | ---: | ---: | ---: | ---: |
| Voxtral Mini 3B | 2.98 | 0.91 / 0.80 | 11.00 | 1.43 / 2.22 |
| Voxtral Mini 4B Realtime | 2.83 | 0.48 / 0.57 | 1.63 | 2.78 / 5.68 |
| Parakeet unified 0.6B | 0.48 | 0.51 / 0.26 | 10.83 | 0.10 / 0.21 |
| Qwen3-ASR 0.6B | 0.59 | 0.90 / 0.68 | 0.24 | 0.22 / 0.51 |
| Qwen3-ASR 1.7B | 1.32 | 0.79 / 0.82 | 0.35 | 0.40 / 0.95 |
| Qwen 1.7B, generic vocabulary | 1.32 | 0.79 / 0.74 | 0.35 | 0.43 / 1.00 |
| Qwen 1.7B, user dictionary | 1.32 | 0.93 / 0.70 | 0.40 | 0.41 / 0.93 |
| Oruk Qwen 1.7B | 1.32 | 0.95 / 0.70 | 0.35 | 0.39 / 0.92 |
| Granite 470M | 0.28 | 0.64 / 0.36 | 0.11 | 0.06 / 0.12 |
| Fluid raw | 0.75 | 0.89 / 16.54 | 0.66 | 0.17 / 0.55 |
| Fluid clean | 0.81 | 0.77 / 16.63 | 0.72 | 0.18 / 0.48 |
| Rumble-2-Flash | 0.73 | 1.38 / 3.48 | 1.48 | 0.16 / 0.36 |
| Granite 5.0 2B written | 2.30 | 5.82 / 10.79 | 1.52 | 0.49 / 1.09 |
| Granite 5.0 2B verbatim | 2.30 | 5.82 / 10.79 | 1.52 | 0.59 / 1.28 |
| Granite 5.0 2B + dictionary | 2.30 | 5.82 / 10.79 | 1.52 | 0.60 / 1.28 |
| Parakeet TDT 1.1B | 0.83 | 0.50 / 0.25 | 0.26 | 0.14 / 0.27 |
| Cohere Transcribe | 1.56 | 0.39 / 0.34 | 0.76 | 0.27 / 0.60 |
| Granite 4.1 2B | 1.60 | 0.76 / 0.55 | 0.36 | 0.51 / 1.22 |
| Granite 4.1 2B Plus | 1.49 | 0.68 / 0.58 | 0.75 | 0.59 / 1.35 |
| Canary Qwen 2.5B | 1.74 | 1.36 / 1.24 | 1.19 | 0.43 / 1.17 |

### Adding cleanup

Gemma 4 E2B Q4_K_M adds **3.11 GB** of weights. All cleanup rows reused the local-cleanup v3 prompt and guards, with temperature 0, top-k 1, seed 1, thinking disabled, a 4,096-token context, one slot and all layers on Metal. These strictly sequential per-clip delay estimates sum the ASR and guarded cleanup durations for the same clip before taking quantiles. They exclude clipboard, hotkey, UI, cold loading and any cost of evicting models. The two models were timed in separate processes. Combined peak RAM still needs an end-to-end measurement.

| Pipeline | Total weights GB | Cleanup max RSS GB | Cleanup load s | Cleanup median / p90 s | ASR + cleanup median / p90 s |
| --- | ---: | ---: | ---: | ---: | ---: |
| Voxtral Mini 3B + Gemma | 6.09 | 3.45 | 2.04 | 0.40 / 0.99 | 1.83 / 3.16 |
| Oruk Qwen 1.7B + Gemma | 4.43 | 3.48 | 1.00 | 0.54 / 1.48 | 0.93 / 2.33 |
| Qwen3-ASR 1.7B + Gemma | 4.43 | 3.21 | 2.63 | 0.52 / 1.45 | 0.92 / 2.38 |
| Voxtral Mini 4B Realtime + Gemma | 5.94 | 3.07 | 2.85 | 0.41 / 1.04 | 3.17 / 6.67 |
| Parakeet unified 0.6B + Gemma | 3.59 | 3.56 | 0.81 | 0.47 / 1.22 | 0.58 / 1.37 |
| Qwen3-ASR 0.6B + Gemma | 3.70 | 3.48 | 0.77 | 0.47 / 1.18 | 0.7 / 1.69 |

## What the errors look like

- Qwen 1.7B and Oruk had the lowest spoken WER, yet still rendered a spoken registration URL as prose, split “Lakeshore,” and heard “nudge” as “note” (0533). Both missed the temperature unit in a recipe (0069). Text cleanup can repair formatting but has no audio to recover a missing unit.
- Voxtral Mini 3B had more spoken word errors than either Qwen while yielding more sendable raw drafts. Its remaining edits included “some days” for “Sundays” (0229), the identifier “idempotency” (0539), and retained filler phrases (0229, 0231, 0551). The larger real-time Voxtral matched Mini 3B's held-out raw ZER on both judges, but scored lower across the whole corpus.
- Granite 2B's written prompt improved intended-text WER relative to verbatim. Codex judged the written version sendable on 82 clips and verbatim on 83, with five wins and six losses. Rumble's decoder returned all 151 drafts without leaking EOS, but its small aggregate gain over Parakeet reversed on check. Granite TurboCTC produced the fastest drafts and the fewest Codex passes of the judged engines.
- [Failure counts](failure-counts.csv) include short/long output, differing digit groups, terminal question marks and missed dictionary occurrences for every complete run. These are diagnostic flags. Written digits, number words, dates and abbreviations can give different counts while preserving meaning.

## What Fluid clean mode deletes

Comparing the same 8-bit MLX weights with the 55 MB adapter switched on and off, a word alignment found four removals of the filler “you know” (0103, 0229, 0231, 0551). One repeated “like” was collapsed in 0103, preserving the comparison “more like a two-week delay.” No meaningful “like” was deleted. Clean mode also removed ordinary stutters, including doubled “did you” in 0124 and 0138.

**Six identifiers that were correct with the adapter off lost repeated digits with it on.** The invoice 8841 became 841 (0474), ticket 8832 became 832 (0737), and the same error affected phone numbers (0119 and 0663), an order ID (0471) and a payment reference (0629). A separate invoice was already incorrect in the raw draft (0391). The clean adapter also dropped part of an email address (0008) and changed Vite to VIP (0218). Those changes matter to someone pasting a draft even when normalized corpus WER moves little.

## Custom words (#215)

Qwen accepted the vocabulary through the existing Rust `RunOptions::vocabulary` field. Granite accepted the same list appended to the written instruction. IBM documents up to 2,000 entries. This experiment used 24, selected from tune names and tools plus plausible distractors, and froze the list for all 151 clips.

| Recognizer | Tune occurrences correct, plain → biased (19) | Check occurrences correct, plain → biased (6) | New wrong terms on check |
| --- | ---: | ---: | --- |
| Qwen 1.7B | 15 → 18 | 6 → 6 | **Zillow** for Zelle (0629), **Amaya** in a benefits URL (0729) |
| Granite 2B written | 16 → 18 | 6 → 6 | **Zillow** for Zelle (0629) |

Whole-word case-insensitive counting also flagged “Docker file” for “Dockerfile” on 0310. That spacing difference is excluded from the wrong-term column. A separate 12-term generic engineering list had just one target occurrence, TypeScript, already correct with and without Qwen bias. The larger list was Priya Raman, Carlos, Hannah, Lakeshore, Castellano, Moreno, Zillow, JavaScript, Docker, Tailwind, Stripe, WebSocket, Delancey, Clearwater, TypeScript, GraphQL, Vitest, Grafana, Datadog, Twilio, Amaya, Lakehurst, Costello and Alina.

Keep #215 opt-in. Bias recovered names on clips containing tune-selected terms. On check there was no remaining target error to fix, and bias introduced unrelated entities. Before enabling it by default, use fresh speakers with actual user dictionaries and include clips where none of their words should appear.

## Runtime and shipping constraints

- **Both Voxtrals, both Parakeets, both base Qwens, Oruk, Cohere, Canary, Granite 4.1/Plus and Granite TurboCTC** ran through a Metal build of Vocet's exact transcribe-cpp **and** transcribe-cpp-sys 0.4.1. The harness loaded each GGUF, created one session, and submitted all clips with English and default run options. The native binary's deployment target was macOS 11. Vocet targets macOS 13. This establishes loader compatibility on macOS 15.7.4. A real macOS 13 end-to-end test remains outstanding.
- **Voxtral Mini 3B and Mini 4B Realtime** each ran as one Apache-2.0 Q4_K_M GGUF without a separate projector. The exact model files weigh 2,984,721,056 and 2,830,493,984 bytes. The real-time model was evaluated offline on whole clips under Vocet's current settings. [Mini 3B weights](https://huggingface.co/handy-computer/Voxtral-Mini-3B-2507-gguf/tree/3d4e8e74a2ada47b400b2957575b2a9c058efe80) · [Realtime 4B weights](https://huggingface.co/handy-computer/Voxtral-Mini-4B-Realtime-2602-gguf/tree/7fd3d424fc558edca2c53808578b0541ef52a81a)
- **Oruk** publishes a 4.08 GB Transformers 5 safetensors checkpoint. The evaluated download for a user would be the 1.32 GB Q4_K_M export. The conversion unpacked the publisher's tokenizer and feature extractor, mapped its audio tower, language model and projector layers to the older Qwen tensor names, and preserved all weight bytes before the upstream BF16 GGUF conversion and quantization. The converter and quantizer came from transcribe.cpp commit `db678a03`; the final GGUF SHA-256 was `057471662f060800a6374708ad679d4abe51775e52ca591a88ea4ade78b273bb`. [Oruk checkpoint](https://huggingface.co/oruk/orukqwen-asr-1.7b/tree/a526248ceb2b829cb8f2e1eec764eafbd6a10fe4)
- **Fluid Speech Pro** used the weights repository's MLX loader with parakeet-mlx 0.7.0, 8-bit weights and the optional clean adapter. The MLX 0.32.4 wheel used here requires macOS 15 on Apple silicon. Supporting macOS 13 with clean mode would require testing the documented ONNX route and packaging a separate adapter. The weights require CC BY 4.0 attribution. [Fluid MLX weights and loader](https://huggingface.co/FluidVoice-Labs/fluid-speech-pro-mlx/tree/aeb1d4e43487586b15d86c0a1dd4511e1cf6b7a0)
- **Rumble-2-Flash** used mlx-audio 0.6.3 with the system tokens `<digits><nofiller><noverbatim>` and English. The runner verified that the loaded decoder's stop set was exactly **99564 and 99562**, and all 151 clips returned. A decoder trace on clips 0007, 0108 and 0749 observed token 99564 entering the EOS branch once per clip, after reported output lengths of 30, 243 and 74 tokens. This Apache-2.0 MLX port has the same macOS deployment limitation. [Rumble Mac model card](https://huggingface.co/TypeWhisper/Rumble-2-Flash-8bit/tree/feb7feb016e58ec3f39f961dfb78e4837325a0d1)
- **Granite Speech 5.0 2B** used IBM's Q4_K_M weights and F16 projector through llama.cpp build 12359's loopback transcription endpoint. The installed binary has a macOS 15 deployment target. Shipping on macOS 13 requires a compatible llama.cpp build and integration test. These three alternate runtimes are outside the 0.4.1 catalog; no newer transcribe.cpp release was verified for them. [IBM GGUF and projector](https://huggingface.co/ibm-granite/granite-speech-5.0-2b-GGUF/tree/945b4581e2b40cd9a3121de265fd8e387bdf782d)

Direct loader probes on 0.4.1 rejected the IBM Granite 2B GGUF with “unsupported architecture” (status 5) and the two MLX safetensors artifacts with a GGUF format error (status 4). No candidate that completed this experiment required an upgrade of Vocet's Rust dependency. The external engines require a different runtime.

Granite's written instruction was “Transcribe the speech into send-ready written English with proper punctuation, capitalization, and digits. Remove filler words and accidental repetitions. Preserve names, meaning, and deliberate repetition.” The control was “Transcribe the speech verbatim in English.” The two requests produced different strings on 147 of 151 clips.

The [Oruk layout converter](prepare_oruk.py) reproduces the English-only intermediate layout. Starting with the linked checkpoint in `oruk-source` and transcribe.cpp at `db678a03`, use the following commands with the upstream converter dependencies installed and its quantizer on PATH. Only the final Q4_K_M file is needed by Vocet.

```sh
python3 eval/asr-screen/prepare_oruk.py oruk-source oruk-legacy
python3 "$TRANSCRIBE_CPP/scripts/convert-qwen3_asr.py" "$PWD/oruk-legacy" oruk-BF16.gguf \
  --repo-id oruk/orukqwen-asr-1.7b --variant qwen3-asr-1.7b
transcribe-quantize oruk-BF16.gguf oruk-Q4_K_M.gguf --quant Q4_K_M
```

## Reading the comparisons

The 151 paired clips come from the public benchmark; prompt choices, candidate selection and repeated looks at this set constrain generalization. For each baseline comparison the paired CSV counts clips that changed zero-edit status and resamples speakers with replacement 10,000 times (seed 215). The percentile interval is on the difference in ZER, in percentage points. A positive point estimate below about four points is inconclusive. Where the interval includes zero, the sign of the gain is also uncertain.

The speed test processes one clip at a time through a loaded model. The Rust path times the inference call over PCM already in memory; the MLX and loopback llama.cpp paths also open or send a local WAV. The VM figures in the linked benchmark measure key release to pasted text. The size column uses decimal GB. macOS maximum resident set and peak memory footprint are separate process accounting measures. Mapped weights and GPU allocations prevent either number alone from predicting the minimum RAM a user needs. No test here establishes performance on an 8 GB Mac.

The generic Qwen vocabulary was GitHub, ChatGPT, TypeScript, Postgres, Kubernetes, PyTorch, Jira, Figma, Linear, Miro, GraphQL and Vitest. A second list drew target names only from tune, then added unrelated names and tools. The same 24 entries went to Granite. The clean-mode audit compared token-level changes within the same clip and checked deleted spans against intended text. A spoken-number sequence that repeats a digit counts as meaningful repetition.

The additional time allowed screening Parakeet TDT 1.1B, Cohere, Granite 4.1/Plus, Canary and both under-cap Voxtrals. Musci was deferred: the reviewed BF16 artifact exceeds 3 GB and no validated under-cap Mac artifact was identified. The separate hosted and Gemma one-pass reports supplied their results.

[Per-clip numeric evidence](clip-scores.csv) · [Paired comparisons by half and judge](paired-differences.csv)

## Release decision

Run the 151 clips end to end in Vocet on an available Lume VM with Voxtral Mini 3B alone and Oruk + Gemma. Compare each against the corresponding direct-file drafts by clip ID and repeat the speaker-paired analysis. Include release-to-paste median and p90, first dictation after model load, and peak unified memory while ASR and cleanup are resident together. Check the packaged native path on macOS 13 before switching defaults. This screen used macOS 15.7.4; it did not change the product or run a VM.
