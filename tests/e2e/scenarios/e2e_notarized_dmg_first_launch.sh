# N2 criterion 4 (#206): a notarized DMG that carries the quarantine flag, as a
# browser download leaves it, opens with no workaround. The steps are the tester
# guide's: copy the DMG in, mount it, drag Vocet.app to Applications, open it.
# There is no `xattr` command anywhere but the one that SETS the flag.
#
# It checks, inside the VM, that:
#   - the DMG and the app are quarantined before the first launch;
#   - `spctl --assess` says "Notarized Developer ID" for the app (the DMG's
#     own assessment is logged too);
#   - the app's signature verifies, its team is the one release-signing.conf
#     names, and it has a stapled ticket (`stapler validate`, if the VM has it);
#   - opening the app from Applications starts it. macOS may ask once whether
#     to open an app from the internet: that prompt is expected. The
#     scenario takes a screenshot of it, clicks Open, and logs exactly which
#     text and buttons the person sees. Any other prompt, or no start, fails.
#
# It is optional and is not in ALL_SCENARIOS, so neither a plain run nor
# scripts/e2e-main.sh nor CI runs it. Run it by name:
#
#   tests/e2e/run.sh e2e_notarized_dmg_first_launch
#   E2E_NOTARIZED_DMG=/path/to/Vocet_0.1.0_aarch64.dmg tests/e2e/run.sh e2e_notarized_dmg_first_launch
#
# E2E_NOTARIZED_DMG defaults to the owner's N1 test build (the notes "Test
# signing run."; never published). It reads only that one file, and skips if it
# isn't there or E2E_NOTARIZED_DMG is set empty. It never signs, builds or
# reads a credential.

NOTARIZED_DMG_DEFAULT=/Users/gauri/orca/workspaces/dictation-app/release-test/target/release-out/v0.1.0/Vocet_0.1.0_aarch64.dmg
NOTARIZED_TEAM=XF2G42MVHN
NOTARIZED_APPS=/Applications
NOTARIZED_DOWNLOADS="/Users/$VM_USER/Downloads"
NOTARIZED_MOUNT="/Users/$VM_USER/e2e-notarized-mount"
# The prompt's windows belong to one of these (found out by running it).
GATEKEEPER_PROMPT_APPS=(CoreServicesUIAgent UserNotificationCenter)

# gatekeeper_prompt_find: prints the app whose window shows Gatekeeper's "Are you
# sure you want to open it?" prompt, and succeeds, if one is on screen. (Its
# first line names the agent that flagged the file: "“Vocet” is an app created by
# the app Safari" here, "downloaded from the Internet" for other agents.)
gatekeeper_prompt_find() {
  local app said
  for app in "${GATEKEEPER_PROMPT_APPS[@]}"; do
    said=$(ax alert-count "$app" "Are you sure you want to open it" 2>/dev/null) || continue
    if [ "$said" != 0 ]; then
      printf '%s' "$app"
      return 0
    fi
  done
  return 1
}

# Waits up to $1 s for the prompt, or for the app to be running with no prompt.
# Prints "prompt <app>", "none" (the app started with no prompt) or "nothing".
gatekeeper_wait() {
  local secs=$1 i app
  for i in $(seq 1 "$secs"); do
    if app=$(gatekeeper_prompt_find); then
      printf 'prompt %s' "$app"
      return 0
    fi
    if grep -q 'INFO .*ready' <<<"$(app_log)"; then
      printf none
      return 0
    fi
    sleep 1
  done
  printf nothing
}

