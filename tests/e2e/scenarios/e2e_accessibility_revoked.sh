# M5.9 criterion 6 (#173): turning Accessibility off while the app runs
# must never freeze the Mac. On the owner's Mac it did: with the app's
# hotkey listening, switching dictation-app off in System Settings ›
# Privacy & Security › Accessibility stopped all keyboard input and the Mac
# had to be forced off. In the VM, before the fix, typed keys stopped
# reaching TextEdit the moment Accessibility was revoked, and the app never
# noticed.
#
# Only in the test VM's clone (dictation-vm or dictation-vm-2), never the
# golden image:
#
# 1. The app runs with its real hotkey; typing in TextEdit and a mouse move
#    work.
# 2. The app's switch in System Settings › Privacy & Security ›
#    Accessibility is turned off the way the owner did it. macOS asks for
#    the user's password in a sheet that refuses software key events, so
#    the password goes in through the VM's VNC server, as a keyboard
#    (vnc_keys.py; the throwaway VM user's, `lume`, or VM_PASSWORD).
# 3. The VM still takes input: typing reaches TextEdit and a mouse move
#    moves the pointer, right after the switch and again 5 s later.
# 4. The app is quit and started again: it says the hotkey is off until
#    Accessibility is allowed (log, tray title and tooltip) and the tray
#    menu offers Relaunch. (The process that was running keeps its grant:
#    on macOS 15.6 a revocation reaches only new processes; see
#    crates/platform/src/hotkey/macos.rs.)
# 5. The switch is turned on again (the password again): the running app
#    sees it and starts its hotkey by itself (the tray clears, Relaunch
#    goes), and a real Right Option hold dictates into TextEdit; quit and
#    started again, the hotkey listens at once and a hold dictates.
#
# However it ends, the cleanup turns the switch back on if the scenario
# turned it off, so later scenarios keep Accessibility. A run starts from a
# fresh clone anyway.
#
# Evidence (macOS, SIP, the switch, the VNC typing, input checks) goes to
# <scenario>.evidence.txt in the run's output.

AX_SETTINGS_URL='x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility'
AX_SETTINGS_APP="System Settings"
AX_SETTINGS_WINDOW=Accessibility
AX_TOGGLE="$APP_NAME"_Toggle
VM_MOUSE="$VM_FIXTURES/mouse.js"
AX_TYPE_KEYS="$VM_FIXTURES/type-keys.js"
# DictationError::HotkeyFailed's text (crates/core) and the tray's title
# for it (crates/app/src/tray.rs).
# What the tray's status line and tooltip say (the code table's message).
AX_HOTKEY_SHOWN="The hotkey isn't working."
# What the log says (DictationError's Display).
AX_HOTKEY_ERROR="The dictation hotkey isn't working. Allow dictation-app in System Settings, Privacy & Security, Accessibility; it comes back by itself."
AX_EVIDENCE=""
# 1 while the scenario may have left the switch off.
AX_SWITCHED_OFF=0

ax_evidence() {
  printf '%s\n' "$*" >>"$AX_EVIDENCE"
}

# The VM's VNC URL (lume knows it while the VM runs).
vnc_url() {
  lume get "$VM" -f json 2>/dev/null |
    jq -r 'if type=="array" then .[0] else . end | .vncUrl // empty'
}

