#!/usr/bin/env bash
# Tests for which VM an e2e run uses, the locks and the VM limit (lib_vm.sh),
# with no VM: `lume` is a stub that records its calls. Part of the quality
# gate (scripts/gate.sh).
#
# - E2E_VM takes dictation-vm (default) or dictation-vm-2; anything else,
#   the golden image and its backup included, is refused before lume runs.
# - A reset clones the golden image, then sets memory and CPUs on the clone
#   (never on the golden image), then starts it.
# - A VM's lock is taken, released however the run ends, refused with a
#   "VM busy" error while a live run holds it, and taken over when stale. Two
#   runs never both get one lock (also while one is slow, or both take over
#   a stale one), a signal at any point of taking it releases it, and a
#   scenario's own traps don't leave it behind.
# - Clones of the golden image are serialised; a third running macOS VM is
#   refused, also when two runs reset their VMs at the same time.
set -euo pipefail

root=$(cd "$(dirname "$0")/../.." && pwd -P)
tmp=$(mktemp -d)
bg_pids=()
cleanup() {
  local pid
  for pid in ${bg_pids[@]+"${bg_pids[@]}"}; do
    kill "$pid" 2>/dev/null || true
    wait "$pid" 2>/dev/null || true
  done
  rm -rf "$tmp"
}
trap cleanup EXIT

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

GOLDEN=dictation-vm-golden
BACKUP=dictation-vm-golden-backup-20261101

# Each case runs in a bash of its own: the real lib.sh, with lume replaced by
# a stub (calls recorded in $STUB/lume.log, `lume ls` read from
# $STUB/ls.json) and the lock directory inside $STUB.
cat >"$tmp/harness.sh" <<'HARNESS'
set -euo pipefail
cd "$ROOT"
export E2E_LOCK_DIR="$STUB/locks"
export E2E_START_VISIBLE_WAIT=${E2E_START_VISIBLE_WAIT:-0}
. tests/e2e/lib.sh

# `lume ls` lists $STUB/ls.json plus every VM `lume run` has started (their
# names, one per line, in $STUB/started).
lume() {
  echo "$*" >>"$STUB/lume.log"
  case $1 in
    ls)
      if [ -e "$STUB/ls.fail" ]; then return 1; fi
      if [ -s "$STUB/started" ]; then
        jq -c --rawfile s "$STUB/started" '. + ([$s | split("\n")[] | select(length > 0)] | map({name: ., os: "macOS", status: "running"}))' "$STUB/ls.json"
      else
        cat "$STUB/ls.json"
      fi
      ;;
    clone) [ -z "${STUB_CLONE_SLEEP:-}" ] || sleep "$STUB_CLONE_SLEEP" ;;
    run)
      [ -z "${STUB_RUN_SLEEP:-}" ] || sleep "$STUB_RUN_SLEEP"
      echo "$2" >>"$STUB/started"
      ;;
  esac
}
wait_for_vm() { :; }
HARNESS

