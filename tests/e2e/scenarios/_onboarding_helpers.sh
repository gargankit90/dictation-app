# Helpers for the onboarding scenarios (U8).

# The tray lists "Resume setup…" (it reads the app's state every few seconds).
tray_has_resume_setup() {
  local i
  for i in $(seq 1 10); do
    case $(tray_menu) in
      *"Resume setup"*) return 0 ;;
    esac
    sleep 1
  done
  log "  the tray has no Resume setup: $(tray_menu | paste -sd '|' -)"
  return 1
}

# The tray doesn't list "Resume setup…", within a few seconds.
tray_has_no_resume_setup() {
  local i
  for i in $(seq 1 10); do
    case $(tray_menu) in
      *"Resume setup"*) sleep 1 ;;
      *) return 0 ;;
    esac
  done
  log "  the tray still has Resume setup: $(tray_menu | paste -sd '|' -)"
  return 1
}

# Setup left at Permissions by an earlier launch, with an engine chosen, for
# the scenarios that start there. The scenario's settings_reset removes both.
seed_setup_at_permissions() {
  vm "mkdir -p $(q "$(dirname "$SETUP_PROGRESS_VM")") &&
    printf '%s' '{\"step\":\"permissions\",\"welcome_passed\":true}' >$(q "$SETUP_PROGRESS_VM") &&
    printf '%s' '{\"version\":2,\"transcription\":\"local\"}' >$(q "$SETTINGS_FILE_VM")"
}

# Screenshots of the setup window (moved from the retired screenshot
# walkthroughs): the appearance, the window from the cua-driver daemon, and
# obs_shot <name> saving it as onboarding-<name>.png in the run's output.
obs_appearance() {
  vm "osascript -e 'tell application \"System Events\" to tell appearance preferences to set dark mode to $([ "$1" = dark ] && echo true || echo false)'"
  sleep 2
}

# The onboarding window's fields, from the cua-driver daemon, which may capture
# windows where an ssh session or a Terminal job may not.
obs_window() {
  vm "~/.local/bin/cua-driver call list_windows '{}'" |
    jq -c '[.windows[] | select(.app_name == "'"$APP_NAME"'" and (.title | contains("Set up")))] | first // empty'
}

# obs_shot <name>: the window, as <name>.png in the run's output.
obs_shot() {
  local window id pid png="$OUT_DIR/onboarding-$1.png"
  sleep 1
  window=$(obs_window)
  if [ -z "$window" ]; then
    log "  no window for $1"
    return 0
  fi
  id=$(printf '%s' "$window" | jq -r .window_id)
  pid=$(printf '%s' "$window" | jq -r .pid)
  vm "rm -f /tmp/obs.png; ~/.local/bin/cua-driver call get_window_state '{\"pid\":$pid,\"window_id\":$id,\"include_accessibility_tree\":false,\"screenshot_out_file\":\"/tmp/obs.png\"}' >/dev/null" || true
  vm "base64 -i /tmp/obs.png" | base64 -d >"$png" || true
  log "  window screenshot $png: $(wc -c <"$png") bytes"
}
