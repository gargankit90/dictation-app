# T2 criterion 7 (P2-05), reworked for P1 round 2 (#241): the user copies
# something while a dictation still owes them their clipboard. Since P1 the
# paste gives the clipboard back as soon as it sees the paste land at the
# target's caret (about 20 ms after the chord), so there is no window worth
# racing there; the window that matters is the unconfirmed paste's: with no
# readable caret the transcript stays on the clipboard and the user's
# clipboard goes back UNCONFIRMED_RESTORE (3 s) after the chord, in the
# background. A copy made in that window must win: it is what's on the
# clipboard at the end, not the old clipboard and not the transcript, and
# nothing writes after it (crates/platform/src/insert: restore_later,
# restore_snapshot_if_unchanged).
#
# The paste goes to Finder with no text field focused, so the app finds no
# caret and the paste is unconfirmed (Finder ignores a text paste). The copy
# is timed from inside the VM: fixtures/clipboard-race.js watches the
# clipboard for the app's dictation write (its private session type), waits
# COPY_DELAY seconds (1 s: well inside the 3 s), checks the dictation's item
# is still there (so no restore ran yet) and copies. It also reports the
# clipboard's change count after its copy, so the scenario can tell that
# nothing wrote to the clipboard afterwards, the late restore included.

USER_COPY_DELAY=${E2E_USER_COPY_DELAY:-1}

e2e_user_copy_wins() {
  local old="e2e-old-clipboard-$$" mine="e2e-user-copy-$$"
  local race_out="$OUT_DIR/e2e_user_copy_wins.race.txt" race status=0 copied
  fresh_app_state
  launch_full
  expect "the app starts and logs ready" wait_for_log 'INFO .*ready'
  vm "open -a Finder" >/dev/null
  expect "Finder is in front" wait_for_front Finder
  set_clipboard "$old"
  expect "the old text is on the clipboard before" test "$(clipboard)" = "$old"

  # The user, watching the clipboard.
  vm "osascript -l JavaScript $(q "$VM_CLIPBOARD_RACE" copy-during-paste "$mine" 30 "$USER_COPY_DELAY")" \
    >"$race_out" 2>&1 &
  race=$!
  expect "the copier is watching the clipboard" wait_for_file_line "$race_out" armed 15

  expect "the dictation gets back to Idle" dictate_once 1
  expect "the app couldn't confirm the paste (no caret in Finder)" \
    wait_for_log 'no readable caret' 5
  wait "$race" || status=$?
  log "  the copier said: $(paste -sd ' ' - <"$race_out")"
  expect "the copier saw the dictation's write and copied (exit status $status)" test "$status" = 0
  expect "the copy landed while the user's clipboard was still owed, before any restore" \
    grep -qx 'pending=yes' "$race_out"
  copied=$(sed -n 's/^copied_count=//p' "$race_out")

  # Past the late restore (3 s after the chord).
  sleep 4
  local now count
  now=$(clipboard)
  log "  clipboard at the end: [$now]"
  expect "the user's copy is on the clipboard at the end" test "$now" = "$mine"
  expect "the old clipboard did not come back over it" test "$now" != "$old"
  expect "the transcript is not left on the clipboard" test "$now" != "$MOCK_TEXT"
  count=$(vm "osascript -l JavaScript $(q "$VM_CLIPBOARD_RACE" count)" || true)
  log "  change count after the copy: ${copied:-?}, now: ${count:-?}"
  expect "nothing wrote to the clipboard after the user's copy, the late restore included" \
    test -n "$copied" -a "$count" = "$copied"

  collect_log e2e_user_copy_wins
  fresh_app_state
}

# wait_for_front <app> [seconds]: until <app> is the frontmost app (default
# 10 s); logs the frontmost app if not.
wait_for_front() {
  local i
  for i in $(seq 1 "${2:-10}"); do
    [ "$(frontmost_app)" = "$1" ] && return 0
    sleep 1
  done
  log "  $1 is not in front (frontmost: $(frontmost_app))"
  return 1
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