# run_case <name> <script> [env...]: runs the script after the harness,
# leaving out, err, status and the stub files in $tmp/<name>/.
run_case() {
  local name=$1 script=$2
  shift 2
  local dir=$tmp/$name
  mkdir -p "$dir"
  [ -e "$dir/ls.json" ] || echo '[]' >"$dir/ls.json"
  set +e
  env ROOT="$root" STUB="$dir" "$@" \
    bash -c '. "$STUB/../harness.sh"; '"$script" >"$dir/out" 2>"$dir/err" </dev/null
  echo $? >"$dir/status"
  set -e
}
status_of() { cat "$tmp/$1/status"; }
calls() { cat "$tmp/$1/lume.log" 2>/dev/null || true; }
no_lume_call() { [ ! -s "$tmp/$1/lume.log" ]; }
# ls_json <case> <name:status>...: what `lume ls -f json` answers.
ls_json() {
  local dir=$tmp/$1 entry first=1
  shift
  mkdir -p "$dir"
  {
    printf '['
    for entry in "$@"; do
      [ "$first" = 1 ] || printf ','
      first=0
      printf '{"name":"%s","os":"macOS","status":"%s"}' "${entry%%:*}" "${entry##*:}"
    done
    printf ']\n'
  } >"$dir/ls.json"
}
# fake_lock <case> <name> <pid>: a lock another run holds (or held).
fake_lock() {
  mkdir -p "$tmp/$1/locks"
  ln -s "$3 2026-11-01 09:30:00" "$tmp/$1/locks/$2.lock"
}
lock_dir() { printf '%s/%s/locks/%s.lock' "$tmp" "$1" "$2"; }
# A lock is a symlink with the holder as its target, dangling by design, so
# -e would say "no lock" for a lock that is there; -L is the test.
is_locked() { [ -L "$(lock_dir "$1" "$2")" ] || [ -d "$(lock_dir "$1" "$2")" ]; }
not_locked() { ! is_locked "$@"; }
lock_pid() { readlink "$(lock_dir "$1" "$2")" | cut -d' ' -f1; }
# Sets LIVE_PID to a process that stays up for the test (not in $(...), so
# cleanup knows it, and with no output, so a pipe doesn't wait for it).
start_live_pid() {
  sleep 120 >/dev/null 2>&1 &
  LIVE_PID=$!
  bg_pids+=("$LIVE_PID")
}
# Sets DEAD_PID to the pid of a process that has ended.
start_dead_pid() {
  true &
  DEAD_PID=$!
  wait "$DEAD_PID"
}

# run_bg <case> <label> <script> [env...]: like run_case, but in the background
# and in a case directory several runs share (their locks, their lume). Output
# in <case>/<label>.out, .err and .status; collect with wait_bg.
bg_runs=()
run_bg() {
  local name=$1 label=$2 script=$3
  shift 3
  local dir=$tmp/$name
  mkdir -p "$dir"
  [ -e "$dir/ls.json" ] || echo '[]' >"$dir/ls.json"
  (
    set +e
    env ROOT="$root" STUB="$dir" "$@" \
      bash -c '. "$STUB/../harness.sh"; '"$script" >"$dir/$label.out" 2>"$dir/$label.err" </dev/null
    echo $? >"$dir/$label.status"
  ) &
  bg_runs+=("$!")
  bg_pids+=("$!")
}
wait_bg() {
  local pid
  for pid in ${bg_runs[@]+"${bg_runs[@]}"}; do wait "$pid" || true; done
  bg_runs=()
}
# wait_for_file <path>: up to 10 seconds.
wait_for_file() {
  local i
  for i in $(seq 1 100); do
    [ -e "$1" ] && return 0
    sleep 0.1
  done
  return 1
}
# acquired_count <case> <label>...: how many of those runs got the lock.
acquired_count() {
  local name=$1 label n=0
  shift
  for label in "$@"; do
    grep -q '^acquired$' "$tmp/$name/$label.out" && n=$((n + 1))
  done
  echo "$n"
}

# --- Which VM -----------------------------------------------------------------
run_case default_vm 'echo "$VM"'
check "the default VM is dictation-vm" test "$(cat "$tmp/default_vm/out")" = dictation-vm
run_case vm1 'echo "$VM"' E2E_VM=dictation-vm
check "E2E_VM=dictation-vm is accepted" test "$(cat "$tmp/vm1/out")" = dictation-vm
run_case vm2 'echo "$VM"' E2E_VM=dictation-vm-2
check "E2E_VM=dictation-vm-2 is accepted" test "$(cat "$tmp/vm2/out")" = dictation-vm-2

n=0
for bad in "$GOLDEN" "$BACKUP" dictation-vm-3 dictation-vm-2-old dictation-vm2 Dictation-VM "dictation-vm "; do
  n=$((n + 1))
  run_case "bad$n" 'echo reached' E2E_VM="$bad"
  check "E2E_VM='$bad' is refused" test "$(status_of "bad$n")" != 0
  check "E2E_VM='$bad': says which names are allowed" has "E2E_VM must be one of: dictation-vm dictation-vm-2" "$tmp/bad$n/err"
  check "E2E_VM='$bad': nothing runs after the refusal" lacks reached "$tmp/bad$n/out"
  check "E2E_VM='$bad': lume is never called" no_lume_call "bad$n"
