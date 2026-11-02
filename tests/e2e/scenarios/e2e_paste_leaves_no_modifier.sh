# M5.6 criterion 1, issue #140: the app's synthetic ⌘V (MacPasteKeys: V
# down and up with the Command flag, no Command key events) must not leave
# Command in the session's modifier state, or other apps would see Command
# held after a dictation: `NSEvent.modifierFlags`, `CGEventSourceFlagsState`,
# and key events made without a source (`CGEventCreateKeyboardEvent(NULL,
# …)`), which take their flags from that state. In M5.5a such an event typed
# ⌘H ⌘E ⌘L ⌘O for "hello" after earlier pastes.
#
# Reads, from Terminal (the GUI session; an ssh session has a session state
# of its own), CGEventSourceFlagsState for the combined session (0) and the
# HID system (1), and the flags of a fresh source-less key event, before and
# after a synthetic --press/--release dictation, then after a real Right
# Option hold. Each paste must land, and none of the three may carry the
# Command mask (0x100000) or a Command key's device bit (0x8 left, 0x10
# right) afterwards.

# The JXA that prints "combined=<n> hid=<n> sourceless=<n>" (decimal flags).
# Reading the state needs no Accessibility; creating an event without
# posting it neither.
PASTE_FLAGS_JXA='ObjC.import("ApplicationServices");
function run() {
  const event = $.CGEventCreateKeyboardEvent(null, 0, true);
  return "combined=" + $.CGEventSourceFlagsState(0) +
    " hid=" + $.CGEventSourceFlagsState(1) +
    " sourceless=" + $.CGEventGetFlags(event);
}'
# Command's mask and both Command keys' device bits.
PASTE_FLAGS_COMMAND=$((0x100000 | 0x8 | 0x10))

# Prints the three flag words, read in Terminal.
modifier_flags() {
  vm_terminal "osascript -l JavaScript $(q "$VM_RUN_DIR/modifier-flags.js")"
}

# no_command <label> <modifier_flags output>: true if none of the three
# words has a Command bit; logs the words either way.
no_command() {
  local label=$1 words=$2 word name value bad=0
  log "  modifier flags $label: [$words]"
  for name in combined hid sourceless; do
    word=$(printf '%s\n' "$words" | tr ' ' '\n' | sed -n "s/^$name=//p")
    case $word in
      '' | *[!0-9]*)
        log "  no number for $name"
        return 1
        ;;
    esac
    value=$((word & PASTE_FLAGS_COMMAND))
    if [ "$value" != 0 ]; then
      log "  $name carries Command bits $(printf '0x%x' "$value")"
      bad=1
    fi
  done
  [ "$bad" = 0 ]
}

e2e_paste_leaves_no_modifier() {
  arm_key_cleanup e2e_paste_leaves_no_modifier
  fresh_app_state
  printf '%s\n' "$PASTE_FLAGS_JXA" |
    vm "mkdir -p $(q "$VM_RUN_DIR") && cat >$(q "$VM_RUN_DIR/modifier-flags.js")"
  launch_full
  expect "the app starts and logs ready" wait_for_log 'INFO .*ready'
  expect "the real hotkey is on (Accessibility granted to the app)" hotkey_is_on

  local flags status
  flags=$(modifier_flags) || status=$?
  expect "before any paste, Command is not set" no_command "before" "$flags"

  textedit_fresh
  expect "the synthetic dictation gets back to Idle" dictate_once 1
  expect "its paste landed in TextEdit" test "$(textedit_trimmed)" = "$MOCK_TEXT"
  flags=$(modifier_flags) || status=$?
  expect "after the synthetic dictation's paste, Command is not set" \
    no_command "after the synthetic paste" "$flags"

  textedit_fresh
  local before
  before=$(idle_count)
  expect "Right Option goes down and up (the job exits 0)" job_says "posted down
posted up" real_key_hold 1.5
  expect "the hotkey dictation gets back to Idle" wait_for_idle_count $((before + 1))
  expect "its paste landed in TextEdit" test "$(textedit_trimmed)" = "$MOCK_TEXT"
  flags=$(modifier_flags) || status=$?
  expect "after the hotkey dictation's paste, Command is not set" \
    no_command "after the hotkey paste" "$flags"
  [ -z "${status:-}" ] || log "  a flags job exited with status $status"

  collect_log e2e_paste_leaves_no_modifier
  key_scenario_end
}
