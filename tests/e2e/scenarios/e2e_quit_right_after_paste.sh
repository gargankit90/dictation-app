# P1 round 4 (#241, Codex round 3): Quit right after a dictation still gives
# the user their clipboard back. The app restores the clipboard
# RESTORE_DELAY (1.5 s) after the chord on a timer; a Quit inside that window
# used to exit first and leave the transcript on the clipboard. Now the
# shutdown finishes the pending restore (at its time) before the app exits.
#
# A Terminal job is armed before the dictation: it waits for the app to log
# Inserting, then clicks Quit in the tray at once (fixtures/ax.js), so the
# Quit lands inside the 1.5 s. After the app is gone, the clipboard must
# hold the user's text again, and the log must say the restore ran in the
# quit path.

QRP_SENTINEL="e2e-quit-right-after-paste"

# wait_for_vm_file <path> [timeout]: true once the file exists in the VM.
wait_for_vm_file() {
  local i
  for i in $(seq 1 "${2:-20}"); do
    vm "test -e $(q "$1")" && return 0
    sleep 1
  done
  return 1
}

e2e_quit_right_after_paste() {
  local quitter text clip
  fresh_app_state
  launch_full
  expect "the app starts and logs ready" wait_for_log 'INFO .*ready'
  # The quitter: from Terminal (Accessibility is Terminal's), in the
  # background, so the dictation can go on. It writes a ready file just before
  # it starts watching, and the scenario waits for that file, so the dictation
  # never starts before the quitter is armed (a Terminal that is slow to start
  # a job used to be covered by a fixed 2 s sleep). Its output carries a
  # timestamp for armed, detected and clicked.
  local ready
  ready="$VM_RUN_DIR/qrp-quitter-ready"
  vm "mkdir -p $(q "$VM_RUN_DIR") && rm -f $(q "$ready")"
  VM_TERMINAL_TIMEOUT=60 vm_terminal "echo armed \$(date +%T); : >$(q "$ready"); for i in \$(seq 1 1000); do cat $(q "$VM_LOG_DIR")/$LOG_PREFIX.log* 2>/dev/null | grep -q phase=Inserting && break; sleep 0.02; done; echo detected \$(date +%T); osascript -l JavaScript $(q "$VM_AX") tray-click $(q "$APP_NAME") Quit; st=\$?; echo clicked \$(date +%T); exit \$st" \
    >"$OUT_DIR/e2e_quit_right_after_paste.quitter.txt" 2>&1 &
  quitter=$!
  expect "the quitter is armed before the dictation starts" wait_for_vm_file "$ready" 60
  textedit_fresh
  set_clipboard "$QRP_SENTINEL"
  launch --press
  sleep 1
  launch --release
  expect "the app quits right after the paste" wait_for_no_app 20
  expect "the quitter finished and clicked Quit" wait "$quitter"
  log "  the quitter said: $(paste -sd ' ' - <"$OUT_DIR/e2e_quit_right_after_paste.quitter.txt")"
  text=$(textedit_trimmed)
  log "  TextEdit holds: [$text]"
  expect "the transcript was pasted" test "$text" = "$MOCK_TEXT"
  clip=$(vm "pbpaste")
  log "  clipboard after the quit: [$clip]"
  expect "the user's clipboard is back after the quit" test "$clip" = "$QRP_SENTINEL"
  expect "the restore ran in the quit path" wait_for_log 'restored the clipboard before quitting' 2
  local gap
  gap=$(log_gap_ms 'phase=Inserting' 'restored the clipboard before quitting' 2>/dev/null || true)
  log "  Inserting to the quit-path restore: ${gap:-?} ms"
  collect_log e2e_quit_right_after_paste
  textedit_close
  fresh_app_state
}