done
run_case empty_vm 'echo "$VM"' E2E_VM=
check "an empty E2E_VM means the default" test "$(cat "$tmp/empty_vm/out")" = dictation-vm

# --- Reset: clone, then set memory and CPUs on the clone, then run ------------
run_case reset1 'reset_vm'
check "a reset works" test "$(status_of reset1)" = 0
check "the reset checks the limit, stops, deletes, clones, sets, checks again, then runs (in that order)" \
  test "$(calls reset1 | cut -d' ' -f1-2 | tr '\n' '|')" = \
  "ls -f|stop dictation-vm|delete dictation-vm|clone $GOLDEN|set dictation-vm|ls -f|run dictation-vm|"
check "the clone is made from the golden image" has "clone $GOLDEN dictation-vm" "$tmp/reset1/lume.log"
check "default memory is 6GB and 4 CPUs" has "set dictation-vm --memory 6GB --cpu 4" "$tmp/reset1/lume.log"
check "lume set is called once, on the clone only" test "$(calls reset1 | grep -c '^set ')" = 1
check "lume set never names the golden image" lacks "set $GOLDEN" "$tmp/reset1/lume.log"

run_case reset2 'reset_vm' E2E_VM=dictation-vm-2 E2E_VM_MEMORY=5GB E2E_VM_CPU=3
check "dictation-vm-2: the clone is dictation-vm-2" has "clone $GOLDEN dictation-vm-2" "$tmp/reset2/lume.log"
check "E2E_VM_MEMORY and E2E_VM_CPU are used" has "set dictation-vm-2 --memory 5GB --cpu 3" "$tmp/reset2/lume.log"
check "dictation-vm-2: stop, delete and run name dictation-vm-2" \
  test "$(calls reset2 | grep -cE '^(stop|delete|run) dictation-vm-2( |$)')" = 3
check "dictation-vm-2: dictation-vm is never touched" lacks " dictation-vm " "$tmp/reset2/lume.log"
check "the golden image is only ever named as the clone's source" \
  test "$(calls reset2 | grep -c "$GOLDEN")" = 1
check "the clone lock is gone after the reset" not_locked reset2 golden-clone
check "so is the start lock" not_locked reset2 vm-start

# set_vm_resources refuses anything but a run's VM, even if VM is overwritten
run_case set_golden 'VM='"$GOLDEN"'; set_vm_resources'
check "set_vm_resources refuses the golden image" test "$(status_of set_golden)" != 0
check "and never calls lume set" no_lume_call set_golden

# --- The VM lock ---------------------------------------------------------------
run_case lock_ok 'vm_lock_take; lock_holder dictation-vm >&2; lock_release dictation-vm; [ ! -L "$E2E_LOCK_DIR/dictation-vm.lock" ]'
check "the lock is taken and released" test "$(status_of lock_ok)" = 0
check "the lock held a pid and a start time" grep -Eq '^[0-9]+ since [0-9-]+ [0-9:]+$' "$tmp/lock_ok/err"

run_case lock_held_inside 'vm_lock_take; [ "$(lock_read dictation-vm | cut -d" " -f1)" = "$$" ]'
check "while held, the lock names this run as its holder" test "$(status_of lock_held_inside)" = 0

run_case lock_vm2 'vm_lock_take; [ -L "$E2E_LOCK_DIR/dictation-vm-2.lock" ] && [ ! -L "$E2E_LOCK_DIR/dictation-vm.lock" ]' E2E_VM=dictation-vm-2
check "dictation-vm-2 uses its own lock" test "$(status_of lock_vm2)" = 0

