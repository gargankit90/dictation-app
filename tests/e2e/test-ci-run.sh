#!/usr/bin/env bash
# Tests for ci-run.sh (T6), with no VM and no build: E2E_RUN is a fake run.sh
# that obeys real lock symlinks in a temporary E2E_LOCK_DIR. Checks the pool
# order, the wait and the "skipped: no VM free" end, that the golden image is
# never named, that real-key/real-mic scenarios are excluded, and that a
# failing scenario still leaves the summary, SHA, VM name and logs behind
# (with a key-shaped string hidden). Part of the quality gate, through
# test-runner.sh.
set -euo pipefail

root=$(cd "$(dirname "$0")/../.." && pwd -P)
tmp=$(mktemp -d)
holder=
cleanup() {
  if [ -n "$holder" ]; then
    kill "$holder" 2>/dev/null || true
    wait "$holder" 2>/dev/null || true
  fi
  rm -rf "$tmp"
}
trap cleanup EXIT

passed=0
failed=0
check() {
  local what=$1
  shift
  if "$@"; then passed=$((passed + 1)); else
    failed=$((failed + 1))
    echo "FAIL: $what" >&2
  fi
}
has() { grep -qF -- "$1" "$2"; }
lacks() { ! grep -qF -- "$1" "$2"; }

# The fake run.sh: a taken lock is "VM busy" (the message run.sh prints);
# otherwise it logs its VM and arguments and writes results.txt, failing when
# FAKE_FAIL names the VM.
cat >"$tmp/run.sh" <<'FAKE'
#!/usr/bin/env bash
echo "$E2E_VM $*" >>"$FAKE_CALLS"
if [ -L "$E2E_LOCK_DIR/$E2E_VM.lock" ]; then
  mkdir -p "$E2E_OUT_DIR"
  echo "[e2e] VM busy: $E2E_VM is in use by a run (pid 1)" >&2
  exit 1
fi
mkdir -p "$E2E_OUT_DIR"
echo "app log with gsk_abcdefghijklmnop1234" >"$E2E_OUT_DIR/e2e_x.app.log"
if [ "${FAKE_FAIL:-}" = "$E2E_VM" ]; then
  printf 'PASS     e2e_a\nFAIL     e2e_x\n1 passed, 1 failed, 0 skipped\n' >"$E2E_OUT_DIR/results.txt"
  echo "[e2e] the run failed" >&2
  exit 1
fi
printf 'PASS     e2e_a\n1 passed, 0 failed, 0 skipped\n' >"$E2E_OUT_DIR/results.txt"
FAKE
chmod +x "$tmp/run.sh"
sleep 600 &
holder=$!

# case <name> <fail vm or ""> <locks...>: runs the wrapper; sets rc.
rc=0
run_case() {
  local name=$1 fail=$2 lock
  shift 2
  rm -rf "$tmp/locks" "$tmp/$name"
  mkdir -p "$tmp/locks" "$tmp/$name"
  for lock in "$@"; do ln -s "$holder $(date '+%F %T')" "$tmp/locks/$lock.lock"; done
  : >"$tmp/$name/calls"
  rc=0
  (
    cd "$root"
    E2E_RUN="$tmp/run.sh" E2E_LOCK_DIR="$tmp/locks" FAKE_CALLS="$tmp/$name/calls" \
      FAKE_FAIL="$fail" CI_OUT_ROOT="$tmp/$name/out" CI_SKIP_BUILD=1 CI_VM_WAIT="${WAIT:-0}" CI_VM_POLL=1 \
      GITHUB_SHA=0123456789abcdef CI_EVIDENCE_DIR="$tmp/$name/ev" \
      GITHUB_STEP_SUMMARY="$tmp/$name/summary" tests/e2e/ci-run.sh
  ) >"$tmp/$name/stdout" 2>&1 || rc=$?
}

