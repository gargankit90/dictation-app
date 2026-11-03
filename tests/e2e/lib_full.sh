# Helpers for the full-app scenarios (M4.3): the real hotkey, the real paste
# into TextEdit, the real clipboard; only the audio (a WAV file) and the
# transcript (the mock transcriber) are stand-ins. Sourced by run.sh after
# lib.sh, so it can use lib.sh's helpers.

# What the mock transcriber answers (--mock-text on the first launch).
MOCK_TEXT="hello world"
VM_SILENCE="$VM_FIXTURES/silence-16k.wav"
VM_RIGHT_OPTION="$VM_FIXTURES/right-option.js"
VM_POST_KEY="$VM_FIXTURES/post-key.js"
VM_AX="$VM_FIXTURES/ax.js"
VM_CLIPBOARD_RACE="$VM_FIXTURES/clipboard-race.js"
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
# An empty untitled TextEdit document in front, so a paste lands in it. Then
# checks (up to 10 s) that TextEdit really is the frontmost app: a paste goes
# to whatever is in front, and a Terminal job may still be activating.
textedit_fresh() {
  local i
  vm_osascript '
    tell application "TextEdit"
      close every document saving no
      make new document
      activate
    end tell' >/dev/null
  sleep 1
  for i in $(seq 1 20); do
    if vm_osascript 'tell application "System Events" to get name of first process whose frontmost is true' |
      grep -qx TextEdit; then
      # Once more now that it is in front: after the app quits from its
      # tray menu, the first `activate` makes TextEdit frontmost without
      # making its window key, and a paste goes nowhere. A second one, sent
      # once TextEdit is frontmost, makes the window key. (T2 VM probe:
      # after a tray Quit, 17 of 17 pastes missed without it, including 3
      # with no copy of the app running at activation and 3 with 3 s more
      # wait; 6 of 6 landed with it. With no tray Quit, 8 of 8 landed.)
      vm_osascript 'tell application "TextEdit" to activate' >/dev/null
      sleep 1
      return 0
    fi
    vm_osascript 'tell application "TextEdit" to activate' >/dev/null
    sleep 0.5
  done
  log "  TextEdit didn't come to the front (frontmost: $(frontmost_app))"
  return 1
}

# The name of the frontmost app, for the log when a paste check fails (#203).
frontmost_app() {
  vm_osascript 'tell application "System Events" to get name of first process whose frontmost is true' 2>/dev/null ||
    echo "unknown"
}

