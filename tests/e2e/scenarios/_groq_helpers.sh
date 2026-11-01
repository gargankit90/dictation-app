# Helpers shared by e2e_real_groq and e2e_error_bad_key (M4.4). Sourced by
# run.sh with the scenarios (every scenarios/*.sh), but defines no scenario.
#
# The app reads its API key from the login keychain, so a test has the app
# put one there, the way the Settings window would, and remove it again
# (--debug-set-api-key-stdin and --debug-delete-api-key, debug builds only).
#
# How the secrets are kept out of sight:
#
# - The key (and the VM user's password, below) travel to the VM on the stdin
#   of ssh, never in an argument, so neither is in a process list on the host
#   or in the VM's ssh session, or in any file or log.
# - Every function that expands one first turns shell tracing (`bash -x`,
#   `set -x`) off and puts it back as it was when it is done, so a trace
#   captured from a diagnostic run or CI never holds a secret either. The
#   `{ set +x; } 2>/dev/null` form hides the trace of the `set +x` itself. A
#   scenario must not expand $GROQ_API_KEY itself: it calls these functions,
#   which read it from the environment.
# - The key is removed again whenever the run ends, however it ends
#   (groq_guard_start), and a removal that can't be verified is reported.

# The golden image's login keychain is locked in an ssh session (nobody typed
# the password), and an unlock lasts only for the session that made it, so
# every keychain command unlocks it first. The password is the throwaway VM
# user's (`lume`, see the VM setup docs); set VM_PASSWORD if yours differs.
# It is sent as the first line of stdin and read by the VM's shell.
GROQ_KEYCHAIN_FILE='$HOME/Library/Keychains/login.keychain-db'
# The app's executable in the VM, run directly (not through `open`) so its
# stdin is the ssh session's.
GROQ_VM_APP_BINARY="$VM_APP/Contents/MacOS/$APP_NAME"
# Whether a key may be in the VM's keychain because this run put it there.
GROQ_KEY_STORED=0

# groq_retrace <$- as saved before the set +x>: turns tracing back on if it
# was on.
groq_retrace() {
  case $1 in
    *x*) set -x ;;
  esac
}

# True if GROQ_API_KEY is set and not empty.
groq_key_present() {
  local traced=$- present=1
  { set +x; } 2>/dev/null
  if [ -n "${GROQ_API_KEY:-}" ]; then present=0; fi
  groq_retrace "$traced"
  return "$present"
}

# groq_key_store [key]: saves <key> (default: $GROQ_API_KEY) as the Groq key
# in the VM's login keychain by running the app itself
# (--debug-set-api-key-stdin, debug builds only), which stores it through the
# same path as the Settings window. The app creates the item, so it is in the
# item's access list and reads it later without a prompt nobody is there to
# answer; an item made by the `security` tool would not be.
groq_key_store() {
  local traced=$- status=0
  { set +x; } 2>/dev/null
  _groq_key_store "${1-${GROQ_API_KEY:-}}" || status=$?
  groq_retrace "$traced"
  return "$status"
}

# Tracing is off in here (see groq_key_store).
_groq_key_store() {
  local key=$1 out
  # The app rejects a key with spaces; catch it here so nothing is sent.
  case $key in
    '' | *[[:space:][:cntrl:]]*) die "the API key is empty or has spaces or control characters" ;;
  esac
  # Set first: even a store that fails half way may have left a key.
  GROQ_KEY_STORED=1
  if ! out=$(printf '%s\n' "$key" |
    _groq_vm_with_password "$(q "$GROQ_VM_APP_BINARY") --debug-set-api-key-stdin" 2>&1); then
    die "could not store the key in the VM's keychain: ${out//"$key"/<key>}"
  fi
}

# _groq_vm_with_password <command>: runs <command> in the VM after unlocking
# the login keychain there. The VM user's password is the first stdin line
# (read into a variable by the VM's shell, not an argument); the rest of this
# function's stdin goes on to <command>. Tracing must be off.
_groq_vm_with_password() {
  {
    printf '%s\n' "${VM_PASSWORD:-lume}"
    cat
  } | vm "read -r pw && security unlock-keychain -p \"\$pw\" \"$GROQ_KEYCHAIN_FILE\" && unset pw && $1"
}

# Removes the Groq key from the VM's login keychain, through the app too
# (--debug-delete-api-key). No key there is fine; anything else that goes
# wrong is reported (and fails), since then the key may still be in the
# keychain.
groq_key_remove() {
  local traced=$- status=0 out
  { set +x; } 2>/dev/null
  if out=$(_groq_vm_delete 2>&1); then
    GROQ_KEY_STORED=0
  else
    status=1
    log "  could not remove the key from the VM's keychain (is it still there?): ${out:-no output}"
  fi
  groq_retrace "$traced"
  return "$status"
}