# released however the run ends
run_case end_normal 'arm_run_traps; vm_lock_take; true'
check "a normal end releases the lock" not_locked end_normal dictation-vm
check "a normal end exits 0" test "$(status_of end_normal)" = 0
run_case end_die 'arm_run_traps; vm_lock_take; die boom'
check "die releases the lock" not_locked end_die dictation-vm
check "die keeps its exit status" test "$(status_of end_die)" = 1
run_case end_status 'arm_run_traps; vm_lock_take; exit 7'
check "exit 7 releases the lock" not_locked end_status dictation-vm
check "and keeps status 7" test "$(status_of end_status)" = 7
for signal in INT TERM HUP; do
  run_case "end_$signal" 'arm_run_traps; vm_lock_take; kill -'"$signal"' $$; sleep 1; echo survived'
  check "$signal releases the lock" not_locked "end_$signal" dictation-vm
  check "$signal ends the run" test "$(status_of "end_$signal")" != 0
  check "$signal: the run did not carry on" lacks survived "$tmp/end_$signal/out"
done
run_case end_rearm 'arm_run_traps; vm_lock_take; trap - EXIT INT TERM HUP; arm_run_traps; true'
check "re-arming after a scenario's own traps still releases the lock" not_locked end_rearm dictation-vm

# busy
start_live_pid
pid=$LIVE_PID
fake_lock busy dictation-vm "$pid"
run_case busy 'arm_run_traps; vm_lock_take; echo reached'
check "a live run holding the VM: refused" test "$(status_of busy)" != 0
check "the error says VM busy with the pid" has "VM busy: dictation-vm is in use by a run (pid $pid since 2026-11-01 09:30:00)" "$tmp/busy/err"
check "the busy run does not carry on" lacks reached "$tmp/busy/out"
check "the holder's lock is untouched" test "$(lock_pid busy dictation-vm)" = "$pid"
check "the holder's lock survives the refused run's exit" is_locked busy dictation-vm
fake_lock busy_reset dictation-vm "$pid"
run_case busy_reset 'arm_run_traps; vm_lock_take; reset_vm'
check "busy: a run never gets as far as lume" no_lume_call busy_reset

# the other VM's lock doesn't matter
fake_lock other_vm dictation-vm "$pid"
run_case other_vm 'arm_run_traps; vm_lock_take' E2E_VM=dictation-vm-2
check "dictation-vm busy, dictation-vm-2 free: it runs" test "$(status_of other_vm)" = 0
check "dictation-vm's lock is untouched" test "$(lock_pid other_vm dictation-vm)" = "$pid"

# stale
start_dead_pid
dead=$DEAD_PID
fake_lock stale dictation-vm "$dead"
run_case stale 'vm_lock_take; lock_read dictation-vm | cut -d" " -f1'
check "a lock whose run is gone is taken over" test "$(status_of stale)" = 0
check "and says so" has "took over the stale 'dictation-vm' lock (pid $dead is gone)" "$tmp/stale/err"
check "the new lock holds the new run's pid" test "$(cat "$tmp/stale/out")" != "$dead"
run_case stale_release 'arm_run_traps; vm_lock_take'
fake_lock stale_release dictation-vm "$dead"
run_case stale_release 'arm_run_traps; vm_lock_take'
check "a taken-over lock is released at the end" not_locked stale_release dictation-vm

# a directory lock from an older version of lib_vm.sh
mkdir -p "$tmp/empty_lock/locks/dictation-vm.lock"
run_case empty_lock 'vm_lock_take'
check "an old-style lock directory with no pid is taken over" test "$(status_of empty_lock)" = 0
mkdir -p "$tmp/old_dead/locks/dictation-vm.lock"
echo "$dead" >"$tmp/old_dead/locks/dictation-vm.lock/pid"
run_case old_dead 'vm_lock_take'
check "an old-style lock directory of a dead run is taken over" test "$(status_of old_dead)" = 0
mkdir -p "$tmp/old_live/locks/dictation-vm.lock"
echo "$pid" >"$tmp/old_live/locks/dictation-vm.lock/pid"
run_case old_live 'vm_lock_take'
check "an old-style lock directory of a live run is respected" test "$(status_of old_live)" != 0

# a run doesn't release a lock it doesn't hold
fake_lock not_mine dictation-vm "$pid"
run_case not_mine 'lock_release dictation-vm'
check "lock_release leaves another run's lock alone" is_locked not_mine dictation-vm


