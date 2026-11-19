# Can one local Gemma call replace Parakeet and text cleanup?

**Keep Parakeet followed by Gemma text cleanup for Vocet.** Gemma E2B can return cleaned text directly from audio in well under a second on an M4 Pro. Both judges accepted 75 of its 151 drafts without edits. With Parakeet followed by E2B, Claude accepted 99 and Codex accepted 94. Giving the same E2B weights a second call to edit their own transcription raised acceptance to 88 clips on both judges. Separating recognition from editing helped even without changing models.

## Quality on the public Voice Arena clips

ZER is the percentage of drafts the judge would paste unchanged. AMEND measures character edits, with lower values better. Each column uses the same 151 clips. The independent speaker halves contain 76 tune and 75 check clips. These are unofficial amend-va 0.2.1 v1 scores from Claude Opus 5.5 and GPT-6 Astra at max effort. Identical judge inputs reuse saved judgments. [Full tables with tune/check ZER, AMEND and WER, speaker-bootstrap intervals, long clips and paired outcomes](tables.md).

| Run | Claude ZER | Codex ZER | Claude AMEND | Codex AMEND | Intended-text WER |
| --- | ---: | ---: | ---: | ---: | ---: |
| Parakeet raw | 53.0% | 50.3% | 1.38% | 1.36% | 7.02% |
| Parakeet + Gemma 4 E2B text cleanup (v3, guard) | 65.6% | 62.3% | 0.76% | 0.79% | 6.42% |
| Parakeet + Gemma 4 E4B text cleanup (v3, guard) | 65.6% | 63.6% | 0.64% | 0.71% | 6.30% |
| Gemma 4 E2B one pass, no guard | 49.7% | 49.7% | 1.46% | 1.45% | 8.20% |
| Gemma 4 E2B one pass + guard | 49.0% | 49.0% | 1.64% | 1.61% | 8.42% |
| Gemma 4 E2B plain transcription | 46.4% | 47.7% | 1.87% | 1.76% | 8.74% |
| Gemma 4 E2B plain then separate v3 text cleanup | 58.3% | 58.3% | 1.17% | 1.15% | 8.11% |
| Gemma 4 E4B one pass, no guard | 53.6% | 53.0% | 1.34% | 1.28% | 7.29% |
| Gemma 4 E4B one pass + guard | Pending | 53.0% | Pending | 1.24% | 7.28% |
| Gemma 4 E4B plain transcription | Pending | 53.6% | Pending | 1.36% | 7.41% |

Claude completed every primary comparison, including E4B one-pass. Its session cap interrupted the supplemental E4B plain control; 71 plain drafts and 25 guarded drafts lack Claude judgments (overlapping inputs). Partial-cache averages are omitted. The run used 338 new completed Claude judgments and the remaining results reused cached answers.

Against the Parakeet + E2B pipeline, unguarded E2B one-pass won 13 clips and lost 37 by Claude's zero-edit decision. Codex counted 15 wins and 34 losses. On the check half both judges accepted 49.3% of one-pass drafts, compared with 64.0% and 60.0% for the pipeline.

The guard needs a plain transcript for comparison, so deploying it requires a second audio pass. It restored E2B's plain draft on 31 clips (25 number, four name, two URL/email flags) and lowered both judges' acceptance by one clip. Adding the two measured request times clip by clip gives a guarded median of 1.24 s for E2B and 2.65 s for E4B. A misheard identifier in the plain draft can become the fallback.

Moving the full cleanup instruction into the user turn increased tune intended-text WER from 7.89% to 8.28%. The check-half results use the original system instruction.

## Long dictation and content loss

All 13 recordings longer than 30 seconds produced output from 38 requests across 25 joins. We decoded 16 kHz mono audio, cut at the quietest 300 ms window between 18 and 29 seconds, included 150 ms on either side of each cut, then joined the independent replies with spaces. E2B one-pass passed on 1/13 long clips and 74/138 shorter clips on each judge. Parakeet + E2B passed on 4/13 long clips on each. Longer drafts also give a judge more opportunities to find an edit, so this comparison cannot isolate the cost of splitting. [Google's audio input specification](https://ai.google.dev/gemma/docs/capabilities/audio).

The alignment screen flagged five E2B one-pass clips. On dict_0428 it heard “we are setting up” as “or sitting at”, and on dict_0487 it changed “giving you a” to “give me your”. The screen also flagged spellings of technical terms, so its counts include false positives. It found no repeated three-word sequence at a one-pass join. Plain E2B drew a split-point flag on dict_0119 for a differently formatted phone number. With the guard enabled, the ends of dict_0544 and dict_0729 lost recognizable email or portal components. Short drops and context lost across separate calls can escape this screen. [Flagged passages](audit.csv) · [Counts by run](audit.md).

## Download, delay and shipping

| Audio model | Weights + projector | Load | Median / p90 per clip | Peak footprint / max RSS |
| --- | ---: | ---: | ---: | ---: |
| E2B Q4_K_M | 3.11 + 0.99 GB | 1.21 s | 0.67 / 1.55 s | 1.59 / 4.70 GB |
| E4B Q4_K_M | 4.98 + 0.99 GB | 1.21 s | 1.60 / 3.71 s | 1.82 / 6.84 GB |

The main runs used llama.cpp build 12359 (b1b770d90), llama-server with the model's chat template, Metal, and the F16 multimodal projector. The table measures the unguarded one-pass runs. Peak footprint and maximum resident set are distinct process measurements from macOS time -l. Warm clip times cover model requests and joining, while load measures server startup to readiness. Shared Mac activity and filesystem caching make both rough. E2B plain took 0.60 s median and the separate E2B transcription + text-editing sequence took 1.30 s. The historical Parakeet harness measured release-to-paste in a VM, so its delay cannot establish a speedup on this Mac.

A Rust app could launch the native sidecar or bind llama.cpp and its multimodal library. The tested Homebrew llama-server binary declares macOS 15.0 as its minimum. Supporting Vocet on macOS 13 would require building compatible native dependencies, packaging and signing them, and testing Metal and audio inference there. Using the 477 MB Parakeet English model plus the 3.11 GB E2B text model totals about 3.58 GB. The 4.09 GB one-pass download costs roughly 0.51 GB more because the projector outweighs the recognizer it replaces. E2B text cleanup alone took 0.52 s median after Parakeet finished.

A 30-short-clip cross-check with Google's original E2B weights under Transformers on MPS found spoken WER of 6.29% for one-pass versus 6.09% for llama.cpp on the same clips. Moving to the original bf16 weights did not rescue this sample. This was a runtime check with a fixed subset, not a second full benchmark. [Cross-check results](runs/hf_check/summary.txt).
