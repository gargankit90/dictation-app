#!/usr/bin/env bash
# Tests for the e2e runner's own helpers (T2, P2-05), with no VM: a guest
# failure, a deadline or a stale result must turn a scenario red, and a
# cleanup must never turn a failure green. Part of the quality gate
# (scripts/gate.sh).
#
# `vm` is replaced by a stub that runs only the runner's own bookkeeping
# (mkdir, rm, cat, chmod, pkill -f of its jobs) in a temporary directory, and
# "Terminal" runs a job script with bash there; osascript, open and sleep
# inside a job are fakes on PATH. Nothing touches this Mac's apps, keys or
# clipboard. For the real `vm` (its deadline), ssh is a fake on PATH.
set -euo pipefail

root=$(cd "$(dirname "$0")/../.." && pwd -P)
tmp=$(mktemp -d)
# However the run ends (passed, failed or interrupted), no fixture process
# outlives it, KEEP=1 or not (#164); KEEP=1 keeps the cases' files for a
# look and prints where.
. "$root/tests/e2e/fixture_cleanup.sh"
trap finish_run EXIT
# An interrupt ends the run through the same EXIT trap.
trap 'exit 130' INT
trap 'exit 143' TERM

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
has() { grep -qF -- "$1" "$2"; }
lacks() { ! grep -qF -- "$1" "$2"; }

mkdir -p "$tmp/bin" "$tmp/jobbin"

# --- Fakes on PATH (used by the jobs and by the real vm) ---------------------

# osascript -l JavaScript <script> [args]: the key scripts and ax.js.
cat >"$tmp/bin/osascript" <<'FAKE'
#!/bin/bash
echo "$*" >>"$STUB/osa-calls"
script=$3
shift 3
[ -z "${STUB_OSA_SLEEP:-}" ] || /bin/sleep "$STUB_OSA_SLEEP"
held=up
[ "${STUB_KEY_STUCK:-0}" = 0 ] || held=held
case $script in
  *right-option.js)
    case $1 in
      down | up) echo "posted $1" ;;
      state) echo "right option $held" ;;
    esac
    # The app's side: a down, then an up, is one dictation (see app_log).
    case $1 in
      down) : >"$STUB/key-down" ;;
      up)
        if [ -e "$STUB/key-down" ]; then
          rm -f "$STUB/key-down"
          echo dictation >>"$STUB/dictations"
        fi
        ;;
    esac
    ;;
  *post-key.js)
    case $2 in
      down | up) echo "posted $2" ;;
      state) echo "$1 $held" ;;
    esac
    ;;
  *ax.js)
    [ "${STUB_AX_STATUS:-0}" = 0 ] || exit "$STUB_AX_STATUS"
    case $1 in
      tray-click) echo "clicked $3 dictation-app" ;;
      *) echo "fake ax: $*" ;;
    esac
    ;;
  *)
    echo "fake osascript: unknown script $script" >&2
    exit 2
    ;;
esac
exit "${STUB_OSA_STATUS:-0}"
FAKE
# Jobs open apps (which must not happen here) and sleep (quickly here; on
# the jobs' PATH only, so the host side's deadlines keep real seconds).
printf '#!/bin/bash\necho "open $*" >>"$STUB/osa-calls"\n' >"$tmp/bin/open"
printf '#!/bin/bash\nexec /bin/sleep 0.01\n' >"$tmp/jobbin/sleep"
# ssh ... <command>: runs the command (the last argument) here; exec, so a
# deadline's TERM reaches the command itself.
cat >"$tmp/bin/ssh" <<'FAKE'
#!/bin/bash
for last; do :; done
exec bash -c "$last"
FAKE
# A job that outlives every wait the cases make (10 minutes): only a TERM ends
# it. Its path holds $tmp, so the exit trap's pkill -f "$tmp/" reaps it too.
ln -s /bin/sleep "$tmp/longsleep"
chmod +x "$tmp/bin/"* "$tmp/jobbin/"*

