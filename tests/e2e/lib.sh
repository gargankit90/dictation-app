# Shared helpers for the e2e scenarios; sourced by run.sh.
#
# Everything talks to one Lume VM over ssh: $VM, `dictation-vm` or, with
# E2E_VM=dictation-vm-2, the second one (lib_vm.sh). The golden image
# `dictation-vm-golden` is only ever cloned, never started or changed.

SSH_KEY="${SSH_KEY:-$HOME/.ssh/dictation_vm_ed25519}"
VM_USER=lume
# The app is called Vocet (the .app and the process); its log files keep their
# old prefix, so settings, the saved key and logs carry over an update (U1).
APP_NAME=Vocet
# The executable inside the bundle is Vocet too (mainBinaryName), so Login
# Items and Activity Monitor say Vocet; the log files keep their old prefix.
# OLD_PROC_NAME is the executable of builds from before the rename: a copy of
# it still counts as the app running.
PROC_NAME=Vocet
OLD_PROC_NAME=dictation-app
LOG_PREFIX=dictation-app
# The name before the rename, still on the golden image.
OLD_APP_NAME=dictation-app
# Where the app lives in the VM, and where it logs (Tauri's app_log_dir).
VM_APP="/Users/$VM_USER/Applications/$APP_NAME.app"
VM_LOG_DIR="/Users/$VM_USER/Library/Logs/com.dictationapp.desktop"
# Test audio (tests/e2e/fixtures/) in the VM. The scenarios record this file
# instead of the microphone (--debug-audio-file, debug builds only), so they
# need no microphone permission or input device.
VM_FIXTURES="/Users/$VM_USER/e2e-fixtures"
VM_TONE="$VM_FIXTURES/tone-16k.wav"

log() { printf '[e2e %s] %s\n' "$(date +%T)" "$*" >&2; }
die() {
  log "ERROR: $*"
  exit 1
}

# shellcheck source=tests/e2e/lib_vm.sh
. "$(dirname "${BASH_SOURCE[0]}")/lib_vm.sh"

vm_ip() {
  lume get "$VM" -f json 2>/dev/null |
    jq -r 'if type=="array" then .[0] else . end | .ipAddress // empty'
}

# Quotes each argument for the VM's login shell and joins them with spaces,
# so paths and arguments with spaces or shell characters arrive intact.
q() {
  local out=() arg
  for arg in "$@"; do
    out+=("$(printf '%q' "$arg")")
  done
  printf '%s' "${out[*]-}"
}

# Runs a command in the VM (as the GUI user, but outside the GUI session:
# GUI apps must be started with `open`, which LaunchServices runs in it).
# Returns the command's exit status. The whole command has a deadline,
# VM_CMD_TIMEOUT seconds (default 120), not just the connection: a command
# that hangs (a dialog nobody answers, a stuck osascript) is cancelled and
# vm returns 124, so a scenario can't wait forever on the VM.
vm() {
  local limit=${VM_CMD_TIMEOUT:-120} pid watchdog status=0 start=$SECONDS
  # In the background so the watchdog can stop it; `<&0` keeps the caller's
  # stdin (a background command would otherwise read /dev/null).
  ssh -i "$SSH_KEY" -o HostKeyAlias="$VM" -o BatchMode=yes -o ConnectTimeout=5 \
    "$VM_USER@$VM_IP" "$@" <&0 &
  pid=$!
  # Its output goes nowhere, so a $(vm ...) doesn't wait for it to end.
  deadline_watch "$limit" "$pid" >/dev/null 2>&1 &
  watchdog=$!
  wait "$pid" 2>/dev/null || status=$?
  kill "$watchdog" 2>/dev/null || true
  wait "$watchdog" 2>/dev/null || true
  if [ "$status" = 143 ] && [ $((SECONDS - start + 1)) -ge "$limit" ]; then
    log "vm: no answer within ${limit}s; cancelled: $(printf '%.100s' "$*")"
    return 124
  fi
  return "$status"
}

