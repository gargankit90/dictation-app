# L1 [manual] criterion 8, run by the Tester: pick "On this Mac" in Settings,
# download the default model (the real Hugging Face file), dictate real
# speech (a `say` clip, --debug-audio-file) into TextEdit with the real
# on-device engine, then do it again with the VM's network off if the VM
# allows it. Records release-to-text latency. In ALL_SCENARIOS since L1
# round 2, so post-merge runs keep checking it; it downloads ~477 MB each
# run. The offline half needs passwordless sudo in the VM (#222).

textedit_has_phrase() { textedit_text | grep -qi "dictation test"; }

e2e_local_model() {
  local t0 t1 ms
  fresh_app_state
  settings_reset
  vm "rm -rf '/Users/$VM_USER/Library/Application Support/com.dictationapp.desktop/models'"
  E2E_TRANSCRIBER=settings launch --debug-audio-file "$VM_FIXTURES/speech-16k.wav"
  expect "the app starts" wait_for_log 'engine rebuilt' 30
  textedit_fresh
  expect "Settings opens" open_settings
  expect "choose On this Mac" ax_ok radio-press "$APP_NAME" "$SETTINGS_WINDOW" "On this Mac"
  expect "the models are listed" ax_ok wait-text "$APP_NAME" "$SETTINGS_WINDOW" "Parakeet Unified 0.6B" 15
  expect "press Download (first model)" ax_ok press "$APP_NAME" "$SETTINGS_WINDOW" Download
  expect "progress is shown" ax_ok wait-text "$APP_NAME" "$SETTINGS_WINDOW" "Downloading" 30
  expect "the model finishes downloading" ax_ok wait-text "$APP_NAME" "$SETTINGS_WINDOW" "Downloaded" 1500
  expect "press Save" ax_ok press "$APP_NAME" "$SETTINGS_WINDOW" Save
  expect "Settings saved." ax_ok wait-text "$APP_NAME" "$SETTINGS_WINDOW" "Settings saved." 20
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
