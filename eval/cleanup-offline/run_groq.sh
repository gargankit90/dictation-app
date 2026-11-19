#!/usr/bin/env bash
# The Groq runs of the offline cleanup eval, for the OWNER to run once (agents never handle the key):
#
#   GROQ_API_KEY="$(security find-generic-password -s com.dictationapp.desktop -a groq-api-key -w)" eval/cleanup-offline/run_groq.sh
#
# Reads GROQ_API_KEY from its environment only (never printed or written). Runs, in order:
#   oss20b_v1   openai/gpt-oss-20b  + v1 (the VM run's model and prompt: the offline-vs-VM check)
#   oss120b_v4  openai/gpt-oss-120b + v4 (v3 rules + fixing misheard technical words)
#   oss120b_g3  openai/gpt-oss-120b + g3 (the Groq cleanup worker's v3.txt)
#   oss120b_g4  openai/gpt-oss-120b + g4 (the Groq cleanup worker's v4.txt)
#   oss120b_v1  openai/gpt-oss-120b + v1
# 151 requests each (about 750 in all, several minutes), cached in results/<label>/cleanup.jsonl, so a rerun
# only repeats what failed. Text only goes to Groq (the saved Whisper transcripts), never audio.
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd)
: "${GROQ_API_KEY:?set GROQ_API_KEY in this command environment, see the header}"
export GROQ_API_KEY
status=0
for run in "oss20b_v1 openai/gpt-oss-20b v1-blurt-smart" "oss120b_v4 openai/gpt-oss-120b v4" \
           "oss120b_g3 openai/gpt-oss-120b g3" "oss120b_g4 openai/gpt-oss-120b g4" \
           "oss120b_v1 openai/gpt-oss-120b v1-blurt-smart"; do
  set -- $run
  echo "== $1"
  python3 "$HERE/offline_cleanup.py" --provider groq --model "$2" --prompt "$HERE/prompts/$3.txt" --label "$1" \
    --attempts 10 --workers 2 || status=$?
done
echo "done (exit $status); tell the Lead so the worker can score these runs"
exit $status
