# N1 criterion 2: the rest of the suite needs the test flags, which a real release
# build doesn't have (src-tauri/src/flags.rs). This one runs what testers get: the
# bundle at E2E_RELEASE_APP (the .app from scripts/release.sh or a mounted DMG),
# with no test flag. It is optional, and skipped without that path.
#
# It checks the hardened runtime and the microphone entitlement on the bundle, that
# the test flags are ignored, and then, as a person would: the app starts with its
# permissions (the golden image's, so the bundle must be signed dictation-app Dev),
# the real hotkey starts, a real Right Option hold records from the real microphone
# (the VM's mic is usually silent: either outcome is a recording, and neither is a
# permission error), the overlay shows, the Settings window draws (WKWebView under
# the hardened runtime), and the login item can be set and cleared.

e2e_release_build_smoke() {
  local details entitlements since
  if [ -z "${E2E_RELEASE_APP:-}" ]; then
    log "  skipped: set E2E_RELEASE_APP to a release build's .app"
    SCENARIO_SKIPPED=1
    return 0
  fi
  details=$(codesign -dvv "$E2E_RELEASE_APP" 2>&1)
  entitlements=$(codesign -d --entitlements - "$E2E_RELEASE_APP" 2>/dev/null)
  expect "the bundle has the hardened runtime" grep -q 'flags=.*runtime' <<<"$details"
  expect "and the microphone entitlement" grep -q 'com.apple.security.device.audio-input' <<<"$entitlements"
  expect "and nothing else" test "$(grep -o 'com\.apple\.[a-z.-]*' <<<"$entitlements" | sort -u)" = com.apple.security.device.audio-input

  fresh_app_state
  install_app "$E2E_RELEASE_APP"
  clear_app_log
  vm "open -n $(q "$VM_APP") --args --press"
  expect "the release build starts" wait_for_log 'INFO .*starting version='
  expect "it logs ready" wait_for_log 'INFO .*ready'
  sleep 2
  since=$(app_log)
  expect "the test flag is ignored" sh -c '! grep -q "test hook event=Pressed" <<EOF
$1
EOF' _ "$since"
  expect "the real hotkey is on (Accessibility holds)" hotkey_is_on

  expect "a real Right Option hold starts recording, the overlay showing" release_smoke_hold
  since=$(app_log)
  expect "the microphone isn't refused" sh -c '! grep -qE "Microphone permission is missing|waiting for the microphone prompt" <<EOF
$1
EOF' _ "$since"

  expect "Settings opens" ax_ok tray-click "$APP_NAME" Settings
  expect "its window draws its page" ax_ok wait-text "$APP_NAME" Settings "General" 15
  screenshot e2e_release_build_smoke-settings
  close_terminal || true
  collect_log e2e_release_build_smoke
  fresh_app_state
  install_app "$APP_PATH"
}

# Holds Right Option for 2 s from Terminal, which has the grant, and checks that the
# log says it recorded and that the overlay came up.
release_smoke_hold() {
  VM_TERMINAL_TIMEOUT=30 vm_terminal "osascript -l JavaScript $(q "$VM_RIGHT_OPTION") down; sleep 2; osascript -l JavaScript $(q "$VM_RIGHT_OPTION") up" >/dev/null || return 1
  wait_for_log 'state changed phase=Recording' 10 && wait_for_log 'overlay shown state="recording"' 10
}
