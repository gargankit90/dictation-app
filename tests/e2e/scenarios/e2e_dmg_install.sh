# M5.4 criterion 3: install the way docs/TESTERS.md says, in a freshly
# restored VM. The DMG goes into the VM as a browser download would leave it
# (with the quarantine flag), is mounted, the app is copied out, and the
# guide's own `xattr` command (read from docs/TESTERS.md, so the guide can't
# drift from what is tested) clears the quarantine. A first dictation (mock
# transcriber, tone fixture) pastes into TextEdit. Then the guide's sections
# 3 to 5: both permissions hold (permissions_hold), the made-up key is typed
# into Settings, Test & Save works against the local provider (the
# T4a seam), and a dictation pastes the provider's transcript. The app that
# runs is the one from the DMG, replacing the build run.sh installed.
#
# What it cannot check: the Gatekeeper dialog itself and the Microphone and
# Accessibility prompts (the golden image has both granted to the app signed
# `dictation-app Dev`, as a tester's Mac has once they allowed them).

TESTERS_DOC=docs/TESTERS.md

VM_DMG_DIR="/Users/$VM_USER/Downloads"
VM_DMG_MOUNT="/Users/$VM_USER/e2e-dmg-mount"

# The DMG of the debug build run.sh built (built here if it isn't there,
# next to the .app so the other scenarios' bundle is left alone).
e2e_dmg_build() {
  local dmg
  dmg=$(find target/debug/bundle/dmg -name '*.dmg' -print -quit 2>/dev/null || true)
  if [ -z "$dmg" ] || [ "${build:-1}" = 1 ]; then
    log "building the DMG"
    CI=true npm run tauri build -- --debug --bundles app,dmg >"$OUT_DIR/build-dmg.log" 2>&1 ||
      die "DMG build failed; see $OUT_DIR/build-dmg.log"
    dmg=$(find target/debug/bundle/dmg -name '*.dmg' -print -quit)
  fi
  [ -n "$dmg" ] || die "no DMG under target/debug/bundle/dmg"
  printf '%s' "$dmg"
}

# True when `path` in the VM carries no quarantine flag.
e2e_not_quarantined() {
  ! vm "xattr -p com.apple.quarantine $(q "$1")" >/dev/null 2>&1
}

