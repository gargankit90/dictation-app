# M5.4 criterion 2 (the scripted part): install the way the tester README
# says. The DMG goes into the VM as a browser download would leave it (with
# the quarantine flag), is mounted, the app is copied out, the README's one
# `xattr` command clears the quarantine, and a first dictation (mock
# transcriber, tone fixture) pastes into TextEdit. The app that runs is the
# one from the DMG, replacing the build run.sh installed.
#
# What it cannot check: the Gatekeeper dialog itself and the Microphone and
# Accessibility prompts (a person does those; see the PR's "How to check").

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
  vm "xattr -dr com.apple.quarantine $(q "$VM_APP")"
  expect "the README's xattr command clears the quarantine" e2e_not_quarantined "$VM_APP"

  # README step 5: a first dictation.
  launch_full
  expect "the app from the DMG starts and logs ready" wait_for_log 'INFO .*ready'
  textedit_fresh
  expect "the dictation gets back to Idle" dictate_once 1
  text=$(textedit_trimmed)
  log "  TextEdit holds: [$text]"
  expect "the transcript is in TextEdit" test "$text" = "$MOCK_TEXT"
  expect "one app is running" test "$(app_count)" = 1

  collect_log e2e_dmg_install
  textedit_close
  vm "rm -f $(q "$VM_DMG_DIR/$name")"
}
