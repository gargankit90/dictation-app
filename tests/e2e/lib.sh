# Shared helpers for the e2e scenarios; sourced by run.sh.
#
# Everything talks to the Lume VM `dictation-vm` over ssh. The golden image
# `dictation-vm-golden` is only ever cloned, never started or changed.

VM=dictation-vm
GOLDEN=dictation-vm-golden
SSH_KEY="${SSH_KEY:-$HOME/.ssh/dictation_vm_ed25519}"
VM_USER=lume
APP_NAME=dictation-app
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
vm() {
  ssh -i "$SSH_KEY" -o HostKeyAlias="$VM" -o BatchMode=yes -o ConnectTimeout=5 \
    "$VM_USER@$VM_IP" "$@"
}

# Stops and deletes dictation-vm, clones it fresh from the golden image and
# boots it headless. Waits until ssh answers.
reset_vm() {
  log "resetting $VM from $GOLDEN"
  lume stop "$VM" >/dev/null 2>&1 || true
  lume delete "$VM" --force >/dev/null 2>&1 || true
  lume clone "$GOLDEN" "$VM" >/dev/null || die "lume clone failed"
  lume run "$VM" --no-display --detach >/dev/null || die "lume run failed"
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
  tar -C "$(dirname "$app")" -cf - "$(basename "$app")" | vm "tar -C ~/Applications -xf -"
  vm "codesign --verify --deep $(q "$VM_APP")" || die "the app's signature didn't survive the copy"
}

# Copies tests/e2e/fixtures/ into the VM at $VM_FIXTURES.
install_fixtures() {
  local dir=$1
  log "installing the audio fixtures in the VM"
  vm "rm -rf $(q "$VM_FIXTURES") && mkdir -p $(q "$VM_FIXTURES")"
  tar -C "$dir" -cf - . | vm "tar -C $(q "$VM_FIXTURES") -xf -"
}

# Quits every running copy of the app and clears its logs.
fresh_app_state() {
  local name
  name=$(q "$APP_NAME")
  vm "pkill -x $name; sleep 1; pkill -9 -x $name; rm -rf $(q "$VM_LOG_DIR")" || true
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
  vm "pgrep -x $(q "$APP_NAME") | wc -l" | tr -d ' '
}

# The app's log so far (all rotated files, oldest first).
app_log() {
  # sh, not the login shell (zsh), so a glob with no log yet isn't an error.
  vm "sh -c 'cat \"\$0\"/\"\$1\".log* 2>/dev/null' $(q "$VM_LOG_DIR" "$APP_NAME")" || true
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
