# Where the reused files come from

Copied unchanged (read-only sources; other workers own those worktrees):

| File here | Origin |
| --- | --- |
| `guard.py`, `offline_cleanup.py`, `test_offline_cleanup.py`, `prompts/v4.txt`, `prompts/v1-blurt-smart.txt` | `eval/cleanup-offline/` on branch `eval/cleanup-offline` (PR #327) |
| `input/nemotron_sol_v4/*` | `eval/cleanup-offline/paired-results/nemotron_sol_v4/` (PR #327): the Nemotron → GPT-6.2-sol v4 pairing and its judge CSVs, the baseline for paired wins/losses |
| `input/mistralai__voxtral-*/*`, `input/nvidia__nemotron-3.5-transcribe/harness_results.csv` | `eval/hosted-asr/results/<label>/` on branch `eval/hosted-asr` (PR #330): the saved hosted raw transcripts, their usage and the Codex judge cache so far |
| `../voicearena/score_codex_cli.py` | branch `eval/codex-judge` |
| `../groq-cleanup/split.csv` | `eval/groq-cleanup/split.csv` (fixed speaker split, tune/check) |

Adapted:

- `run_two_pass.py` is `eval/cleanup-offline/run_nemotron_sol.py` (PR #327) with the Voxtral inputs, a
  ledger shared with part B (`budget.py`), a per-run sub-cap and four attempts per request so 429s
  are retried.
- `audio_llm.py` takes `wav_bytes()`, the request hygiene (no response bodies on errors, the key only
  in the header) and the retry/backoff pattern from `eval/hosted-asr/transcribe_openrouter.py`
  (PR #330).
- `prompts/audio_onepass_v4.txt` is `prompts/v4.txt` reworded for audio input. The "transcript
  between tags" framing became "one audio recording". The ASR-error rules ("Thank you." that nobody
  said, misheard words) became "never write words nobody said; spell technical terms correctly".
  The other rules are unchanged.
- `../voicearena/score_gemini_cli.py` is `../voicearena/score_codex_cli.py` with the call routed through
  the Antigravity CLI (`agy -p`, Gemini 4 Argon High) instead of `codex exec`. The prompt, schema,
  parsing, cache and output columns are unchanged.
- `closed_asr.py` reuses `audio_llm.py`'s `wav_bytes()`, guard step, ledger, cache and retry pattern,
  with request builders for Google's Interactions API and OpenAI's `/v1/audio/transcriptions`
  (both from the providers' docs, 2026-11-19).
- `local/` (Gemma 4 E2B v3 via llama.cpp) is copied from `eval/local-cleanup` (local-cleanup-eval worktree).
