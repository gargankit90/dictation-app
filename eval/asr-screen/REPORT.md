# On-device recognition on Voice Arena

**Interim recommendation.** Put base Qwen3-ASR 1.7B through the end-to-end VM test. It scored 57.6% Codex ZER against 49.0% for direct-file Parakeet. With Gemma E2B v3 cleanup, base Qwen scored 70.9% and Oruk 74.2%. The 3.3-point difference between those cleaned systems is unconfirmed. Claude scoring and a separate timing pass are pending.

This experiment compares local engines on the 151 public Voice Arena Dictation Dataset clips (revision ab671bb2, CC BY-NC 4.0). Each file was decoded to 16 kHz mono float PCM. Vocet's local recognizer receives those samples directly with English selected. The Rust harness used the release build of transcribe-cpp and transcribe-cpp-sys 0.4.1 with Metal. The test machine was an M4 Pro with 24 GB of unified memory running macOS 15.7.4.

## Direct-file baseline validation

| Engine | Direct spoken WER | VM spoken WER | Direct intended WER | Direct intended normalized exact | Direct and VM drafts identical after normalization |
| --- | ---: | ---: | ---: | ---: | ---: |
| Parakeet unified 0.6B | 3.71% | 3.87% | 7.23% | 29.8% | 115/151 |
| Qwen3-ASR 0.6B | 4.04% | 4.33% | 9.35% | 23.2% | 99/151 |

WER pools word edits across the corpus after Whisper English normalization. The direct path has slightly lower spoken WER for both engines. Formatting affects how often users can paste a draft without edits. Raw strings matched the VM on 47 and 48 clips respectively, so ZER requires separate judging.

## Prior runs on the same clips

| Engine | Claude ZER | Codex ZER | Claude AMEND |
| --- | ---: | ---: | ---: |
| Parakeet unified 0.6B, VM | 53.0% | 50.3% | 1.38% |
| Qwen3-ASR 0.6B, VM | 49.0% | 47.0% | 1.70% |
| Parakeet + Gemma 4 E2B v3 cleanup | 65.6% | 62.3% | 0.76% |
| Gemma 4 E2B one-pass, unguarded | 49.7% | 49.7% | 1.46% |
| Gemma 4 E4B one-pass, unguarded | 53.6% | — | 1.34% |
| Groq Whisper large v3 turbo, VM | 60.3% | 57.0% | 1.43% |
| Hosted Nemotron 3.5 Transcribe | — | 64.9% | — |
| AssemblyAI | 86.8% | 85.4% | 0.26% |

The hosted comparison uses the separate experiment's results. Nemotron's Codex advantage over Groq was 7.9 points overall and 1.3 on check. Gemma E2B one-pass lost 37 sendable drafts and gained 13 against Parakeet + E2B text cleanup by Claude's judgment. [Voice Arena VM results](../voicearena/REPORT.md) · [Local cleanup](https://github.com/gargankit90/dictation-app/blob/eval/local-cleanup/eval/local-cleanup/REPORT.md) · [Gemma one-pass](https://github.com/gargankit90/dictation-app/blob/eval/gemma-onepass/eval/gemma-onepass/REPORT.md) · [Hosted screen](https://github.com/gargankit90/dictation-app/blob/eval/hosted-asr/eval/hosted-asr/REPORT.md)

## What Fluid clean mode deletes

Comparing the same 8-bit MLX weights with the 55 MB adapter switched on and off, a word alignment found four removals of the filler “you know” (0103, 0229, 0231, 0551). One repeated “like” was collapsed in 0103, preserving the comparison “more like a two-week delay.” No meaningful “like” was deleted. Clean mode also removed ordinary stutters, including doubled “did you” in 0124 and 0138.

**Six identifiers that were correct with the adapter off lost repeated digits with it on.** The invoice 8841 became 841 (0474), ticket 8832 became 832 (0737), and the same error affected phone numbers (0119 and 0663), an order ID (0471) and a payment reference (0629). A separate invoice was already incorrect in the raw draft (0391). The clean adapter also dropped part of an email address (0008) and changed Vite to VIP (0218). Those changes matter to someone pasting a draft even when normalized corpus WER moves little.

## Screen results

Each cell lists **tune / check / all**, in percent. The original speaker split contains 76 tune clips from 37 speakers and 75 check clips from 34 other speakers. The exact-match column compares normalized words with the intended written reference. The judge can accept different wording or formatting.