# --- Taking a lock is atomic: two runs never both get it ------------------------
# (R1) The first run is slow between starting to take the lock and having it
# (the `date` it calls for the lock's time stalls). The second run must not
# mistake that for a dead owner.
run_bg slow_owner a '
  date() { if [ "${1:-}" = "+%F %T" ]; then touch "$STUB/a_stalled"; sleep 3; fi; command date "$@"; }
  arm_run_traps; vm_lock_take; echo acquired; sleep 3'
wait_for_file "$tmp/slow_owner/a_stalled" || true
run_bg slow_owner b 'arm_run_traps; vm_lock_take; echo acquired; sleep 4'
wait_bg
check "a slow first owner and a second run: exactly one gets the lock" \
  test "$(acquired_count slow_owner a b)" = 1
check "the one that lost was told the VM is busy" \
  test "$(cat "$tmp/slow_owner/a.status")$(cat "$tmp/slow_owner/b.status")" = 10 -o "$(cat "$tmp/slow_owner/a.status")$(cat "$tmp/slow_owner/b.status")" = 01
check "and the winner's lock is gone only once the winner is" not_locked slow_owner dictation-vm

# Four runs find the same stale lock at once: one takes it over.
start_dead_pid
fake_lock stale_race dictation-vm "$DEAD_PID"
for label in a b c d; do
  run_bg stale_race $label 'arm_run_traps; vm_lock_take; echo acquired; sleep 2'
done
wait_bg
check "four runs, one stale lock: exactly one takes it over" \
  test "$(acquired_count stale_race a b c d)" = 1
check "the others are told it is busy" test "$(grep -l 'VM busy' "$tmp"/stale_race/?.err | wc -l | tr -d ' ')" = 3

# A takeover that was interrupted (its reclaim lock has a dead owner): the next
# run clears that and takes over.
start_dead_pid
fake_lock reclaim_dead dictation-vm "$DEAD_PID"
fake_lock reclaim_dead dictation-vm.reclaim "$DEAD_PID"
run_case reclaim_dead 'arm_run_traps; vm_lock_take'
check "an abandoned reclaim lock doesn't block a takeover" test "$(status_of reclaim_dead)" = 0
check "and nothing is left behind" not_locked reclaim_dead dictation-vm
check "not even the reclaim lock" not_locked reclaim_dead dictation-vm.reclaim

# A takeover under way (live reclaimer): this run keeps off.
fake_lock reclaim_live dictation-vm "$DEAD_PID"
fake_lock reclaim_live dictation-vm.reclaim "$pid"
run_case reclaim_live 'arm_run_traps; vm_lock_take; echo reached'
check "someone else is taking over the stale lock: this run does not" test "$(status_of reclaim_live)" != 0
check "it leaves the stale lock and the other's reclaim lock alone" \
  test "$(lock_pid reclaim_live dictation-vm)" = "$DEAD_PID" -a "$(lock_pid reclaim_live dictation-vm.reclaim)" = "$pid"
fake_lock reclaim_live_pid dictation-vm "$pid"
run_case reclaim_live_pid 'arm_run_traps; vm_lock_take'
check "a young lock whose run is alive is never taken over" test "$(status_of reclaim_live_pid)" != 0

# --- A signal while taking a lock releases it ---------------------------------
# (R3) TERM right after the lock is created, before the run has done anything
# with it; right before; and inside a takeover.
run_case sig_after 'ln() { command ln "$@"; kill -TERM $$; }; mv() { command mv "$@"; kill -TERM $$; }; arm_run_traps; vm_lock_take; echo survived'
check "TERM just after the VM lock is created: the run ends" test "$(status_of sig_after)" = 143
check "and the lock is gone" not_locked sig_after dictation-vm
check "and the run did not carry on" lacks survived "$tmp/sig_after/out"
run_case sig_before 'ln() { kill -TERM $$; command ln "$@"; }; arm_run_traps; vm_lock_take; echo survived'
check "TERM just before it is created: no lock" not_locked sig_before dictation-vm
run_case sig_clone 'ln() { command ln "$@"; kill -TERM $$; }; mv() { command mv "$@"; kill -TERM $$; }; arm_run_traps; clone_golden; echo survived'
check "TERM just after the clone lock is created: the lock is gone" not_locked sig_clone golden-clone
check "and nothing was cloned" no_lume_call sig_clone
for signal in INT HUP; do
  run_case "sig_$signal" 'ln() { command ln "$@"; kill -'"$signal"' $$; }; mv() { command mv "$@"; kill -'"$signal"' $$; }; arm_run_traps; vm_lock_take'
  check "$signal while taking the lock: it is released" not_locked "sig_$signal" dictation-vm