# --- The harness each case runs in a bash of its own ----------------------------
#
# Switches:
#   STUB_REAL_VM=1       keep lib.sh's vm (ssh is the fake above)
#   STUB_TERMINAL        run (default): "Terminal" runs the job to its end before
#                        `open` returns, so the first poll finds the result and
#                        no poll count or clock decides the case; async: it
#                        starts the job in the background (for the cases about
#                        a job that outlasts its deadline, or is waited for);
#                        stale: it writes a done file with another job's token
#                        instead
#   STUB_PLANT_OLD_DONE  an earlier job's done file is there before the start
#   STUB_PREP_STATUS     status of the job's preparation step
#   STUB_FAST_SLEEP=1    the host's sleep takes 0.05 s (the deadlines count
#                        sleeps, so they shrink with it)
#   STUB_OSA_STATUS, STUB_OSA_SLEEP, STUB_KEY_STUCK, STUB_AX_STATUS: the fake
#                        osascript's exit status, delay, a key that stays held,
#                        ax.js failing
#   STUB_LAUNCH_STATUS, STUB_FRESH_STATUS, STUB_APP_COUNT: launch, the setup
#                        kill, the number of running apps
cat >"$tmp/harness.sh" <<'HARNESS'
set -euo pipefail
cd "$ROOT"
E2E_RUN_ID=test-run
E2E_JOB_COUNTER=$STUB/jobs
. tests/e2e/lib.sh
. tests/e2e/lib_full.sh
for f in tests/e2e/scenarios/*.sh; do . "$f"; done

OUT_DIR=$STUB/out
mkdir -p "$OUT_DIR"
VM_IP=192.0.2.1
VM_RUN_DIR=$STUB/case-vm/run
SSH_KEY=/dev/null

if [ "${STUB_FAST_SLEEP:-0}" = 1 ]; then
  sleep() { command sleep 0.05; }
fi

if [ "${STUB_REAL_VM:-0}" != 1 ]; then
  vm() {
    local cmd="$*" file done_file
    echo "$cmd" >>"$STUB/vm-commands"
    case $cmd in
      "open -a Terminal "*)
        eval "file=${cmd#open -a Terminal }"
        case ${STUB_TERMINAL:-run} in
          run) PATH="$STUB/../jobbin:$PATH" bash "$file" >/dev/null 2>&1 </dev/null ;;
          async) PATH="$STUB/../jobbin:$PATH" bash "$file" >/dev/null 2>&1 </dev/null & ;;
          stale) printf 'an-earlier-job-token 0\n' >"${file%.command}.done" ;;
        esac
        return 0
        ;;
      *"pkill -x"* | *killall* | *osascript* | "open "*)
        return 0 # never here
        ;;
      "mkdir -p $VM_RUN_DIR"* | "mkdir -p '$VM_RUN_DIR'"*)
        [ "${STUB_PREP_STATUS:-0}" = 0 ] || return "$STUB_PREP_STATUS"
        if [ "${STUB_PLANT_OLD_DONE:-0}" = 1 ]; then
          done_file=$(printf '%s' "$cmd" | sed -nE 's/.*rm -f ([^ ]+\.done) .*/\1/p')
          mkdir -p "$VM_RUN_DIR"
          printf 'an-earlier-job-token 0\n' >"$done_file"
          echo "$done_file" >"$STUB/planted"
        fi
        bash -c "$cmd"
        ;;
      "cat "*"$VM_RUN_DIR"* | "cat >"*"$VM_RUN_DIR"* | "pkill -f "*"$VM_RUN_DIR"*)
        bash -c "$cmd"
        ;;
      *)
        cat >/dev/null 2>&1 || true
        return 0
        ;;
    esac
  }
fi
fresh_app_state() {
  echo fresh >>"$STUB/calls"
  return "${STUB_FRESH_STATUS:-0}"
}
launch() {
  echo "launch $*" >>"$STUB/calls"
  return "${STUB_LAUNCH_STATUS:-0}"
}
app_count() { echo "${STUB_APP_COUNT:-1}"; }
# Each dictation the fake keys made (a down, then an up) logs its phases.
app_log() {
  echo "INFO dictation_app_lib: ready hotkey=RightAlt"
  local phase n
  [ -e "$STUB/dictations" ] || return 0
  while read -r n; do
    for phase in Recording Transcribing Inserting Idle; do
      echo "INFO dictation_app_lib: state changed phase=$phase"
    done
  done <"$STUB/dictations"
}
wait_for_log() { app_log | grep -Eq "$1"; }
textedit_fresh() { :; }
textedit_text() { echo "${STUB_TEXT-hello world}"; }
textedit_close() { :; }
collect_log() { echo "collect $1" >>"$STUB/calls"; }
screenshot() { :; }
close_terminal() { echo close_terminal >>"$STUB/calls"; }
HARNESS

