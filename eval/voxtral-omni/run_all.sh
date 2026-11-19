#!/usr/bin/env bash
# Owner command for parts A and B (one OpenRouter key, one $2.00 cap across both):
#
#   OPENROUTER_API_KEY="$(security find-generic-password -s com.dictationapp.eval -a openrouter-api-key -w)" \
#     eval/voxtral-omni/run_all.sh
#
# Part A (run_two_pass.py): GPT-6.2-sol low + v4 + guard on the saved Voxtral Mini and Small hosted
# transcripts (text only, 302 requests). Part B (audio_llm.py): 4 audio-input chat models x 151 clips
# x 2 prompts (1,208 requests; the clips go to OpenRouter and its providers). A runs alongside B.
# The key stays in this process's environment; nothing prints or stores it. Rerun the same command to
# resume: cached answers are not asked again, and spend already made counts against the cap.
set -uo pipefail
HERE=$(cd "$(dirname "$0")" && pwd)
[ -n "${OPENROUTER_API_KEY:-}" ] || { echo "OPENROUTER_API_KEY is not set" >&2; exit 2; }
command -v uv >/dev/null || { echo "uv is needed: https://docs.astral.sh/uv/" >&2; exit 2; }
CLIPS=$HERE/../voicearena/clips
if [ ! -f "$CLIPS/manifest.csv" ]; then
  echo "fetching the 151 clips into $CLIPS"
  uv run --quiet "$HERE/../voicearena/fetch_dataset.py" "$CLIPS" || exit 2
fi
mkdir -p "$HERE/results"
python3 "$HERE/run_two_pass.py" > >(sed -u 's/^/[A] /') 2>&1 &
a=$!
uv run --quiet "$HERE/audio_llm.py" "$@" 2>&1 | sed -u 's/^/[B] /'
b=${PIPESTATUS[0]}
wait "$a"; a=$?
python3 - "$HERE/results/budget.jsonl" <<'PY'
import json, sys
used = sum(json.loads(l)["microdollars"] for l in open(sys.argv[1]) if l.strip()) / 1e6
print(f"total OpenRouter spend recorded: ${used:.4f} of $2.00")
PY
echo "part A exit $a, part B exit $b (0 = every clip answered)"
exit $(( a > b ? a : b ))