e2e_notarized_dmg_first_launch() {
  local dmg=${E2E_NOTARIZED_DMG-$NOTARIZED_DMG_DEFAULT} name seen app_dir info prompt_app

  if [ -z "$dmg" ] || [ ! -f "$dmg" ]; then
    log "  skipped: no notarized DMG (set E2E_NOTARIZED_DMG to one; default $NOTARIZED_DMG_DEFAULT)"
    SCENARIO_SKIPPED=1
    return 0
  fi
  name=$(basename "$dmg")
  log "  DMG: $dmg"

  fresh_app_state
  # Where a browser leaves it, flagged as downloaded.
  vm "mkdir -p $(q "$NOTARIZED_DOWNLOADS") && rm -f $(q "$NOTARIZED_DOWNLOADS/$name")"
  VM_CMD_TIMEOUT=600 vm "cat > $(q "$NOTARIZED_DOWNLOADS/$name")" <"$dmg"
  vm "xattr -w com.apple.quarantine '0083;$(printf '%x' "$(date +%s)");Safari;' $(q "$NOTARIZED_DOWNLOADS/$name")"
  expect "the downloaded DMG is quarantined" \
    vm "xattr -p com.apple.quarantine $(q "$NOTARIZED_DOWNLOADS/$name")"
  info=$(vm "spctl --assess --type open --context context:primary-signature --verbose=2 $(q "$NOTARIZED_DOWNLOADS/$name") 2>&1" || true)
  log "  spctl on the DMG: $(tr '\n' ' ' <<<"$info")"

  # Guide step 1: open the DMG, drag the app to Applications.
  vm "hdiutil detach $(q "$NOTARIZED_MOUNT") -force >/dev/null 2>&1; rm -rf $(q "$NOTARIZED_MOUNT"); mkdir -p $(q "$NOTARIZED_MOUNT")"
  expect "the DMG mounts in the VM" \
    vm "hdiutil attach $(q "$NOTARIZED_DOWNLOADS/$name") -nobrowse -readonly -mountpoint $(q "$NOTARIZED_MOUNT") -quiet"
  expect "the DMG holds the app and an Applications shortcut" \
    vm "test -d $(q "$NOTARIZED_MOUNT/$APP_NAME.app") && test -e $(q "$NOTARIZED_MOUNT/Applications")"
  app_dir=$NOTARIZED_APPS
  if ! vm "test -w $(q "$app_dir")"; then
    app_dir="/Users/$VM_USER/Applications"
    log "  /Applications isn't writable for $VM_USER; using $app_dir"
  fi
  vm "rm -rf $(q "$app_dir/$APP_NAME.app") && ditto $(q "$NOTARIZED_MOUNT/$APP_NAME.app") $(q "$app_dir/$APP_NAME.app")"
  vm "hdiutil detach $(q "$NOTARIZED_MOUNT") -quiet" || true
  # What Finder does with an app dragged out of a quarantined DMG.
  vm "xattr -rw com.apple.quarantine '0083;$(printf '%x' "$(date +%s)");Safari;' $(q "$app_dir/$APP_NAME.app")"
  expect "the app starts out quarantined" \
    vm "xattr -p com.apple.quarantine $(q "$app_dir/$APP_NAME.app")"

  # What macOS says about it, before it is opened.
  info=$(vm "spctl --assess --type execute --verbose=4 $(q "$app_dir/$APP_NAME.app") 2>&1" || true)
  log "  spctl on the app: $(tr '\n' ' ' <<<"$info")"
  expect "spctl says Notarized Developer ID" grep -q 'source=Notarized Developer ID' <<<"$info"
  info=$(vm "codesign -dvv $(q "$app_dir/$APP_NAME.app") 2>&1" || true)
  expect "the app's signature verifies" vm "codesign --verify --deep --strict $(q "$app_dir/$APP_NAME.app")"
  expect "it is signed by the team in release-signing.conf" grep -q "TeamIdentifier=$NOTARIZED_TEAM" <<<"$info"
  expect "with the hardened runtime" grep -q 'flags=.*runtime' <<<"$info"
  if vm "xcrun --find stapler" >/dev/null 2>&1; then
    expect "the app has a stapled ticket" vm "xcrun stapler validate $(q "$app_dir/$APP_NAME.app")"
  else
    log "  no stapler in the VM; the ticket is checked by scripts/release.sh on the host"
  fi

  # Guide step 2: open it from Applications. No xattr, no Open Anyway.
  clear_app_log
  vm "open $(q "$app_dir/$APP_NAME.app")"
  seen=$(gatekeeper_wait 45)
  log "  first launch: $seen"
  case $seen in
    "prompt "*)
      prompt_app=${seen#prompt }
      log "  Gatekeeper's \"Are you sure you want to open it?\" prompt is up (expected), shown by $prompt_app"
      screenshot e2e_notarized_dmg_first_launch-prompt
      {
        echo "--- $prompt_app (what the person sees: its text and buttons)"
        ax dump "$prompt_app" 2>&1
      } >"$OUT_DIR/e2e_notarized_dmg_first_launch-prompt.ax.txt" || true
      log "  the prompt's UI tree is in $OUT_DIR/e2e_notarized_dmg_first_launch-prompt.ax.txt"
      expect "its Open button is clicked" ax_ok alert-press "$prompt_app" "Are you sure you want to open it" Open 10
      ;;
    none) log "  no prompt: the app opened straight away" ;;
    *)
      screenshot e2e_notarized_dmg_first_launch-stuck
      for prompt_app in "${GATEKEEPER_PROMPT_APPS[@]}"; do
        ax dump "$prompt_app" >>"$OUT_DIR/e2e_notarized_dmg_first_launch-stuck.ax.txt" 2>&1 || true
      done
      expect "the app starts or shows only the expected prompt" false
      ;;
  esac
  expect "the app starts and logs ready" wait_for_log 'INFO .*ready' 45
  expect "one app is running" test "$(app_count)" = 1
  screenshot e2e_notarized_dmg_first_launch-running
  info=$(vm "xattr -p com.apple.quarantine $(q "$app_dir/$APP_NAME.app") 2>&1" || true)
  log "  quarantine attribute after the first launch: $info"
  expect "no other prompt is left on screen" test -z "$(gatekeeper_prompt_find || true)"

  collect_log e2e_notarized_dmg_first_launch

  # Back to the golden image's state for the scenarios that follow.
  fresh_app_state
  vm "rm -f $(q "$NOTARIZED_DOWNLOADS/$name"); rm -rf $(q "$NOTARIZED_MOUNT")"
  vm "rm -rf $(q "$app_dir/$APP_NAME.app")"
  install_app "$APP_PATH"
}
