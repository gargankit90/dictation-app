# M5.5a criterion 3 (#93): the smart leading space in the real app. The app
# reads the character before the cursor in TextEdit over Accessibility.
# "hello" typed with real key events (fixtures/type-keys.js, run in
# Terminal, which has the Accessibility grant), then a dictation of "world"
# gives "hello world"; at the start of an empty document the same dictation
# gives "world", and after "hello " (a space already there) no second space
# is added.

VM_TYPE_KEYS="$VM_FIXTURES/type-keys.js"

# Types `text` (a-z and spaces) into TextEdit with real key events.
type_in_textedit() {
  vm_terminal "open -a TextEdit; sleep 1; osascript -l JavaScript $(q "$VM_TYPE_KEYS") $(q "$1")"
}

e2e_smart_space() {
  fresh_app_state
  launch --debug-audio-file "$VM_TONE" --mock-text world
  expect "the app starts and logs ready" wait_for_log 'INFO .*ready'

  # After a word: one space.
  textedit_fresh
  expect "hello is typed into TextEdit" test "$(type_in_textedit hello)" = "typed 5"
  expect "TextEdit holds hello" test "$(textedit_text)" = "hello"
  expect "the dictation after a word gets back to Idle" dictate_once 1
  local text
  text=$(textedit_text)
  log "  after a word, TextEdit holds: [$text]"
  expect "dictating world after hello gives 'hello world'" test "$text" = "hello world"

  # At the start of an empty document: no space.
  textedit_fresh
  expect "the dictation in an empty document gets back to Idle" dictate_once 1
  text=$(textedit_text)
  log "  in an empty document, TextEdit holds: [$text]"
  expect "dictating world in an empty document gives 'world'" test "$text" = "world"

  # After a space: no second space.
  textedit_fresh
  expect "'hello ' is typed into TextEdit" test "$(type_in_textedit 'hello ')" = "typed 6"
  expect "the dictation after a space gets back to Idle" dictate_once 1
  text=$(textedit_text)
  log "  after a space, TextEdit holds: [$text]"
  expect "dictating world after 'hello ' gives 'hello world'" test "$text" = "hello world"

  expect "the text read before the cursor is not in the log" \
    sh -c "! printf '%s' \"\$1\" | grep -qi 'hello'" _ "$(app_log)"

  collect_log e2e_smart_space
  textedit_close
  close_terminal
}
