#!/usr/bin/env bash
# Transcribe the Voice Arena public set (151 clips) with every hosted open-weight candidate on OpenRouter.
#
#   OPENROUTER_API_KEY="$(security find-generic-password -s com.dictationapp.eval -a openrouter-api-key -w)" \
#     eval/hosted-asr/run_openrouter.sh [extra transcribe_openrouter.py args, e.g. --limit 3]
#
# The key comes only from the environment; it is never printed or written. The audio goes to
# OpenRouter and the provider it routes each request to (see REPORT.md). Rerun the same command to
# resume: clips already transcribed are skipped. Spend stops at $2.00 (--max-usd) across all models.
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd)
CLIPS=$HERE/../voicearena/clips

[ -n "${OPENROUTER_API_KEY:-}" ] || { echo "OPENROUTER_API_KEY is not set" >&2; exit 2; }
command -v uv >/dev/null || { echo "uv is needed: https://docs.astral.sh/uv/" >&2; exit 2; }
if [ ! -f "$CLIPS/manifest.csv" ]; then
  echo "fetching the 151 clips into $CLIPS"
  uv run --quiet "$HERE/../voicearena/fetch_dataset.py" "$CLIPS"
fi

# Open-weight models OpenRouter serves that beat whisper-large-v3-turbo (8.75) on the Open ASR
# Leaderboard, plus two screens (README.md says why each is here).
MODELS=(
  nvidia/nemotron-3.5-transcribe        # OAL 6.13
  qwen/qwen3-asr-1.7b                   # OAL 6.15
  nvidia/parakeet-tdt-0.6b-v3           # OAL 7.04
  qwen/qwen3-asr-0.6b                   # OAL 7.07
  mistralai/voxtral-small-24b-2507-stt  # OAL 7.44
  moondream/parakeet-ultra              # not on the OAL; base of fluid-speech-pro (6.06)
  openai/whisper-large-v3-turbo         # control: Groq's model through the same route (OAL 8.75)
)
args=()
for m in "${MODELS[@]}"; do args+=(--model "$m"); done
cd "$HERE"
exec uv run --quiet "$HERE/transcribe_openrouter.py" "${args[@]}" "$@"