# deadline_watch <seconds> <pid>: sends TERM to <pid> once <seconds> have
# passed, unless it ended first (checked every second, so the watcher itself
# ends within a second of it).
deadline_watch() {
  local limit=$1 pid=$2 i=0
  while [ "$i" -lt "$limit" ]; do
    sleep 1
    kill -0 "$pid" 2>/dev/null || return 0
    i=$((i + 1))
  done
  kill -TERM "$pid" 2>/dev/null || true
}

# Stops and deletes $VM, clones it fresh from the golden image, sets its
# memory and CPUs and boots it headless. Waits until ssh answers.
reset_vm() {
  log "resetting $VM from $GOLDEN"
  refuse_third_vm
  lume stop "$VM" >/dev/null 2>&1 || true
  lume delete "$VM" --force >/dev/null 2>&1 || true
  clone_golden
  set_vm_resources
  start_vm
  wait_for_vm
}

wait_for_vm() {
  local i
  for i in $(seq 1 90); do
    VM_IP=$(vm_ip)
    if [ -n "$VM_IP" ] && vm true 2>/dev/null; then
      log "$VM is up at $VM_IP"
      wait_for_gui
      return 0
    fi
    sleep 2
  done
  die "$VM did not come up with ssh within 3 minutes"
}

# ssh answers before the GUI session does, and `open` fails in that gap
# ("Domain does not support specified action"). Waits (up to 2 minutes) for
# the user's GUI domain and the Dock, then goes on either way: a session that
# never comes up shows as the first `open` failing, with a better message.
wait_for_gui() {
  local i
  for i in $(seq 1 60); do
    if vm 'launchctl print "gui/$(id -u)" >/dev/null 2>&1 && pgrep -x Dock >/dev/null' 2>/dev/null; then
      log "$VM's GUI session is up"
      return 0
    fi
    sleep 2
  done
  log "warning: $VM's GUI session did not come up within 2 minutes; continuing"
}

# Copies the built .app into the VM. tar keeps the bundle's symlinks and
# signature intact; files copied this way carry no quarantine attribute, so
# Gatekeeper doesn't block the self-signed build.
install_app() {
  local app=$1
  log "installing $(basename "$app") in the VM"
  vm "mkdir -p ~/Applications && rm -rf $(q "$VM_APP")"
  # The golden image still holds the app under its old name (dictation-app.app,
  # same bundle id). Two copies would let System Settings and Launch Services
  # name the app after the old one, so remove it (U1).
  vm "rm -rf \"\$HOME/Applications/$OLD_APP_NAME.app\""
  tar -C "$(dirname "$app")" -cf - "$(basename "$app")" |
    VM_CMD_TIMEOUT=600 vm "tar -C ~/Applications -xf -"
  # Launch Services must know the bundle id for `tccutil reset` to accept it
  # (it knew it from the old copy, which is gone).
  vm "/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f $(q "$VM_APP")" || true
  vm "codesign --verify --deep $(q "$VM_APP")" || die "the app's signature didn't survive the copy"
}

# Copies tests/e2e/fixtures/ into the VM at $VM_FIXTURES.
install_fixtures() {
  local dir=$1
  log "installing the audio fixtures in the VM"
  vm "rm -rf $(q "$VM_FIXTURES") && mkdir -p $(q "$VM_FIXTURES")"
  tar -C "$dir" -cf - . | vm "tar -C $(q "$VM_FIXTURES") -xf -"
}

# Kills every running copy of the app (TERM, then KILL) and clears its logs.
# A setup step, not a Quit: a scenario whose journey is quitting uses
# quit_from_tray (lib_full.sh), which fails if the app doesn't go by
# itself. Returns 1 (and says so) if a copy is still running 5 seconds
# after the KILL, so a scenario can't go on against a stale app.
fresh_app_state() {
  local name i
  name=$(q "$PROC_NAME")
  vm "pkill -x $name; pkill -x $OLD_PROC_NAME; sleep 1; pkill -9 -x $name; pkill -9 -x $OLD_PROC_NAME; rm -rf $(q "$VM_LOG_DIR")" || true
  for i in 1 2 3 4 5; do
    [ "$(app_count)" = 0 ] && return 0
    sleep 1
  done
  log "  the app is still running after pkill -9"
  return 1
}

