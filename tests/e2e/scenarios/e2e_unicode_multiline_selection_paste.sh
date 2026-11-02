# T4a, #118: the paste keeps every character: emoji outside the BMP (sent by
# the provider as JSON surrogate pairs), flags and ZWJ sequences, combining
# marks, right-to-left text, tabs and newlines inside the transcript.
#
# Into existing text in TextEdit: once at a caret (the text before and after
# it must stay), once replacing a selected range (only the selection goes),
# then the caret case again in a fresh document, which a replayed or doubled
# paste would spoil. Each time the whole document is read back as base64 of
# its UTF-8 (textedit_b64), so no whitespace can hide, and compared with the
# exact expected text: the transcript trimmed of its surrounding whitespace
# and newlines and nothing else (crates/core/src/text.rs `clean`), its inner
# whitespace kept. Each caret follows a space, so the smart leading space
# (M5.5a, #93) adds nothing. The clipboard is the user's again after each.
#
# Texts are written as for esc_b64 (\n, \t, \u{hex}).

# What the provider sends: surrounding blanks and newlines, then an emoji,
# a decomposed é (e + U+0301), Hebrew, an inner blank line and an indented
# line.
UNICODE_RAW_1='  \n\u{1F600} smile, e\u{301}t\u{e9} combining, \u{5E9}\u{5DC}\u{5D5}\u{5DD} RTL,\nsecond line\n\n  indented line  \n'
UNICODE_CLEAN_1='\u{1F600} smile, e\u{301}t\u{e9} combining, \u{5E9}\u{5DC}\u{5D5}\u{5DD} RTL,\nsecond line\n\n  indented line'
# A flag (two regional indicators), a ZWJ sequence, Arabic and a tab.
UNICODE_RAW_2='\n\u{1F1FA}\u{1F1F3} flag, \u{1F469}\u{200D}\u{1F4BB} coder, \u{645}\u{631}\u{62D}\u{628}\u{627}\nsecond\tline \u{2014} done\n'
UNICODE_CLEAN_2='\u{1F1FA}\u{1F1F3} flag, \u{1F469}\u{200D}\u{1F4BB} coder, \u{645}\u{631}\u{62D}\u{628}\u{627}\nsecond\tline \u{2014} done'
# The document of the caret case: the caret goes after the prefix.
UNICODE_PREFIX_1='Line one stays.\nInsert here: '
UNICODE_SUFFIX_1='and the rest stays.\nLast line.\n'
# The selection case: the selection is SELECTED, after the prefix.
UNICODE_PREFIX_2='Keep this. '
UNICODE_SELECTED_2='REPLACE ME'
UNICODE_SUFFIX_2=' please keep this too.\nLast line stays.\n'

# unicode_doc <text> <caret: characters from the start> [selected
# characters]: a fresh document holding <text>, the caret (or a selection)
# placed with real keys: to the start, right, then shift+right.
unicode_doc() {
  local moves=(cmd+up) want
  textedit_fresh || return 1
  textedit_set "$(esc_b64 "$1")" || return 1
  [ "$2" = 0 ] || moves+=("right*$2")
  [ "${3:-0}" = 0 ] || moves+=("shift+right*$3")
  want="pressed $((1 + $2 + ${3:-0}))"
  job_says "$want" keys "${moves[@]}"
}

# unicode_dictate <raw text>: the provider answers <raw text>; one dictation
# back to Idle.
unicode_dictate() {
  provider_reply "$(esc_b64 "$1")"
  dictate_once 1
}

e2e_unicode_multiline_selection_paste() {
  local mark
  arm_key_cleanup e2e_unicode_multiline_selection_paste
  KEY_CLEANUP_HOOK=provider_cleanup
  fresh_app_state
  settings_reset
  expect "the local provider starts" provider_start "$PROVIDER_GOOD_KEY"
  provider_key_store "$PROVIDER_GOOD_KEY"
  expect "the app starts on the local provider" launch_on_provider
  set_clipboard "$PROVIDER_SENTINEL"

  # At a caret.
  expect "a document with the caret inside its text" \
    unicode_doc "$UNICODE_PREFIX_1$UNICODE_SUFFIX_1" "$(esc_chars "$UNICODE_PREFIX_1")"
  mark=$(phase_mark)
  expect "a dictation of the first text ends Idle" unicode_dictate "$UNICODE_RAW_1"
  expect "it was pasted" test "$(phases_since "$mark")" = "Recording Transcribing Inserting Idle"
  expect "the text went in at the caret, exactly, and the rest stayed" \
    textedit_is "$(esc_b64 "$UNICODE_PREFIX_1$UNICODE_CLEAN_1$UNICODE_SUFFIX_1")"
  expect "the clipboard is the user's again" test "$(clipboard)" = "$PROVIDER_SENTINEL"

  # Over a selection.
  expect "a document with a range selected" \
    unicode_doc "$UNICODE_PREFIX_2$UNICODE_SELECTED_2$UNICODE_SUFFIX_2" \
    "$(esc_chars "$UNICODE_PREFIX_2")" "$(esc_chars "$UNICODE_SELECTED_2")"
  mark=$(phase_mark)
  expect "a dictation of the second text ends Idle" unicode_dictate "$UNICODE_RAW_2"
  expect "it was pasted" test "$(phases_since "$mark")" = "Recording Transcribing Inserting Idle"
  expect "it replaced the selection only, exactly" \
    textedit_is "$(esc_b64 "$UNICODE_PREFIX_2$UNICODE_CLEAN_2$UNICODE_SUFFIX_2")"
  expect "the clipboard is the user's again" test "$(clipboard)" = "$PROVIDER_SENTINEL"

  # Once more, in a fresh document: one paste, not a replay of the last two.
  expect "a fresh document with the caret inside its text" \
    unicode_doc "$UNICODE_PREFIX_1$UNICODE_SUFFIX_1" "$(esc_chars "$UNICODE_PREFIX_1")"
  expect "the first text again ends Idle" unicode_dictate "$UNICODE_RAW_1"
  expect "it went in exactly once" \
    textedit_is "$(esc_b64 "$UNICODE_PREFIX_1$UNICODE_CLEAN_1$UNICODE_SUFFIX_1")"
  expect "the clipboard is the user's again" test "$(clipboard)" = "$PROVIDER_SENTINEL"
  expect "three requests in all" provider_requests_are 3

  [ "$SCENARIO_FAILED" = 0 ] || dump_ui e2e_unicode_multiline_selection_paste
  collect_log e2e_unicode_multiline_selection_paste
  key_scenario_end
}
