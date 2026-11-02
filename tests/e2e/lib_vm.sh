# Which Lume VM a run uses, the locks that keep two runs apart, and the limit
# on running VMs. Sourced by lib.sh (after log and die).
#
# Two VMs, because Apple allows two macOS VMs at once: `dictation-vm` for
# Developers and `dictation-vm-2` for Testers. Both are clones of the golden
# image, which is only ever cloned, never started, changed or deleted.
# tests/e2e/test-vm-lock.sh checks this file with a stubbed lume.

GOLDEN=dictation-vm-golden
# The only names a run may use as its VM. Anything else (the golden image,
# its backups, a typo) is refused before lume is called.
E2E_VM_NAMES=(dictation-vm dictation-vm-2)
# Apple's limit on macOS VMs running at the same time.
E2E_MAX_RUNNING_VMS=2
E2E_LOCK_DIR="${E2E_LOCK_DIR:-$HOME/.cache/dictation-e2e}"
# The locks this process holds, released by release_run_locks.
E2E_HELD_LOCKS=()

validate_vm_name() {
  local name=$1 ok
  for ok in "${E2E_VM_NAMES[@]}"; do
    [ "$name" = "$ok" ] && return 0
  done
  return 1
}

VM=${E2E_VM:-dictation-vm}
validate_vm_name "$VM" ||
  die "E2E_VM must be one of: ${E2E_VM_NAMES[*]} (got '$VM'); the golden image and its backups are never a run's VM"

# --- Locks -------------------------------------------------------------------
# A lock is the symlink $E2E_LOCK_DIR/<name>.lock whose target is the holder's
# "<pid> <time>". `ln -s` either creates it, owner and all, or fails because
# it exists, so there is no moment when a lock is held but its owner is
# unknown. That single create is the only way to take a lock.
#
# A lock that exists is never taken over. If its pid is alive the run is
# refused as busy; if its pid is gone the lock is stale and the run fails
# with the command that clears it (clear_stale_lock, behind
# `tests/e2e/run.sh --clear-stale-lock <name>`, which removes a lock only if
# its pid is not running). A run that dies without running its traps
# (kill -9) is the only way to get one. No automatic recovery: one needs a
# second lock to guard it, and that one can go stale too.
#
# Locks are registered in E2E_HELD_LOCKS *before* they are attempted, and
# lock_release removes a lock only if its pid is this process's, so the EXIT
# trap (arm_run_traps) is correct at any point of an acquisition: a signal
# before, during or after the `ln -s`, a lock lost to someone else (nothing
# to remove) and a second release (already gone) all end clean.

lock_path() { printf '%s/%s.lock' "$E2E_LOCK_DIR" "$1"; }

pid_alive() {
  case $1 in '' | *[!0-9]*) return 1 ;; esac
  kill -0 "$1" 2>/dev/null
}

# lock_read <name>: prints the lock's "pid time"; fails if there is no lock.
# (A directory lock, from an older version of this file, reads as its pid and
# since files.)
lock_read() {
  local path
  path=$(lock_path "$1")
  if [ -L "$path" ]; then
    readlink "$path" 2>/dev/null
  elif [ -d "$path" ]; then
    printf '%s %s' "$(cat "$path/pid" 2>/dev/null || true)" "$(cat "$path/since" 2>/dev/null || true)"
  else
    return 1
  fi
}

# lock_holder <name>: "pid since time" of the current holder, "" if none.
lock_holder() {
  local cur
  cur=$(lock_read "$1") || return 0
  printf '%s since %s' "${cur%% *}" "${cur#* }"
}

lock_register() {
  local other
  for other in ${E2E_HELD_LOCKS[@]+"${E2E_HELD_LOCKS[@]}"}; do
    [ "$other" = "$1" ] && return 0
  done
  E2E_HELD_LOCKS+=("$1")
}

lock_forget() {
  local held=() other
  for other in ${E2E_HELD_LOCKS[@]+"${E2E_HELD_LOCKS[@]}"}; do
    [ "$other" = "$1" ] || held+=("$other")
  done
  E2E_HELD_LOCKS=(${held[@]+"${held[@]}"})
}

