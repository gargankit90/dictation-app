#!/usr/bin/env bash
# Tests for how the opt-in e2e scenarios handle the API key (M4.4), with no
# VM, no key and no network: `vm` and the other VM-facing helpers are
# stubbed, and the key is a made-up sentinel. Part of the quality gate
# (scripts/gate.sh).
#
# - The key (and the VM password) never show in the output or in a
#   `bash -x` trace of a run, nor in an argument of any command sent to the
#   VM; they arrive on stdin.
# - The key is removed from the VM's keychain however the run ends: success,
#   a failing command, a signal, a failed removal (which is reported).
set -euo pipefail

root=$(cd "$(dirname "$0")/../.." && pwd -P)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

SENTINEL=gsk_FAKE-sentinel-key_NOT-REAL_7f3a9c
PW_SENTINEL=vm-password-NOT-REAL-5d21

passed=0
failed=0
check() { # description, then a command that must succeed
  local what=$1
  shift
  if "$@"; then
    passed=$((passed + 1))
  else
    failed=$((failed + 1))
    echo "FAIL: $what" >&2
  fi
}
lacks() { ! grep -qF -- "$1" "$2"; }
has() { grep -qF -- "$1" "$2"; }

# The harness each case runs in a bash of its own: the real library, helpers
# and scenarios, with the VM replaced by files in $STUB. Behaviour switches:
#   STUB_LAUNCH_STATUS   status of `launch` (default 0)
#   STUB_LAUNCH_SIGNAL   a signal `launch` sends the run
#   STUB_TEXTEDIT_STATUS status of groq_textedit_new
#   STUB_DELETE_STATUS   status of the VM's delete command
cat >"$tmp/harness.sh" <<'HARNESS'
set -euo pipefail
cd "$ROOT"
. tests/e2e/lib.sh
for f in tests/e2e/scenarios/*.sh; do . "$f"; done

OUT_DIR=$STUB
VM_IP=192.0.2.1
SCENARIO_FAILED=0
SCENARIO_SKIPPED=0
calls=0

# The VM: records each command's arguments and stdin; a store puts the marker
# file in place, a delete takes it away.
vm() {
  calls=$((calls + 1))
  echo "$*" >>"$STUB/args"
  local in
  in=$(cat)
  printf '%s\n' "$in" >>"$STUB/stdin"
  case $* in
    *--debug-set-api-key-stdin*) : >"$STUB/stored" ;;
    *--debug-delete-api-key*)
      if [ "${STUB_DELETE_STATUS:-0}" != 0 ]; then
        echo "cannot reach the keychain"
        return "$STUB_DELETE_STATUS"
      fi
      rm -f "$STUB/stored"
      echo delete >>"$STUB/deletes"
      ;;
    *"say -o"*) ;;
  esac
}
fresh_app_state() { :; }
launch() {
  if [ -n "${STUB_LAUNCH_SIGNAL:-}" ]; then kill "-$STUB_LAUNCH_SIGNAL" $$; sleep 1; fi
  return "${STUB_LAUNCH_STATUS:-0}"
}
wait_for_log() { :; }
app_log() { echo "INFO nothing to see here"; }
logged_phases() { printf '%s\n' Recording Transcribing Inserting Idle; }
groq_dictate() { :; }
groq_textedit_new() { return "${STUB_TEXTEDIT_STATUS:-0}"; }
groq_textedit_text() { echo "The quick brown fox jumps over the lazy dog."; }
groq_textedit_quit() { :; }
collect_log() { :; }
HARNESS

# run_case <name> <scenario> [env...]: runs the scenario in a fresh bash
# (under bash -x when TRACE=1), leaving stdout/stderr, the stub files and
# the exit status in $tmp/<name>/.
run_case() {
  local name=$1 scenario=$2
  shift 2
  local dir=$tmp/$name
  mkdir -p "$dir"
  # The bad-key scenario's checks of the app's log need a log the stubs don't
  # make; what matters here is the key, so its FAIL records are not an error.
  local tail='[ "$SCENARIO_FAILED" = 0 ]'
  if [ "$scenario" = e2e_error_bad_key ]; then tail=true; fi
  local flags=(-e)
  if [ "${TRACE:-0}" = 1 ]; then flags=(-x); fi
  # perl puts SIGINT back to its default first. A job started with `&` by a
  # shell without job control (a gate run in the background, a CI wrapper)
  # inherits SIGINT as *ignored*, and bash can neither trap nor be stopped by
  # a signal that was ignored when it started: the INT cases would run to the
  # end and exit 0 (#147). Nothing else here depends on the load.
  set +e
  env ROOT="$root" STUB="$dir" VM_PASSWORD="$PW_SENTINEL" "$@" \
    perl -e '$SIG{INT} = "DEFAULT"; exec @ARGV or die "exec: $!"' \
    bash "${flags[@]}" -c '. "$STUB/../harness.sh"; '"$scenario"'; '"$tail" \
    >"$dir/out" 2>"$dir/err" </dev/null
  echo $? >"$dir/status"
  set -e
}
status_of() { cat "$tmp/$1/status"; }
stored() { [ -e "$tmp/$1/stored" ]; }
deletes() { [ -e "$tmp/$1/deletes" ] && wc -l <"$tmp/$1/deletes" | tr -d ' ' || echo 0; }
no_secret_outside_stdin() { # case: neither secret in the output, trace or VM arguments
  local f
  for f in out err args; do
    [ -e "$tmp/$1/$f" ] || continue
    lacks "$SENTINEL" "$tmp/$1/$f" || return 1
    lacks "$PW_SENTINEL" "$tmp/$1/$f" || return 1
  done
}

# --- a run that works ------------------------------------------------------
KEY=GROQ_API_KEY=$SENTINEL
run_case ok e2e_real_groq "$KEY"
check "a good run exits 0" test "$(status_of ok)" = 0
check "the key was stored, and removed after the dictation" \
  test "$(deletes ok)" = 1 -a ! -e "$tmp/ok/stored"
check "the key went to the VM on stdin" has "$SENTINEL" "$tmp/ok/stdin"
check "so did the VM password" has "$PW_SENTINEL" "$tmp/ok/stdin"
check "neither secret is in the output or in a VM command's arguments" no_secret_outside_stdin ok
check "the key is removed once, not again by the exit cleanup" test "$(deletes ok)" = 1
check "the key is stored by the app's own hook, not the security tool" \
  has "Contents/MacOS/Vocet --debug-set-api-key-stdin" "$tmp/ok/args"
check "and removed by the app's hook" has "--debug-delete-api-key" "$tmp/ok/args"
check "the security tool never touches the key" lacks "generic-password" "$tmp/ok/args"

# --- bash -x ---------------------------------------------------------------
TRACE=1 run_case traced e2e_real_groq "$KEY"
check "the traced run worked too" test "$(status_of traced)" = 0
check "the trace shows the run (it is a real trace)" has "+ groq_key_store" "$tmp/traced/err"
check "bash -x: neither secret in stdout, stderr or the VM arguments" no_secret_outside_stdin traced
check "bash -x: the key still reached the VM on stdin" has "$SENTINEL" "$tmp/traced/stdin"

TRACE=1 run_case traced_fail e2e_real_groq "$KEY" STUB_LAUNCH_STATUS=23
check "bash -x, failed run: neither secret in any output" no_secret_outside_stdin traced_fail

TRACE=1 run_case traced_bad e2e_error_bad_key
check "bash -x on the bad-key scenario: no secret in any output" no_secret_outside_stdin traced_bad

# --- opt-in ----------------------------------------------------------------
run_case skipped e2e_real_groq
check "no key: the scenario is skipped, the VM never touched" test ! -e "$tmp/skipped/args"
check "no key: it exits 0" test "$(status_of skipped)" = 0

# --- the key never outlives the run ---------------------------------------
run_case launch_fails e2e_real_groq "$KEY" STUB_LAUNCH_STATUS=23
check "a failing launch after the store: the run's status is kept" test "$(status_of launch_fails)" = 23
check "a failing launch after the store: the key is removed" \
  test -e "$tmp/launch_fails/deletes" -a ! -e "$tmp/launch_fails/stored"

run_case textedit_fails e2e_real_groq "$KEY" STUB_TEXTEDIT_STATUS=24
check "a failing TextEdit setup: the status is kept" test "$(status_of textedit_fails)" = 24
check "a failing TextEdit setup: the key is removed" test ! -e "$tmp/textedit_fails/stored"

# The status is 128 + the signal number, and must be exactly that: a run that
# ignored the signal and finished would exit 0, one that died of it without
# its cleanup would leave the key.
for signal in INT:130 TERM:143 HUP:129; do
  name=${signal%:*}
  expected=${signal#*:}
  run_case "signal_$name" e2e_real_groq "$KEY" STUB_LAUNCH_SIGNAL=$name
  check "$name after the store: the key is removed" test ! -e "$tmp/signal_$name/stored"
  check "$name after the store: the run fails with $expected" test "$(status_of "signal_$name")" = "$expected"
  check "$name after the store: the cleanup ran" test "$(deletes "signal_$name")" = 1
done

run_case bad_key_fails e2e_error_bad_key STUB_LAUNCH_STATUS=25
check "the bad-key scenario: a failing launch keeps its status" test "$(status_of bad_key_fails)" = 25
check "the bad-key scenario: a failing launch still removes the key" test ! -e "$tmp/bad_key_fails/stored"

run_case bad_key_ok e2e_error_bad_key
check "the bad-key scenario works" test "$(status_of bad_key_ok)" = 0
check "and leaves no key" test ! -e "$tmp/bad_key_ok/stored"

# --- a removal that fails is reported --------------------------------------
run_case delete_fails e2e_real_groq "$KEY" STUB_DELETE_STATUS=1
check "a failed removal fails the run" test "$(status_of delete_fails)" != 0
check "a failed removal is reported" has "may still be" "$tmp/delete_fails/err"
check "the failed removal's output hides the key" no_secret_outside_stdin delete_fails

run_case delete_fails_after_error e2e_real_groq "$KEY" STUB_DELETE_STATUS=1 STUB_LAUNCH_STATUS=23
check "a failed removal after an earlier failure keeps that status" \
  test "$(status_of delete_fails_after_error)" = 23
check "and says the key may still be there" has "may still be" "$tmp/delete_fails_after_error/err"

# --- the key must be plain, so it can't carry a command --------------------
run_case odd_key e2e_real_groq GROQ_API_KEY="abc def;touch x"
check "a key with odd characters is refused" test "$(status_of odd_key)" != 0
check "and is never sent" test ! -e "$tmp/odd_key/args" -o "$(grep -c -e --debug-set-api-key-stdin "$tmp/odd_key/args" || true)" = 0
check "and nothing was left to remove" test ! -e "$tmp/odd_key/deletes"

echo "key handling tests: $passed passed, $failed failed"
[ "$failed" = 0 ]