# ax_switch <on|off>: presses the app's Accessibility switch in System
# Settings, types the password into the sheet macOS shows over VNC, and
# waits until the switch reads <on|off> with the sheet gone. In this VM's
# clone only.
ax_switch() {
  local want=$1 url said status=0 state i
  case $VM in
    dictation-vm | dictation-vm-2) ;;
    *)
      log "  refusing to change Accessibility in $VM"
      return 1
      ;;
  esac
  url=$(vnc_url)
  [ -n "$url" ] || {
    log "  no VNC URL for $VM"
    return 1
  }
  vm "open $(q "$AX_SETTINGS_URL")" || return 1
  ax_ok wait-window "$AX_SETTINGS_APP" "$AX_SETTINGS_WINDOW" 15 >/dev/null || return 1
  state=$(ax check-get "$AX_SETTINGS_APP" "$AX_SETTINGS_WINDOW" "$AX_TOGGLE" 2>&1) || true
  ax_evidence "switch before turning it $want: [$state]"
  # The switch moves at once and the sheet comes up; check-set may report
  # either, so its answer isn't the result.
  ax check-set "$AX_SETTINGS_APP" "$AX_SETTINGS_WINDOW" "$AX_TOGGLE" "$want" >/dev/null 2>&1 || true
  sleep 2
  # The keys go to the frontmost window: System Settings' sheet, not the
  # Terminal that ran ax.js.
  vm "open -a $(q "$AX_SETTINGS_APP")" || return 1
  sleep 1
  screenshot "e2e_accessibility_revoked-sheet-$want"
  said=$(python3 -I "$E2E_DIR/vnc_keys.py" "$url" "${VM_PASSWORD:-lume}" RETURN 2>&1) || status=$?
  ax_evidence "password over VNC: [$said] status $status"
  log "  VNC: $said"
  [ "$status" = 0 ] || return 1
  for i in $(seq 1 10); do
    sleep 1
    # One window (Accessibility) once the sheet has gone.
    said=$(ax windows "$AX_SETTINGS_APP" 2>&1) || true
    state=$(ax check-get "$AX_SETTINGS_APP" "$AX_SETTINGS_WINDOW" "$AX_TOGGLE" 2>&1) || true
    case "$want/$state/$said" in
      on/checked/1\ * | off/unchecked/1\ *)
        ax_evidence "switch $want: [$state], windows [$said]"
        screenshot "e2e_accessibility_revoked-switch-$want"
        vm "osascript -e 'tell application \"System Settings\" to quit'" >/dev/null 2>&1 || true
        return 0
        ;;
    esac
  done
  ax_evidence "switch never read $want: [$state], windows [$said]"
  screenshot "e2e_accessibility_revoked-switch-$want-failed"
  return 1
}

# typed_in_textedit <word>: TextEdit in front, then "<word> " typed with
# real key events from Terminal; true if TextEdit's text then ends with the
# word (ignoring case: TextEdit capitalizes a first word). A stalled window
# server never delivers them. The job says which app was in front when it
# typed, so a miss can be told from a focus problem. TextEdit is read a
# second after the typing, a few times: while its autocorrection is pending
# a read can hang (see e2e_smart_space).
typed_in_textedit() {
  local said status=0 text i front
  front="osascript -l JavaScript -e 'ObjC.import(\"AppKit\"); \"front=\" + \$.NSWorkspace.sharedWorkspace.frontmostApplication.localizedName.js'"
  said=$(VM_TERMINAL_TIMEOUT=20 vm_terminal \
    "open -a TextEdit; sleep 1; $front; osascript -l JavaScript $(q "$AX_TYPE_KEYS") $(q "$1 ")") || status=$?
  ax_evidence "type $1: [$(printf '%s' "$said" | paste -sd ' ' -)] status $status"
  log "  typing: $(printf '%s' "$said" | paste -sd ' ' -)"
  [ "$status" = 0 ] || return 1
  for i in 1 2 3; do
    sleep 1
    text=$(textedit_trimmed | tr '[:upper:]' '[:lower:]')
    case $text in
      *"$1") break ;;
    esac
  done
  log "  TextEdit holds: [$text]"
  ax_evidence "TextEdit after typing $1: [$text]"
  case $text in
    *"$1") return 0 ;;
    *) return 1 ;;
  esac
}

# mouse_moves <x> <y>: a real mouse move from Terminal puts the pointer at
# (x, y).
mouse_moves() {
  local said status=0
  said=$(VM_TERMINAL_TIMEOUT=20 vm_terminal "osascript -l JavaScript $(q "$VM_MOUSE") $1 $2") || status=$?
  ax_evidence "mouse to $1 $2: [$said] status $status"
  log "  mouse: $said"
  [ "$status" = 0 ] && [ "$said" = "at $1 $2" ]
}