# textedit_ready [seconds]: true once TextEdit is frontmost with an empty,
# readable document (default 10 s). Else logs the frontmost app and fails, so
# a paste that goes nowhere can be told from one that was never aimed right.
textedit_ready() {
  local timeout=${1:-10} i
  for i in $(seq 1 "$timeout"); do
    if [ "$(frontmost_app)" = TextEdit ] && textedit_is_empty 2>/dev/null; then
      return 0
    fi
    sleep 1
  done
  log "  TextEdit is not frontmost with an empty document (frontmost: $(frontmost_app))"
  return 1
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

# A name for this run, part of every Terminal job's name, so a job's files
# can't be mistaken for those of a job from another run.
E2E_RUN_ID=${E2E_RUN_ID:-$(date +%Y%m%d%H%M%S)-$$}
# Numbers the Terminal jobs. A file, not a variable: vm_terminal mostly runs
# in a $(...), a subshell, where a variable's increment would be lost.
E2E_JOB_COUNTER=${E2E_JOB_COUNTER:-${TMPDIR:-/tmp}/e2e-jobs-$E2E_RUN_ID}

# The next job number (1, 2, ...).
next_job_number() {
  local n=0
  [ ! -s "$E2E_JOB_COUNTER" ] || n=$(cat "$E2E_JOB_COUNTER")
  n=$((n + 1))
  printf '%s\n' "$n" >"$E2E_JOB_COUNTER"
  printf '%s\n' "$n"
}

# Runs a shell command line in Terminal.app, in the VM's GUI session, and
# waits for it (VM_TERMINAL_TIMEOUT seconds, default 30). ssh sessions have no
# Accessibility grant, so they can't post key events; Terminal has one in
# the golden image. Prints the command's output and returns the command's
# exit status, so a guest failure fails the caller even when the output
# looks right.
#
# Each job has its own name (the run's id, a job number and the calling
# shell's pid, so jobs started at once from subshells differ too) and a
# random token. The job writes "<token> <status>" to its .done file through a
# temporary file and a rename, so the file is never half written; any earlier
# .done file under the name is removed before the job starts, and a .done
# file without this job's token is refused (125). A job that doesn't finish
# in time is killed, so it can't go on posting events once the caller has
# moved on (a killed hold job releases the key itself, see real_key_hold),
# and vm_terminal returns 124.
vm_terminal() {
  local timeout=${VM_TERMINAL_TIMEOUT:-30} i id job token answer status
  id="$E2E_RUN_ID-$(next_job_number)-$(sh -c 'echo $PPID')"
  job="$VM_RUN_DIR/job-$id"
  token="$id-$RANDOM$RANDOM"
  if ! vm "mkdir -p $(q "$VM_RUN_DIR") && rm -f $(q "$job.done" "$job.done.tmp" "$job.out")"; then
    log "  could not prepare Terminal job $id"
    return 125
  fi
  # A subshell, so that an exit in the command still reaches the status line.
  if ! {
    printf '#!/bin/zsh\n'
    printf '( %s ) >%s 2>&1\n' "$1" "$(q "$job.out")"
    printf 'st=$?\n'
    printf 'echo %s "$st" >%s && mv -f %s %s\n' "$(q "$token")" \
      "$(q "$job.done.tmp")" "$(q "$job.done.tmp")" "$(q "$job.done")"
  } | vm "cat >$(q "$job.command") && chmod +x $(q "$job.command")"; then
    log "  could not write Terminal job $id"
    return 125
  fi
  if ! vm "open -a Terminal $(q "$job.command")"; then
    log "  could not start Terminal job $id"
    return 125
  fi
  for i in $(seq 1 "$timeout"); do
    answer=$(vm "cat $(q "$job.done") 2>/dev/null" || true)
    if [ -n "$answer" ]; then
      case $answer in
        "$token "[0-9]*)
          status=${answer#"$token "}
          vm "cat $(q "$job.out")" || true
          return "$status"
          ;;
        *)
          log "  Terminal job $id: its done file says [$answer], not this job's token; refusing it"
          vm "pkill -f $(q "$VM_RUN_DIR/[j]ob-$id.command")" >/dev/null 2>&1 || true
          return 125
          ;;
      esac
    fi
    sleep 1
  done
  log "  Terminal job $id did not finish in ${timeout}s; cancelling it"
  # The bracket keeps pkill from matching the ssh command line that runs it.
  vm "pkill -f $(q "$VM_RUN_DIR/[j]ob-$id.command")" || true
  vm "cat $(q "$job.out") 2>/dev/null" || true
  return 124
}

# job_says <expected output> <command...>: runs a command that runs a
# Terminal job (real_key, post_key, ax, ...) and is true only if the job
# exited 0 and printed exactly <expected output>; says what went wrong
# otherwise. For checks that used to compare only the output.
job_says() {
  local want=$1 got status=0
  shift
  got=$("$@") || status=$?
  if [ "$status" != 0 ]; then
    log "  the Terminal job failed (exit status $status); it said: [$got]"
    return 1
  fi
  if [ "$got" != "$want" ]; then
    log "  the Terminal job said [$got], not [$want]"
    return 1
  fi
}

# Posts a real Right Option event (down or up) from Terminal; prints what
# the script said ("posted down", or why not). `state` prints whether the
# system thinks the key is held ("right option held" or "right option up").
real_key() {
  vm_terminal "osascript -l JavaScript $(q "$VM_RIGHT_OPTION") $1"
}

# post_key <key> <down|up|state>: like real_key for any modifier key
# (fixtures/post-key.js: left-option, right-command, ...). Prints
# "posted down" or "posted up", or for state "<key> held" or "<key> up".
post_key() {
  vm_terminal "osascript -l JavaScript $(q "$VM_POST_KEY") $(q "$1") $(q "$2")"
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
  key_hold_job "osascript -l JavaScript $(q "$VM_RIGHT_OPTION")" "$1" "${2:-TextEdit}"
}

