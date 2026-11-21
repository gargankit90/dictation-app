#!/usr/bin/env bash
# Tests for which VM an e2e run uses, the locks and the VM limit (lib_vm.sh),
# with no VM: `lume` is a stub that records its calls. Part of the quality
# gate (scripts/gate.sh).
#
# - E2E_VM takes dictation-vm (default) or dictation-vm-2; anything else,
#   the golden image and its backup included, is refused before lume runs.
# - A reset clones the golden image, then sets memory and CPUs on the clone
#   (never on the golden image), then starts it.
# - A VM's lock is released however the run ends, and refused with a "VM
#   busy" error while a live run holds it. A lock whose run is gone is never
#   taken over: the run fails naming `run.sh --clear-stale-lock <vm>`, which
#   removes it only if its pid is not running. A scenario's own traps don't
#   leave it behind.
# - Clones of the golden image are serialised; a third running macOS VM is
#   refused, and a stale start lock stops a reset.
#
# The races and signal timings of taking the locks were trimmed in SIMP1:
# two VMs and one operator don't need them.
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
    run) echo "$2" >>"$STUB/started" ;;
  esac
}
wait_for_vm() { :; }
terminal_ignore_saved_state() { :; }
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
# A `lume` on PATH for run_sh: logs its calls to $STUB/lume.log and answers
# `lume ls` from $STUB/ls.json (or fails, with $STUB/ls.fail), like the stub in
# the harness.
mkdir -p "$tmp/bin"
cat >"$tmp/bin/lume" <<'LUME'
#!/bin/sh
echo "$*" >>"$STUB/lume.log"
case $1 in
  ls) [ ! -e "$STUB/ls.fail" ] && cat "$STUB/ls.json" ;;