# A real Right Option hold dictates into a fresh TextEdit document.
ax_hold_dictates() {
  local before text
  textedit_fresh || return 1
  before=$(idle_count)
  job_says "posted down
posted up" real_key_hold 1.5 || return 1
  wait_for_idle_count $((before + 1)) 15 || return 1
  text=$(textedit_trimmed)
  log "  TextEdit holds: [$text]"
  ax_evidence "dictated: [$text]"
  [ "$text" = "$MOCK_TEXT" ]
}

# The cleanup hook: the switch back on if the scenario may have left it off.
ax_restore() {
  [ "$AX_SWITCHED_OFF" = 1 ] || return 0
  log "  cleanup: turning Accessibility back on for the app"
  ax_switch_on
}

# ax_switch on, and remember it is on.
ax_switch_on() {
  ax_switch on && AX_SWITCHED_OFF=0
}

e2e_accessibility_revoked() {
  local started
  AX_EVIDENCE="$OUT_DIR/e2e_accessibility_revoked.evidence.txt"
  : >"$AX_EVIDENCE"
  ax_evidence "macOS: $(vm 'sw_vers -productVersion' 2>&1)"
  ax_evidence "SIP: $(vm 'csrutil status' 2>&1)"
  arm_key_cleanup e2e_accessibility_revoked
  KEY_CLEANUP_HOOK=ax_restore
  fresh_app_state
  launch_full
  expect "the app starts and logs ready" wait_for_log 'INFO .*ready'
  expect "the real hotkey is on (Accessibility granted to the app)" hotkey_is_on
  textedit_fresh
  expect "typing reaches TextEdit before the switch" typed_in_textedit before
  expect "the mouse moves before the switch" mouse_moves 300 300

  # 2. Off, as the owner did it.
  AX_SWITCHED_OFF=1
  expect "Accessibility is switched off for the app (password over VNC)" ax_switch off
  started=$SECONDS

  # 3. The VM still takes input.
  expect "the app is still running" test "$(app_count)" = 1
  expect "typing still reaches TextEdit after the switch" typed_in_textedit after
  expect "the mouse still moves after the switch" mouse_moves 400 350
  ax_evidence "first input check done $((SECONDS - started)) s after the switch"
  sleep 5
  expect "typing still reaches TextEdit 5 s on" typed_in_textedit later
  expect "the mouse still moves 5 s on" mouse_moves 300 300
  ax_evidence "second input check done $((SECONDS - started)) s after the switch"

  # 4. Started again without Accessibility: the plain guidance, Relaunch.
  expect "the app quits" fresh_app_state
  launch_full
  expect "the app starts again and logs ready" wait_for_log 'INFO .*ready'
  expect "it knows Accessibility is off" \
    wait_for_log 'PermissionMissing: the hotkey is off until Accessibility is granted' 10
  expect "it says the hotkey is off until Accessibility is allowed" \
    wait_for_log "shown to the user error=The dictation hotkey isn.t working" 10
  expect "the tray says the hotkey isn't working, with its fix" \
    env_tray_fix "Choose another key…" tray_shows "! $AX_HOTKEY_SHOWN" "Vocet: $AX_HOTKEY_SHOWN" 10
  expect "typing still reaches TextEdit with the app running unallowed" typed_in_textedit still

  # 5. Allowed again: the hotkey comes back by itself (the supervisor), and
  # after a relaunch too.
  expect "Accessibility is switched on again (password over VNC)" ax_switch_on
  expect "the hotkey comes back by itself (the tray is ready again)" tray_is_clear 15
  expect "a real hold dictates in the running app" ax_hold_dictates
  expect "the app quits" fresh_app_state
  launch_full
  expect "the app is relaunched and logs ready" wait_for_log 'INFO .*ready'
  expect "the hotkey listens at once after the relaunch" hotkey_is_on
  expect "a real hold dictates after the relaunch" ax_hold_dictates
  log "  evidence: $(paste -sd '|' "$AX_EVIDENCE")"

  collect_log e2e_accessibility_revoked
  key_scenario_end
}
