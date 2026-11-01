#!/usr/bin/env bash
# Runs the e2e scenarios against the built app inside the Lume VM. Not part
# of CI (CI has no VM). See tests/e2e/README.md.
#
#   tests/e2e/run.sh                     # build, reset the VM, run every scenario
#   tests/e2e/run.sh e2e_startup         # just one scenario
#   tests/e2e/run.sh --no-build --no-reset e2e_press_release
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
  e2e_startup e2e_press_release e2e_real_mic
  e2e_mock_paste e2e_clipboard_restore e2e_silence e2e_rapid
  e2e_hold_release e2e_held_at_launch
  e2e_error_bad_key e2e_real_groq
  e2e_overlay_focus
  e2e_smart_space e2e_layout_paste e2e_image_clipboard_restore
  e2e_launch_at_login
)
# Scenarios that need something the VM may lack (a microphone, a Groq key):
# they may SKIP. Any other scenario that SKIPs fails the run.
OPTIONAL_SCENARIOS=(e2e_real_mic e2e_real_groq)
APP_PATH=target/debug/bundle/macos/$APP_NAME.app
build=1
reset=1
scenarios=()
for arg in "$@"; do
  case $arg in
    --no-build) build=0 ;;
    --no-reset) reset=0 ;;
    -h | --help)
      sed -n '2,10p' "$0"
      exit 0
      ;;
    -*) die "unknown option $arg" ;;
    *) scenarios+=("$arg") ;;
  esac
done
[ ${#scenarios[@]} -gt 0 ] || scenarios=("${ALL_SCENARIOS[@]}")
for scenario in "${scenarios[@]}"; do
  declare -F "$scenario" >/dev/null || die "no scenario named $scenario"
done

for tool in lume jq ssh tar; do
  command -v "$tool" >/dev/null || die "$tool is not installed"
done
[ -f "$SSH_KEY" ] || die "no VM ssh key at $SSH_KEY"

OUT_DIR=$E2E_DIR/out/$(date +%Y%m%d-%H%M%S)
mkdir -p "$OUT_DIR"

if [ $build = 1 ]; then
  log "building $APP_PATH"
  npm run tauri build -- --debug --bundles app >"$OUT_DIR/build.log" 2>&1 ||
    die "build failed; see $OUT_DIR/build.log"
fi
[ -d "$APP_PATH" ] || die "no app at $APP_PATH; run without --no-build"

if [ $reset = 1 ]; then
  reset_vm
else
  wait_for_vm
fi
install_app "$APP_PATH"
install_fixtures "$E2E_DIR/fixtures"

is_optional() {
  local name
  for name in "${OPTIONAL_SCENARIOS[@]}"; do
    [ "$name" = "$1" ] && return 0
  done
  return 1
}

passed=()
failed=()
skipped=()
bad_skips=()
results=()
for scenario in "${scenarios[@]}"; do
  log "scenario $scenario"
  SCENARIO_FAILED=0
  SCENARIO_SKIPPED=0
  "$scenario"
  if [ "$SCENARIO_SKIPPED" = 1 ]; then
    skipped+=("$scenario")
    if is_optional "$scenario"; then
      log "scenario $scenario: SKIP (optional)"
      results+=("SKIP     $scenario (optional)")
    else
      bad_skips+=("$scenario")
      log "scenario $scenario: SKIP (required: this fails the run)"
      results+=("SKIP     $scenario (REQUIRED: counts as a failure)")
    fi
  elif [ "$SCENARIO_FAILED" = 1 ]; then
    failed+=("$scenario")
    log "scenario $scenario: FAIL"
    results+=("FAIL     $scenario")
  else
    passed+=("$scenario")
    log "scenario $scenario: PASS"
    results+=("PASS     $scenario")
  fi
done
fresh_app_state

# One line per scenario and a total, also saved with the run's output.
summary="${#passed[@]} passed, ${#failed[@]} failed, ${#skipped[@]} skipped"
{
  printf '%s\n' "${results[@]}"
  echo "$summary"
} | tee "$OUT_DIR/results.txt" >&2
log "output in $OUT_DIR"
if [ ${#failed[@]} -gt 0 ] || [ ${#bad_skips[@]} -gt 0 ]; then
  die "$summary; failed: ${failed[*]:-none}; required but skipped: ${bad_skips[*]:-none}"
fi
if [ ${#skipped[@]} -gt 0 ]; then
  log "OK, but skipped: ${skipped[*]} (optional)"
fi
log "no scenario failed ($summary)"
