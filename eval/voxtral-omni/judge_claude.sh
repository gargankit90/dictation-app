#!/usr/bin/env bash
# Second-judge (Claude Opus 5.5) pass, in parallel lanes. Each lane runs its folders in order so a
# guarded one-pass run reuses the verdicts of its unguarded run (score.sh seeds the shared cache).
#   eval/voxtral-omni/judge_claude.sh partb|partA-top|partA-rest
set -uo pipefail
HERE=$(cd "$(dirname "$0")" && pwd)
R=$HERE/results
L=$HERE/results-local
export CLAUDE_WORKERS=${CLAUDE_WORKERS:-3}
lane() {
  for r in "$@"; do
    echo "=== $(basename "$r") ==="
    "$HERE/score.sh" "$r" claude 2>&1 | grep -v '^  judged' | tail -4
  done
}
case "${1:?which}" in
  partb)
    lane "$R/mimo_v25_plain" "$R/mimo_v25_onepass" "$R/mimo_v25_onepass_guard" &
    lane "$R/nemotron_omni_plain" "$R/nemotron_omni_onepass" "$R/nemotron_omni_onepass_guard" &
    lane "$R/inkling_small_plain" "$R/inkling_small_onepass" "$R/inkling_small_onepass_guard" &
    lane "$R/voxtral_small_chat_plain" "$R/voxtral_small_chat_onepass" "$R/voxtral_small_chat_onepass_guard" &
    ;;
  partA-top)
    lane "$R/nemotron__glm_v4" "$R/nemotron__luna_v4" &
    lane "$L/nemotron__gemma_v3" "$R/voxtral_small__sol_v4" &
    lane "$R/voxtral_mini_dev__sol_v4" "$R/voxtral_mini_dev__glm_v4" &
    lane "$R/voxtral_mini__sol_v4" "$R/voxtral_mini__glm_v4" &
    ;;
  partA-rest)
    lane "$R/oruk_dev__sol_v4" "$R/oruk_dev__glm_v4" "$R/oruk_dev__luna_v4" "$R/oruk_dev__oss_v4" &
    lane "$R/nemotron__oss_v4" "$R/voxtral_mini__luna_v4" "$R/voxtral_mini__oss_v4" "$L/voxtral_mini__gemma_v3" &
    lane "$R/voxtral_mini_dev__luna_v4" "$R/voxtral_mini_dev__oss_v4" &
    lane "$R/voxtral_small__glm_v4" "$R/voxtral_small__luna_v4" "$R/voxtral_small__oss_v4" "$L/voxtral_small__gemma_v3" &
    ;;
esac
wait
echo "lanes done: $1"