done
fake_lock sig_reclaim dictation-vm "$DEAD_PID"
run_case sig_reclaim 'ln() { local rc=0; command ln "$@" || rc=$?; case "$*" in *.reclaim.lock*) kill -TERM $$ ;; esac; return $rc; }; arm_run_traps; vm_lock_take'
check "TERM inside a takeover: the run ends" test "$(status_of sig_reclaim)" = 143
check "its reclaim lock is released" not_locked sig_reclaim dictation-vm.reclaim
run_case sig_reclaim2 'arm_run_traps; vm_lock_take'
check "and the next run takes the stale lock over" test "$(status_of sig_reclaim2)" = 0

# lock_release twice, and for a lock this run never took
run_case release_twice 'vm_lock_take; lock_release dictation-vm; lock_release dictation-vm; lock_release golden-clone; release_run_locks; release_run_locks'
check "releasing twice, or what was never held, is harmless" test "$(status_of release_twice)" = 0

# --- A scenario's own traps don't leave the lock behind -----------------------
# (R2) Scenarios arm their own EXIT trap (arm_key_cleanup, groq_guard_start...),
# which replaces the runner's. The runner runs them through run_scenario.
SCEN='
scen_ok() { true; }
scen_die() { trap '"'"'echo scenario-cleanup >&2; exit 1'"'"' EXIT; die "simulated failure"; }
scen_term_self() { trap '"'"'echo scenario-cleanup >&2; exit 143'"'"' EXIT; trap "exit 143" TERM; kill -TERM $BASHPID; sleep 5; echo survived; }
scen_term_runner() { trap '"'"'echo scenario-cleanup >&2; exit 1'"'"' EXIT; kill -TERM $$; sleep 1; true; }
scen_exit7() { exit 7; }
scen_failed() { SCENARIO_FAILED=1; }
scen_skipped() { SCENARIO_SKIPPED=1; }
scen_both() { SCENARIO_FAILED=1; SCENARIO_SKIPPED=1; }
'
run_case scen_die "$SCEN"'arm_run_traps; vm_lock_take; run_scenario scen_die; echo survived'
check "a scenario that dies while its own EXIT trap is armed: the run ends" test "$(status_of scen_die)" = 1
check "the VM lock is released" not_locked scen_die dictation-vm
check "the scenario's own cleanup ran, once" test "$(grep -c scenario-cleanup "$tmp/scen_die/err")" = 1
check "the run did not carry on" lacks survived "$tmp/scen_die/out"
run_case scen_key "$SCEN"'
  . tests/e2e/lib_full.sh
  key_cleanup_steps() { echo steps >&2; }
  collect_log() { :; }
  scen_key() { arm_key_cleanup scen_key; die "simulated failure"; }
  arm_run_traps; vm_lock_take; run_scenario scen_key'
check "the real arm_key_cleanup: a failure ends the run" test "$(status_of scen_key)" = 1
check "the VM lock is released" not_locked scen_key dictation-vm
check "and its cleanup steps ran" has steps "$tmp/scen_key/err"
run_case scen_term_self "$SCEN"'arm_run_traps; vm_lock_take; run_scenario scen_term_self; echo survived'
check "a scenario that gets TERM: its cleanup runs, the run ends" test "$(status_of scen_term_self)" != 0
check "the VM lock is released" not_locked scen_term_self dictation-vm
check "its cleanup ran" has scenario-cleanup "$tmp/scen_term_self/err"
run_case scen_term_runner "$SCEN"'arm_run_traps; vm_lock_take; run_scenario scen_term_runner; echo survived'
check "TERM to the run while a scenario's trap is armed: the run ends with 143" test "$(status_of scen_term_runner)" = 143
check "the VM lock is released" not_locked scen_term_runner dictation-vm
check "the scenario's cleanup ran" has scenario-cleanup "$tmp/scen_term_runner/err"
run_case scen_exit7 "$SCEN"'arm_run_traps; vm_lock_take; run_scenario scen_exit7; echo survived'
check "a scenario that exits non-zero ends the run" test "$(status_of scen_exit7)" = 1
check "the VM lock is released" not_locked scen_exit7 dictation-vm
check "and the error names the scenario" has "scenario scen_exit7 ended the run (status 7)" "$tmp/scen_exit7/err"
run_case scen_flags "$SCEN"'
  arm_run_traps; vm_lock_take
  for s in scen_ok scen_failed scen_skipped scen_both; do
    run_scenario $s; echo "$s $SCENARIO_FAILED $SCENARIO_SKIPPED"
  done'
