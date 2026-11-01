# M5.1 criterion 3: while the overlay shows, the paste still lands in the
# focused app. A dictation with the mock transcriber into TextEdit: the
# overlay must show (Recording, then Transcribing) and hide again, TextEdit
# must stay the frontmost app the whole time (sampled every 0.2 s from
# before the press until after Idle), and the text must land in TextEdit.
# The overlay's native window must still be the non-activating NSPanel once
# created (#102 review, M1): the app reads it back on the main thread and
# logs its class and flags.

# Where the frontmost-app watcher writes in the VM.
VM_FRONT_LOG="/Users/$VM_USER/e2e-front.txt"
VM_FRONT_STOP="/Users/$VM_USER/e2e-front.stop"

# The frontmost app's name, through LaunchServices (no Automation grant).
frontmost_app() {
  vm "lsappinfo info -only name \"\$(lsappinfo front)\"" | sed -nE 's/.*"LSDisplayName"="([^"]*)".*/\1/p'
}

# Samples the frontmost app every 0.2 s in the VM until front_watch_stop
# (or at most 60 s), one name per line, into $VM_FRONT_LOG.
front_watch_start() {
  vm "rm -f $(q "$VM_FRONT_LOG" "$VM_FRONT_STOP")"
  # shellcheck disable=SC2016 # expanded in the VM
  local loop='i=0; while [ ! -e "$1" ] && [ $i -lt 300 ]; do lsappinfo info -only name "$(lsappinfo front)"; sleep 0.2; i=$((i+1)); done'
  vm "nohup sh -c $(q "$loop") _ $(q "$VM_FRONT_STOP") >$(q "$VM_FRONT_LOG") 2>&1 </dev/null &"
}

# Stops the watcher and prints the names it saw, one per line.
front_watch_stop() {
  vm "touch $(q "$VM_FRONT_STOP"); sleep 0.5; rm -f $(q "$VM_FRONT_STOP")"
  vm "cat $(q "$VM_FRONT_LOG")" | sed -nE 's/.*"LSDisplayName"="([^"]*)".*/\1/p'
}

# The app's on-screen windows, one JSON object per line ({layer, width,
# height}), from CGWindowListCopyWindowInfo through JXA (works over ssh; window
# titles would need Screen Recording, so they aren't used). cua-driver's
# list_windows leaves out windows above the normal layer, such as the panel.
app_windows_on_screen() {
  # shellcheck disable=SC2016 # JavaScript, run in the VM
  printf '%s\n' \
    "ObjC.import('CoreGraphics');" \
    'const all = ObjC.deepUnwrap(ObjC.castRefToObject($.CGWindowListCopyWindowInfo($.kCGWindowListOptionOnScreenOnly, 0)));' \
    "all.filter(w => w.kCGWindowOwnerName === '$APP_NAME').forEach(w => console.log(JSON.stringify({layer: w.kCGWindowLayer, width: w.kCGWindowBounds.Width, height: w.kCGWindowBounds.Height})));" |
    vm "osascript -l JavaScript - 2>&1"
}

# The overlay panel (420x72 at the status level, 25) is on screen.
overlay_on_screen() {
  local windows
  windows=$(app_windows_on_screen) || return 1
  log "  app windows on screen: $(printf '%s' "$windows" | paste -sd ' ' -)"
  printf '%s\n' "$windows" | grep -q '"layer":25,"width":420,"height":72'
}

overlay_off_screen() {
  ! overlay_on_screen
}

e2e_overlay_focus() {
  fresh_app_state
  launch_full
  expect "the app starts and logs ready" wait_for_log 'INFO .*ready'
  log "  $(app_log | grep -m1 'overlay native window' || echo 'no overlay native window line')"
  expect "the overlay is still a non-activating panel after creation" \
    wait_for_log 'INFO .*overlay native window non_activating_panel=true .*is_panel=true can_become_key=false can_become_main=false non_activating=true backs_webview=true' 5
  textedit_fresh
  expect "TextEdit is in front before the dictation" test "$(frontmost_app)" = TextEdit

  front_watch_start
  local before
  before=$(idle_count)
  launch --press
  expect "the overlay shows Recording" wait_for_log 'overlay shown state="?recording' 10
  expect "the overlay window is on screen while recording" overlay_on_screen
  screenshot e2e_overlay_focus.recording
  sleep 1
  launch --release
  expect "the dictation gets back to Idle" wait_for_idle_count $((before + 1))
  expect "the overlay showed Transcribing" wait_for_log 'overlay shown state="?transcribing' 5
  expect "the overlay hides again" wait_for_log 'overlay hidden' 5
  sleep 1
  expect "the overlay window is off screen again" overlay_off_screen
  local fronts others
  fronts=$(front_watch_stop)
  others=$(printf '%s\n' "$fronts" | grep -vx TextEdit | sort -u | paste -sd ',' - || true)
  log "  frontmost samples: $(printf '%s\n' "$fronts" | grep -c . || true), not TextEdit: [${others}]"
  expect "the frontmost app was sampled" test -n "$fronts"
  expect "TextEdit stayed the frontmost app throughout" test -z "$others"

  local text
  text=$(textedit_trimmed)
  log "  TextEdit holds: [$text]"
  expect "the transcript landed in TextEdit" test "$text" = "$MOCK_TEXT"
  expect "the transcript is not in the log" \
    sh -c "! printf '%s' \"\$1\" | grep -q \"\$2\"" _ "$(app_log)" "$MOCK_TEXT"
  expect "one app is running" test "$(app_count)" = 1

  collect_log e2e_overlay_focus
  textedit_close
  vm "rm -f $(q "$VM_FRONT_LOG")" || true
}