# lock_try <name>: takes the lock with one `ln -s`. Returns 0 if it did, 1 if
# a live process holds it, 2 if the holder is gone (a stale lock, left alone;
# the pid is in LOCK_STALE_PID). Callers register the name first (lock_take
# does).
lock_try() {
  local name=$1 path mine cur tries=0
  path=$(lock_path "$name")
  mkdir -p "$E2E_LOCK_DIR"
  mine="$$ $(date '+%F %T')"
  while [ "$tries" -lt 4 ]; do
    tries=$((tries + 1))
    # (`ln -s` onto an old-style lock directory would link inside it.)
    if { [ -L "$path" ] || [ ! -d "$path" ]; } && ln -s "$mine" "$path" 2>/dev/null; then
      return 0
    fi
    # Held. By whom? (Gone again by the time we look: try again.)
    cur=$(lock_read "$name") || continue
    LOCK_STALE_PID=${cur%% *}
    if pid_alive "$LOCK_STALE_PID"; then
      return 1
    fi
    return 2
  done
  return 1
}

# lock_refuse <name> <lock_try status>: ends the run with the reason the lock
# wasn't taken: busy (a live holder) or stale (its pid is gone).
lock_refuse() {
  local name=$1 rc=$2
  lock_forget "$name"
  if [ "$rc" = 2 ]; then
    die "stale lock for $name from pid ${LOCK_STALE_PID:-unknown} (no longer running); remove it with: tests/e2e/run.sh --clear-stale-lock $name"
  elif validate_vm_name "$name"; then
    die "VM busy: $name is in use by a run (pid $(lock_holder "$name")). Wait for it, or use the other VM (E2E_VM=...)"
  else
    die "'$name' is busy (pid $(lock_holder "$name"))"
  fi
}

# lock_take <name> [wait_seconds]: takes the lock, waiting up to wait_seconds
# (default 0) for a live holder; dies if it is still busy then, or at once if
# the lock is stale.
lock_take() {
  local name=$1 wait=${2:-0} i=0 rc
  lock_register "$name"
  while :; do
    rc=0
    lock_try "$name" || rc=$?
    [ "$rc" = 0 ] && return 0
    if [ "$rc" = 2 ] || [ "$i" -ge "$wait" ]; then
      lock_refuse "$name" "$rc"
    fi
    sleep 1
    i=$((i + 1))
  done
}

# lock_release <name>: releases the lock if this process holds it; does
# nothing if it doesn't (never held, lost, already released).
lock_release() {
  local name=$1 path cur
  path=$(lock_path "$name")
  if cur=$(lock_read "$name") && [ "${cur%% *}" = "$$" ]; then
    rm -rf "$path"
  fi
  lock_forget "$name"
}

# The VM's lock: one run per VM at a time.
vm_lock_take() { lock_take "$VM"; }

# The locks clear_stale_lock may remove: the two VMs' and the two host-wide
# ones (the clone lock and the start lock).
E2E_CLEARABLE_LOCKS=("${E2E_VM_NAMES[@]}" golden-clone vm-start)

# clear_stale_lock <name>: removes a lock whose holder is gone. Refuses for a
# holder that is still running and for any name but those above (the golden
# image and its backups never have a lock).
clear_stale_lock() {
  local name=${1:-} ok found=0 cur pid
  for ok in "${E2E_CLEARABLE_LOCKS[@]}"; do
    [ "$name" = "$ok" ] && found=1
  done
  [ "$found" = 1 ] ||
    die "can't clear a lock named '$name'; the locks are: ${E2E_CLEARABLE_LOCKS[*]}"
  if ! cur=$(lock_read "$name"); then
    log "there is no '$name' lock"
    return 0
  fi
  pid=${cur%% *}
  pid_alive "$pid" &&
    die "not removing the '$name' lock: pid $pid is still running (since ${cur#* })"
  # Gone since we read it, or replaced by a live run's: leave that alone.
  [ "$(lock_read "$name" || true)" = "$cur" ] ||
    die "the '$name' lock changed while checking it; run this again"
  rm -rf "$(lock_path "$name")"
  log "removed the stale '$name' lock (pid ${pid:-unknown} is not running)"
}

release_run_locks() {
  local name
  for name in ${E2E_HELD_LOCKS[@]+"${E2E_HELD_LOCKS[@]}"}; do
    lock_release "$name"
  done
}

# Releases the run's locks however it ends: a normal end or `die` (EXIT) and
# Ctrl-C, kill, a closed terminal (which `exit`, so EXIT runs). Arm it before
# taking any lock. A run that dies without running its traps (kill -9) leaves
# a stale lock, which --clear-stale-lock removes.
arm_run_traps() {
  trap release_run_locks EXIT
  trap 'exit 130' INT
  trap 'exit 143' TERM
  trap 'exit 129' HUP
}