# Both free: dictation-vm-2 first, and only it.
run_case free ""
check "free: exits 0" test "$rc" = 0
check "free: uses dictation-vm-2 first" has "dictation-vm-2 " "$tmp/free/calls"
check "free: doesn't touch dictation-vm" lacks "dictation-vm --no-build" "$tmp/free/calls"
check "free: records the VM" has "vm: dictation-vm-2" "$tmp/free/ev/about.txt"
check "free: records the SHA" has "commit: 0123456789abcdef" "$tmp/free/ev/about.txt"
check "free: keeps results.txt" has "1 passed" "$tmp/free/ev/results.txt"
check "free: excludes the real-key scenario" has "--exclude e2e_real_groq" "$tmp/free/calls"
check "free: excludes the real-mic scenario" has "--exclude e2e_real_mic" "$tmp/free/calls"
check "free: reuses the build" has "--no-build" "$tmp/free/calls"
check "free: the step summary has the result" has "PASSED" "$tmp/free/summary"

# vm-2 held: falls back to dictation-vm.
run_case second "" dictation-vm-2
check "vm-2 held: exits 0" test "$rc" = 0
check "vm-2 held: tried vm-2 first" has "dictation-vm-2 " "$tmp/second/calls"
check "vm-2 held: then used dictation-vm" has "vm: dictation-vm" "$tmp/second/ev/about.txt"
check "vm-2 held: no empty output dir left" test ! -e "$tmp/second/out/ci-"*"-dictation-vm-2"

# Both held: waits (more than one try), then "skipped", exit 0.
WAIT=2 run_case busy "" dictation-vm-2 dictation-vm
check "both held: exits 0" test "$rc" = 0
check "both held: says skipped: no VM free" has "skipped: no VM free" "$tmp/busy/ev/about.txt"
check "both held: summary says it too" has "skipped: no VM free" "$tmp/busy/summary"
check "both held: waited and tried again" test "$(grep -c '^dictation-vm-2 ' "$tmp/busy/calls")" -ge 2

# A failing scenario: exit 1, with the evidence kept and keys hidden.
run_case fail dictation-vm-2
check "failure: exits 1" test "$rc" = 1
check "failure: names the VM" has "vm: dictation-vm-2" "$tmp/fail/ev/about.txt"
check "failure: names the SHA" has "commit: 0123456789abcdef" "$tmp/fail/ev/about.txt"
check "failure: status failed" has "FAILED" "$tmp/fail/ev/about.txt"
check "failure: keeps the per-scenario summary" has "FAIL     e2e_x" "$tmp/fail/ev/results.txt"
check "failure: keeps the scenario's log" test -f "$tmp/fail/ev/e2e_x.app.log"
check "failure: hides key-shaped strings" lacks "gsk_abcdefghijklmnop1234" "$tmp/fail/ev/e2e_x.app.log"
check "failure: keeps the run's own output" test -f "$tmp/fail/ev/run-dictation-vm-2.log"
check "failure: doesn't fall through to the other VM" lacks "dictation-vm --no-build" "$tmp/fail/calls"

# CI_VM_POOL narrows the pool but can't widen it.
run_case narrow ""
check "pool env: refuses the golden image" sh -c "cd '$root' && CI_VM_POOL=dictation-vm-golden E2E_RUN='$tmp/run.sh' CI_SKIP_BUILD=1 CI_EVIDENCE_DIR='$tmp/narrow/ev2' tests/e2e/ci-run.sh >/dev/null 2>&1; test \$? = 2"

# Never the golden image or a third VM.
check "the pool names only the two VMs" \
  grep -q '^POOL=(dictation-vm-2 dictation-vm)$' "$root/tests/e2e/ci-run.sh"
check "the golden image is not named in the wrapper" \
  sh -c "! grep -v '^#' '$root/tests/e2e/ci-run.sh' | grep -q golden"

# The wrapper reads run.sh's refusal messages; keep them in step.
check "run.sh still says 'VM busy: '" grep -q 'VM busy: ' "$root/tests/e2e/lib_vm.sh"
check "run.sh still says 'stale lock for '" grep -q 'stale lock for ' "$root/tests/e2e/lib_vm.sh"

echo "ci-run tests: $passed passed, $failed failed"
[ "$failed" = 0 ]