# run_case <name> <bash code> [env...]: runs the code after the harness in a
# fresh bash, leaving stdout, stderr and the exit status in $tmp/case-<name>/.
run_case() {
  local name=$1 code=$2
  shift 2
  local dir=$tmp/case-$name
  mkdir -p "$dir"
  set +e
  env ROOT="$root" STUB="$dir" PATH="$tmp/bin:$PATH" "$@" \
    bash -c '. "$STUB/../harness.sh"; '"$code" >"$dir/stdout" 2>"$dir/stderr" </dev/null
  echo $? >"$dir/status"
  set -e
}
status_of() { cat "$tmp/case-$1/status"; }
out_of() { cat "$tmp/case-$1/stdout"; }
stdout() { echo "$tmp/case-$1/stdout"; }
err() { echo "$tmp/case-$1/stderr"; }
calls() { echo "$tmp/case-$1/calls"; }

# A scenario through run_scenario; prints its result as the case's output.
# The fake Terminal runs each job to its end before `open` returns (see
# STUB_TERMINAL), so a case never races a poll count against a job that a busy
# machine has not scheduled yet (#149: a 20 s budget of 0.05 s polls ran out
# under load). A case that tests the deadline sets its own (a later env
# assignment wins), with a job that cannot end before it.
scenario_case() {
  local name=$1 scenario=$2
  shift 2
  run_case "$name" "run_scenario $scenario; echo \"\$SCENARIO_RESULT\"" \
    STUB_FAST_SLEEP=1 VM_TERMINAL_TIMEOUT=400 "$@"
}

# --- vm: the whole command has a deadline --------------------------------------

run_case vm_status 'vm "exit 7"' STUB_REAL_VM=1
check "vm returns the guest command's status" test "$(status_of vm_status)" = 7

run_case vm_stdin 'printf "piped through\n" | vm cat' STUB_REAL_VM=1
check "vm passes its stdin to the guest command" test "$(out_of vm_stdin)" = "piped through"

# A command that would run for a minute: the deadline (2 s) is far from both
# that end and the bound below, so a loaded machine cannot blur them (#149).
run_case vm_deadline 'start=$SECONDS; s=0; VM_CMD_TIMEOUT=2 vm "sleep 60" || s=$?; echo "$s $((SECONDS - start))"' \
  STUB_REAL_VM=1
read -r vm_status vm_took <"$(stdout vm_deadline)"
check "a guest command past its deadline returns 124" test "$vm_status" = 124
check "and is cancelled at the deadline, not when it ends" test "$vm_took" -le 40
check "and says so" has "no answer within 2s" "$(err vm_deadline)"

run_case vm_fast 'start=$SECONDS; for i in 1 2 3 4 5; do x=$(vm "echo hi"); done; echo "$x $((SECONDS - start))"' \
  STUB_REAL_VM=1
read -r vm_fast_out vm_fast_took <"$(stdout vm_fast)"
check "vm's output comes back through \$(...)" test "$vm_fast_out" = hi
# Held up, each of the five calls would wait out the watcher's one-second
# sleep: 5 s at least. 3 s leaves room for a busy machine (#149).
check "the deadline's watcher doesn't hold up a \$(vm ...)" test "$vm_fast_took" -le 3

# --- vm_terminal: the guest's status, deadlines, stale results ---------------------

run_case term_ok 'vm_terminal "echo hello"'
check "a Terminal job that works returns 0" test "$(status_of term_ok)" = 0
check "and prints the job's output" test "$(out_of term_ok)" = hello

run_case term_exit 'vm_terminal "echo before failing; exit 23"'
check "a guest exit status of 23 is vm_terminal's status" test "$(status_of term_exit)" = 23
check "and the job's output still comes back" test "$(out_of term_exit)" = "before failing"

# A job that takes a while is waited for, and its result is the answer: the
# case that used to be every scenario's, now kept for itself, with a wait
# (3000 polls, over two minutes) that only a hung job would use up.
run_case term_late 'vm_terminal "/bin/sleep 0.5; echo late"' STUB_TERMINAL=async STUB_FAST_SLEEP=1 VM_TERMINAL_TIMEOUT=3000
check "a Terminal job that ends a while after it started is waited for" test "$(status_of term_late)" = 0
check "and its output comes back" test "$(out_of term_late)" = late