check "run_scenario reports ok, failed, skipped and both, and the run goes on" \
  test "$(tr '\n' '|' <"$tmp/scen_flags/out")" = "scen_ok 0 0|scen_failed 1 0|scen_skipped 0 1|scen_both 1 1|"
check "and the lock is released at the end" not_locked scen_flags dictation-vm

# --- The clone lock -----------------------------------------------------------
fake_lock clone_busy golden-clone "$pid"
run_case clone_busy 'reset_vm' E2E_CLONE_LOCK_WAIT=1
check "a clone in progress elsewhere: this one waits, then gives up" test "$(status_of clone_busy)" != 0
check "the error names the lock" has "'golden-clone' is busy" "$tmp/clone_busy/err"
check "lume clone was not called" lacks "clone " "$tmp/clone_busy/lume.log"
check "lume set was not called" lacks "set " "$tmp/clone_busy/lume.log"

fake_lock clone_stale golden-clone "$dead"
run_case clone_stale 'reset_vm'
check "a stale clone lock is taken over and the reset goes on" test "$(status_of clone_stale)" = 0
check "the clone happened" has "clone $GOLDEN dictation-vm" "$tmp/clone_stale/lume.log"

# --- At most two running macOS VMs ---------------------------------------------
ls_json three_vms "$GOLDEN:running" dictation-vm-2:running dictation-vm:stopped
run_case three_vms 'reset_vm'
check "two others running: a third is refused" test "$(status_of three_vms)" != 0
check "the error says Apple allows 2" has "Apple allows 2 at once" "$tmp/three_vms/err"
check "nothing was stopped, deleted or cloned" lacks "delete" "$tmp/three_vms/lume.log"
check "nothing was cloned" lacks "clone" "$tmp/three_vms/lume.log"

ls_json own_running dictation-vm:running dictation-vm-2:running "$GOLDEN:stopped" "$BACKUP:stopped"
run_case own_running 'reset_vm'
check "its own VM running and one other: fine (the reset stops it first)" test "$(status_of own_running)" = 0

ls_json one_other dictation-vm-2:running dictation-vm:stopped
run_case one_other 'reset_vm'
check "one other running: fine" test "$(status_of one_other)" = 0

ls_json stopped_ok "$GOLDEN:stopped" "$BACKUP:stopped" dictation-vm-2:stopped
run_case stopped_ok 'reset_vm'
check "nothing running: fine" test "$(status_of stopped_ok)" = 0

ls_json three_vm2 dictation-vm:running "$GOLDEN:running"
run_case three_vm2 'reset_vm' E2E_VM=dictation-vm-2
check "dictation-vm-2: dictation-vm and golden running: refused" test "$(status_of three_vm2)" != 0

# (R4) Two runs reset their two VMs together while one other macOS VM runs:
# only one more may start. lume run takes a second, so without one lock around
# the count and the start both count one other VM and both start.
ls_json two_resets unrelated:running "$GOLDEN:stopped"
run_bg two_resets a 'arm_run_traps; vm_lock_take; reset_vm' E2E_VM=dictation-vm STUB_RUN_SLEEP=1
run_bg two_resets b 'arm_run_traps; vm_lock_take; reset_vm' E2E_VM=dictation-vm-2 STUB_RUN_SLEEP=1
wait_bg
check "two resets at once with one other VM running: one of them is refused" \
  test "$(cat "$tmp/two_resets/a.status")$(cat "$tmp/two_resets/b.status")" = 10 -o "$(cat "$tmp/two_resets/a.status")$(cat "$tmp/two_resets/b.status")" = 01
