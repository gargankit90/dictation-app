#!/usr/bin/env bash
# The OpenRouter runs of the offline cleanup eval (big open-weight models), for the OWNER to run once
# (agents never handle the key):
#
#   OPENROUTER_API_KEY="$(security find-generic-password -s com.dictationapp.eval -a openrouter-api-key -w)" eval/cleanup-offline/run_openrouter.sh
#
# Reads OPENROUTER_API_KEY from its environment only (never printed or written). Prompt v4 on the 151
# saved Whisper texts, the app's guard, one model at a time, cheapest first. Licences checked on
# Hugging Face (MIT or Apache-2.0, open weights). A hard cap of $2.00 total spend (OpenRouter's own
# per-reply cost, summed over every run) stops everything; a rerun resumes from cache and keeps the cap.
#   or_dsv41flash_v4   deepseek/deepseek-v4.1-flash   (MIT)
#   or_gemma4_31b_v4   google/gemma-4-31b-it          (Apache-2.0)
#   or_glm53flash_v4   z-ai/glm-5.3-flash             (MIT)
#   or_mistral3_v4     mistralai/mistral-large-2512   (Mistral Large 3, Apache-2.0)
#   or_dsv4pro_v4      deepseek/deepseek-v4-pro       (MIT)
#   or_qwen35big_v4    qwen/qwen3.5-397b-a17b         (Apache-2.0)
# About 900 text-only requests; expected spend about $0.60 (see REPORT.md).
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd)
: "${OPENROUTER_API_KEY:?set OPENROUTER_API_KEY in this command environment, see the header}"
export OPENROUTER_API_KEY
status=0
for run in "or_dsv41flash_v4 deepseek/deepseek-v4.1-flash" "or_gemma4_31b_v4 google/gemma-4-31b-it" \
           "or_glm53flash_v4 z-ai/glm-5.3-flash" "or_mistral3_v4 mistralai/mistral-large-2512" \
           "or_dsv4pro_v4 deepseek/deepseek-v4-pro" "or_qwen35big_v4 qwen/qwen3.5-397b-a17b"; do
  set -- $run
  echo "== $1"
  python3 "$HERE/offline_cleanup.py" --provider openrouter --model "$2" --prompt "$HERE/prompts/v4.txt" \
    --label "$1" --attempts 6 --workers 4 --timeout 60 --max-cost 2.00 ${OR_EXTRA:-} || status=$?
done
echo "done (exit $status); tell the Lead so the worker can score these runs"
exit $status
