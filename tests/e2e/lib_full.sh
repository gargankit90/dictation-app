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
# tone) instead of the microphone and "transcribes" MOCK_TEXT.
launch_full() {
  launch --debug-audio-file "${1:-$VM_TONE}" --mock-text "$MOCK_TEXT"
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
# waits for it. ssh sessions have no Accessibility grant, so they can't post
# key events; Terminal has one in the golden image. Prints the command's
# output.
vm_terminal() {
  local job="$VM_RUN_DIR/job-$RANDOM" i
  vm "mkdir -p $(q "$VM_RUN_DIR")"
  printf '#!/bin/zsh\n{ %s; } >%s.out 2>&1\ntouch %s.done\n' "$1" "$job" "$job" |
    vm "cat >$(q "$job.command") && chmod +x $(q "$job.command")"
  vm "open -a Terminal $(q "$job.command")"
  for i in $(seq 1 30); do
    if vm "test -e $(q "$job.done")"; then
      vm "cat $(q "$job.out")"
      return 0
    fi
    sleep 1
  done
  log "  the Terminal script did not finish"
  return 1
}

# Posts a real Right Option event (down or up) from Terminal; prints what
# the script said ("posted down", or why not).
real_key() {
  vm_terminal "osascript -l JavaScript $(q "$VM_RIGHT_OPTION") $1"
}

# Holds the real Right Option for `seconds`: down, wait, up, all in one
# script so the hold time is accurate.
real_key_hold() {
  local script
  script=$(q "$VM_RIGHT_OPTION")
  vm_terminal "osascript -l JavaScript $script down; sleep $1; osascript -l JavaScript $script up"
}

# Closes Terminal and removes its scripts and saved windows.
close_terminal() {
  vm "pkill -x Terminal; sleep 1; rm -rf $(q "$VM_RUN_DIR") $(q "$VM_HOME/Library/Saved Application State/com.apple.Terminal.savedState")" || true
}