# post_key_hold <key> <seconds> [app]: real_key_hold for any modifier key
# (fixtures/post-key.js). Prints "posted down" and "posted up".
post_key_hold() {
  key_hold_job "osascript -l JavaScript $(q "$VM_POST_KEY") $(q "$1")" "$2" "${3:-TextEdit}"
}

# key_hold_job <command that takes down|up> <seconds> <app>: the Terminal job
# behind real_key_hold and post_key_hold.
key_hold_job() {
  local key=$1 app=$3 cmd
  cmd="open -a $(q "$app"); sleep 1; down=0"
  cmd="$cmd; release() { if [ \$down = 1 ]; then $key up; fi; }"
  cmd="$cmd; trap release EXIT; trap 'exit 1' INT TERM HUP"
  # `|| exit`: a down that fails ends the job with its status (the trap
  # still posts the up).
  cmd="$cmd; down=1; $key down || exit"
  if [ "${E2E_INJECT_FAILURE:-}" = mid_hold ]; then
    cmd="$cmd; echo 'injected failure while the key is down' >&2; exit 1"
  fi
  # `wait`, not a plain sleep: the shell runs a trap only after the command
  # in front of it ends, but a signal ends `wait` at once.
  cmd="$cmd; sleep $2 & wait \$!; $key up || exit; down=0"
  vm_terminal "$cmd"
}

# --- The app's UI: tray, menus, windows (fixtures/ax.js) -------------------------

# ax <command> [args...]: runs fixtures/ax.js in Terminal (Accessibility is
# Terminal's; the ssh session has none), against the app unless the command
# names another (see ax.js for the commands). Prints what it printed; exits
# with its status.
ax() {
  local args=() arg
  for arg in "$@"; do args+=("$(q "$arg")"); done
  vm_terminal "osascript -l JavaScript $(q "$VM_AX") ${args[*]}"
}

# ax_ok <command> [args...]: runs ax and logs what it said; true only if it
# exited 0. For expect.
ax_ok() {
  local said status=0
  said=$(ax "$@") || status=$?
  log "  ax $1: ${said:-(nothing)}${status:+ (exit status $status)}"
  return "$status"
}

# The tray's status item: "count=", "title=", "help=" (tooltip) and
# "description=" lines.
tray_info() {
  ax tray-info "$APP_NAME"
}

# tray_field <name> <tray_info output>: one field's value.
tray_field() {
  printf '%s\n' "$2" | sed -n "s/^$1=//p"
}

# Saves the app's UI tree (and the tray's state) to
# $OUT_DIR/<name>.ax.txt, for a failure. Never fails.
dump_ui() {
  local file="$OUT_DIR/$1.ax.txt"
  {
    echo "--- tray"
    ax tray-info "$APP_NAME" 2>&1
    echo "--- windows"
    ax windows "$APP_NAME" 2>&1
    echo "--- tree"
    ax dump "$APP_NAME" 2>&1
  } >"$file" || true
  log "  UI tree saved to $file"
}

# Quits the app from its tray menu, the way a user does, and waits up to
# QUIT_TIMEOUT seconds (default 15) for every copy to exit by itself. If
# one is still running then, that is a failed Quit: it says so, saves the
# log, kills it so the next step starts clean, and returns 1, so the
# scenario fails however the kill went. Only after it worked does the
# scenario go on.
quit_from_tray() {
  local said status=0
  said=$(ax tray-click "$APP_NAME" "Quit") || status=$?
  log "  tray: $said"
  if [ "$status" != 0 ]; then
    log "  could not click Quit in the tray menu (exit status $status)"
  fi
  if wait_for_no_app "${QUIT_TIMEOUT:-15}"; then
    [ "$status" = 0 ] || return 1
    return 0
  fi
  log "  the app was still running ${QUIT_TIMEOUT:-15}s after Quit; killing it (the scenario fails)"
  collect_log "quit-failed-$(date +%H%M%S)"
  fresh_app_state || true
  return 1
}

# --- The overlay's rendered text (T4b, #119) -----------------------------------
#
# What the overlay window really shows, read from its WebView's accessibility
# tree (ax.js overlay): "hidden" when the panel isn't on screen, otherwise
# "shown: " and the text WebKit laid out ("shown: " alone for a blank page).
# The log's `overlay shown state=…` only says what the app sent.

