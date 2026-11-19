### ZER (drafts needing no edit, %) and AMEND (% of characters to fix)

| Run | Claude ZER tune / check / **all** (95%) | Codex ZER tune / check / **all** (95%) | Claude AMEND | Codex AMEND | WER vs spoken | WER vs intended |
| --- | --- | --- | ---: | ---: | ---: | ---: |
| Parakeet raw | 51.3 / 54.7 / **53.0** (45–62) | 48.7 / 52.0 / **50.3** (42–59) | 1.38 | 1.36 | 3.85 | 7.02 |
| Parakeet + Gemma 4 E2B text cleanup (v3, guard) | 67.1 / 64.0 / **65.6** (58–73) | 64.5 / 60.0 / **62.3** (55–70) | 0.76 | 0.79 | 5.73 | 6.42 |
| Parakeet + Gemma 4 E4B text cleanup (v3, guard) | 67.1 / 64.0 / **65.6** (58–73) | 65.8 / 61.3 / **63.6** (56–71) | 0.64 | 0.71 | 5.67 | 6.30 |
| Gemma 4 E2B one pass, no guard | 50.0 / 49.3 / **49.7** (41–58) | 50.0 / 49.3 / **49.7** (41–58) | 1.46 | 1.45 | 4.80 | 8.20 |
| Gemma 4 E2B one pass + guard | 48.7 / 49.3 / **49.0** (41–58) | 48.7 / 49.3 / **49.0** (41–58) | 1.64 | 1.61 | 4.71 | 8.42 |
| Gemma 4 E2B plain transcription | 48.7 / 44.0 / **46.4** (38–55) | 51.3 / 44.0 / **47.7** (39–56) | 1.87 | 1.76 | 4.74 | 8.74 |
| Gemma 4 E2B plain_then_v3 | 63.2 / 53.3 / **58.3** (50–67) | 63.2 / 53.3 / **58.3** (50–67) | 1.17 | 1.15 | 6.56 | 8.11 |
| Gemma 4 E4B one pass, no guard | 52.6 / 54.7 / **53.6** (46–61) | 52.6 / 53.3 / **53.0** (45–61) | 1.34 | 1.28 | 3.95 | 7.29 |
| Gemma 4 E4B one pass + guard | – | 52.6 / 53.3 / **53.0** (44–61) | – | 1.24 | 3.94 | 7.28 |
| Gemma 4 E4B plain transcription | – | 56.6 / 50.7 / **53.6** (46–62) | – | 1.36 | 3.95 | 7.41 |

### Clips over 30 s (13, split for Gemma) vs up to 30 s (138)

| Run | Claude ZER ≤30 s / >30 s | Codex ZER ≤30 s / >30 s | Claude AMEND ≤30 s / >30 s | WER vs spoken >30 s |
| --- | --- | --- | --- | ---: |
| Parakeet raw | 57.2 / 7.7 | 54.3 / 7.7 | 1.54 / 1.04 | 3.69 |
| Parakeet + Gemma 4 E2B text cleanup (v3, guard) | 68.8 / 30.8 | 65.2 / 30.8 | 0.82 / 0.63 | 5.01 |
| Parakeet + Gemma 4 E4B text cleanup (v3, guard) | 68.8 / 30.8 | 66.7 / 30.8 | 0.68 / 0.55 | 5.10 |
| Gemma 4 E2B one pass, no guard | 53.6 / 7.7 | 53.6 / 7.7 | 1.68 / 1.01 | 4.87 |
| Gemma 4 E2B one pass + guard | 52.9 / 7.7 | 52.9 / 7.7 | 1.86 / 1.19 | 5.01 |
| Gemma 4 E2B plain transcription | 50.7 / 0.0 | 52.2 / 0.0 | 2.16 / 1.26 | 5.10 |
| Gemma 4 E2B plain_then_v3 | 63.8 / 0.0 | 63.8 / 0.0 | 1.25 / 1.01 | 5.65 |
| Gemma 4 E4B one pass, no guard | 58.0 / 7.7 | 57.2 / 7.7 | 1.55 / 0.91 | 4.42 |
| Gemma 4 E4B one pass + guard | – / – | 57.2 / 7.7 | – / – | 4.19 |
| Gemma 4 E4B plain transcription | – / – | 57.2 / 15.4 | – / – | 4.37 |

### Paired, clip by clip: wins / losses / ties (by "needs no edit")

| Run | vs Parakeet raw: Claude | Codex | vs Parakeet + E2B text cleanup: Claude | Codex |
| --- | --- | --- | --- | --- |
| Parakeet raw | – | – | 2 / 21 / 128 | 3 / 21 / 127 |
| Parakeet + Gemma 4 E2B text cleanup (v3, guard) | 21 / 2 / 128 | 21 / 3 / 127 | – | – |
| Parakeet + Gemma 4 E4B text cleanup (v3, guard) | 21 / 2 / 128 | 21 / 1 / 129 | 3 / 3 / 145 | 4 / 2 / 145 |
| Gemma 4 E2B one pass, no guard | 17 / 22 / 112 | 18 / 19 / 114 | 13 / 37 / 101 | 15 / 34 / 102 |
| Gemma 4 E2B one pass + guard | 17 / 23 / 111 | 18 / 20 / 113 | 13 / 38 / 100 | 15 / 35 / 101 |
| Gemma 4 E2B plain transcription | 11 / 21 / 119 | 13 / 17 / 121 | 11 / 40 / 100 | 14 / 36 / 101 |
| Gemma 4 E2B plain_then_v3 | 27 / 19 / 105 | 28 / 16 / 107 | 13 / 24 / 114 | 16 / 22 / 113 |
| Gemma 4 E4B one pass, no guard | 18 / 17 / 116 | 19 / 15 / 117 | 15 / 33 / 103 | 17 / 31 / 103 |
| Gemma 4 E4B one pass + guard | – | 18 / 14 / 119 | – | 16 / 30 / 105 |
| Gemma 4 E4B plain transcription | – | 19 / 14 / 118 | – | 18 / 31 / 102 |

### AMEND and intended-text WER by half

| Run | Claude AMEND tune / check | Codex AMEND tune / check | WER tune / check |
| --- | --- | --- | --- |
| Parakeet raw | 1.46 / 1.30 | 1.28 / 1.43 | 6.33 / 7.72 |
| Parakeet + Gemma 4 E2B text cleanup (v3, guard) | 0.72 / 0.80 | 0.65 / 0.93 | 5.58 / 7.26 |
| Parakeet + Gemma 4 E4B text cleanup (v3, guard) | 0.60 / 0.67 | 0.60 / 0.80 | 5.25 / 7.35 |
| Gemma 4 E2B one pass, no guard | 1.45 / 1.48 | 1.38 / 1.53 | 7.89 / 8.50 |
| Gemma 4 E2B one pass + guard | 1.62 / 1.66 | 1.53 / 1.69 | 7.95 / 8.90 |
| Gemma 4 E2B plain transcription | 1.92 / 1.83 | 1.70 / 1.83 | 8.40 / 9.08 |
| Gemma 4 E2B plain_then_v3 | 1.04 / 1.30 | 1.00 / 1.30 | 7.59 / 8.62 |
| Gemma 4 E4B one pass, no guard | 1.60 / 1.09 | 1.44 / 1.13 | 7.20 / 7.38 |
| Gemma 4 E4B one pass + guard | – | 1.41 / 1.07 | 7.20 / 7.35 |
| Gemma 4 E4B plain transcription | – | 1.51 / 1.22 | 7.32 / 7.50 |
