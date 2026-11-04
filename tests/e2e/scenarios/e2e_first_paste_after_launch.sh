# P1 (#239, #203): the first dictation after the app starts is pasted. For
# each hotkey, the default Right Option and the combination Ctrl+Shift+A, the
# app is relaunched E2E_FIRST_PASTE_RUNS times (default 3); each time, as
# soon as it is ready, TextEdit is put in front with an empty document and
# the key is held once with real key events. The mock's text must land in
# TextEdit every time. Each try logs one OBS line: landed or not, the
# clipboard afterwards (the user's must be back), the phases, and how long
# after the chord the paste showed in TextEdit (the app's `landed_ms`; the
# bug: a target that read later than the old fixed 300 ms pasted the user's
# old clipboard instead). The user's clipboard must be back within 5 s.
#
# The hotkey is set in the settings file before each launch (the app reads it
# at startup); the key cleanup removes the file at the end. Uses helpers
# from other scenario files, which run.sh sources first:
# settings_write_hotkey (e2e_hotkey_shortcuts), settings_reset
# (e2e_settings_save_reopen), frontmost_app (_provider_helpers).

VM_COMBO="$VM_FIXTURES/combo.js"
FP_SENTINEL="e2e-first-paste-sentinel"

# fp_hold_job <key>: one Terminal job that puts TextEdit in front and holds
# <key> (right-option or ctrl-shift-a) for 1.5 s with real key events.
fp_hold_job() {
  local hold
  case $1 in
    right-option) hold="osascript -l JavaScript $(q "$VM_RIGHT_OPTION") down && sleep 1.5; osascript -l JavaScript $(q "$VM_RIGHT_OPTION") up" ;;
    ctrl-shift-a) hold="osascript -l JavaScript $(q "$VM_COMBO") 1.5" ;;
    *) die "fp_hold_job: unknown key $1" ;;
  esac
  vm_terminal "open -a TextEdit; sleep 1; $hold"
}

# fp_try <key> <spec> <n>: relaunch on <spec>, dictate once with <key>; true
# if the mock's text landed in TextEdit.
fp_try() {
  local key=$1 spec=$2 n=$3 text clip landed=no
  fresh_app_state || return 1
  settings_write_hotkey "$spec"
  set_clipboard "$FP_SENTINEL"
  launch_full
  wait_for_log "ready .*hotkey=$(printf '%s' "$spec" | sed 's/+/\\+/g')" 20 || {
    log "  try $n ($key): the app didn't log ready on $spec"
    return 1
  }
  textedit_fresh || return 1
  fp_hold_job "$key" >/dev/null || log "  try $n ($key): the hold job failed"
  wait_for_idle_count 1 15 || log "  try $n ($key): the dictation didn't get back to Idle"
  sleep 0.5
  text=$(textedit_trimmed)
  # A paste the app couldn't confirm (TextEdit too busy to answer for its
  # caret) leaves the transcript on the clipboard and gives the user's back
  # UNCONFIRMED_RESTORE (3 s) after the chord: wait up to 5 s for it.
  for _ in 1 2 3 4 5 6 7 8 9 10; do
    clip=$(clipboard)
    [ "$clip" = "$FP_SENTINEL" ] && break
    sleep 0.5
  done
  [ "$text" = "$MOCK_TEXT" ] && [ "$clip" = "$FP_SENTINEL" ] && landed=yes
  log "OBS: first-paste key=$key try=$n landed=$landed textedit=[$text] clipboard=[$clip] frontmost=[$(frontmost_app)] phases=[$(logged_phases | paste -sd ' ' -)] read=[$(app_log | grep -Eo "landed_ms=[0-9]+|wasn't seen landing|no readable caret" | tail -1)]"
  if [ "$landed" = no ]; then
    app_log | grep -E 'WARN|ERROR|paste|clipboard' | sed 's/^/    /' >&2 || true
    collect_log "e2e_first_paste_after_launch-$key-$n"
  fi
  [ "$landed" = yes ]
}

e2e_first_paste_after_launch() {
  local runs=${E2E_FIRST_PASTE_RUNS:-3} n key spec landed total
  arm_key_cleanup e2e_first_paste_after_launch left-control left-shift
  KEY_CLEANUP_HOOK=settings_reset
  for key in right-option ctrl-shift-a; do
    case $key in
      right-option) spec=RightAlt ;;
      ctrl-shift-a) spec=Ctrl+Shift+A ;;
    esac
    landed=0
    for n in $(seq 1 "$runs"); do
      if fp_try "$key" "$spec" "$n"; then
        landed=$((landed + 1))
      fi
    done
    total="$landed of $runs"
    log "OBS: first-paste key=$key landed $total"
    expect "the first paste after each launch lands with $key ($total)" test "$landed" = "$runs"
  done
  collect_log e2e_first_paste_after_launch
  textedit_close
  key_scenario_end
}
