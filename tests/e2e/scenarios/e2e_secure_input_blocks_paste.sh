# T2 criterion 7 (P2-05, M4.2 #1): with Secure Input on, a dictation pastes
# nothing, leaves the user's clipboard alone and says why. Secure Input
# comes from Terminal's Secure Keyboard Entry, as a user would have it: it is
# on while Terminal is the active app, so Terminal is where the paste would
# land. The target is a Terminal window running `cat`, with the terminal in
# non-canonical mode, which saves whatever is pasted into it to a file
# (secure_target_start); the file is the document the scenario reads back.
#
# First a control dictation with Secure Keyboard Entry off: its text must
# land in the file, which proves the file would show a paste. Then Terminal
# is restarted with Secure Keyboard Entry on (the
# com.apple.Terminal SecureKeyboardEntry preference, read when Terminal
# starts; if that doesn't turn Secure Input on, the Terminal menu item
# instead), Secure Input is checked to be on (ioreg's
# kCGSSessionSecureInputPID), and a second dictation must leave the file
# and the clipboard as they were and report "A password field is focused;
# dictation wasn't pasted.". Secure Keyboard Entry is turned off again
# however the scenario ends, and Secure Input must then be off.

# Set when the scenario starts (VM_RUN_DIR comes from lib_full.sh, which
# test-key-handling.sh doesn't load).
SECURE_TARGET=""
SECURE_STOP=""
SECURE_JOB_PID=""
SECURE_BLOCKED_ERROR="A password field is focused; dictation wasn't pasted."

# Starts the target: a Terminal job that saves what is pasted into its
# window to SECURE_TARGET until SECURE_STOP appears (at most 60 s). Runs in
# the background on the host; waits until `cat` is reading.
secure_target_start() {
  local target stop n=$1
  target=$(q "$SECURE_TARGET")
  stop=$(q "$SECURE_STOP")
  vm "mkdir -p $(q "$VM_RUN_DIR") && rm -f $target $stop"
  VM_TERMINAL_TIMEOUT=90 vm_terminal "stty -icanon -echo min 1 time 0 </dev/tty; cat -u </dev/tty >$target & c=\$!; i=0; while [ ! -e $stop ] && [ \$i -lt 600 ]; do sleep 0.1; i=\$((i + 1)); done; kill \$c; stty sane </dev/tty; echo stopped" \
    >"$OUT_DIR/e2e_secure_input_blocks_paste.target-$n.txt" 2>&1 &
  SECURE_JOB_PID=$!
  local i
  for i in $(seq 1 20); do
    if vm "test -e $target"; then
      log "  target $n is reading (Terminal in front)"
      return 0
    fi
    sleep 1
  done
  log "  target $n did not start"
  return 1
}

# Stops the target and waits for its job; true if the job ended well.
secure_target_stop() {
  local status=0
  vm "touch $(q "$SECURE_STOP")" || true
  [ -z "$SECURE_JOB_PID" ] || wait "$SECURE_JOB_PID" || status=$?
  SECURE_JOB_PID=""
  return "$status"
}

secure_target_text() {
  vm "cat $(q "$SECURE_TARGET") 2>/dev/null" || true
}

# wait_target_has <text> [seconds]: until the target file holds <text>.
wait_target_has() {
  local want=$1 timeout=${2:-10} i
  for i in $(seq 1 "$timeout"); do
    case $(secure_target_text) in *"$want"*) return 0 ;; esac
    sleep 1
  done
  log "  the target holds [$(secure_target_text)]"
  return 1
}

# Secure Keyboard Entry off, Terminal quit, and Secure Input off afterwards.
# Returns 1 if Secure Input is still on.
secure_input_cleanup() {
  [ -z "$SECURE_JOB_PID" ] || secure_target_stop || true
  # Terminal first: on quit it may write its own preferences back.
  close_terminal
  terminal_secure_entry false || true
  if ! wait_secure_input off 10; then
    log "  cleanup: Secure Input is still on in the VM (held by pid $(secure_input_pid))"
    return 1
  fi
}