esac
LUME
chmod +x "$tmp/bin/lume"
# run_sh <case> <args...>: tests/e2e/run.sh itself, with the case's lock
# directory and lume stub (for --clear-stale-lock, which needs no VM).
run_sh() {
  local name=$1
  shift
  local dir=$tmp/$name
  mkdir -p "$dir"
  [ -e "$dir/ls.json" ] || echo '[]' >"$dir/ls.json"
  set +e
  (cd "$root" && PATH="$tmp/bin:$PATH" STUB="$dir" E2E_LOCK_DIR="$dir/locks" bash tests/e2e/run.sh "$@") >"$dir/out" 2>"$dir/err" </dev/null
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
# fake_lock <case> <name> <pid> [note]: a lock another run holds (or held).
fake_lock() {
  mkdir -p "$tmp/$1/locks"
  ln -s "$3 2026-11-01 09:30:00${4:+ $4}" "$tmp/$1/locks/$2.lock"
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
check "the reset checks the limit, stops, deletes, clones, sets, checks again, runs, then confirms in lume ls (in that order)" \
  test "$(calls reset1 | cut -d' ' -f1-2 | tr '\n' '|')" = \
  "ls -f|stop dictation-vm|delete dictation-vm|clone $GOLDEN|set dictation-vm|ls -f|run dictation-vm|ls -f|"
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

# stale: never taken over; the run fails and says how to clear it
start_dead_pid
dead=$DEAD_PID
fake_lock stale dictation-vm "$dead"
run_case stale 'arm_run_traps; vm_lock_take; echo reached'
check "a lock whose run is gone: the run fails" test "$(status_of stale)" != 0
check "it says the lock is stale, whose pid, and how to clear it" \
  has "stale lock for dictation-vm from pid $dead (no longer running); remove it with: tests/e2e/run.sh --clear-stale-lock dictation-vm" "$tmp/stale/err"
check "it does not carry on" lacks reached "$tmp/stale/out"
check "it does not take the lock over" test "$(lock_pid stale dictation-vm)" = "$dead"
check "its own exit leaves the stale lock as it was" is_locked stale dictation-vm
check "no 'took over' message" lacks "took over" "$tmp/stale/err"
fake_lock stale_reset dictation-vm "$dead"
run_case stale_reset 'arm_run_traps; vm_lock_take; reset_vm'
check "a stale VM lock: a run never gets as far as lume" no_lume_call stale_reset
fake_lock stale_vm2 dictation-vm-2 "$dead"
run_case stale_vm2 'arm_run_traps; vm_lock_take' E2E_VM=dictation-vm-2
check "dictation-vm-2's stale lock names dictation-vm-2" \
  has "stale lock for dictation-vm-2 from pid $dead (no longer running); remove it with: tests/e2e/run.sh --clear-stale-lock dictation-vm-2" "$tmp/stale_vm2/err"

# a directory lock from an older version of lib_vm.sh counts as a lock too
mkdir -p "$tmp/empty_lock/locks/dictation-vm.lock"
run_case empty_lock 'vm_lock_take'
check "an old-style lock directory with no pid is stale, not taken over" test "$(status_of empty_lock)" != 0
check "and says so" has "stale lock for dictation-vm from pid unknown" "$tmp/empty_lock/err"
mkdir -p "$tmp/old_dead/locks/dictation-vm.lock"
echo "$dead" >"$tmp/old_dead/locks/dictation-vm.lock/pid"
run_case old_dead 'vm_lock_take'
check "an old-style lock directory of a dead run is stale" test "$(status_of old_dead)" != 0
check "and is still there" test -d "$tmp/old_dead/locks/dictation-vm.lock"
mkdir -p "$tmp/old_live/locks/dictation-vm.lock"
echo "$pid" >"$tmp/old_live/locks/dictation-vm.lock/pid"
run_case old_live 'vm_lock_take'
check "an old-style lock directory of a live run is respected" test "$(status_of old_live)" != 0
check "as busy, not stale" has "VM busy: dictation-vm is in use by a run (pid $pid since" "$tmp/old_live/err"

# a run doesn't release a lock it doesn't hold
fake_lock not_mine dictation-vm "$pid"
run_case not_mine 'lock_release dictation-vm'
check "lock_release leaves another run's lock alone" is_locked not_mine dictation-vm


# --- run.sh --clear-stale-lock <vm> ----------------------------------------------
start_dead_pid
dead=$DEAD_PID
for name in dictation-vm dictation-vm-2 vm-start golden-clone; do
  fake_lock "clear_$name" "$name" "$dead"
  run_sh "clear_$name" --clear-stale-lock "$name"
  check "--clear-stale-lock $name: removes a lock whose run is gone" test "$(status_of "clear_$name")" = 0
  check "$name: the lock is gone" not_locked "clear_$name" "$name"
  check "$name: it says what it removed" has "removed the stale '$name' lock (pid $dead is not running)" "$tmp/clear_$name/err"
done
run_case clear_then_take 'arm_run_traps; vm_lock_take'
check "after clearing, the next run takes the VM" test "$(status_of clear_then_take)" = 0

fake_lock clear_live dictation-vm "$pid"
run_sh clear_live --clear-stale-lock dictation-vm
check "--clear-stale-lock refuses a lock whose run is alive" test "$(status_of clear_live)" != 0
check "and says why" has "not removing the 'dictation-vm' lock: pid $pid is still running" "$tmp/clear_live/err"
check "the live run's lock is untouched" test "$(lock_pid clear_live dictation-vm)" = "$pid"
for name in vm-start golden-clone dictation-vm-2; do
  fake_lock "clear_live_$name" "$name" "$pid"
  run_sh "clear_live_$name" --clear-stale-lock "$name"
  check "$name: a live holder is refused too" test "$(status_of "clear_live_$name")" != 0
  check "$name: and its lock stays" is_locked "clear_live_$name" "$name"
done

n=0
for bad in "$GOLDEN" "$BACKUP" dictation-vm-3 dictation-vm.reclaim dictation-vm-golden.lock ../dictation-vm Dictation-VM ""; do
  n=$((n + 1))
  # A (dead) lock under that name exists, as bait.
  mkdir -p "$tmp/clear_bad$n/locks"
  [ -z "$bad" ] || [ "${bad#*/}" != "$bad" ] || fake_lock "clear_bad$n" "$bad" "$dead"
  run_sh "clear_bad$n" --clear-stale-lock "$bad"
  check "--clear-stale-lock '$bad' is refused" test "$(status_of "clear_bad$n")" != 0
  check "'$bad': the error lists the locks it may clear" has "the locks are: dictation-vm dictation-vm-2 golden-clone vm-start" "$tmp/clear_bad$n/err"
  check "'$bad': nothing was removed" test "$(ls "$tmp/clear_bad$n/locks" | wc -l | tr -d ' ')" = "$([ -z "$bad" ] || [ "${bad#*/}" != "$bad" ] && echo 0 || echo 1)"
done
run_sh clear_noarg --clear-stale-lock
check "--clear-stale-lock with no name is an error" test "$(status_of clear_noarg)" != 0
run_sh clear_none --clear-stale-lock dictation-vm
check "no lock at all: nothing to do, no error" test "$(status_of clear_none)" = 0
check "and it says so" has "there is no 'dictation-vm' lock" "$tmp/clear_none/err"
mkdir -p "$tmp/clear_olddir/locks/dictation-vm.lock"
echo "$dead" >"$tmp/clear_olddir/locks/dictation-vm.lock/pid"
run_sh clear_olddir --clear-stale-lock dictation-vm
check "an old-style lock directory of a dead run is cleared too" not_locked clear_olddir dictation-vm
mkdir -p "$tmp/clear_olddir_live/locks/dictation-vm.lock"
echo "$pid" >"$tmp/clear_olddir_live/locks/dictation-vm.lock/pid"
run_sh clear_olddir_live --clear-stale-lock dictation-vm
check "and a live one's is refused" is_locked clear_olddir_live dictation-vm
run_sh clear_help --help
check "--help lists --clear-stale-lock" has "--clear-stale-lock" "$tmp/clear_help/out"

# --- A scenario's own traps don't leave the lock behind -----------------------
# (R2) Scenarios arm their own EXIT trap (arm_key_cleanup, apikey_guard_start...),
# which would replace the runner's. run_scenario (lib.sh) runs each in a
# subshell, so the runner's traps stay armed: the lock is held while the run
# goes on and released when it ends.
SCEN='
OUT_DIR=$STUB/scen-out; mkdir -p "$OUT_DIR"
collect_log() { :; }
held() { [ -L "$E2E_LOCK_DIR/dictation-vm.lock" ] && echo held || echo not-held; }
scen_ok() { true; }
scen_die() { trap '"'"'echo scenario-cleanup >&2; exit 1'"'"' EXIT; die "simulated failure"; }
scen_term_self() { trap '"'"'echo scenario-cleanup >&2; exit 143'"'"' EXIT; trap "exit 143" TERM; kill -TERM $BASHPID; sleep 5; echo survived; }
scen_term_runner() { trap '"'"'echo scenario-cleanup >&2; exit 1'"'"' EXIT; kill -TERM $$; sleep 1; true; }
scen_exit7() { exit 7; }
scen_failed() { SCENARIO_FAILED=1; }
scen_skipped() { SCENARIO_SKIPPED=1; }
'
run_case scen_die "$SCEN"'arm_run_traps; vm_lock_take; run_scenario scen_die; echo "$SCENARIO_RESULT $(held)"; trap -p EXIT'
check "a scenario that dies with its own EXIT trap armed fails, and the run goes on" \
  test "$(head -1 "$tmp/scen_die/out")" = "FAIL held"
check "the runner's EXIT trap is still armed" has release_run_locks "$tmp/scen_die/out"
check "the VM lock is released when the run ends" not_locked scen_die dictation-vm
check "the scenario's own cleanup ran, once" test "$(grep -c scenario-cleanup "$tmp/scen_die/err")" = 1
run_case scen_key "$SCEN"'
  . tests/e2e/lib_full.sh
  key_cleanup_steps() { echo steps >&2; }
  scen_key() { arm_key_cleanup scen_key; die "simulated failure"; }
  arm_run_traps; vm_lock_take; run_scenario scen_key; echo "$SCENARIO_RESULT $(held)"'
check "the real arm_key_cleanup: the scenario fails, the run goes on holding the lock" \
  test "$(head -1 "$tmp/scen_key/out")" = "FAIL held"
check "its cleanup steps ran" has steps "$tmp/scen_key/err"
check "the VM lock is released when the run ends" not_locked scen_key dictation-vm
run_case scen_term_self "$SCEN"'arm_run_traps; vm_lock_take; run_scenario scen_term_self; echo "$SCENARIO_RESULT $(held)"'
check "a scenario that gets TERM: it fails, the run goes on" test "$(head -1 "$tmp/scen_term_self/out")" = "FAIL held"
check "its cleanup ran" has scenario-cleanup "$tmp/scen_term_self/err"
check "the VM lock is released when the run ends" not_locked scen_term_self dictation-vm
run_case scen_term_runner "$SCEN"'arm_run_traps; vm_lock_take; run_scenario scen_term_runner; echo survived'
check "TERM to the run while a scenario's trap is armed: the run ends with 143" test "$(status_of scen_term_runner)" = 143
check "the VM lock is released" not_locked scen_term_runner dictation-vm
check "the scenario's cleanup ran" has scenario-cleanup "$tmp/scen_term_runner/err"
run_case scen_exit7 "$SCEN"'arm_run_traps; vm_lock_take; run_scenario scen_exit7; echo "$SCENARIO_RESULT $(held)"'
check "a scenario that exits non-zero fails, the run goes on" test "$(head -1 "$tmp/scen_exit7/out")" = "FAIL held"
check "the VM lock is released when the run ends" not_locked scen_exit7 dictation-vm
run_case scen_flags "$SCEN"'
  arm_run_traps; vm_lock_take
  for s in scen_ok scen_failed scen_skipped; do
    run_scenario $s; echo "$s $SCENARIO_RESULT"
  done'
check "run_scenario reports ok, failed and skipped, and the run goes on" \
  test "$(tr '\n' '|' <"$tmp/scen_flags/out")" = "scen_ok PASS|scen_failed FAIL|scen_skipped SKIP|"
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
check "a stale clone lock: the reset fails" test "$(status_of clone_stale)" != 0
check "and says how to clear it" \
  has "stale lock for golden-clone from pid $dead (no longer running); remove it with: tests/e2e/run.sh --clear-stale-lock golden-clone" "$tmp/clone_stale/err"
check "nothing was cloned" lacks "clone " "$tmp/clone_stale/lume.log"
check "and the stale lock is left alone" test "$(lock_pid clone_stale golden-clone)" = "$dead"

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

# The start lock left by a run that died: the next reset says so and starts nothing.
fake_lock start_stale vm-start "$dead"
run_case start_stale 'reset_vm'
check "a stale start lock: the reset fails" test "$(status_of start_stale)" != 0
check "and says how to clear it" \
  has "stale lock for vm-start from pid $dead (no longer running); remove it with: tests/e2e/run.sh --clear-stale-lock vm-start" "$tmp/start_stale/err"
check "lume run was not called" lacks "run " "$tmp/start_stale/lume.log"

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