# The job outlasts the deadline (2 s) by far, so only the deadline can end the
# wait, and a loaded machine cannot make the job finish first (#149). It also
# outlasts job_gone's 30 s wait below, so it is gone only if the deadline
# really cancelled it (T5 R1).
run_case term_deadline "start=\$SECONDS; s=0; VM_TERMINAL_TIMEOUT=2 vm_terminal '$tmp/longsleep 600' || s=\$?; echo \"\$s \$((SECONDS - start))\"" \
  STUB_TERMINAL=async
read -r term_status term_took <"$(stdout term_deadline)"
check "a Terminal job past its deadline fails (124)" test "$term_status" = 124
check "at the deadline" test "$term_took" -le 20
check "and says it was cancelled" has "did not finish in 2s" "$(err term_deadline)"
# pkill only sends TERM: wait for the job to be gone rather than look once.
job_gone() { # case: true once no job of the case runs, within 30 s
  local i
  for i in $(seq 1 300); do
    pgrep -f "$tmp/case-$1/case-vm/run/[j]ob-" >/dev/null || return 0
    sleep 0.1
  done
  return 1
}
check "and the job is killed" job_gone term_deadline

run_case term_stale 'vm_terminal "echo the real job"' STUB_TERMINAL=stale
check "a done file with another job's token is refused (125)" test "$(status_of term_stale)" = 125
check "and its status isn't taken for the job's" lacks "the real job" "$(stdout term_stale)"
check "and the refusal is reported" has "not this job's token" "$(err term_stale)"

run_case term_planted 'vm_terminal "echo fresh result; exit 3"' STUB_PLANT_OLD_DONE=1
check "a done file left from an earlier job was there before the start" test -s "$tmp/case-term_planted/planted"
check "it is removed first, and the job's own status (3) comes back" test "$(status_of term_planted)" = 3
check "with the job's own output" test "$(out_of term_planted)" = "fresh result"

run_case term_ids 'a=$(vm_terminal "echo a"); b=$(vm_terminal "echo b"); echo "$a$b"'
check "two jobs from \$(...) subshells both work" test "$(out_of term_ids)" = ab
check "and get different job names" \
  test "$(grep -c '^open -a Terminal ' "$tmp/case-term_ids/vm-commands")" = 2 -a \
  "$(grep '^open -a Terminal ' "$tmp/case-term_ids/vm-commands" | sort -u | wc -l | tr -d ' ')" = 2
check "numbered from one counter across subshells" test "$(cat "$tmp/case-term_ids/jobs")" = 2

run_case term_prep 'vm_terminal "echo never"' STUB_PREP_STATUS=1
check "a job that can't be prepared in the VM fails (125)" test "$(status_of term_prep)" = 125

run_case job_says_status 'job_says "posted down" vm_terminal "echo posted down; exit 1"'
check "job_says: the right output from a failed job is a failure" test "$(status_of job_says_status)" != 0
check "job_says says the job failed" has "exit status 1" "$(err job_says_status)"

# --- Scenarios: a guest failure turns them red --------------------------------------

scenario_case hold_ok e2e_hold_release
check "the harness is real: e2e_hold_release passes when the guest works" test "$(out_of hold_ok)" = PASS
check "and its cleanup released Right Option" has "right-option.js up" "$tmp/case-hold_ok/osa-calls"

scenario_case hold_guest_fails e2e_hold_release STUB_OSA_STATUS=1
check "a guest exit status of 1 with the expected output fails the scenario" \
  test "$(out_of hold_guest_fails)" = FAIL

# The osascript of the first job sleeps 30 s, the wait is 10 polls of 0.05 s:
# the job cannot end first however slowly the polls run (#149).
scenario_case held_timeout e2e_hold_release STUB_TERMINAL=async STUB_OSA_SLEEP=30 VM_TERMINAL_TIMEOUT=10
check "Terminal jobs past their deadline fail the scenario" test "$(out_of held_timeout)" = FAIL
check "the deadline is reported" has "did not finish" "$(err held_timeout)"

scenario_case hold_stuck e2e_hold_release STUB_KEY_STUCK=1
check "a scenario that passed fails if its cleanup can't release the key" \
  test "$(out_of hold_stuck)" = FAIL
check "and the cleanup failure is reported" has "may still be held" "$(err hold_stuck)"

scenario_case setup_launch_fails e2e_held_at_launch STUB_LAUNCH_STATUS=23
check "a failed launch after a key-down fails the scenario" test "$(out_of setup_launch_fails)" = FAIL
check "with the launch's own status, 23" has "ended early (exit status 23)" "$(err setup_launch_fails)"
check "and the cleanup still released the key" \
  sh -c 'grep -F "right-option.js down" "$1" >/dev/null && tail -n 2 "$1" | grep -F right-option.js >/dev/null' \
  _ "$tmp/case-setup_launch_fails/osa-calls"
