# M5.5a criterion 4 (#83): the paste chord uses the key that types "v" in
# the selected keyboard layout (the real TIS/UCKeyTranslate lookup), on
# AZERTY (French), Dvorak, Russian, Dvorak Right-Handed and Turkish. For each,
# the input source is switched in the VM (fixtures/input-source.js, run in
# Terminal so the switch reaches the GUI session; enabled first if the clone
# doesn't have it), a dictation goes into TextEdit, and TextEdit must hold
# the transcript. The layout is checked again after the dictation, not between
# bringing TextEdit forward and dictating: a Terminal job would take the front
# (and the paste).
#
# Which layouts prove the lookup (#136): the key that types V under ⌘ (what
# the app's UCKeyTranslate lookup returns; read on macOS 15 from every
# installed layout) is key 9, QWERTY's V, on AZERTY and Russian, so they
# prove no regression, not the lookup. It is 47 on Dvorak, 43 on Dvorak
# Right-Handed and 8 on Turkish: there a chord sent on QWERTY's key 9 is
# another shortcut or nothing, so the transcript lands only if the lookup
# works. (Colemak keeps V on key 9 under ⌘, and Workman is not installed.)
# The layout the VM started on is put back, and the sources the scenario
# enabled are disabled again, however the scenario ends.

VM_INPUT_SOURCE="$VM_FIXTURES/input-source.js"
LAYOUTS=(
  com.apple.keylayout.French
  com.apple.keylayout.Dvorak
  com.apple.keylayout.Russian
  com.apple.keylayout.Dvorak-Right
  com.apple.keylayout.Turkish-Standard
)
LAYOUT_ORIGINAL=""
LAYOUT_ADDED=()

input_source() {
  vm_terminal "osascript -l JavaScript $(q "$VM_INPUT_SOURCE" "$@") 2>&1"
}

# Selects `id`, remembering whether it had to be enabled; succeeds only if
# the VM says it is selected.
select_layout() {
  local said
  said=$(input_source select "$1") || return 1
  log "  $said"
  case $said in
    "selected $1 added") LAYOUT_ADDED+=("$1") ;;
    "selected $1") ;;
    *) return 1 ;;
  esac
}

layout_restore() {
  local id
  if [ -n "$LAYOUT_ORIGINAL" ]; then
    input_source select "$LAYOUT_ORIGINAL" >/dev/null || log "  could not select $LAYOUT_ORIGINAL again"
  fi
  for id in ${LAYOUT_ADDED[@]+"${LAYOUT_ADDED[@]}"}; do
    input_source disable "$id" >/dev/null || log "  could not disable $id"
  done
  LAYOUT_ADDED=()
}

layout_cleanup_on_exit() {
  local status=$?
  trap - EXIT INT TERM HUP
  set +e
  log "  e2e_layout_paste ended with status $status; restoring the layout"
  collect_log e2e_layout_paste
  layout_restore
  fresh_app_state
  textedit_close
  close_terminal
  exit "$status"
}

e2e_layout_paste() {
  trap layout_cleanup_on_exit EXIT
  trap 'exit 130' INT
  trap 'exit 143' TERM HUP
  fresh_app_state
  LAYOUT_ORIGINAL=$(input_source current) || LAYOUT_ORIGINAL=""
  log "  the VM starts on $LAYOUT_ORIGINAL"
  expect "the current layout is known" test -n "$LAYOUT_ORIGINAL"
  launch_full
  expect "the app starts and logs ready" wait_for_log 'INFO .*ready'

  local id text count=0 current
  for id in "${LAYOUTS[@]}"; do
    if ! select_layout "$id"; then
      expect "$id is selected" false
      continue
    fi
    textedit_fresh
    count=$((count + 1))
    expect "the dictation on $id gets back to Idle" dictate_once 1
    text=$(textedit_trimmed)
    log "  on $id, TextEdit holds: [$text]"
    expect "the paste on $id put the transcript in TextEdit" test "$text" = "$MOCK_TEXT"
    current=$(input_source current)
    expect "$id was still selected after the dictation" test "$current" = "$id"
  done
  expect "every layout was tried" test "$count" = "${#LAYOUTS[@]}"

  collect_log e2e_layout_paste
  trap - EXIT INT TERM HUP
  layout_restore
  current=$(input_source current)
  expect "the VM is back on $LAYOUT_ORIGINAL" test "$current" = "$LAYOUT_ORIGINAL"
  textedit_close
  close_terminal
}
