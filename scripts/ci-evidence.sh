#!/usr/bin/env bash
# Gathers what a failed CI job needs for diagnosis into target/test-evidence/
# (#128): which commit, OS and nextest profile it was, and nextest's JUnit
# report, with anything shaped like an API key hidden. CI has no real key;
# this is a backstop for fixtures. Never fails the job.
#
#   scripts/ci-evidence.sh <label> <job status>
set -uo pipefail

label=${1:-unknown}
status=${2:-unknown}
out=target/test-evidence
mkdir -p "$out"

# Groq (gsk_...) and OpenAI (sk-...) key shapes.
hide_keys() {
  sed -E 's/(gsk_|sk-)[A-Za-z0-9_-]{8,}/[key hidden]/g'
}

{
  echo "commit: ${GITHUB_SHA:-$(git rev-parse HEAD 2>/dev/null || echo unknown)}"
  echo "job: $label"
  echo "status: $status"
  echo "nextest profile: ${NEXTEST_PROFILE:-default}"
  echo "rustc: $(rustc --version 2>/dev/null || echo unknown)"
  echo "os: $(uname -srm)"
} >"$out/about.txt"

if [[ -f target/nextest/ci/junit.xml ]]; then
  hide_keys <target/nextest/ci/junit.xml >"$out/junit.xml"
fi
if [[ -f "$out/gate.log" ]]; then
  hide_keys <"$out/gate.log" >"$out/gate.log.tmp" && mv "$out/gate.log.tmp" "$out/gate.log"
fi
ls -l "$out"
exit 0