check "and saved the app's log" has "collect e2e_held_at_launch" "$(calls setup_launch_fails)"

scenario_case launch_and_cleanup_fail e2e_held_at_launch STUB_LAUNCH_STATUS=23 STUB_KEY_STUCK=1
check "a failed cleanup after a failure keeps the original status (23)" \
  has "ended early (exit status 23)" "$(err launch_and_cleanup_fail)"
check "and still reports the cleanup failure" has "may still be held" "$(err launch_and_cleanup_fail)"
check "and the scenario fails" test "$(out_of launch_and_cleanup_fail)" = FAIL

scenario_case setup_fails e2e_mock_paste STUB_FRESH_STATUS=1
check "a failed setup (the old app won't go) fails the scenario" test "$(out_of setup_fails)" = FAIL
check "before anything is launched" lacks "launch" "$(calls setup_fails)"

# --- run_scenario and run_scenarios ---------------------------------------------------

fakes='s_pass() { :; }
s_fail() { SCENARIO_FAILED=1; }
s_skip() { SCENARIO_SKIPPED=1; return 0; }
s_exit0() { exit 0; echo unreachable; }
s_false() { false; echo "BAD: set -e was ignored"; }
'

run_case early_exit "$fakes"'run_scenario s_exit0; echo "$SCENARIO_RESULT"'
check "a scenario that exits 0 half way is not a pass" test "$(out_of early_exit)" = FAIL

run_case set_e "$fakes"'set +e; run_scenarios s_false s_pass; echo "status $?"'
check "a failing command ends the scenario (set -e holds inside run_scenarios)" \
  lacks "BAD: set -e was ignored" "$(stdout set_e)"
check "and it fails" has "FAIL     s_false" "$(err set_e)"
check "the next scenario still runs and passes" has "PASS     s_pass" "$(err set_e)"
check "and the run fails" has "status 1" "$(stdout set_e)"

run_case optional_skip "$fakes"'OPTIONAL_SCENARIOS=(s_skip); set +e; run_scenarios s_pass s_skip; echo "status $?"'
check "an optional skip doesn't fail the run" has "status 0" "$(stdout optional_skip)"
check "but is in the summary" has "SKIP     s_skip (optional)" "$(err optional_skip)"
check "and in the total" has "1 passed, 0 failed, 1 skipped" "$(err optional_skip)"
check "and named at the end" has "skipped: s_skip (optional)" "$(err optional_skip)"

run_case required_skip "$fakes"'OPTIONAL_SCENARIOS=(); set +e; run_scenarios s_pass s_skip; echo "status $?"'
check "a required skip fails the run" has "status 1" "$(stdout required_skip)"
check "and says so" has "SKIP     s_skip (REQUIRED: counts as a failure)" "$(err required_skip)"

run_case a_failure "$fakes"'set +e; run_scenarios s_fail s_pass; echo "status $?"'
check "a failed check fails the run" has "status 1" "$(stdout a_failure)"
check "results.txt lists it" has "FAIL     s_fail" "$tmp/case-a_failure/out/results.txt"

# --- Quit must work by itself ----------------------------------------------------------

run_case quit_ok 'quit_from_tray' STUB_FAST_SLEEP=1 STUB_APP_COUNT=0
check "a Quit after which the app is gone passes" test "$(status_of quit_ok)" = 0
check "and nothing was killed" sh -c "! grep -q fresh '$tmp/case-quit_ok/calls' 2>/dev/null"

run_case quit_hangs 'quit_from_tray' STUB_FAST_SLEEP=1 STUB_APP_COUNT=1 QUIT_TIMEOUT=3
check "a Quit after which the app keeps running fails" test "$(status_of quit_hangs)" != 0
check "it is killed afterwards, for the next step" has fresh "$(calls quit_hangs)"
check "and the failed Quit is reported" has "still running 3s after Quit" "$(err quit_hangs)"

run_case quit_click_fails 'quit_from_tray' STUB_FAST_SLEEP=1 STUB_APP_COUNT=0 STUB_AX_STATUS=1
check "a Quit that couldn't be clicked fails, even if the app is gone" \
  test "$(status_of quit_click_fails)" != 0

# --- The local provider's helpers (T4a) ------------------------------------------------
#
# A check of "nothing happened" must not pass because the VM couldn't be
# read.