OVERLAY_HIDDEN=hidden
OVERLAY_RECORDING="shown: Recording"
OVERLAY_TRANSCRIBING="shown: Transcribing…"

# overlay_shows <want> [seconds]: until the overlay shows exactly <want>,
# within [seconds] (default 5); says what it showed instead.
overlay_shows() {
  job_says "$1" ax overlay-wait "$APP_NAME" "$1" "${2:-5}"
}

# overlay_watch <seconds>: what the overlay showed over the next <seconds>,
# one "<ms> <state>" line at the start and at each change.
overlay_watch() {
  ax overlay-watch "$APP_NAME" "$1"
}

# log_gap_ms <from regex> <to regex>: the milliseconds between the last log
# line matching <from> and the first line after it matching <to>; fails if
# either is missing.
log_gap_ms() {
  app_log | awk -v from="$1" -v to="$2" '
    function secs(line,   t, h, m, s) {
      t = substr(line, index(line, "T") + 1)
      h = substr(t, 1, 2); m = substr(t, 4, 2); s = substr(t, 7, 9)
      return h * 3600 + m * 60 + s
    }
    $0 ~ from { start = secs($0); end = ""; next }
    start != "" && end == "" && $0 ~ to { end = secs($0) }
    END {
      if (start == "" || end == "") exit 1
      gap = end - start
      if (gap < 0) gap += 86400
      printf "%d\n", gap * 1000 + 0.5
    }'
}

# --- TextEdit and processes, small predicates for expect -----------------------

# TextEdit's document is empty (blank counts). A document that can't be read
# is not empty: a read that fails (the VM's deadline, an AppleScript timeout,
# TextEdit busy in a modal loop) prints nothing, and nothing must not pass
# for an empty document (#153).
textedit_is_empty() {
  local text
  if ! text=$(textedit_text); then
    log "  could not read TextEdit's text"
    return 1
  fi
  [ -z "$(printf '%s' "$text" | tr -d '[:space:]')" ]
}

# app_count_is <n> [seconds]: true once exactly <n> copies run, within
# [seconds] (default 1: now).
app_count_is() {
  local want=$1 timeout=${2:-1} i
  for i in $(seq 1 "$timeout"); do
    [ "$(app_count)" = "$want" ] && return 0
    [ "$i" = "$timeout" ] || sleep 1
  done
  log "  $(app_count) copies of the app are running, not $want"
  return 1
}

# log_count <extended regex>: how many lines of the app's log match.
log_count() {
  app_log | grep -Ec "$1" || true
}

# --- Secure Input (Terminal's Secure Keyboard Entry) ------------------------------

# The pid holding Secure Input, or nothing when it is off: the console
# session's kCGSSessionSecureInputPID (on the registry's root, so -d 1),
# which ssh can read. Absent or 0 means off.
secure_input_pid() {
  vm "ioreg -l -w 0 -d 1 | grep -o '\"kCGSSessionSecureInputPID\"=[0-9]*' || true" |
    sed -nE 's/.*=([1-9][0-9]*)$/\1/p' | head -n 1
}

# wait_secure_input <on|off> [seconds]: until Secure Input is on (held by
# some process) or off, within [seconds] (default 10).
wait_secure_input() {
  local want=$1 timeout=${2:-10} i pid
  for i in $(seq 1 "$timeout"); do
    pid=$(secure_input_pid)
    if [ "$want" = on ] && [ -n "$pid" ]; then return 0; fi
    if [ "$want" = off ] && [ -z "$pid" ]; then return 0; fi
    sleep 1
  done
  log "  Secure Input is not $want (holder: ${pid:-none})"
  return 1
}

# Terminal's Secure Keyboard Entry preference, read at Terminal's start.
terminal_secure_entry() {
  vm "defaults write com.apple.Terminal SecureKeyboardEntry -bool $1"
}

