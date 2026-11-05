# L1 [manual] criterion 8, run by the Tester: in Settings' Speech section (U6),
# download the default model, select its row (no Save: it applies once the
# model has loaded) (the real Hugging Face file), dictate real
# speech (a `say` clip, --debug-audio-file) into TextEdit with the real
# on-device engine, then do it again with the VM's network off if the VM
# allows it. Records release-to-text latency. In ALL_SCENARIOS since L1
# round 2, so post-merge runs keep checking it; it downloads ~477 MB each
# run. The offline half needs passwordless sudo in the VM (#222).

textedit_has_phrase() { textedit_text | grep -qi "dictation test"; }

# Waits (up to 25 min) for the model to show as downloaded. It looks every
# 60 s; if the download broke (a flaky network: the row says it is paused and
# Download continues it) it presses Download again, so one dropped connection
# doesn't end the run. Each ax call is its own short Terminal job: one longer
# than VM_TERMINAL_TIMEOUT (30 s by default) is killed, which is what ended
# the old single 1500 s wait early.
download_wait() {
  local deadline=$((SECONDS + 1500))
  while [ "$SECONDS" -lt "$deadline" ]; do
    # U12: a finished row says nothing, but has its "…" menu and no progress.
    if VM_TERMINAL_TIMEOUT=90 ax_ok wait-text "$APP_NAME" "$SETTINGS_WINDOW" "More actions for English" 60 &&
      ! ax_ok wait-text "$APP_NAME" "$SETTINGS_WINDOW" "↓ " 1 &&
      ! ax_ok wait-text "$APP_NAME" "$SETTINGS_WINDOW" "aused" 1; then
      return 0
    fi
    if ax_ok wait-text "$APP_NAME" "$SETTINGS_WINDOW" "Paused · " 2; then
      log "  the download stopped part way; pressing Resume"
      ax_ok press "$APP_NAME" "$SETTINGS_WINDOW" "Resume" || true
    elif ax_ok wait-text "$APP_NAME" "$SETTINGS_WINDOW" "Paused: no connection" 2; then
      log "  the download lost its connection; pressing Retry"
      ax_ok press "$APP_NAME" "$SETTINGS_WINDOW" "Retry" || true
    fi
  done
  return 1
}

e2e_local_model() {
  local t0 t1 ms
  fresh_app_state
  settings_reset
  vm "rm -rf '/Users/$VM_USER/Library/Application Support/com.dictationapp.desktop/models'"
  E2E_TRANSCRIBER=settings launch --debug-audio-file "$VM_FIXTURES/speech-16k.wav"
  expect "the app starts" wait_for_log 'engine rebuilt' 30
  textedit_fresh
  expect "Settings opens" open_settings
  expect "open the Speech section" ax_ok radio-press "$APP_NAME" "$SETTINGS_WINDOW" Speech
  # No key saved, and the own key is the engine: its sheet opens by itself (U12).
  expect "Done closes the key sheet" ax_ok press "$APP_NAME" "$SETTINGS_WINDOW" Done
  expect "the models are listed" ax_ok wait-text "$APP_NAME" "$SETTINGS_WINDOW" "On this Mac" 15
  expect "press Download (first model)" ax_ok press-first "$APP_NAME" "$SETTINGS_WINDOW" Download
  expect "progress is shown" ax_ok wait-text "$APP_NAME" "$SETTINGS_WINDOW" "↓ " 30
  expect "the model finishes downloading" download_wait
  expect "select English (first model)" ax_ok radio-press "$APP_NAME" "$SETTINGS_WINDOW" English
  expect "the model loads" wait_for_log 'on-device model loaded' 120
  expect "the settings choose it" settings_file_has transcription local
  expect "closing hides Settings" close_settings
  vm "ls -l '/Users/$VM_USER/Library/Application Support/com.dictationapp.desktop/models'" >&2 || true

  local n
  for n in 1 2 3; do
    textedit_fresh
    t0=$(perl -MTime::HiRes=time -e 'printf "%.0f", time*1000')
    dictate_once 2
    t1=$(perl -MTime::HiRes=time -e 'printf "%.0f", time*1000')
    ms=$((t1 - t0))
    log "  LATENCY online run $n: whole press+2s+release->Idle ${ms} ms (minus 2000 ms hold, and ~1 s ssh/launch overhead)"
    expect "online run $n pasted a transcript containing 'dictation test'" \
      textedit_has_phrase
  done
  log "  TextEdit holds: $(textedit_trimmed)"

  # Offline.
  if vm "sudo -n true" 2>/dev/null; then
    local svc
    svc=$(vm "networksetup -listallnetworkservices | sed 1d | head -1")
    log "  turning off network service: $svc"
    vm "sudo -n networksetup -setnetworkserviceenabled '$svc' off" || true
    sleep 3
    if vm "curl -s -m 5 -o /dev/null https://huggingface.co"; then
      log "  NOTE: VM still has network after disabling; offline check is not proven"
    else
      log "  VM is offline (curl to huggingface.co fails)"
      textedit_fresh
      t0=$(perl -MTime::HiRes=time -e 'printf "%.0f", time*1000')
      dictate_once 2
      t1=$(perl -MTime::HiRes=time -e 'printf "%.0f", time*1000')
      log "  LATENCY offline: $((t1 - t0)) ms total"
      expect "offline: transcript pasted" textedit_has_phrase
    fi
    vm "sudo -n networksetup -setnetworkserviceenabled '$svc' on" || true
  else
    log "  NOTE: no passwordless sudo in the VM; cannot switch the network off"
  fi
  [ "$SCENARIO_FAILED" = 0 ] || dump_ui e2e_local_model
  collect_log e2e_local_model
}
