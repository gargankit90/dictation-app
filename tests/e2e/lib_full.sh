# Helpers for the full-app scenarios (M4.3): the real hotkey, the real paste
# into TextEdit, the real clipboard; only the audio (a WAV file) and the
# transcript (the mock transcriber) are stand-ins. Sourced by run.sh after
# lib.sh, so it can use lib.sh's helpers.

# What the mock transcriber answers (--mock-text on the first launch).
MOCK_TEXT="hello world"
VM_SILENCE="$VM_FIXTURES/silence-16k.wav"
VM_RIGHT_OPTION="$VM_FIXTURES/right-option.js"
# Where the scripts run in Terminal.app leave their output.
VM_HOME="/Users/$VM_USER"
VM_RUN_DIR="$VM_HOME/e2e-run"

# The first launch of a full-app scenario: records `file` (default: the
# tone) instead of the microphone and "transcribes" MOCK_TEXT. Any further
# arguments are more flags for the app (e2e_rapid's --mock-text-sequence).
launch_full() {
  local wav=${1:-$VM_TONE}
  [ $# -eq 0 ] || shift
  launch --debug-audio-file "$wav" --mock-text "$MOCK_TEXT" "$@"
}

# --- TextEdit ---------------------------------------------------------------

# Runs AppleScript in the VM. ssh sessions are allowed to control TextEdit
# and System Events (one-time Automation grants in the golden image).
vm_osascript() {
  printf '%s\n' "$1" | vm "osascript -"
}

# An empty untitled TextEdit document in front, so a paste lands in it.
textedit_fresh() {
  vm_osascript '
    tell application "TextEdit"
      close every document saving no
      make new document
      activate
    end tell' >/dev/null
  sleep 1
}

textedit_text() {
  vm_osascript 'tell application "TextEdit" to get text of document 1'
}

# TextEdit's text with surrounding whitespace removed.
textedit_trimmed() {
  textedit_text | sed -e 's/^[[:space:]]*//' -e 's/[[:space:]]*$//'
}

# Closes TextEdit's documents without saving, so the next run starts clean.
textedit_close() {
  vm_osascript 'tell application "TextEdit" to close every document saving no' >/dev/null 2>&1 || true
}

# --- Clipboard ----------------------------------------------------------------

set_clipboard() {
  printf '%s' "$1" | vm "pbcopy"
}

clipboard() {
  vm "pbpaste"
}

# --- Dictating ----------------------------------------------------------------

# How many times the log has gone back to Idle.
idle_count() {
  logged_phases | grep -c '^Idle$' || true
}

# Waits up to `timeout` seconds until the log has gone back to Idle `count`
# times.
wait_for_idle_count() {
  local count=$1 timeout=${2:-30} i
  for i in $(seq 1 "$timeout"); do
    [ "$(idle_count)" -ge "$count" ] && return 0
    sleep 1
  done
  return 1
}

# One dictation with the synthetic --press and --release flags (the release
# `hold` seconds after the press); returns once the pipeline is back in Idle.
dictate_once() {
  local hold=${1:-1} before
  before=$(idle_count)
  launch --press
  sleep "$hold"
  launch --release
  wait_for_idle_count $((before + 1))
}

# The real hotkey is on: the log has no PermissionMissing.
hotkey_is_on() {
  ! app_log | grep -q 'PermissionMissing'
}

# --- Real key events ------------------------------------------------------------

# Runs a shell command line in Terminal.app, in the VM's GUI session, and
# waits for it (VM_TERMINAL_TIMEOUT seconds, default 30). ssh sessions have no
# Accessibility grant, so they can't post key events; Terminal has one in
# the golden image. Prints the command's output. A job that doesn't finish in
# time is killed, so it can't go on posting events once the caller has moved
# on (a killed hold job releases the key itself, see real_key_hold).
vm_terminal() {
  local n=$RANDOM timeout=${VM_TERMINAL_TIMEOUT:-30} i job
  job="$VM_RUN_DIR/job-$n"
  vm "mkdir -p $(q "$VM_RUN_DIR")"
  # A subshell, so that an exit in the command still reaches `touch .done`.
  printf '#!/bin/zsh\n( %s ) >%s.out 2>&1\ntouch %s.done\n' "$1" "$job" "$job" |
    vm "cat >$(q "$job.command") && chmod +x $(q "$job.command")"
  vm "open -a Terminal $(q "$job.command")"
  for i in $(seq 1 "$timeout"); do
    if vm "test -e $(q "$job.done")"; then
      vm "cat $(q "$job.out")"
      return 0
    fi
    sleep 1
  done
  log "  the Terminal script did not finish; cancelling it"
  # The bracket keeps pkill from matching the ssh command line that runs it.
  vm "pkill -f $(q "$VM_RUN_DIR/[j]ob-$n.command")" || true
  return 1
}

# Posts a real Right Option event (down or up) from Terminal; prints what
# the script said ("posted down", or why not). `state` prints whether the
# system thinks the key is held ("right option held" or "right option up").
real_key() {
  vm_terminal "osascript -l JavaScript $(q "$VM_RIGHT_OPTION") $1"
}

# Holds the real Right Option for `seconds`: down, wait, up, all in one
# script so the hold time is accurate. Running it in Terminal brings
# Terminal to the front, where the paste would land, so it first puts `app`
# (default TextEdit) back in front with `open -a`, which needs no
# Automation grant.
#
# The job releases the key itself if it ends any other way (an error, a
# signal, the caller's timeout killing it), whatever the caller does next.
real_key_hold() {
  local script app=${2:-TextEdit} cmd
  script=$(q "$VM_RIGHT_OPTION")
  cmd="open -a $(q "$app"); sleep 1; down=0"
  cmd="$cmd; release() { if [ \$down = 1 ]; then osascript -l JavaScript $script up; fi; }"
  cmd="$cmd; trap release EXIT; trap 'exit 1' INT TERM HUP"
  cmd="$cmd; down=1; osascript -l JavaScript $script down"
  if [ "${E2E_INJECT_FAILURE:-}" = mid_hold ]; then
    cmd="$cmd; echo 'injected failure while the key is down' >&2; exit 1"
  fi
  cmd="$cmd; sleep $1; osascript -l JavaScript $script up; down=0"
  vm_terminal "$cmd"
}

# Closes Terminal and removes its scripts and saved windows.
close_terminal() {
  vm "pkill -x Terminal; sleep 1; rm -rf $(q "$VM_RUN_DIR") $(q "$VM_HOME/Library/Saved Application State/com.apple.Terminal.savedState")" || true
}

# --- Cleanup for the scenarios that post real keys -------------------------------

# A scenario that posts a key-down arms this before its first event and ends
# with key_scenario_end. If it dies in between (a failed command under
# `set -e`, die, Ctrl-C, kill), the EXIT trap leaves the VM as if the
# scenario had ended well: Right Option up, the app quit, no Terminal job
# running, no document left open; and the app's log and a screenshot are
# saved for the failure. The exit status is the one the scenario died with.
KEY_SCENARIO=""

arm_key_cleanup() {
  KEY_SCENARIO=$1
  trap key_cleanup_on_exit EXIT
  trap 'exit 130' INT
  trap 'exit 143' TERM HUP
}

# Posts Right Option up whether or not it is down (an up with no down does
# nothing), after the app is gone so the app can't react to it.
key_cleanup_steps() {
  (
    set +e
    # Jobs still running in Terminal (the bracket: see vm_terminal).
    vm "pkill -f $(q "$VM_RUN_DIR/[j]ob-")" >/dev/null 2>&1
    fresh_app_state
    real_key up >/dev/null
    textedit_close
    close_terminal
  ) || true
}

key_cleanup_on_exit() {
  local status=$?
  trap - EXIT INT TERM HUP
  set +e
  if [ "$status" != 0 ]; then
    log "  $KEY_SCENARIO ended with status $status; releasing Right Option and cleaning up"
    collect_log "$KEY_SCENARIO"
  fi
  key_cleanup_steps
  exit "$status"
}

# The normal end of a scenario armed with arm_key_cleanup (after collect_log).
key_scenario_end() {
  trap - EXIT INT TERM HUP
  key_cleanup_steps
}

# For testing the cleanup: E2E_INJECT_FAILURE=<point> makes a scenario fail
# at that point, as a crash would. Points: after_key_down (right after a
# key-down is posted) and mid_hold (inside real_key_hold's job, between its
# down and up).
inject_failure() {
  if [ "${E2E_INJECT_FAILURE:-}" = "$1" ]; then
    die "injected failure at $1"
  fi
}