check "so only one VM was started" test "$(wc -l <"$tmp/two_resets/started" | tr -d ' ')" = 1
check "the refused one says Apple allows 2" test "$(cat "$tmp/two_resets/a.err" "$tmp/two_resets/b.err" | grep -c 'Apple allows 2 at once')" = 1
check "the start lock is gone" not_locked two_resets vm-start
check "so are both VM locks" test "$(ls "$tmp/two_resets/locks" | wc -l | tr -d ' ')" = 0

# Two resets, nothing else running: both start (the lock only serialises).
ls_json two_ok "$GOLDEN:stopped"
run_bg two_ok a 'arm_run_traps; vm_lock_take; reset_vm' E2E_VM=dictation-vm STUB_RUN_SLEEP=1
run_bg two_ok b 'arm_run_traps; vm_lock_take; reset_vm' E2E_VM=dictation-vm-2 STUB_RUN_SLEEP=1
wait_bg
check "two resets with nothing else running: both succeed" \
  test "$(cat "$tmp/two_ok/a.status")$(cat "$tmp/two_ok/b.status")" = 00
check "and both VMs were started" test "$(sort "$tmp/two_ok/started" | tr '\n' ' ')" = "dictation-vm dictation-vm-2 "

# The start lock held by another run for too long: this one waits, then gives
# up without starting anything.
fake_lock start_busy vm-start "$pid"
run_case start_busy 'reset_vm' E2E_START_LOCK_WAIT=1
check "a start in progress elsewhere: this one gives up after the wait" test "$(status_of start_busy)" != 0
check "the error names the lock" has "'vm-start' is busy" "$tmp/start_busy/err"
check "lume run was not called" lacks "run " "$tmp/start_busy/lume.log"

# The start lock is held until lume ls lists the new VM as running.
ls_json visible unrelated:stopped
run_case visible 'reset_vm' E2E_START_VISIBLE_WAIT=3
check "after lume run, the reset waits for lume ls to list the VM" test "$(status_of visible)" = 0
check "lume ls was asked again after the run" test "$(calls visible | grep -c '^ls ')" -ge 3

mkdir -p "$tmp/ls_fails"
: >"$tmp/ls_fails/ls.fail"
run_case ls_fails 'reset_vm'
check "lume ls failing stops the run" test "$(status_of ls_fails)" != 0
check "and says why" has "lume ls failed" "$tmp/ls_fails/err"

# --- ssh host key for the second VM -------------------------------------------
mkdir -p "$tmp/hk"
printf 'dictation-vm ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIE5M4TnlRfake\ndictation-vm ssh-rsa AAAAB3NzaC1yc2EAAAAfake\n' >"$tmp/hk/known"
run_case hk_add 'ensure_host_key_alias' E2E_VM=dictation-vm-2 KNOWN_HOSTS_FILE="$tmp/hk/known"
check "dictation-vm-2 gets dictation-vm's host keys" test "$(grep -c '^dictation-vm-2 ' "$tmp/hk/known")" = 2
run_case hk_again 'ensure_host_key_alias' E2E_VM=dictation-vm-2 KNOWN_HOSTS_FILE="$tmp/hk/known"
check "and only once" test "$(grep -c '^dictation-vm-2 ' "$tmp/hk/known")" = 2
check "dictation-vm's own entries are unchanged" test "$(grep -c '^dictation-vm ' "$tmp/hk/known")" = 2
: >"$tmp/hk/empty"
run_case hk_none 'ensure_host_key_alias' E2E_VM=dictation-vm-2 KNOWN_HOSTS_FILE="$tmp/hk/empty"
check "no keys to copy: a clear error" has "no host key for dictation-vm-2" "$tmp/hk_none/err"

echo "vm lock tests: $passed passed, $failed failed"
[ "$failed" = 0 ]