| Engine and mode | Spoken WER ↓ | Intended WER ↓ | Intended exact ↑ |
| --- | ---: | ---: | ---: |
| Canary Qwen 2.5B | 7.44/8.97/8.20 | 11.38/12.84/12.11 | 23.7/30.7/27.2 |
| Cohere Transcribe | 14.34/15.82/15.08 | 17.33/18.75/18.04 | 23.7/33.3/28.5 |
| Fluid Speech Pro, clean | 3.48/4.70/4.09 | 6.99/7.98/7.48 | 22.4/32.0/27.2 |
| Fluid Speech Pro, raw | 2.73/4.06/3.39 | 7.32/8.43/7.87 | 23.7/28.0/25.8 |
| Granite Speech 4.1 2B | 4.31/4.30/4.30 | 7.62/7.71/7.66 | 22.4/33.3/27.8 |
| Granite Speech 4.1 2B Plus | 4.20/5.02/4.61 | 7.71/8.52/8.11 | 25.0/32.0/28.5 |
| Granite 5.0 2B, written + dictionary | 3.74/4.03/3.88 | 7.59/7.74/7.66 | 22.4/32.0/27.2 |
| Granite 5.0 2B, verbatim | 3.76/4.64/4.20 | 7.20/7.56/7.38 | 23.7/32.0/27.8 |
| Granite 5.0 2B, written | 3.88/4.67/4.27 | 6.60/7.62/7.11 | 26.3/30.7/28.5 |
| Oruk Qwen 1.7B | 2.16/2.79/2.47 | 8.04/8.07/8.05 | 23.7/29.3/26.5 |
| Oruk Qwen 1.7B + Gemma E2B | 5.03/5.37/5.20 | 6.48/7.17/6.83 | 23.7/36.0/29.8 |
| Parakeet unified 0.6B | 3.51/3.92/3.71 | 6.96/7.50/7.23 | 25.0/34.7/29.8 |
| Qwen3-ASR 0.6B | 3.53/4.56/4.04 | 8.96/9.75/9.35 | 19.7/26.7/23.2 |
| Qwen3-ASR 1.7B | 2.27/2.84/2.56 | 7.32/7.80/7.56 | 25.0/32.0/28.5 |
| Qwen3-ASR 1.7B + Gemma E2B | 5.29/5.52/5.40 | 6.07/7.08/6.57 | 23.7/37.3/30.5 |
| Qwen 1.7B, generic vocabulary | 2.33/2.76/2.54 | 7.17/7.68/7.42 | 26.3/33.3/29.8 |
| Qwen 1.7B, user dictionary | 2.30/3.02/2.66 | 7.14/7.95/7.54 | 26.3/29.3/27.8 |
| Rumble-2-Flash | 3.36/3.89/3.62 | 7.05/7.95/7.50 | 26.3/30.7/28.5 |
| Parakeet TDT 1.1B | 4.05/4.47/4.26 | 8.78/9.69/9.24 | 25.0/25.3/25.2 |
| Granite 470M TurboCTC | 3.68/4.47/4.07 | 7.80/8.43/8.11 | 19.7/24.0/21.9 |

### Sendable drafts and edit burden

ZER measures the percentage of drafts requiring zero edits. AMEND pools character edits over character counts on each half. The reference, draft and spoken transcript form the amend-va 0.2.1 v1 judge input. Identical prompts reuse each judge's own cached answers. Dashes indicate unavailable judgments.

| Engine and mode | Claude ZER ↑ | Claude AMEND ↓ | Codex ZER ↑ | Codex AMEND ↓ |
| --- | ---: | ---: | ---: | ---: |
| Fluid Speech Pro, clean | — | — | 52.6/53.3/53.0 | 1.49/1.29/1.39 |
| Fluid Speech Pro, raw | — | — | 51.3/50.7/51.0 | 1.62/1.47/1.54 |
| Granite 5.0 2B, written + dictionary | — | — | 51.3/58.7/55.0 | 1.47/1.36/1.42 |
| Granite 5.0 2B, verbatim | — | — | 48.7/61.3/55.0 | 1.50/1.24/1.37 |
| Granite 5.0 2B, written | — | — | 51.3/57.3/54.3 | 1.27/1.31/1.29 |
| Oruk Qwen 1.7B | — | — | 53.9/60.0/57.0 | 1.57/1.59/1.58 |
| Oruk Qwen 1.7B + Gemma E2B | — | — | 76.3/72.0/74.2 | 0.47/0.71/0.59 |
| Parakeet unified 0.6B | — | — | 46.1/52.0/49.0 | 1.42/1.24/1.33 |
| Qwen3-ASR 0.6B | — | — | 48.7/46.7/47.7 | 1.80/1.98/1.89 |
| Qwen3-ASR 1.7B | — | — | 56.6/58.7/57.6 | 1.41/1.47/1.44 |
| Qwen3-ASR 1.7B + Gemma E2B | — | — | 71.1/70.7/70.9 | 0.51/0.74/0.63 |
| Qwen 1.7B, user dictionary | — | — | 56.6/56.0/56.3 | 1.39/1.50/1.45 |
| Rumble-2-Flash | — | — | 52.6/50.7/51.7 | 1.58/1.41/1.49 |
| Granite 470M TurboCTC | — | — | 44.7/34.7/39.7 | 1.79/1.89/1.84 |

## Custom words (#215)