# Waits up to `timeout` seconds until no copy of the app is running.
wait_for_no_app() {
  local timeout=${1:-10} i
  for i in $(seq 1 "$timeout"); do
    [ "$(app_count)" = 0 ] && return 0
    sleep 1
  done
  return 1
}

# Clears the app's log (with the app quit), so a relaunch's lines can be
# counted on their own.
clear_app_log() {
  vm "rm -rf $(q "$VM_LOG_DIR")"
}

# Launches the app in the VM's GUI session with `args` as its arguments.
# -n starts a new process even if one runs (as a second launch from the
# command line would); the single-instance plugin then forwards and exits.
launch() {
  # The engine is chosen when the app starts, so only a first launch gets
  # --transcriber. Scenarios run on the mock (no key, no network) unless
  # they set E2E_TRANSCRIBER=settings for a real provider (M4.4).
  local engine=()
  if [ "$(app_count)" = 0 ]; then
    engine=(--transcriber "${E2E_TRANSCRIBER:-mock}")
  fi
  # -g: don't bring the app to the front, or the paste would miss the
  # window the scenario put there.
  vm "open -g -n $(q "$VM_APP") --args $(q ${engine[@]+"${engine[@]}"} "$@")"
}

app_count() {
  vm "(pgrep -x $(q "$PROC_NAME"); pgrep -x $OLD_PROC_NAME) | wc -l" | tr -d ' '
}

# The app's log so far (all rotated files, oldest first).
app_log() {
  # sh, not the login shell (zsh), so a glob with no log yet isn't an error.
  vm "sh -c 'cat \"\$0\"/\"\$1\".log* 2>/dev/null' $(q "$VM_LOG_DIR" "$LOG_PREFIX")" || true
}

# Waits up to `timeout` seconds for an extended regex to appear in the log.
wait_for_log() {
  local pattern=$1 timeout=${2:-20} i
  for i in $(seq 1 "$timeout"); do
    if app_log | grep -Eq "$pattern"; then
      return 0
    fi
    sleep 1
  done
  return 1
}

# wait_for_log_count <regex> <n> [timeout]: waits up to `timeout` seconds
# (default 20) until at least <n> lines of the log match.
wait_for_log_count() {
  local pattern=$1 want=$2 timeout=${3:-20} i
  for i in $(seq 1 "$timeout"); do
    if [ "$(app_log | grep -Ec "$pattern" || true)" -ge "$want" ]; then
      return 0
    fi
    sleep 1
  done
  return 1
}

# The phases logged so far, one per line ("Recording", "Idle", ...).
logged_phases() {
  app_log | sed -nE 's/.*state changed phase=([A-Za-z]+).*/\1/p'
}

# Saves a screenshot of the VM's desktop (through the cua-driver daemon in
# the VM) as $OUT_DIR/<name>.png. Never fails a scenario.
screenshot() {
  local png="$OUT_DIR/$1.png"
  if vm "~/.local/bin/cua-driver call get_desktop_state '{}'" 2>/dev/null |
    jq -r '.screenshot_png_b64 // empty' | base64 -d >"$png" 2>/dev/null && [ -s "$png" ]; then
    log "screenshot saved to $png"
  else
    rm -f "$png"
    log "no screenshot for $1"
  fi
}

# Saves the app log and a screenshot for this scenario into the run's output
# dir.
collect_log() {
  local scenario=$1
  app_log >"$OUT_DIR/$scenario.app.log"
  log "app log saved to $OUT_DIR/$scenario.app.log"
  screenshot "$scenario"
}

# expect <description> <command...>: records PASS or FAIL for one check.
expect() {
  local what=$1
  shift
  if "$@"; then
    log "  PASS: $what"
  else
    log "  FAIL: $what"
    SCENARIO_FAILED=1
  fi
}

# --- Running scenarios ---------------------------------------------------------

