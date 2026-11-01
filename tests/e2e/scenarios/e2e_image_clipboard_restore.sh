# M5.5a criterion 5 (#83): an image on the clipboard survives a dictation.
# A PNG (fixtures/image-24.png) is put on the clipboard with
# fixtures/clipboard-image.js; after a dictation into TextEdit the clipboard
# holds the same PNG, byte for byte, and no text.

VM_CLIPBOARD_IMAGE="$VM_FIXTURES/clipboard-image.js"
VM_IMAGE="$VM_FIXTURES/image-24.png"

clipboard_image() {
  vm "osascript -l JavaScript $(q "$VM_CLIPBOARD_IMAGE" "$@") 2>&1"
}

# The SHA-256 of the PNG on the clipboard, or nothing if there is none.
clipboard_png_sha() {
  local out="$VM_HOME/e2e-clipboard.png"
  clipboard_image save "$out" >/dev/null &&
    vm "shasum -a 256 $(q "$out") | cut -d' ' -f1; rm -f $(q "$out")"
}

e2e_image_clipboard_restore() {
  local want said types
  fresh_app_state
  launch_full
  expect "the app starts and logs ready" wait_for_log 'INFO .*ready'
  textedit_fresh
  want=$(vm "shasum -a 256 $(q "$VM_IMAGE") | cut -d' ' -f1")
  said=$(clipboard_image set "$VM_IMAGE")
  log "  $said"
  expect "the PNG is on the clipboard before" test "$(clipboard_png_sha)" = "$want"

  expect "the dictation gets back to Idle" dictate_once 1
  expect "the transcript was pasted" test "$(textedit_trimmed)" = "$MOCK_TEXT"
  types=$(clipboard_image types)
  log "  clipboard types after: $types"
  expect "the same PNG is back on the clipboard" test "$(clipboard_png_sha)" = "$want"
  expect "no text is left on the clipboard" test -z "$(clipboard)"

  collect_log e2e_image_clipboard_restore
  textedit_close
}
