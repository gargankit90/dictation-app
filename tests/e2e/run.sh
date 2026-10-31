#!/usr/bin/env bash
# Runs the e2e scenarios against the built app inside the Lume VM. Not part
# of CI (CI has no VM). See tests/e2e/README.md.
#
#   tests/e2e/run.sh                     # build, reset the VM, run every scenario
#   tests/e2e/run.sh e2e_startup         # just one scenario
#   tests/e2e/run.sh --no-build --no-reset e2e_press_release
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"
E2E_DIR=tests/e2e
# shellcheck source=tests/e2e/lib.sh
. "$E2E_DIR/lib.sh"
for scenario_file in "$E2E_DIR"/scenarios/*.sh; do
  # shellcheck source=/dev/null
  . "$scenario_file"
done

ALL_SCENARIOS=(e2e_startup e2e_press_release)
APP_PATH=target/debug/bundle/macos/$APP_NAME.app
build=1
reset=1
scenarios=()
for arg in "$@"; do
  case $arg in
    --no-build) build=0 ;;
    --no-reset) reset=0 ;;
    -h | --help)
      sed -n '2,7p' "$0"
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

failed=()
for scenario in "${scenarios[@]}"; do
  log "scenario $scenario"
  SCENARIO_FAILED=0
  "$scenario"
  if [ "$SCENARIO_FAILED" = 1 ]; then
    failed+=("$scenario")
    log "scenario $scenario: FAIL"
  else
    log "scenario $scenario: PASS"
  fi
done
fresh_app_state

log "output in $OUT_DIR"
if [ ${#failed[@]} -gt 0 ]; then
  die "failed: ${failed[*]}"
fi
log "all scenarios passed: ${scenarios[*]}"