e2e_dmg_install() {
  local dmg name text
  dmg=$(e2e_dmg_build)
  name=$(basename "$dmg")
  log "  DMG: $dmg"
  expect "the DMG passes scripts/check-dmg.sh" scripts/check-dmg.sh "$dmg"

  fresh_app_state
  # Put the DMG where a browser would, flagged as downloaded.
  vm "mkdir -p $(q "$VM_DMG_DIR") && rm -f $(q "$VM_DMG_DIR/$name")"
  vm "cat > $(q "$VM_DMG_DIR/$name")" <"$dmg"
  vm "xattr -w com.apple.quarantine '0083;$(printf '%x' "$(date +%s)");Safari;' $(q "$VM_DMG_DIR/$name")"
  expect "the downloaded DMG is quarantined" \
    vm "xattr -p com.apple.quarantine $(q "$VM_DMG_DIR/$name")"

  # README step 1: open the DMG, drag the app out. (~/Applications stands in
  # for /Applications, which the VM's user can't write without a prompt.)
  vm "hdiutil detach $(q "$VM_DMG_MOUNT") -force >/dev/null 2>&1; rm -rf $(q "$VM_DMG_MOUNT"); mkdir -p $(q "$VM_DMG_MOUNT")"
  expect "the DMG mounts in the VM" \
    vm "hdiutil attach $(q "$VM_DMG_DIR/$name") -nobrowse -readonly -mountpoint $(q "$VM_DMG_MOUNT") -quiet"
  expect "the DMG holds the app and an Applications shortcut" \
    vm "test -d $(q "$VM_DMG_MOUNT/$APP_NAME.app") && test -e $(q "$VM_DMG_MOUNT/Applications")"
  vm "mkdir -p ~/Applications && rm -rf $(q "$VM_APP") && ditto $(q "$VM_DMG_MOUNT/$APP_NAME.app") $(q "$VM_APP")"
  vm "hdiutil detach $(q "$VM_DMG_MOUNT") -quiet" || true
  expect "the copied app's signature verifies" \
    vm "codesign --verify --deep --strict $(q "$VM_APP")"
  expect "the licence notice is inside the app" \
    vm "test -s $(q "$VM_APP/Contents/Resources/THIRD-PARTY-LICENSES.txt")"

  # README step 2: macOS flags an app dragged out of a downloaded DMG; the
  # README's xattr command clears it.
  vm "xattr -rw com.apple.quarantine '0083;$(printf '%x' "$(date +%s)");Safari;' $(q "$VM_APP")"
  expect "the app starts out quarantined" \
    vm "xattr -p com.apple.quarantine $(q "$VM_APP")"
  # The guide's command, for /Applications; the VM's user installs into
  # ~/Applications (see above), so only the path is swapped.
  local doc_command vm_command
  doc_command=$(grep -m 1 '^xattr -dr com.apple.quarantine /Applications/dictation-app.app$' "$TESTERS_DOC" || true)
  expect "docs/TESTERS.md has the quarantine command" test -n "$doc_command"
  vm_command=${doc_command//\/Applications\/dictation-app.app/$(q "$VM_APP")}
  log "  running the guide's command: $vm_command"
  [ -z "$vm_command" ] || vm "$vm_command"
  expect "the guide's xattr command clears the quarantine" e2e_not_quarantined "$VM_APP"

  # README step 5: a first dictation.
  launch_full
  expect "the app from the DMG starts and logs ready" wait_for_log 'INFO .*ready'
  textedit_fresh
  expect "the dictation gets back to Idle" dictate_once 1
  text=$(textedit_trimmed)
  log "  TextEdit holds: [$text]"
  expect "the transcript is in TextEdit" test "$text" = "$MOCK_TEXT"
  expect "one app is running" test "$(app_count)" = 1

  collect_log e2e_dmg_install-mock
  textedit_close
  vm "rm -f $(q "$VM_DMG_DIR/$name")"

  e2e_dmg_guide_steps
}

# The guide's sections 3 to 5 with the app from the DMG: permissions, the key
# in Settings, a first dictation on the local provider.
e2e_dmg_guide_steps() {
  local version
  version=$(vm "defaults read $(q "$VM_APP/Contents/Info") CFBundleShortVersionString")
  arm_key_cleanup e2e_dmg_install
  KEY_CLEANUP_HOOK=provider_cleanup

  # Section 3: Microphone and Accessibility.
  expect "section 3: Microphone and Accessibility hold" permissions_hold "$version"

  # Section 4: the key, typed into Settings (no key saved before).
  fresh_app_state
  settings_reset
  groq_key_remove
  expect "the local provider starts" provider_start "$PROVIDER_GOOD_KEY"
  expect "the app starts on the local provider" launch_on_provider
  expect "with no key saved yet" wait_for_log 'engine settings provider=groq .*has_key=false'
  expect "section 4: Settings… opens from the tray" open_settings
  expect "the Speech section opens" ax_ok radio-press "$APP_NAME" "$SETTINGS_WINDOW" Speech
  expect "the key is typed into the key field" \
    ax_ok secret-set "$APP_NAME" "$SETTINGS_WINDOW" api-key "$PROVIDER_GOOD_KEY"
  expect "Test & Save" ax_ok press "$APP_NAME" "$SETTINGS_WINDOW" "Test & Save"
  expect "says it is connected and saved" \
    ax_ok wait-text "$APP_NAME" "$SETTINGS_WINDOW" "✓ Connected · saved in Keychain" 15
  expect "the test reached the local provider with the typed key" provider_request_has 1 auth=good
  GROQ_KEY_STORED=1
  expect "the engine is rebuilt with the key" wait_for_log 'engine settings provider=groq .*has_key=true'
  expect "closing the window hides it" close_settings

  # Section 5: dictate.
  textedit_fresh
  expect "section 5: the first dictation ends Idle" dictate_once 1
  expect "it was sent with the saved key" provider_request_has 2 auth=good
  expect "TextEdit holds the transcript" textedit_is "$(esc_b64 "$PROVIDER_TEXT")"
  expect "two requests in all (Test & Save and the dictation)" provider_requests_are 2

  [ "$SCENARIO_FAILED" = 0 ] || dump_ui e2e_dmg_install
  collect_log e2e_dmg_install
  key_scenario_end
}
