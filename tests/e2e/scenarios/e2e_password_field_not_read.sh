# M5.8 #2 (#135): a field that says it is a password field (AXSubrole
# AXSecureTextField) is never read for the smart leading space. The target
# is fixtures/password-field.js: a window whose one field holds "hello" with
# the cursor after it, run from Terminal (the GUI session). The app logs at
# debug (DICTATION_LOG=debug), so the log says why no space was added.
#
# First the control: a plain field (no subrole, AppKit's own answer). A
# dictation of "world" gives exactly "hello world", which proves the app
# reads this field and that the fixture would show a space. Then the same
# field answering AXSecureTextField: the dictation gives exactly
# "helloworld" (no space, the field wasn't read), the debug log says
# `no leading space error=a password field has focus` once, and the log
# never holds the field's text. Secure Input stays off throughout (the
# fixture doesn't turn it on), so it is the subrole that stops the read.

VM_PASSWORD_FIELD="$VM_FIXTURES/password-field.js"
PASSWORD_FIELD_NOT_READ='no leading space error=a password field has focus'
# This run's fixture directory; set when the scenario starts.
PASSWORD_DIR=""
PASSWORD_JOB_PID=""

# The first launch, logging at debug. `open --env` sets the app's
# environment for this launch only; later --press/--release launches are
# handed to this process.
launch_debug_log() {
  vm "open -g -n --env DICTATION_LOG=debug $(q "$VM_APP") --args $(q --transcriber mock --debug-audio-file "$VM_TONE" --mock-text world)"
}

# password_field_start <secure|plain>: the fixture in front, focused, the
# cursor after "hello". Runs as a Terminal job in the background on the
# host; waits (up to 20 s) for its `ready` file.
password_field_start() {
  local kind=$1 dir="$PASSWORD_DIR/$1" i
  vm "rm -rf $(q "$dir") && mkdir -p $(q "$dir")"
  VM_TERMINAL_TIMEOUT=150 vm_terminal "osascript -l JavaScript $(q "$VM_PASSWORD_FIELD" "$kind" "$dir")" \
    >"$OUT_DIR/e2e_password_field_not_read.$kind.txt" 2>&1 &
  PASSWORD_JOB_PID=$!
  for i in $(seq 1 40); do
    if vm "test -e $(q "$dir/ready")"; then
      log "  the $kind field is focused (pid $(vm "cat $(q "$dir/pid")"))"
      return 0
    fi
    sleep 0.5
  done
  log "  the $kind field didn't get focus: $(vm "cat $(q "$dir/text")" 2>&1)"
  return 1
}

# password_field_text <secure|plain>: what the field holds now.
password_field_text() {
  vm "cat $(q "$PASSWORD_DIR/$1/text")"
}

# password_field_is <secure|plain> <text> [seconds]: until the field holds
# exactly <text> (default 10 s).
password_field_is() {
  local got i
  for i in $(seq 1 $((${3:-10} * 2))); do
    got=$(password_field_text "$1" 2>/dev/null || true)
    [ "$got" = "$2" ] && return 0
    sleep 0.5
  done
  log "  the $1 field holds [$got], not [$2]"
  return 1
}

# password_field_stop <secure|plain>: asks the fixture to quit, then stops
# the pid it wrote (its own process only) if it hasn't, and waits for its
# Terminal job. True once that process is gone.
password_field_stop() {
  local dir="$PASSWORD_DIR/$1"
  vm "touch $(q "$dir/stop")" || true
  [ -z "$PASSWORD_JOB_PID" ] || wait "$PASSWORD_JOB_PID" || true
  PASSWORD_JOB_PID=""
  vm "p=\$(cat $(q "$dir/pid") 2>/dev/null) || exit 0; i=0; while kill -0 \$p 2>/dev/null && [ \$i -lt 20 ]; do sleep 0.25; i=\$((i + 1)); done; kill \$p 2>/dev/null; sleep 0.5; ! kill -0 \$p 2>/dev/null"
}

password_field_on_exit() {
  local status=$?
  trap - EXIT INT TERM HUP
  set +e
  if [ "$status" != 0 ]; then
    log "  e2e_password_field_not_read ended with status $status; stopping its fixture"
    collect_log e2e_password_field_not_read
  fi
  local kind
  for kind in plain secure; do
    password_field_stop "$kind" >/dev/null 2>&1
  done
  fresh_app_state || true
  close_terminal
  exit "$status"
}

e2e_password_field_not_read() {
  PASSWORD_DIR="$VM_RUN_DIR/password-field-$E2E_RUN_ID"
  trap password_field_on_exit EXIT
  trap 'exit 130' INT
  trap 'exit 143' TERM HUP
  fresh_app_state
  close_terminal
  expect "Secure Input is off to begin with" wait_secure_input off 10
  launch_debug_log
  expect "the app starts and logs ready" wait_for_log 'INFO .*ready'

  # Control: the plain field is read, and gets its space.
  expect "the plain field is focused after 'hello'" password_field_start plain
  expect "the dictation into the plain field gets back to Idle" dictate_once 1
  expect "the plain field holds exactly 'hello world' (it was read)" \
    password_field_is plain "hello world"
  # Nothing logs at debug before the first press, so this is checked here.
  expect "the app logs at debug" wait_for_log ' DEBUG ' 5
  expect "no password field was reported for it" test "$(log_count "$PASSWORD_FIELD_NOT_READ")" = 0
  expect "the plain fixture stopped" password_field_stop plain

  # The same field, saying it is a password field.
  expect "the password field is focused after 'hello'" password_field_start secure
  expect "Secure Input is still off (only the subrole says password)" wait_secure_input off 1
  expect "the dictation into the password field gets back to Idle" dictate_once 1
  expect "the password field holds exactly 'helloworld' (no space added)" \
    password_field_is secure "helloworld"
  expect "the debug log says the field wasn't read, once" \
    test "$(log_count "$PASSWORD_FIELD_NOT_READ")" = 1
  expect "the password fixture stopped" password_field_stop secure

  expect "the fields' text is not in the log" \
    sh -c "! printf '%s' \"\$1\" | grep -qi 'hello'" _ "$(app_log)"
  local phases
  phases=$(logged_phases | paste -sd ' ' -)
  log "  phases logged: $phases"
  expect "both dictations pasted" \
    test "$phases" = "Recording Transcribing Inserting Idle Recording Transcribing Inserting Idle"

  trap - EXIT INT TERM HUP
  collect_log e2e_password_field_not_read
  fresh_app_state
  close_terminal
}
