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

# Each scenario runs in a subshell of its own (run_scenario, lib.sh), so one
# that dies can't end the run or hide the others' results. Outside any
# condition, or the scenarios' `set -e` would be ignored.
set +e
run_scenarios "${scenarios[@]}"
status=$?
set -e
fresh_app_state || log "warning: the app was still running at the end of the run"
log "output in $OUT_DIR"
[ "$status" = 0 ] || die "the run failed; see $OUT_DIR/results.txt"