run_case provider_unread 'vm() { return 124; }; provider_requests_are 0'
check "no requests: a provider whose events can't be read fails, not 0" \
  test "$(status_of provider_unread)" != 0
check "and says so" has "could not read the provider's events" "$(err provider_unread)"

run_case provider_counts 'vm() { printf "listening port=1 pid=2\nrequest n=1 auth=bad\nreplied n=1 status=401\nrequest n=2 auth=good\n"; }
  provider_requests_are 2 && provider_request_has 2 auth=good && ! provider_request_has 1 auth=good'
check "the provider's requests are counted and read from its events" \
  test "$(status_of provider_counts)" = 0

run_case textedit_unread 'vm() { return 124; }; textedit_is ""'
check "an empty TextEdit: a document that can't be read fails, not empty" \
  test "$(status_of textedit_unread)" != 0

# The harness stubs textedit_text; these cases use the real one (lib_full.sh)
# over a stubbed vm.
run_case textedit_empty_unread '. tests/e2e/lib_full.sh; vm() { return 124; }; textedit_is_empty'
check "textedit_is_empty: a document that can't be read is not empty (#153)" \
  test "$(status_of textedit_empty_unread)" != 0
check "and says so" has "could not read TextEdit" "$(err textedit_empty_unread)"

run_case textedit_empty_blank '. tests/e2e/lib_full.sh; vm() { printf " \n\n"; }; textedit_is_empty'
check "textedit_is_empty: a blank document is empty" test "$(status_of textedit_empty_blank)" = 0

run_case textedit_empty_text '. tests/e2e/lib_full.sh; vm() { printf "pasted\n"; }; textedit_is_empty'
check "textedit_is_empty: a document with text is not empty" test "$(status_of textedit_empty_text)" != 0

run_case textedit_exact 'vm() { esc_b64 "a\nb\n"; }; textedit_is "$(esc_b64 "a\nb\n")" && ! textedit_is "$(esc_b64 "a\nb")"'
check "TextEdit's text is compared exactly, a trailing newline too" \
  test "$(status_of textedit_exact)" = 0

# #164: a run in miniature ends its fixtures however it ends, KEEP=1 or not,
# and only KEEP=1 keeps its files. It has a longsleep, and a job holding a
# /bin/sleep (no $tmp in its command line) that answers TERM by starting
# another longsleep, as a held-key job starts a release.
cleanup_case() { # name, KEEP, how the run ends: pass, fail or term
  local dir="$tmp/cleanup-$1" status=0
  mkdir -p "$dir"
  ln -s /bin/sleep "$dir/longsleep"
  KEEP=$2 bash -c '
    tmp=$1
    . "$2/tests/e2e/fixture_cleanup.sh"
    trap finish_run EXIT
    trap "exit 143" TERM
    # Not on the runner'"'"'s output: a survivor would hold a pipe open.
    "$tmp/longsleep" 600 >/dev/null 2>&1 &
    bash -c "trap \"\$0 601 &\" TERM; /bin/sleep 602 & wait" "$tmp/longsleep" >/dev/null 2>&1 &
    sleep 0.5
    case $3 in
      pass) exit 0 ;;
      fail) exit 1 ;;
      term) kill -TERM $$ ;;
    esac
  ' _ "$dir" "$root" "$3" >/dev/null 2>"$dir.err" || status=$?
  echo "$status" >"$dir.status"
}
no_fixture_left() { # dir
  [ -z "$(pgrep -f "$1/" || true)" ] && [ -z "$(pgrep -fx '/bin/sleep 602' || true)" ]
}
for how in pass:0 fail:1 term:143; do
  for keep in 1 0; do
    cleanup_case "${how%%:*}-keep$keep" "$keep" "${how%%:*}"
    dir="$tmp/cleanup-${how%%:*}-keep$keep"
    check "#164: a run that ends by ${how%%:*} with KEEP=$keep keeps its status" \
      test "$(cat "$dir.status")" = "${how#*:}"
    check "and leaves no fixture process" no_fixture_left "$dir"
    if [ "$keep" = 1 ]; then
      check "and keeps its files, saying where" has "cases kept in $dir" "$dir.err"
      check "(they are there)" test -L "$dir/longsleep"
    else
      check "and removes its files" test ! -e "$dir"
    fi
  done
done

echo "runner tests: $passed passed, $failed failed"
[ "$failed" = 0 ]
