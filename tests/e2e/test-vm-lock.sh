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
#   "VM busy" error while a live run holds it, and taken over when stale.
# - Clones of the golden image are serialised; a third running macOS VM is
#   refused.
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

lume() {
  echo "$*" >>"$STUB/lume.log"
  case $1 in
    ls) if [ -e "$STUB/ls.fail" ]; then return 1; fi; cat "$STUB/ls.json" ;;
    clone) [ -z "${STUB_CLONE_SLEEP:-}" ] || sleep "$STUB_CLONE_SLEEP" ;;
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
  local dir=$tmp/$1/locks/$2.lock
  mkdir -p "$dir"
  echo "$3" >"$dir/pid"
  echo "2026-11-01 09:30:00" >"$dir/since"
}
lock_dir() { printf '%s/%s/locks/%s.lock' "$tmp" "$1" "$2"; }
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
check "the reset stops, deletes, clones, sets, then runs (in that order)" \
  test "$(calls reset1 | cut -d' ' -f1-2 | sed -n '2,$p' | tr '\n' '|')" = \
  "stop dictation-vm|delete dictation-vm|clone $GOLDEN|set dictation-vm|run dictation-vm|"
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
check "the clone lock is gone after the reset" test ! -e "$(lock_dir reset2 golden-clone)"

# set_vm_resources refuses anything but a run's VM, even if VM is overwritten
run_case set_golden 'VM='"$GOLDEN"'; set_vm_resources'
check "set_vm_resources refuses the golden image" test "$(status_of set_golden)" != 0
check "and never calls lume set" no_lume_call set_golden

# --- The VM lock ---------------------------------------------------------------
run_case lock_ok 'vm_lock_take; cat "$E2E_LOCK_DIR/dictation-vm.lock/pid" >&2; lock_release dictation-vm; [ ! -e "$E2E_LOCK_DIR/dictation-vm.lock" ]'
check "the lock is taken and released" test "$(status_of lock_ok)" = 0
check "the lock held the run's pid" test "$(cat "$tmp/lock_ok/err")" != ""

run_case lock_held_inside 'vm_lock_take; [ -f "$E2E_LOCK_DIR/dictation-vm.lock/pid" ] && [ -f "$E2E_LOCK_DIR/dictation-vm.lock/since" ]'
check "while held, the lock has a pid and a start time" test "$(status_of lock_held_inside)" = 0

run_case lock_vm2 'vm_lock_take; [ -d "$E2E_LOCK_DIR/dictation-vm-2.lock" ] && [ ! -e "$E2E_LOCK_DIR/dictation-vm.lock" ]' E2E_VM=dictation-vm-2
check "dictation-vm-2 uses its own lock" test "$(status_of lock_vm2)" = 0

# released however the run ends
run_case end_normal 'arm_run_traps; vm_lock_take; true'
check "a normal end releases the lock" test ! -e "$(lock_dir end_normal dictation-vm)"
check "a normal end exits 0" test "$(status_of end_normal)" = 0
run_case end_die 'arm_run_traps; vm_lock_take; die boom'
check "die releases the lock" test ! -e "$(lock_dir end_die dictation-vm)"
check "die keeps its exit status" test "$(status_of end_die)" = 1
run_case end_status 'arm_run_traps; vm_lock_take; exit 7'
check "exit 7 releases the lock" test ! -e "$(lock_dir end_status dictation-vm)"
check "and keeps status 7" test "$(status_of end_status)" = 7
for signal in INT TERM HUP; do
  run_case "end_$signal" 'arm_run_traps; vm_lock_take; kill -'"$signal"' $$; sleep 1; echo survived'
  check "$signal releases the lock" test ! -e "$(lock_dir "end_$signal" dictation-vm)"
  check "$signal ends the run" test "$(status_of "end_$signal")" != 0
  check "$signal: the run did not carry on" lacks survived "$tmp/end_$signal/out"
done
run_case end_rearm 'arm_run_traps; vm_lock_take; trap - EXIT INT TERM HUP; arm_run_traps; true'
check "re-arming after a scenario's own traps still releases the lock" test ! -e "$(lock_dir end_rearm dictation-vm)"

# busy
start_live_pid
pid=$LIVE_PID
fake_lock busy dictation-vm "$pid"
run_case busy 'arm_run_traps; vm_lock_take; echo reached'
check "a live run holding the VM: refused" test "$(status_of busy)" != 0
check "the error says VM busy with the pid" has "VM busy: dictation-vm is in use by a run (pid $pid since 2026-11-01 09:30:00)" "$tmp/busy/err"
check "the busy run does not carry on" lacks reached "$tmp/busy/out"
check "the holder's lock is untouched" test "$(cat "$(lock_dir busy dictation-vm)/pid")" = "$pid"
check "the holder's lock survives the refused run's exit" test -d "$(lock_dir busy dictation-vm)"
fake_lock busy_reset dictation-vm "$pid"
run_case busy_reset 'arm_run_traps; vm_lock_take; reset_vm'
check "busy: a run never gets as far as lume" no_lume_call busy_reset

# the other VM's lock doesn't matter
fake_lock other_vm dictation-vm "$pid"
run_case other_vm 'arm_run_traps; vm_lock_take' E2E_VM=dictation-vm-2
check "dictation-vm busy, dictation-vm-2 free: it runs" test "$(status_of other_vm)" = 0
check "dictation-vm's lock is untouched" test "$(cat "$(lock_dir other_vm dictation-vm)/pid")" = "$pid"

# stale
start_dead_pid
dead=$DEAD_PID
fake_lock stale dictation-vm "$dead"
run_case stale 'vm_lock_take; cat "$E2E_LOCK_DIR/dictation-vm.lock/pid"'
check "a lock whose run is gone is taken over" test "$(status_of stale)" = 0
check "and says so" has "took over the stale 'dictation-vm' lock (pid $dead is gone)" "$tmp/stale/err"
check "the new lock holds the new run's pid" test "$(cat "$tmp/stale/out")" != "$dead"
run_case stale_release 'arm_run_traps; vm_lock_take'
fake_lock stale_release dictation-vm "$dead"
run_case stale_release 'arm_run_traps; vm_lock_take'
check "a taken-over lock is released at the end" test ! -e "$(lock_dir stale_release dictation-vm)"

mkdir -p "$tmp/empty_lock/locks/dictation-vm.lock"
run_case empty_lock 'vm_lock_take'
check "a lock directory with no pid (its run died half way) is taken over" test "$(status_of empty_lock)" = 0

# a run doesn't release a lock it doesn't hold
fake_lock not_mine dictation-vm "$pid"
run_case not_mine 'lock_release dictation-vm'
check "lock_release leaves another run's lock alone" test -d "$(lock_dir not_mine dictation-vm)"

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