# --- Limits and setup --------------------------------------------------------

# Refuses to go on if starting $VM would make a third macOS VM run. A $VM that
# is running now doesn't count: the reset stops it first. Only a check; two
# runs can pass it together. start_vm repeats it under the start lock.
refuse_third_vm() {
  local json others
  json=$(lume ls -f json 2>/dev/null) || die "lume ls failed; can't count the running VMs"
  others=$(printf '%s' "$json" |
    jq -r --arg vm "$VM" '[.[] | select(.os == "macOS" and .status == "running" and .name != $vm)] | length') ||
    die "can't read the output of lume ls"
  if [ "$others" -ge "$E2E_MAX_RUNNING_VMS" ]; then
    die "$others other macOS VMs are running and Apple allows $E2E_MAX_RUNNING_VMS at once; stop one first (lume ls)"
  fi
}

# Applies the run's memory and CPU settings to the fresh clone, before it is
# started. Only ever the run's own VM, never the golden image.
set_vm_resources() {
  validate_vm_name "$VM" || die "refusing lume set on '$VM'"
  lume set "$VM" --memory "${E2E_VM_MEMORY:-6GB}" --cpu "${E2E_VM_CPU:-4}" >/dev/null ||
    die "lume set failed on $VM"
}

# Clones the golden image into $VM, one clone at a time on this Mac.
clone_golden() {
  lock_take golden-clone "${E2E_CLONE_LOCK_WAIT:-600}"
  lume clone "$GOLDEN" "$VM" >/dev/null || die "lume clone failed"
  lock_release golden-clone
}

# ssh checks the VM's host key under the alias $VM. Clones keep the golden
# image's host key, so a second VM's alias gets the first one's key.
ensure_host_key_alias() {
  local known=${KNOWN_HOSTS_FILE:-$HOME/.ssh/known_hosts}
  ssh-keygen -F "$VM" -f "$known" >/dev/null 2>&1 && return 0
  grep -q '^dictation-vm ' "$known" 2>/dev/null ||
    die "no host key for $VM in $known, and none for dictation-vm to copy; ssh to the VM once"
  log "adding $VM to $known with dictation-vm's host keys (clones share the golden image's)"
  sed -n 's/^dictation-vm /'"$VM"' /p' "$known" >>"$known"
}

# Starts $VM. The count of running VMs and the start are one step under the
# start lock, which is held until `lume ls` shows the new VM as running: two
# runs resetting their VMs together would otherwise both count one other VM
# and both start, one too many. If the VM isn't listed by the deadline the
# start is undone (the VM is stopped) and the run fails; the lock is never
# released while the start is unconfirmed. Lock order: VM lock, then (never
# together) the clone lock or the start lock.
start_vm() {
  lock_take vm-start "${E2E_START_LOCK_WAIT:-300}"
  refuse_third_vm
  lume run "$VM" --no-display --detach >/dev/null || abort_start "lume run failed"
  wait_vm_listed_running ||
    abort_start "lume ls didn't list $VM as running within ${E2E_START_VISIBLE_WAIT:-120} s"
  lock_release vm-start
}

# Polls `lume ls` until it lists $VM as running, for E2E_START_VISIBLE_WAIT
# seconds (default 120), checking at least once. A failing `lume ls` counts as
# not listed. Returns 1 at the deadline.
wait_vm_listed_running() {
  local wait=${E2E_START_VISIBLE_WAIT:-120} start=$SECONDS
  while :; do
    if lume ls -f json 2>/dev/null |
      jq -e --arg vm "$VM" 'any(.[]; .name == $vm and .status == "running")' >/dev/null 2>&1; then
      return 0
    fi
    [ $((SECONDS - start)) -lt "$wait" ] || return 1
    sleep 1
  done
}

# The start of $VM wasn't confirmed: stops it (it may be starting), releases
# the start lock only then, and ends the run.
abort_start() {
  local why=$1 stopped=0
  validate_vm_name "$VM" || die "refusing lume stop on '$VM'"
  lume stop "$VM" >/dev/null 2>&1 && stopped=1
  lock_release vm-start
  if [ "$stopped" = 1 ]; then
    die "$why; stopped $VM. Check lume ls, then run again"
  fi
  die "$why, and lume stop $VM failed; it may still be starting. Check lume ls and stop it before running again"
}
