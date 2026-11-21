#!/usr/bin/env bash
# BENCH1 in this repo: eval/app_bench/bench.sh with this Mac's VMs. The bench VM is its own clone of the
# golden image and stands in for dictation-vm-2 while the bench holds that VM's e2e lock (the Lead,
# 2026-11-21), so a Tester's tests/e2e/run.sh says "VM busy" instead of resetting a VM in use.
#
#   eval/bench1.sh <bench.sh command> ...       e.g. eval/bench1.sh lock; eval/bench1.sh vm-start --timed
#
# Only dictation-vm-bench is used: never the golden image, dictation-vm or dictation-vm-2 themselves.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
export BENCH_VM=dictation-vm-bench
export BENCH_BASE_IMAGE=dictation-vm-golden
export BENCH_LOCK_FILE=${E2E_LOCK_DIR:-$HOME/.cache/dictation-e2e}/dictation-vm-2.lock
export BENCH_SSH_KEY=${SSH_KEY:-$HOME/.ssh/dictation_vm_ed25519}
export BENCH_HOSTKEY_FROM=dictation-vm
export BENCH_EXTRA_FILES="$here/bench1_vocet_check.py"
exec "$here/app_bench/bench.sh" "$@"
