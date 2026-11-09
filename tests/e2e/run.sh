#!/usr/bin/env bash
# Runs the e2e scenarios against the built app inside the Lume VM. PRs don't
# run it (the Tester does); after each merge the Lead runs
# scripts/e2e-main.sh, which calls tests/e2e/ci-run.sh. See tests/e2e/README.md.
#
#   tests/e2e/run.sh                     # build, reset the VM, run every scenario
#   tests/e2e/run.sh e2e_startup         # just one scenario
#   tests/e2e/run.sh --no-build --no-reset e2e_press_release
#   E2E_VM=dictation-vm-2 tests/e2e/run.sh           # the second VM
#   tests/e2e/run.sh --exclude e2e_real_groq         # all but this one (repeatable)
#   tests/e2e/run.sh --clear-stale-lock <vm>         # remove a dead run's lock
#
# Exits 0 only if no scenario failed and no required one was skipped. The
# optional ones (OPTIONAL_SCENARIOS) may be skipped; the summary says so.
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"
E2E_DIR=tests/e2e
# shellcheck source=tests/e2e/lib.sh
. "$E2E_DIR/lib.sh"
# shellcheck source=tests/e2e/lib_full.sh
. "$E2E_DIR/lib_full.sh"
for scenario_file in "$E2E_DIR"/scenarios/*.sh; do
  # shellcheck source=/dev/null
  . "$scenario_file"
done

ALL_SCENARIOS=(
  e2e_startup e2e_single_instance_cold_race e2e_press_release
  e2e_microphone_denied_then_recover e2e_real_mic
  e2e_mock_paste e2e_paste_leaves_no_modifier e2e_clipboard_restore e2e_user_copy_wins
  e2e_silence e2e_rapid
  e2e_secure_input_blocks_paste
  e2e_hold_release e2e_held_at_launch e2e_hotkey_listener_recovers
  e2e_quit_during_recording_then_relaunch e2e_tray_error_clears_on_next_recording
  e2e_settings_save_reopen e2e_onboarding_first_run e2e_onboarding_five_steps
  e2e_onboarding_layout e2e_onboarding_round2
  e2e_settings_key_provider_recovery e2e_cancel_and_busy_presses
  e2e_focus_switch_during_transcription e2e_unicode_multiline_selection_paste
  e2e_quit_during_transcription_then_relaunch
  e2e_error_bad_key e2e_real_groq
  e2e_overlay_focus e2e_overlay_rendered_states_and_error e2e_pill_states
  e2e_smart_space e2e_password_field_not_read e2e_layout_paste e2e_image_clipboard_restore
  e2e_dmg_install e2e_update_keeps_permissions e2e_update_from_dictation_app
  e2e_update_keychain_control
  e2e_launch_at_login
  e2e_hotkey_shortcuts e2e_no_words_transcript e2e_hotkey_recorder
  e2e_accessibility_revoked e2e_setup_no_permissions e2e_setup_own_key_no_permissions
  e2e_local_model
  e2e_first_paste_after_launch e2e_quit_right_after_paste e2e_tray_menu_screenshots
  e2e_onboarding_screenshots e2e_setup_redesign_screens
  e2e_history
  e2e_release_build_smoke
  e2e_update_privileged_install
  e2e_update_to_new_certificate
)
# Scenarios that need something the VM may lack (a microphone, a Groq key):
# they may SKIP. Any other scenario that SKIPs fails the run.
OPTIONAL_SCENARIOS=(e2e_real_mic e2e_real_groq e2e_update_keychain_control e2e_tray_menu_screenshots e2e_onboarding_screenshots e2e_setup_redesign_screens e2e_release_build_smoke e2e_notarized_dmg_first_launch)
APP_PATH=target/debug/bundle/macos/$APP_NAME.app
build=1
reset=1
scenarios=()
excluded=()
while [ $# -gt 0 ]; do
  arg=$1
  shift
  case $arg in
    --clear-stale-lock)
      # <vm>: dictation-vm, dictation-vm-2 (or vm-start, golden-clone). Takes
      # no VM, tools or build: it only looks at the lock.
      [ $# -gt 0 ] || die "--clear-stale-lock needs a lock name: ${E2E_CLEARABLE_LOCKS[*]}"
      clear_stale_lock "$1"
      exit 0
      ;;
    --no-build) build=0 ;;
    --no-reset) reset=0 ;;
    --exclude)
      [ $# -gt 0 ] || die "--exclude needs a scenario name"
      excluded+=("$1")
      shift
      ;;
    -h | --help)
      sed -n '2,14p' "$0"
      exit 0
      ;;
    -*) die "unknown option $arg" ;;
    *) scenarios+=("$arg") ;;
  esac
done
[ ${#scenarios[@]} -gt 0 ] || scenarios=("${ALL_SCENARIOS[@]}")
for scenario in ${excluded[@]+"${excluded[@]}"}; do
  declare -F "$scenario" >/dev/null || die "--exclude: no scenario named $scenario"
  kept=()
  for name in "${scenarios[@]}"; do
    [ "$name" = "$scenario" ] || kept+=("$name")
  done
  scenarios=(${kept[@]+"${kept[@]}"})
done
[ ${#scenarios[@]} -gt 0 ] || die "no scenario left to run"
for scenario in "${scenarios[@]}"; do
  declare -F "$scenario" >/dev/null || die "no scenario named $scenario"
done

for tool in lume jq ssh tar; do
  command -v "$tool" >/dev/null || die "$tool is not installed"
done
[ -f "$SSH_KEY" ] || die "no VM ssh key at $SSH_KEY"
ensure_host_key_alias

# The VM's name is in the output dir, so two runs (one per VM) don't share one.
OUT_DIR=${E2E_OUT_DIR:-$E2E_DIR/out/$(date +%Y%m%d-%H%M%S)-$VM}
mkdir -p "$OUT_DIR"

if [ $build = 1 ]; then
  log "building $APP_PATH"
  npm run tauri build -- --debug --bundles app >"$OUT_DIR/build.log" 2>&1 ||
    die "build failed; see $OUT_DIR/build.log"
fi
[ -d "$APP_PATH" ] || die "no app at $APP_PATH; run without --no-build"

# One run per VM at a time, held until the run ends (after the build, which
# doesn't need the VM). The traps come first, so no lock is ever held without
# them.
arm_run_traps
vm_lock_take

if [ $reset = 1 ]; then
  reset_vm
else
  wait_for_vm
  terminal_ignore_saved_state
fi
install_app "$APP_PATH"
install_fixtures "$E2E_DIR/fixtures"

# Each scenario runs in a subshell of its own (run_scenario, lib.sh), so one
# that dies can't end the run or hide the others' results. Outside any
# condition, or the scenarios' `set -e` would be ignored.
set +e
run_scenarios "${scenarios[@]}"
status=$?
set -e
fresh_app_state || log "warning: the app was still running at the end of the run"
rm -f "$E2E_JOB_COUNTER"
log "output in $OUT_DIR"
[ "$status" = 0 ] || die "the run failed; see $OUT_DIR/results.txt"
