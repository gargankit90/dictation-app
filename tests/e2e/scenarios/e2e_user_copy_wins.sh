# T2 criterion 7 (P2-05): the user copies something while a dictation's
# paste is pending. The paste borrows the clipboard (it writes the
# transcript, sends Cmd-V, and 1.5 s later, on a timer, puts the old
# clipboard back: RESTORE_DELAY since P1),
# and the restore must not undo a copy made in that window: the user's newer
# copy is what's on the clipboard at the end, not the old clipboard and not
# the transcript (crates/platform/src/insert: restore_snapshot_if_unchanged).
#
# The copy is timed from inside the VM: fixtures/clipboard-race.js watches
# the clipboard for the app's dictation write (its private session type),
# waits COPY_DELAY seconds (after the Cmd-V, well inside the 1.5 s), checks
# the dictation's item is still there (so the restore hasn't run yet) and
# copies. It also reports the clipboard's change count after its copy, so
# the scenario can tell that nothing wrote to the clipboard afterwards.

USER_COPY_DELAY=${E2E_USER_COPY_DELAY:-0.5}

e2e_user_copy_wins() {
  local old="e2e-old-clipboard-$$" mine="e2e-user-copy-$$"
  local race_out="$OUT_DIR/e2e_user_copy_wins.race.txt" race status=0 copied
  fresh_app_state
  launch_full
  expect "the app starts and logs ready" wait_for_log 'INFO .*ready'
  textedit_fresh
  set_clipboard "$old"
  expect "the old text is on the clipboard before" test "$(clipboard)" = "$old"

  # The user, watching the clipboard.
  vm "osascript -l JavaScript $(q "$VM_CLIPBOARD_RACE" copy-during-paste "$mine" 30 "$USER_COPY_DELAY")" \
    >"$race_out" 2>&1 &
  race=$!
  expect "the copier is watching the clipboard" wait_for_file_line "$race_out" armed 15

  expect "the dictation gets back to Idle" dictate_once 1
  wait "$race" || status=$?
  log "  the copier said: $(paste -sd ' ' - <"$race_out")"
  expect "the copier saw the dictation's write and copied (exit status $status)" test "$status" = 0
  expect "the copy landed while the paste was pending, before any restore" \
    grep -qx 'pending=yes' "$race_out"
  copied=$(sed -n 's/^copied_count=//p' "$race_out")

  local text now count
  text=$(textedit_trimmed)
  log "  TextEdit holds: [$text]"
  expect "the paste used the transcript, not the user's copy" test "$text" = "$MOCK_TEXT"
  # The restore (or not) runs 1.5 s after the chord, after Idle: wait past it.
  sleep 2
  now=$(clipboard)
  log "  clipboard at the end: [$now]"
  expect "the user's copy is on the clipboard at the end" test "$now" = "$mine"
  expect "the old clipboard did not come back over it" test "$now" != "$old"
  expect "the transcript is not left on the clipboard" test "$now" != "$MOCK_TEXT"
  count=$(vm "osascript -l JavaScript $(q "$VM_CLIPBOARD_RACE" count)" || true)
  log "  change count after the copy: ${copied:-?}, now: ${count:-?}"
  expect "nothing wrote to the clipboard after the user's copy" \
    test -n "$copied" -a "$count" = "$copied"

  collect_log e2e_user_copy_wins
  textedit_close
  fresh_app_state
}

# wait_for_file_line <host file> <line> [seconds]: until the file (written
# by a background command) has that exact line, within [seconds] (default 10).
wait_for_file_line() {
  local file=$1 line=$2 timeout=${3:-10} i
  for i in $(seq 1 "$timeout"); do
    grep -qx -- "$line" "$file" 2>/dev/null && return 0
    sleep 1
  done
  return 1
}
