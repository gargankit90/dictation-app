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
# unknown (a mkdir-then-write-the-pid lock has one, and a contender can't tell
# a slow owner from a dead one). Every holder is therefore verifiable: a lock
# whose pid is alive is never taken over, however young it is.
#
# A lock whose pid is gone is stale and is taken over. Two runs must not both
# take over one stale lock, so a takeover first takes <name>.reclaim.lock the
# same way, re-reads the lock under it and removes it only if it is still the
# stale one it judged. A reclaimer that dies leaves a reclaim lock with a dead
# pid, which the next contender clears.
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

# lock_try <name>: takes the lock if it is free or stale. Returns 1 if a live
# process holds it. Callers register the name first (lock_take does).
lock_try() {
  local name=$1 path rname rpath mine cur pid rcur tries=0
  path=$(lock_path "$name")
  rname=$name.reclaim
  rpath=$(lock_path "$rname")
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
    pid=${cur%% *}
    if pid_alive "$pid"; then
      return 1
    fi
    # Stale. One run at a time takes it over.
    lock_register "$rname"
    if ! ln -s "$$" "$rpath" 2>/dev/null; then
      # Another run is taking it over, or died doing so.
      rcur=$(readlink "$rpath" 2>/dev/null) || continue
      pid_alive "${rcur%% *}" && return 1
      [ "$(readlink "$rpath" 2>/dev/null || true)" = "$rcur" ] && rm -f "$rpath"
      continue
    fi
    # We hold the reclaim lock: is it still the same stale lock?
    if [ "$(lock_read "$name" || true)" = "$cur" ]; then
      rm -rf "$path"
      log "took over the stale '$name' lock (pid ${pid:-unknown} is gone)"
    fi
    if [ ! -d "$path" ] && ln -s "$mine" "$path" 2>/dev/null; then
      lock_release "$rname"
      return 0
    fi
    lock_release "$rname" # someone else got in once it was gone
  done
  return 1
}

# lock_take <name> [wait_seconds]: takes the lock, waiting up to wait_seconds
# (default 0) for a live holder; dies with "busy (pid N since T)" if it can't.
lock_take() {
  local name=$1 wait=${2:-0} i=0
  lock_register "$name"
  while ! lock_try "$name"; do
    if [ "$i" -ge "$wait" ]; then
      lock_forget "$name"
      die "'$name' is busy (pid $(lock_holder "$name")); if that run is gone, remove $(lock_path "$name")"
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
vm_lock_take() {
  local holder
  lock_register "$VM"
  if ! lock_try "$VM"; then
    holder=$(lock_holder "$VM")
    lock_forget "$VM"
    die "VM busy: $VM is in use by a run (pid $holder). Wait for it, or use the other VM (E2E_VM=...); if that run is gone, remove $(lock_path "$VM")"
  fi
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
# a stale lock, which the next run takes over.
arm_run_traps() {
  trap release_run_locks EXIT
  trap 'exit 130' INT
  trap 'exit 143' TERM
  trap 'exit 129' HUP
}

# run_scenario <function>: runs one scenario and sets SCENARIO_FAILED and
# SCENARIO_SKIPPED for the runner. The scenario runs in a subshell: it arms its
# own EXIT/INT/TERM traps (arm_key_cleanup, groq_guard_start, ...), which would
# replace the runner's and leave the VM lock behind when it fails. In a
# subshell they replace nothing; the runner's traps stay armed and release the
# locks. A scenario that dies (its own cleanup has run by then) ends the run.
run_scenario() {
  local scenario=$1 rc
  SCENARIO_FAILED=0
  SCENARIO_SKIPPED=0
  set +e
  (
    set -e
    SCENARIO_FAILED=0
    SCENARIO_SKIPPED=0
    "$scenario"
    # 100 + 1 (failed) + 2 (skipped)
    exit $((100 + SCENARIO_FAILED + 2 * SCENARIO_SKIPPED))
  )
  rc=$?
  set -e
  case $rc in
    100) ;;
    101) SCENARIO_FAILED=1 ;;
    102) SCENARIO_SKIPPED=1 ;;
    103)
      SCENARIO_FAILED=1
      SCENARIO_SKIPPED=1
      ;;
    *) die "scenario $scenario ended the run (status $rc)" ;;
  esac
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
# start lock, which is held until the new VM shows as running in `lume ls`:
# two runs resetting their VMs together would otherwise both count one other
# VM and both start, one too many. Lock order: VM lock, then (never together)
# the clone lock or the start lock.
start_vm() {
  lock_take vm-start "${E2E_START_LOCK_WAIT:-300}"
  refuse_third_vm
  lume run "$VM" --no-display --detach >/dev/null || die "lume run failed"
  wait_vm_listed_running
  lock_release vm-start
}

# Waits (up to E2E_START_VISIBLE_WAIT seconds, default 30) for `lume ls` to
# list $VM as running, so the next run's count sees it. Goes on if it never
# does.
wait_vm_listed_running() {
  local wait=${E2E_START_VISIBLE_WAIT:-30} i=0
  while [ "$i" -lt "$wait" ]; do
    if lume ls -f json 2>/dev/null |
      jq -e --arg vm "$VM" 'any(.[]; .name == $vm and .status == "running")' >/dev/null 2>&1; then
      return 0
    fi
    sleep 1
    i=$((i + 1))
  done
  [ "$wait" = 0 ] || log "warning: lume ls doesn't list $VM as running yet; continuing"
  return 0
}