# Tracing must be off. Unlocks, then has the app delete the item.
_groq_vm_delete() {
  _groq_vm_with_password "$(q "$GROQ_VM_APP_BINARY") --debug-delete-api-key" </dev/null
}

# groq_guard_start: from here on, however the run ends (a failed command
# under `set -e`, die, ^C, a TERM or HUP) the key is removed from the VM's
# keychain on the way out, and the run's exit status is kept. Call it before
# the first groq_key_store, and groq_guard_stop when the scenario is done.
groq_guard_start() {
  trap 'groq_guard_exit $?' EXIT
  trap 'exit 130' INT
  trap 'exit 143' TERM
  trap 'exit 129' HUP
}

# Does nothing while a key may still be in the VM's keychain, so the run's
# exit still tries to remove it.
groq_guard_stop() {
  if [ "$GROQ_KEY_STORED" = 1 ]; then
    return 0
  fi
  trap - EXIT INT TERM HUP
}

groq_guard_exit() {
  local status=$1
  trap - EXIT
  if [ "$GROQ_KEY_STORED" = 1 ]; then
    log "cleaning up: removing the key from the VM's keychain"
    if ! groq_key_remove; then
      log "WARNING: the key may still be in the VM's keychain; reset the VM (tests/e2e/run.sh resets it) before anyone else uses it"
    fi
  fi
  exit "$status"
}

# TextEdit, through M4.3's helpers (lib_full.sh): an empty document in
# front to paste into, its text, and closing it without saving.
groq_textedit_new() {
  textedit_fresh
}

groq_textedit_text() {
  textedit_text
}

groq_textedit_quit() {
  textedit_close
}

# Lower case, letters and digits only, one space between words.
groq_words() {
  printf '%s' "$1" | tr '[:upper:]' '[:lower:]' | tr -cs 'a-z0-9' ' ' | sed 's/^ //; s/ $//'
}

# groq_fuzzy_match <expected> <actual> [percent]: whether at least <percent>
# (default 80) of the expected words appear in the actual text, ignoring case
# and punctuation. A speech recogniser may drop or change a word, so an exact
# match would be flaky.
groq_fuzzy_match() {
  local expected actual word found=0 total=0 percent=${3:-80}
  expected=$(groq_words "$1")
  actual=" $(groq_words "$2") "
  for word in $expected; do
    total=$((total + 1))
    case $actual in
      *" $word "*) found=$((found + 1)) ;;
    esac
  done
  log "  matched $found of $total expected words"
  [ "$total" -gt 0 ] && [ $((found * 100)) -ge $((total * percent)) ]
}

# True if the app's log has no line matching the extended regex <pattern>
# (case-insensitive).
groq_log_lacks() {
  ! app_log | grep -qiE -- "$1"
}

# True if the app's log doesn't hold $GROQ_API_KEY. The key reaches grep
# through a pipe (process substitution), not an argument or a file, with
# tracing off.
groq_log_lacks_key() {
  local traced=$- lacks=0
  { set +x; } 2>/dev/null
  if app_log | grep -qF -f <(printf '%s' "$GROQ_API_KEY"); then lacks=1; fi
  groq_retrace "$traced"
  return "$lacks"
}

# Waits up to 20 s for the log to show more than <count> of the lines <pattern>
# matches (extended regex), for a command that repeats.
groq_wait_for_more() {
  local pattern=$1 count=$2 i
  for i in $(seq 1 20); do
    if [ "$(app_log | grep -cE -- "$pattern" || true)" -gt "$count" ]; then
      return 0
    fi
    sleep 1
  done
  return 1
}

# groq_dictate <seconds>: a press, <seconds> later a release, and then waits
# for the pipeline to get back to Idle (up to a minute, for the retries).
# Both are second launches of the running app, which has its audio file
# from its first launch's --debug-audio-file.
groq_dictate() {
  local hold=$1 pressed idle i
  pressed=$(app_log | grep -c 'event=Pressed from="a second launch" delivered=true' || true)
  idle=$(logged_phases | grep -c '^Idle$' || true)
  launch --press
  groq_wait_for_more 'event=Pressed from="a second launch" delivered=true' "$pressed" || return 1
  sleep "$hold"
  launch --release
  for i in $(seq 1 60); do
    if [ "$(logged_phases | grep -c '^Idle$' || true)" -gt "$idle" ]; then
      return 0
    fi
    sleep 1
  done
  return 1
}