secure_input_on_exit() {
  local status=$?
  trap - EXIT INT TERM HUP
  set +e
  if [ "$status" != 0 ]; then
    log "  e2e_secure_input_blocks_paste ended with status $status; turning Secure Keyboard Entry off"
    collect_log e2e_secure_input_blocks_paste
  fi
  if ! secure_input_cleanup; then
    log "  FAIL: the cleanup after e2e_secure_input_blocks_paste failed"
    [ "$status" != 0 ] || status=1
  fi
  fresh_app_state || true
  exit "$status"
}

e2e_secure_input_blocks_paste() {
  local old="e2e-secure-clipboard-$$" holder text
  SECURE_TARGET="$VM_RUN_DIR/secure-target.txt"
  SECURE_STOP="$VM_RUN_DIR/secure-target.stop"
  trap secure_input_on_exit EXIT
  trap 'exit 130' INT
  trap 'exit 143' TERM HUP
  fresh_app_state
  close_terminal
  terminal_secure_entry false
  expect "Secure Input is off to begin with" wait_secure_input off 10
  # One word each, so the file shows which dictation pasted.
  launch_full "$VM_TONE" --mock-text-sequence "controlpaste,blockedpaste"
  expect "the app starts and logs ready" wait_for_log 'INFO .*ready'

  # Control: Secure Keyboard Entry off, the paste lands in the target.
  expect "the target window is reading" secure_target_start 1
  expect "the control dictation gets back to Idle" dictate_once 1
  expect "the control paste landed in the target (so a paste would show)" \
    wait_target_has controlpaste 10
  expect "the control job ended well" secure_target_stop

  # Secure Keyboard Entry on, read when Terminal starts again.
  close_terminal
  terminal_secure_entry true
  expect "the target window is reading again" secure_target_start 2
  if ! wait_secure_input on 10; then
    log "  the preference didn't turn Secure Input on; using Terminal's menu item"
    ax_ok menu-click Terminal Terminal "Secure Keyboard Entry" || true
    # The menu job's window is in front now; a fresh target window goes on top.
    secure_target_stop || true
    secure_target_start 3 || true
  fi
  expect "Secure Input is on" wait_secure_input on 10
  holder=$(secure_input_pid)
  log "  Secure Input held by pid ${holder:-none}: $(vm "ps -o comm= -p ${holder:-1}" 2>/dev/null || true)"

  set_clipboard "$old"
  local before_errors
  before_errors=$(log_count 'shown to the user')
  expect "the dictation with Secure Input on gets back to Idle" dictate_once 1
  expect "Secure Input was still on after it" wait_secure_input on 1
  expect "the app says why nothing was pasted" \
    wait_for_log "shown to the user error=$SECURE_BLOCKED_ERROR" 5
  expect "one error for it" test "$(log_count 'shown to the user')" = $((before_errors + 1))
  expect "the user's clipboard is unchanged" test "$(clipboard)" = "$old"
  expect "the target job ended well" secure_target_stop
  text=$(secure_target_text)
  log "  the target holds: [$text]"
  expect "nothing was pasted into the target" \
    sh -c "! printf '%s' \"\$1\" | grep -q blockedpaste" _ "$text"
  local phases
  phases=$(logged_phases | paste -sd ' ' -)
  log "  phases logged: $phases"
  expect "both dictations went back to Idle" \
    test "$phases" = "Recording Transcribing Inserting Idle Recording Transcribing Inserting Idle"

  trap - EXIT INT TERM HUP
  expect "Secure Keyboard Entry is off again, and Secure Input with it" secure_input_cleanup
  [ "$SCENARIO_FAILED" = 0 ] || dump_ui e2e_secure_input_blocks_paste
  collect_log e2e_secure_input_blocks_paste
  fresh_app_state
}
