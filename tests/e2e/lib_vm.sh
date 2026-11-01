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
# A lock is the directory $E2E_LOCK_DIR/<name>.lock (mkdir is atomic) holding
# `pid` (the run's shell) and `since` (when it took the lock). A lock whose pid
# is gone is stale and is taken over; the takeover itself is serialised with a
# <name>.lock.reclaim directory, so two runs don't both take over one lock.

lock_path() { printf '%s/%s.lock' "$E2E_LOCK_DIR" "$1"; }

# lock_holder <name>: "pid since" of the current holder, "" if none.
lock_holder() {
  local dir
  dir=$(lock_path "$1")
  [ -d "$dir" ] || return 0
  printf '%s since %s' "$(cat "$dir/pid" 2>/dev/null || true)" "$(cat "$dir/since" 2>/dev/null || true)"
}

lock_write() {
  local dir=$1
  date '+%F %T' >"$dir/since"
  printf '%s\n' "$$" >"$dir/pid.tmp"
  mv "$dir/pid.tmp" "$dir/pid"
}

# lock_try <name>: takes the lock if it is free or stale. Returns 1 if a live
# process holds it.
lock_try() {
  local name=$1 dir pid pid_again
  dir=$(lock_path "$name")
  mkdir -p "$E2E_LOCK_DIR"
  if mkdir "$dir" 2>/dev/null; then
    lock_write "$dir"
    return 0
  fi
  pid=$(cat "$dir/pid" 2>/dev/null || true)
  if [ -z "$pid" ]; then
    # Made a moment ago and not filled in yet? Give its owner a second.
    sleep 1
    pid=$(cat "$dir/pid" 2>/dev/null || true)
  fi
  if [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null; then
    return 1
  fi
  # Stale: its run is gone. Only one run at a time takes it over, and only if
  # it is still the same dead holder once that run has the reclaim mutex.
  mkdir "$dir.reclaim" 2>/dev/null || return 1
  pid_again=$(cat "$dir/pid" 2>/dev/null || true)
  if [ "$pid_again" = "$pid" ] && rm -rf "$dir" && mkdir "$dir" 2>/dev/null; then
    lock_write "$dir"
    rmdir "$dir.reclaim"
    log "took over the stale '$name' lock (pid ${pid:-unknown} is gone)"
    return 0
  fi
  rmdir "$dir.reclaim" 2>/dev/null || true
  return 1
}

# lock_take <name> [wait_seconds]: takes the lock, waiting up to wait_seconds
# (default 0) for a live holder; dies with "busy (pid N since T)" if it can't.
lock_take() {
  local name=$1 wait=${2:-0} i=0
  while ! lock_try "$name"; do
    if [ "$i" -ge "$wait" ]; then
      die "'$name' is busy (pid $(lock_holder "$name")); if that run is gone, remove $(lock_path "$name")"
    fi
    sleep 1
    i=$((i + 1))
  done
  E2E_HELD_LOCKS+=("$name")
}

# lock_release <name>: releases the lock if this process holds it.
lock_release() {
  local name=$1 dir held=() other
  dir=$(lock_path "$name")
  if [ "$(cat "$dir/pid" 2>/dev/null || true)" = "$$" ]; then
    rm -rf "$dir"
  fi
  for other in ${E2E_HELD_LOCKS[@]+"${E2E_HELD_LOCKS[@]}"}; do
    [ "$other" = "$name" ] || held+=("$other")
  done
  E2E_HELD_LOCKS=(${held[@]+"${held[@]}"})
}

# The VM's lock: one run per VM at a time.
vm_lock_take() {
  local holder
  if ! lock_try "$VM"; then
    holder=$(lock_holder "$VM")
    die "VM busy: $VM is in use by a run (pid $holder). Wait for it, or use the other VM (E2E_VM=...); if that run is gone, remove $(lock_path "$VM")"
  fi
  E2E_HELD_LOCKS+=("$VM")
}

release_run_locks() {
  local name
  for name in ${E2E_HELD_LOCKS[@]+"${E2E_HELD_LOCKS[@]}"}; do
    lock_release "$name"
  done
}

# Releases the VM lock however the run ends: a normal end or `die` (EXIT) and
# Ctrl-C, kill, a closed terminal (which `exit`, so EXIT runs). A run that dies
# without running its traps (kill -9) leaves a stale lock, which the next run
# takes over.
arm_run_traps() {
  trap release_run_locks EXIT
  trap 'exit 130' INT
  trap 'exit 143' TERM
  trap 'exit 129' HUP
}

# --- Limits and setup --------------------------------------------------------

# Refuses to go on if starting $VM would make a third macOS VM run. A $VM that
# is running now doesn't count: the reset stops it first.
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