# Closes Terminal and removes its scripts and saved windows.
# Waits (up to 10 s) for Terminal to be gone, so a following `open -a
# Terminal` starts a new one, which reads its preferences again.
close_terminal() {
  vm "pkill -x Terminal; i=0; while pgrep -x Terminal >/dev/null && [ \$i -lt 20 ]; do sleep 0.5; i=\$((i + 1)); done; rm -rf $(q "$VM_RUN_DIR") $(q "$VM_HOME/Library/Saved Application State/com.apple.Terminal.savedState")" || true
}

# --- Cleanup for the scenarios that post real keys -------------------------------

# A scenario that posts a key-down arms this before its first event and ends
# with key_scenario_end. If it dies in between (a failed command under
# `set -e`, die, Ctrl-C, kill), the EXIT trap leaves the VM as if the
# scenario had ended well: Right Option (and any other key named when it was
# armed) up, the app quit, no Terminal job running, no document left open;
# and the app's log and a screenshot are saved for the failure. The exit
# status is the one the scenario died with. A cleanup that can't release a
# key or quit the app is reported and fails the scenario, however it ended;
# it never turns a failure into a pass, nor changes the status it died with.
KEY_SCENARIO=""
KEY_CLEANUP_KEYS=""
# A function the cleanup also runs once the app is gone (e2e_settings_save_reopen
# resets the settings with it); its failure fails the cleanup too.
KEY_CLEANUP_HOOK=""

# arm_key_cleanup <scenario> [key...]: keys are post-key.js names
# (left-option, ...) the scenario may hold besides Right Option.
arm_key_cleanup() {
  KEY_SCENARIO=$1
  shift
  KEY_CLEANUP_KEYS="$*"
  trap key_cleanup_on_exit EXIT
  trap 'exit 130' INT
  trap 'exit 143' TERM HUP
}

# Posts Right Option (and KEY_CLEANUP_KEYS) up whether or not it is down (an
# up with no down does nothing), after the app is gone so the app can't react
# to it, then checks that the system sees each key up. Returns 1, after
# saying what failed, if the app wouldn't quit or a key may still be held.
key_cleanup_steps() {
  local ok=0 key
  # Jobs still running in Terminal (the bracket: see vm_terminal).
  vm "pkill -f $(q "$VM_RUN_DIR/[j]ob-")" >/dev/null 2>&1 || true
  if ! fresh_app_state; then
    log "  cleanup: the app would not quit"
    ok=1
  fi
  if [ -n "$KEY_CLEANUP_HOOK" ] && ! "$KEY_CLEANUP_HOOK"; then
    log "  cleanup: $KEY_CLEANUP_HOOK failed"
    ok=1
  fi
  if ! job_says "posted up" real_key up || ! job_says "right option up" real_key state; then
    log "  cleanup: Right Option may still be held in the VM"
    ok=1
  fi
  for key in $KEY_CLEANUP_KEYS; do
    if ! job_says "posted up" post_key "$key" up || ! job_says "$key up" post_key "$key" state; then
      log "  cleanup: $key may still be held in the VM"
      ok=1
    fi
  done
  textedit_close
  close_terminal
  return "$ok"
}

key_cleanup_on_exit() {
  local status=$?
  trap - EXIT INT TERM HUP
  set +e
  if [ "$status" != 0 ]; then
    log "  $KEY_SCENARIO ended with status $status; releasing the keys and cleaning up"
    collect_log "$KEY_SCENARIO"
  fi
  if ! key_cleanup_steps; then
    log "  FAIL: the cleanup after $KEY_SCENARIO failed (see above)"
    [ "$status" != 0 ] || status=1
  fi
  exit "$status"
}

# The normal end of a scenario armed with arm_key_cleanup (after collect_log).
key_scenario_end() {
  trap - EXIT INT TERM HUP
  if ! key_cleanup_steps; then
    log "  FAIL: the cleanup after $KEY_SCENARIO failed (see above)"
    SCENARIO_FAILED=1
  fi
}

# For testing the cleanup: E2E_INJECT_FAILURE=<point> makes a scenario fail
# at that point, as a crash would. Points: after_key_down (right after a
# key-down is posted), mid_hold (inside real_key_hold's job, between its
# down and up) and after_mic_denied (e2e_microphone_denied_then_recover,
# right after the microphone was refused).
inject_failure() {
  if [ "${E2E_INJECT_FAILURE:-}" = "$1" ]; then
    die "injected failure at $1"
  fi
}