Qwen accepted the vocabulary through the existing Rust `RunOptions::vocabulary` field. Granite accepted the same list appended to the written instruction. IBM documents up to 2,000 entries. This experiment used 24, selected from tune names and tools plus plausible distractors, and froze the list for all 151 clips.

| Recognizer | Tune occurrences correct, plain → biased (19) | Check occurrences correct, plain → biased (6) | New wrong terms on check |
| --- | ---: | ---: | --- |
| Qwen 1.7B | 15 → 18 | 6 → 6 | **Zillow** for Zelle (0629), **Amaya** in a benefits URL (0729) |
| Granite 2B written | 16 → 18 | 6 → 6 | **Zillow** for Zelle (0629) |

Whole-word case-insensitive counting also flagged “Docker file” for “Dockerfile” on 0310. That spacing difference is excluded from the wrong-term column. A separate 12-term generic engineering list had just one target occurrence, TypeScript, already correct with and without Qwen bias. The larger list was Priya Raman, Carlos, Hannah, Lakeshore, Castellano, Moreno, Zillow, JavaScript, Docker, Tailwind, Stripe, WebSocket, Delancey, Clearwater, TypeScript, GraphQL, Vitest, Grafana, Datadog, Twilio, Amaya, Lakehurst, Costello and Alina.

Keep #215 opt-in. Bias recovered names on clips containing tune-selected terms. On check there was no remaining target error to fix, and bias introduced unrelated entities. Before enabling it by default, use fresh speakers with actual user dictionaries and include clips where none of their words should appear.

## Runtime and shipping constraints

- **Parakeet, both base Qwens, Oruk and Granite TurboCTC** ran through a Metal build of Vocet's exact transcribe-cpp **and** transcribe-cpp-sys 0.4.1. The harness loaded each GGUF, created one session, and submitted all clips with English and default run options. The native binary's deployment target was macOS 11. Vocet targets macOS 13. This establishes loader compatibility on macOS 15.7.4. A real macOS 13 end-to-end test remains outstanding.
- **Oruk** publishes a 4.08 GB Transformers 5 safetensors checkpoint. The evaluated download for a user would be the 1.32 GB Q4_K_M export. The conversion unpacked the publisher's tokenizer and feature extractor, mapped its audio tower, language model and projector layers to the older Qwen tensor names, and preserved all weight bytes before the upstream BF16 GGUF conversion and quantization. The converter and quantizer came from transcribe.cpp commit `db678a03`; the final GGUF SHA-256 was `057471662f060800a6374708ad679d4abe51775e52ca591a88ea4ade78b273bb`. [Oruk checkpoint](https://huggingface.co/oruk/orukqwen-asr-1.7b/tree/a526248ceb2b829cb8f2e1eec764eafbd6a10fe4)
- **Fluid Speech Pro** used the weights repository's MLX loader with parakeet-mlx 0.7.0, 8-bit weights and the optional clean adapter. The MLX 0.32.4 wheel used here requires macOS 15 on Apple silicon. Supporting macOS 13 with clean mode would require testing the documented ONNX route and packaging a separate adapter. The weights require CC BY 4.0 attribution. [Fluid MLX weights and loader](https://huggingface.co/FluidVoice-Labs/fluid-speech-pro-mlx/tree/aeb1d4e43487586b15d86c0a1dd4511e1cf6b7a0)
- **Rumble-2-Flash** used mlx-audio 0.6.3 with the system tokens `<digits><nofiller><noverbatim>` and English. The runner verified that the loaded decoder's stop set was exactly **99564 and 99562**, and all 151 clips returned. This Apache-2.0 MLX port has the same macOS deployment limitation. [Rumble Mac model card](https://huggingface.co/TypeWhisper/Rumble-2-Flash-8bit/tree/feb7feb016e58ec3f39f961dfb78e4837325a0d1)
- **Granite Speech 5.0 2B** used IBM's Q4_K_M weights and F16 projector through llama.cpp build 12359's loopback transcription endpoint. The installed binary has a macOS 15 deployment target. Shipping on macOS 13 requires a compatible llama.cpp build and integration test. These three alternate runtimes are outside the 0.4.1 catalog; no newer transcribe.cpp release was verified for them. [IBM GGUF and projector](https://huggingface.co/ibm-granite/granite-speech-5.0-2b-GGUF/tree/945b4581e2b40cd9a3121de265fd8e387bdf782d)

Direct loader probes on 0.4.1 rejected the IBM Granite 2B GGUF with “unsupported architecture” (status 5) and the two MLX safetensors artifacts with a GGUF format error (status 4). No candidate that completed this experiment required an upgrade of Vocet's Rust dependency. The external engines require a different runtime.

Granite's written instruction was “Transcribe the speech into send-ready written English with proper punctuation, capitalization, and digits. Remove filler words and accidental repetitions. Preserve names, meaning, and deliberate repetition.” The control was “Transcribe the speech verbatim in English.” The two requests produced different strings on 147 of 151 clips.
