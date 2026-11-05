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
