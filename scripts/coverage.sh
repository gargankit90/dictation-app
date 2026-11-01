#!/usr/bin/env bash
# Line coverage per crate with cargo llvm-cov; fails if a crate is below its
# threshold (docs/ENGINEERING.md). Part of the quality gate.
#
# Each crate is measured with only its own tests, so `app` tests can't lift
# `core` over its bar. A crate with no tests, or no coverage data, fails: a
# filter or `#[cfg]` that hides every test, or a deleted test directory, must
# not pass as "no code yet" (#33, #62). Override a threshold for one run with
# COVERAGE_MIN_<CRATE>, e.g. `COVERAGE_MIN_CORE=99 scripts/coverage.sh`.
#
# COVERAGE_ROOT and COVERAGE_CRATES (space-separated entries as below) point
# the script at a fixture workspace; scripts/test-coverage.sh does that.
set -euo pipefail

cd "${COVERAGE_ROOT:-$(git rev-parse --show-toplevel)}"

# crate dir : package : minimum line coverage (%)
crates=(
  "core:dictation-core:85"
  "transcribe:dictation-transcribe:85"
  "app:dictation-pipeline:70"
)
if [ -n "${COVERAGE_CRATES:-}" ]; then
  read -r -a crates <<<"$COVERAGE_CRATES"
fi

status=0
report=$(mktemp)
trap 'rm -f "$report"' EXIT

printf '%-12s %-22s %9s %9s  %s\n' crate package lines min result

for entry in "${crates[@]}"; do
  IFS=: read -r dir package min <<<"$entry"
  override="COVERAGE_MIN_$(tr '[:lower:]' '[:upper:]' <<<"$dir")"
  min=${!override:-$min}

  cargo llvm-cov clean --workspace >/dev/null 2>&1
  # Test failures fail the script here, before any report is made. So does a
  # package with no tests: nextest exits non-zero when it runs none.
  cargo llvm-cov nextest --no-report -p "$package" --status-level fail

  if ! cargo llvm-cov report -p "$package" --json --summary-only >"$report" 2>"$report.err"; then
    cat "$report.err" >&2
    rm -f "$report.err"
    exit 1
  fi
  rm -f "$report.err"

  percent=$(jq -r '.data[0].totals.lines.percent' "$report")
  if awk -v p="$percent" -v m="$min" 'BEGIN { exit !(p + 0 >= m + 0) }'; then
    result=ok
  else
    result="FAIL (below $min%)"
    status=1
  fi
  printf '%-12s %-22s %8.2f%% %8s%%  %s\n' "$dir" "$package" "$percent" "$min" "$result"
done

for skipped in ports platform src-tauri; do
  [ -z "${COVERAGE_CRATES:-}" ] || break
  printf '%-12s %-22s %9s %9s  %s\n' "$skipped" - - - "not measured (no threshold)"
done

if [ "$status" -ne 0 ]; then
  echo "coverage: a crate is below its threshold" >&2
fi
exit "$status"
