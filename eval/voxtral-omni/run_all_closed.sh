#!/usr/bin/env bash
# Owner command for the closed-model comparison (Gemini + OpenAI transcription, then OpenRouter
# text clean-up of OpenAI's transcripts):
#
#   GEMINI_API_KEY="$(security find-generic-password -s com.dictationapp.eval -a gemini-api-key -w)" \
#   OPENAI_API_KEY="$(security find-generic-password -s com.dictationapp.eval -a openai-api-key -w)" \
#   OPENROUTER_API_KEY="$(security find-generic-password -s com.dictationapp.eval -a openrouter-api-key -w)" \
#     eval/voxtral-omni/run_all_closed.sh
#
# closed_asr.py: 151 clips; Gemini 3.5 Transcribe (verbatim + smart), Gemini 3.5 Flash-Lite and 3.8
# Flash (plain + one-pass), OpenAI gpt-transcribe and gpt-4o-mini-transcribe (plain). The clips go to
# Google and OpenAI. Caps: $3.00 per provider (results/budget_gemini.jsonl, budget_openai.jsonl).
# run_matrix.py: GPT-6-luna, GLM-5.3 Flash and GPT-6.2-sol (prompt v4 + guard) on gpt-transcribe's
# transcripts, text only, counted in the existing OpenRouter ledger (results/budget.jsonl, $3.00).
# A key that is not set skips that part. Keys stay in this process's environment; nothing prints or
# stores them. Rerun the same command to resume: cached answers are not asked again.
set -uo pipefail
HERE=$(cd "$(dirname "$0")" && pwd)
command -v uv >/dev/null || { echo "uv is needed: https://docs.astral.sh/uv/" >&2; exit 2; }
CLIPS=$HERE/../voicearena/clips
if [ ! -f "$CLIPS/manifest.csv" ]; then
  echo "fetching the 151 clips into $CLIPS"
  uv run --quiet "$HERE/../voicearena/fetch_dataset.py" "$CLIPS" || exit 2
fi
uv run --quiet "$HERE/closed_asr.py" "$@"
c=$?
a=0
if [ -n "${OPENROUTER_API_KEY:-}" ] && [ -f "$HERE/results/openai_transcribe_plain/harness_results.csv" ]; then
  python3 "$HERE/run_matrix.py" --only openai_transcribe__luna_v4 --only openai_transcribe__glm_v4 \
    --only openai_transcribe__sol_v4 || a=$?
else
  echo "skipping the clean-up of OpenAI's transcripts (OPENROUTER_API_KEY not set or no transcripts)"
fi
echo "closed models exit $c, clean-up exit $a (0 = every clip answered)"
exit $(( c > a ? c : a ))
