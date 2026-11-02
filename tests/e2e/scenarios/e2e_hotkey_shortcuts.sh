# M5.9 criterion 2 (#172): with the hotkey on Right Command, the user's own
# shortcuts on that key still work and dictate nothing. The owner's report:
# right ⌘+C and right ⌘+V "didn't work". What really happened: the copy
# worked, but holding right ⌘ for the shortcut (about 0.7 s) also started a
# dictation; its transcript was pasted over the word still selected.
#
# Here, in TextEdit with "alpha" selected: right ⌘+C (held about 0.7 s, as in
# the report) copies "alpha", and TextEdit still holds "alpha"; right ⌘+V at
# the end pastes the user's clipboard ("beta"), and nothing else; neither
# shortcut gets past Recording (no Transcribing, no Inserting), and the
# clipboard holds what the user put there. Then holding right ⌘ alone still
# dictates.
#
# The hotkey is set in the settings file before the launch (the app reads it
# at startup); KEY_CLEANUP_HOOK resets it. The shortcuts are real key events
# from fixtures/chord.js in Terminal (Accessibility is Terminal's).

VM_CHORD="$VM_FIXTURES/chord.js"

# settings_write_hotkey <spec>: a settings file with only that hotkey (the
# rest are defaults), for the next launch.
settings_write_hotkey() {
  printf '{"hotkey": "%s"}\n' "$1" |
    vm "mkdir -p $(q "$(dirname "$SETTINGS_FILE_VM")") && cat >$(q "$SETTINGS_FILE_VM")"
}

# textedit_set <text>: TextEdit's front document holds exactly <text>.
textedit_set() {
  vm_osascript "tell application \"TextEdit\" to set text of document 1 to \"$1\"" >/dev/null
}

# chord_in_textedit <chord.js args...>: TextEdit in front, then the chord,
# from one Terminal job; true if it posted.
chord_in_textedit() {
  local args=() arg
  for arg in "$@"; do args+=("$(q "$arg")"); done
  job_says "posted $1+$2" vm_terminal \
    "open -a TextEdit; sleep 1; osascript -l JavaScript $(q "$VM_CHORD") ${args[*]}"
}

# no_dictation_since <phase mark> <what>: since the mark, the pipeline never
# got past Recording (a dictation that started was dropped) and no error
# was shown. Waits 3 s first, so a late transcription would show.
no_dictation_since() {
  local phases
  sleep 3
  phases=$(phases_since "$1")
  log "  phases after $2: [${phases:-none}]"
  case " $phases " in
    *" Transcribing "* | *" Inserting "*)
      log "  $2 started a dictation"
      return 1
      ;;
  esac
  if app_log | grep -q 'shown to the user'; then
    log "  an error was shown: $(app_log | grep 'shown to the user' | tail -1)"
    return 1
  fi
}

e2e_hotkey_shortcuts() {
  local old="e2e-old-clipboard-$$" mine="beta" mark text
  arm_key_cleanup e2e_hotkey_shortcuts right-command left-command
  KEY_CLEANUP_HOOK=settings_reset
  fresh_app_state
  settings_write_hotkey RightCmd
  launch_full
  expect "the app starts on the Right Command hotkey" wait_for_log 'ready .*hotkey=RightCmd'
  expect "the real hotkey is on (Accessibility granted to the app)" hotkey_is_on
  textedit_fresh
  textedit_set alpha
  expect "left ⌘+A selects all of TextEdit's text" chord_in_textedit left-command a 0.1 0.1
  set_clipboard "$old"

  # Right ⌘+C, held as long as in the owner's report.
  mark=$(phase_mark)
  expect "right ⌘+C is posted" chord_in_textedit right-command c 0.5 0.2
  expect "right ⌘+C dictates nothing" no_dictation_since "$mark" "right ⌘+C"
  expect "right ⌘+C copied the selection" test "$(clipboard)" = alpha
  text=$(textedit_trimmed)
  log "  TextEdit holds: [$text]"
  expect "nothing was pasted over the selection" test "$text" = alpha

  # Right ⌘+V at the end of the text.
  set_clipboard "$mine"
  expect "the cursor goes to the end (left ⌘+A, then Right)" \
    job_says "pressed 1" vm_terminal \
    "open -a TextEdit; sleep 1; osascript -l JavaScript $(q "$VM_CHORD") left-command a 0.1 0.1 >/dev/null && osascript -l JavaScript $(q "$VM_FIXTURES/keys.js") right"
  mark=$(phase_mark)
  expect "right ⌘+V is posted" chord_in_textedit right-command v 0.5 0.2
  expect "right ⌘+V dictates nothing" no_dictation_since "$mark" "right ⌘+V"
  text=$(textedit_trimmed)
  log "  TextEdit holds: [$text]"
  expect "right ⌘+V pasted the user's clipboard, and only that" test "$text" = "alpha$mine"
  expect "the clipboard still holds the user's text" test "$(clipboard)" = "$mine"

  # Right ⌘ alone still dictates.
  local before
  before=$(idle_count)
  expect "a right ⌘ hold is posted" job_says "posted down
posted up" post_key_hold right-command 1.5
  expect "the right ⌘ hold dictates" wait_for_idle_count $((before + 1)) 15
  text=$(textedit_trimmed)
  log "  TextEdit holds: [$text]"
  case $text in
    "alpha$mine"*"$MOCK_TEXT") expect "the dictation was pasted after the text" true ;;
    *) expect "the dictation was pasted after the text" false ;;
  esac

  collect_log e2e_hotkey_shortcuts
  key_scenario_end
}
