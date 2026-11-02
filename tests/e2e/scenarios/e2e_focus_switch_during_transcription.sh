# T4a, #117: the user switches windows while a transcription is pending.
#
# The policy (docs/ENGINEERING.md, "Paste target"): the text goes to
# whatever has focus when it is pasted, not where the dictation started.
# Two TextEdit documents: A holds text that must stay byte for byte, B is
# empty. A dictation starts with A in front; while the provider holds its
# reply, B comes to the front; the reply is released: B gets exactly one
# transcript and A is unchanged, and the app's own windows (the overlay,
# Settings) never took focus. Then the same with a detour: while the next
# reply is held, Settings is opened from the tray (it takes focus) and
# closed, and B is put back in front before the release: B gets the second
# transcript after the first (with the smart leading space), A is still
# unchanged and Settings stays closed.

# What A holds, written as for esc_b64.
FOCUS_A_TEXT='Document A stays exactly as it is.\nSecond line of A.\n'

# focus_two_documents: closes TextEdit's documents, then A (with
# FOCUS_A_TEXT) and an empty B; sets FOCUS_DOC_A and FOCUS_DOC_B to their
# names.
focus_two_documents() {
  local names
  names=$(vm_osascript '
    tell application "TextEdit"
      close every document saving no
      set a to make new document
      set b to make new document
      return (name of a) & linefeed & (name of b)
    end tell') || return 1
  FOCUS_DOC_A=$(printf '%s\n' "$names" | sed -n 1p)
  FOCUS_DOC_B=$(printf '%s\n' "$names" | sed -n 2p)
  log "  documents: A is [$FOCUS_DOC_A], B is [$FOCUS_DOC_B]"
  [ -n "$FOCUS_DOC_A" ] && [ -n "$FOCUS_DOC_B" ] || return 1
  vm_jxa "ObjC.import('Foundation');
    const data = \$.NSData.alloc.initWithBase64EncodedStringOptions('$(esc_b64 "$FOCUS_A_TEXT")', 0);
    Application('TextEdit').documents.byName($(printf '%s' "$FOCUS_DOC_A" | jq -Rs .)).text =
      \$.NSString.alloc.initWithDataEncoding(data, \$.NSUTF8StringEncoding).js;
    'set'" >/dev/null
}

# The front TextEdit window's name.
front_document() {
  vm_osascript 'tell application "TextEdit" to get name of front window'
}

# focus_document <name>: brings that document's window to the front, with
# TextEdit the frontmost app (activated twice, as in textedit_fresh, so the
# window is key), and checks both (up to 10 s).
focus_document() {
  local i name
  name=$(printf '%s' "$1" | sed 's/["\\]/\\&/g')
  for i in $(seq 1 10); do
    vm_osascript "
      tell application \"TextEdit\"
        activate
        set index of window \"$name\" to 1
        activate
      end tell" >/dev/null || true
    sleep 1
    if [ "$(frontmost_app)" = TextEdit ] && [ "$(front_document)" = "$1" ]; then
      return 0
    fi
  done
  log "  [$1] didn't come to the front: the frontmost app is [$(frontmost_app)], TextEdit's front window [$(front_document)]"
  return 1
}

# A dictation with synthetic flags whose reply is held: returns once the
# provider has request <n> and holds it.
held_dictation() {
  provider_gate_close
  launch --press
  sleep 1
  launch --release
  provider_wait_event "^held n=$1"
}

# A is unchanged and B holds exactly <escaped text>.
documents_are() {
  textedit_is "$(esc_b64 "$FOCUS_A_TEXT")" "$FOCUS_DOC_A" &&
    textedit_is "$(esc_b64 "$1")" "$FOCUS_DOC_B"
}

e2e_focus_switch_during_transcription() {
  arm_key_cleanup e2e_focus_switch_during_transcription
  KEY_CLEANUP_HOOK=provider_cleanup
  fresh_app_state
  settings_reset
  expect "the local provider starts" provider_start "$PROVIDER_GOOD_KEY"
  provider_key_store "$PROVIDER_GOOD_KEY"
  expect "the app starts on the local provider" launch_on_provider
  set_clipboard "$PROVIDER_SENTINEL"
  expect "two documents, A with text and B empty" focus_two_documents
  expect "A is in front" focus_document "$FOCUS_DOC_A"

  # Switch while the reply is pending.
  expect "a dictation starts with A in front; its reply is held" held_dictation 1
  expect "A is still in front while it waits" test "$(front_document)" = "$FOCUS_DOC_A"
  expect "B comes to the front" focus_document "$FOCUS_DOC_B"
  expect "the reply is released" provider_release
  expect "the dictation ends Idle" wait_for_idle_count 1 15
  expect "it was pasted" test "$(phases_since 0)" = "Recording Transcribing Inserting Idle"
  expect "B got exactly one transcript, A is unchanged" documents_are "$PROVIDER_TEXT"
  expect "TextEdit is still in front (nothing of the app's took focus)" test "$(frontmost_app)" = TextEdit
  expect "with B" test "$(front_document)" = "$FOCUS_DOC_B"
  expect "the clipboard is the user's again" test "$(clipboard)" = "$PROVIDER_SENTINEL"

  # Settings opened and closed while the next reply is pending.
  expect "A is in front again" focus_document "$FOCUS_DOC_A"
  provider_reply "$(esc_b64 again)"
  expect "a second dictation starts; its reply is held" held_dictation 2
  expect "Settings… opens the window" open_settings
  expect "and it closes" close_settings
  expect "B is put in front" focus_document "$FOCUS_DOC_B"
  expect "the reply is released" provider_release
  expect "the second dictation ends Idle" wait_for_idle_count 2 15
  expect "B got the second transcript after the first, A is unchanged" \
    documents_are "$PROVIDER_TEXT again"
  expect "TextEdit is in front" test "$(frontmost_app)" = TextEdit
  expect "with B" test "$(front_document)" = "$FOCUS_DOC_B"
  # Last: ax runs in Terminal, which then has the front.
  expect "Settings stays closed" ax_ok wait-no-window "$APP_NAME" "$SETTINGS_WINDOW" 5
  expect "two requests in all" provider_requests_are 2
  expect "the clipboard is the user's again" test "$(clipboard)" = "$PROVIDER_SENTINEL"

  [ "$SCENARIO_FAILED" = 0 ] || dump_ui e2e_focus_switch_during_transcription
  collect_log e2e_focus_switch_during_transcription
  key_scenario_end
}
