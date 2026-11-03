#!/usr/bin/env bash
# The VM e2e suite for one commit of main: what scripts/e2e-main.sh runs on
# the owner's Mac after each merge. A thin wrapper around run.sh and its locks.
#
#   tests/e2e/ci-run.sh
#
# 1. Builds the app once (run.sh --no-build then reuses it).
# 2. Borrows a VM: dictation-vm-2 first, then dictation-vm, by starting
#    run.sh on it; run.sh's own lock says whether it is free ("VM busy" or a
#    stale lock: try the other). If both are taken it waits, polling, up to
#    CI_VM_WAIT seconds (default 2700) and then ends as "skipped: no VM free"
#    with exit 0: a busy VM is not a broken main.
# 3. Runs every scenario except the ones that need a real key or a real
#    microphone (CI has no API key): e2e_real_groq, e2e_real_mic.
# 4. Collects the evidence in $CI_EVIDENCE_DIR (default target/e2e-evidence):
#    about.txt (SHA, VM, status), results.txt (PASS/FAIL/SKIP per scenario)
#    and the run's logs, with anything shaped like a key hidden. Exit 1 only
#    for a failed build or scenario.
#
# Never names dictation-vm-golden or its backups, never starts a third VM
# (run.sh refuses both). Made-up keys only: the scenarios use the local
# provider (T4a).
#
# For tests: E2E_RUN replaces run.sh, CI_VM_POLL is the seconds between
# tries (default 30), CI_VM_POOL narrows the pool, CI_SKIP_BUILD=1 skips the build, CI_OUT_ROOT is where
# the runs' directories go (default tests/e2e/out).
set -uo pipefail

cd "$(git rev-parse --show-toplevel)"
E2E_DIR=tests/e2e
RUN=${E2E_RUN:-$E2E_DIR/run.sh}
WAIT=${CI_VM_WAIT:-2700}
POLL=${CI_VM_POLL:-30}
EVIDENCE=${CI_EVIDENCE_DIR:-target/e2e-evidence}
# The only VMs CI may use, in order. CI_VM_POOL narrows it (a Developer's
# local run uses only dictation-vm); a name outside these two is refused.
POOL=(dictation-vm-2 dictation-vm)
if [ -n "${CI_VM_POOL:-}" ]; then
  read -r -a POOL <<<"$CI_VM_POOL"
  for name in "${POOL[@]}"; do
    case $name in
      dictation-vm | dictation-vm-2) ;;
      *) echo "CI_VM_POOL: '$name' is not dictation-vm or dictation-vm-2" >&2; exit 2 ;;
    esac
  done
fi
EXCLUDE=(e2e_real_groq e2e_real_mic)

log() { printf '[e2e-ci %s] %s\n' "$(date +%T)" "$*" >&2; }
hide_keys() { sed -E 's/(gsk_|sk-)[A-Za-z0-9_-]{8,}/[key hidden]/g'; }
sha=${GITHUB_SHA:-$(git rev-parse HEAD 2>/dev/null || echo unknown)}

rm -rf "$EVIDENCE"
mkdir -p "$EVIDENCE"
stamp=$(date +%Y%m%d-%H%M%S)
vm=none
status=unknown
run_dir=

# Writes about.txt and the step summary; copies the run's files over.
finish() {
  {
    echo "commit: $sha"
    echo "vm: $vm"
    echo "status: $status"
    echo "time: $(date '+%F %T')"
  } >"$EVIDENCE/about.txt"
  if [ -n "$run_dir" ] && [ -d "$run_dir" ]; then
    local f
    for f in "$run_dir"/*; do
      [ -f "$f" ] || continue
      case $f in
        *.log | *.txt) hide_keys <"$f" >"$EVIDENCE/$(basename "$f")" ;;
        *.png) cp "$f" "$EVIDENCE/" ;;
      esac
    done
  fi
  {
    echo "### VM e2e on \`${sha:0:9}\`: $status"
    echo "VM: $vm"
    [ -f "$EVIDENCE/results.txt" ] && { echo '```'; cat "$EVIDENCE/results.txt"; echo '```'; }
  } | tee -a "${GITHUB_STEP_SUMMARY:-/dev/null}" >&2
}

if [ "${CI_SKIP_BUILD:-0}" != 1 ]; then
  log "building the app"
  if ! npm run tauri build -- --debug --bundles app >"$EVIDENCE/build.log" 2>&1; then
    status="FAILED: the build failed (see build.log)"
    tail -n 40 "$EVIDENCE/build.log" >&2
    finish
    exit 1
  fi
fi

args=(--no-build)
for name in "${EXCLUDE[@]}"; do args+=(--exclude "$name"); done

deadline=$((SECONDS + WAIT))
while :; do
  for candidate in "${POOL[@]}"; do
    out="${CI_OUT_ROOT:-$E2E_DIR/out}/ci-$stamp-$candidate"
    log "trying $candidate"
    E2E_VM=$candidate E2E_OUT_DIR=$out "$RUN" "${args[@]}" 2>&1 | tee "$EVIDENCE/run-$candidate.log" >&2
    rc=${PIPESTATUS[0]}
    if [ "$rc" = 0 ]; then
      vm=$candidate run_dir=$out status="PASSED"
      finish
      exit 0
    fi
    # Not free (a live run holds it, or a dead run left its lock): next VM.
    if grep -qE 'VM busy: |stale lock for ' "$EVIDENCE/run-$candidate.log"; then
      log "$candidate isn't free"
      rm -f "$EVIDENCE/run-$candidate.log"
      rmdir "$out" 2>/dev/null || true
      continue
    fi
    vm=$candidate run_dir=$out status="FAILED (exit $rc)"
    finish
    exit 1
  done
  if [ "$SECONDS" -ge "$deadline" ]; then
    status="skipped: no VM free (waited ${WAIT}s)"
    finish
    exit 0
  fi
  log "no VM free; waiting ${POLL}s"
  sleep "$POLL"
done
