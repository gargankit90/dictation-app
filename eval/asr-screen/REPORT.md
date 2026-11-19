# On-device recognition on Voice Arena

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
