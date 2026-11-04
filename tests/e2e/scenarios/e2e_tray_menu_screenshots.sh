# U5 criterion 8 [manual] evidence: screenshots of the menu-bar icon and the
# open menu in the Ready and bad-key states, in a light and a dark menu bar.
# Nothing is asserted beyond the status line; the pictures go to the run's
# output for the PR. The appearance is put back to light at the end.

tms_appearance() {
  vm "osascript -e 'tell application \"System Events\" to tell appearance preferences to set dark mode to $1'"
}

# tms_shot <name>: the menu opened, photographed, closed.
tms_shot() {
  ax_ok tray-open "$APP_NAME" || return 1
  sleep 1
  screenshot "e2e_tray_menu_screenshots.$1"
  ax_ok tray-close "$APP_NAME"
}

tms_states() {
  local mode=$1
  expect "the menu bar is $mode" tms_appearance "$([ "$mode" = dark ] && echo true || echo false)"
  sleep 2
  # Ready: the mock engine.
  fresh_app_state
  launch_full
  expect "ready ($mode): the app starts" wait_for_log 'INFO .*ready'
  expect "ready ($mode): the status says ready" tray_is_clear
  screenshot "e2e_tray_menu_screenshots.icon-normal-$mode"
  expect "ready ($mode): the open menu" tms_shot "menu-ready-$mode"
  # Bad key: the real engine with no key.
  expect "the app quits" quit_from_tray
  groq_key_remove || true
  E2E_TRANSCRIBER=settings launch --debug-audio-file "$VM_TONE"
  expect "no-key ($mode): the app starts" wait_for_log 'INFO .*ready'
  expect "no-key ($mode): the status names the problem" tray_shows_no_key_error 15
  screenshot "e2e_tray_menu_screenshots.icon-attention-$mode"
  expect "no-key ($mode): the open menu" tms_shot "menu-bad-key-$mode"
  # Recording: hold a recording open for the icon.
  launch --press
  expect "recording ($mode)" wait_for_log 'state changed phase=Recording' 10
  screenshot "e2e_tray_menu_screenshots.icon-recording-$mode"
  launch --release
  sleep 2
  expect "the app quits" quit_from_tray
}

# Copy last dictation: disabled before any dictation, enabled after one, copies
# exactly the dictated text, and disabled again after a relaunch (memory only).
tms_copy_last() {
  fresh_app_state
  launch_full
  expect "copy-last: the app starts" wait_for_log 'INFO .*ready'
  expect "copy-last: disabled before a dictation" \
    sh -c "printf '%s' \"\$1\" | grep -qF '(off) Copy last dictation'" _ "$(tray_menu)"
  textedit_fresh
  expect "copy-last: a dictation ends Idle" dictate_once 1
  expect "copy-last: it pasted" test "$(textedit_trimmed)" = "$MOCK_TEXT"
  sleep 2
  expect "copy-last: enabled after a dictation" \
    sh -c "printf '%s' \"\$1\" | grep -qE '^Copy last dictation\$'" _ "$(tray_menu)"
  set_clipboard "something else"
  expect "copy-last: pressed in the tray" ax_ok tray-click "$APP_NAME" "Copy last dictation"
  sleep 1
  expect "copy-last: the clipboard holds exactly the dictation" test "$(clipboard)" = "$MOCK_TEXT"
  expect "copy-last: the log never holds the text" \
    sh -c "! printf '%s' \"\$1\" | grep -qF \"\$2\"" _ "$(app_log)" "$MOCK_TEXT"
  textedit_close
  expect "the app quits" quit_from_tray
  launch_full
  expect "copy-last: the app is back" wait_for_log 'INFO .*ready'
  expect "copy-last: disabled again after a relaunch" \
    sh -c "printf '%s' \"\$1\" | grep -qF '(off) Copy last dictation'" _ "$(tray_menu)"
  expect "the app quits" quit_from_tray
}

e2e_tray_menu_screenshots() {
  tms_copy_last
  tms_states light
  tms_states dark
  tms_appearance false || true
  collect_log e2e_tray_menu_screenshots
  close_terminal
  fresh_app_state
}