# run_scenario <name>: runs one scenario in a subshell of its own, with
# `set -e`, so an early end (a failed command, die, a trap's exit) ends only
# that scenario, and sets SCENARIO_RESULT to PASS, FAIL or SKIP. A scenario
# passes only if it reached its end with no failed check: one that ended
# early fails with its exit status, even 0 (an `exit 0` half way is not a
# pass). An early end saves the app's log and a screenshot, which a scenario
# that died may not have done.
run_scenario() {
  local name=$1 result status=0 saved_e=0 flags
  result="$OUT_DIR/.result-$name"
  rm -f "$result"
  case $- in *e*) saved_e=1 ;; esac
  # Not `( ... ) || status=$?`: in a || list bash ignores set -e inside the
  # subshell too, and the scenario's failed commands would go unnoticed.
  set +e
  (
    set -e
    SCENARIO_FAILED=0
    SCENARIO_SKIPPED=0
    "$name"
    printf '%s %s\n' "$SCENARIO_FAILED" "$SCENARIO_SKIPPED" >"$result"
  )
  status=$?
  [ "$saved_e" = 0 ] || set -e
  if [ ! -s "$result" ]; then
    log "scenario $name ended early (exit status $status) before reaching its end"
    collect_log "$name" || true
    SCENARIO_RESULT=FAIL
    return 0
  fi
  flags=$(cat "$result")
  rm -f "$result"
  if [ "$status" != 0 ]; then
    # It reached its end, but an exit trap failed it (a failed cleanup).
    log "scenario $name: its cleanup failed (exit status $status)"
    SCENARIO_RESULT=FAIL
  elif [ "${flags#* }" = 1 ]; then
    SCENARIO_RESULT=SKIP
  elif [ "${flags% *}" = 1 ]; then
    SCENARIO_RESULT=FAIL
  else
    SCENARIO_RESULT=PASS
  fi
}

is_optional() {
  local name
  for name in ${OPTIONAL_SCENARIOS[@]+"${OPTIONAL_SCENARIOS[@]}"}; do
    [ "$name" = "$1" ] && return 0
  done
  return 1
}

# run_scenarios <name...>: runs each scenario, prints one line per scenario
# and a total (also to $OUT_DIR/results.txt), and returns 0 only if none
# failed and no required one was skipped. The optional ones
# (OPTIONAL_SCENARIOS) may skip; the summary still says so. Call it outside
# any `if`, `||` or `&&`, or the scenarios' `set -e` is ignored.
run_scenarios() {
  local scenario summary passed=() failed=() skipped=() bad_skips=() results=()
  for scenario in "$@"; do
    log "scenario $scenario"
    run_scenario "$scenario"
    case $SCENARIO_RESULT in
      SKIP)
        skipped+=("$scenario")
        if is_optional "$scenario"; then
          log "scenario $scenario: SKIP (optional)"
          results+=("SKIP     $scenario (optional)")
        else
          bad_skips+=("$scenario")
          log "scenario $scenario: SKIP (required: this fails the run)"
          results+=("SKIP     $scenario (REQUIRED: counts as a failure)")
        fi
        ;;
      FAIL)
        failed+=("$scenario")
        log "scenario $scenario: FAIL"
        results+=("FAIL     $scenario")
        ;;
      *)
        passed+=("$scenario")
        log "scenario $scenario: PASS"
        results+=("PASS     $scenario")
        ;;
    esac
  done
  summary="${#passed[@]} passed, ${#failed[@]} failed, ${#skipped[@]} skipped"
  {
    printf '%s\n' ${results[@]+"${results[@]}"}
    echo "$summary"
  } | tee "$OUT_DIR/results.txt" >&2
  if [ ${#failed[@]} -gt 0 ] || [ ${#bad_skips[@]} -gt 0 ]; then
    log "$summary; failed: ${failed[*]:-none}; required but skipped: ${bad_skips[*]:-none}"
    return 1
  fi
  if [ ${#skipped[@]} -gt 0 ]; then
    log "OK, but skipped: ${skipped[*]} (optional)"
  fi
  log "no scenario failed ($summary)"
}
