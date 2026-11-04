# U4 test report, run 2

PR: #251 at 1ada0cc (round 3: the overlay page reads the newest level every 50 ms while the meter shows)
Result: PASS (behaviour), with one open point on hosted CI (below)

Tester: Claude Code (Sonnet 5.5), VM `dictation-vm-2`, 2026-11-04. Run 1: `U4-run1.md`.

| # | Check | Result | Notes |
| - | ----- | ------ | ----- |
| 1 | `e2e_pill_states` | PASS | Listening, No sound (1.5 s), Paste sent 803 ms by the log, then hidden; no Paste sent on the silent run. |
| 2 | `e2e_overlay_focus` | PASS | Non-activating panel; TextEdit frontmost throughout. |
| 3 | `e2e_overlay_rendered_states_and_error` | PASS | Passed this time (run 1's load flake did not recur on a quieter Mac). |
| 4 | The meter moves with real input | PASS | `e2e_u4_real_input` (my scenario, run 1's, updated for Paste sent): `say` into the VM's "Apple Virtual Sound Device" moves the bars (`U4-screens/r2-light.2-speech.3.png`); muted input gives No sound with the device name (`r2-light.3-no-sound.png`); speech returns to Listening (`r2-light.4-listening-again.png`). Log: `recording no_sound recording transcribing paste_sent hidden`. Typing into TextEdit while the pill showed also worked. |
| 5 | Polling stops when the pill hides | PASS (unit level) | The app has no counter or log of level reads (and by design never logs levels), so I did not observe the reads in the VM. `Overlay.test.tsx` "stops reading when the meter goes: Transcribing, an error, hidden, unmounted" covers it. Mutation: making `useLevels` ignore `on` (`if (false)` for `if (!on)`) fails exactly that test; restored. Also: 60 vitest overlay tests, 34 pipeline overlay/level tests pass. |
| 6 | Paste sent after a good dictation | PASS | `r2-paste-sent.png`: green check, "Paste sent" (800 ms ±100 by the log, 803 ms). |
| 7 | Crash hunt on the No sound path | PASS, 0 crashes in 50 iterations | See below. |
| 8 | Hosted CI | NOT RUN at 1ada0cc | See below. |

## Crash hunt: 0 crashes in 50 iterations

Scenario `e2e_u4_crash_hunt` (source below, not committed as a scenario). It ran
twice (two fresh VM clones, 25 iterations each). Each run:

- Phase 1, real input with the VM's input gain muted (so No sound is real):
  6 holds of 2.2 s then release; 5 back-to-back pairs (hold 2 s, release, a new
  hold right away, hold 2 s, release); 4 real Right Option holds with Esc
  pressed 2 s in (during No sound); 3 real-key 2.2 s holds.
- Phase 2, the silence fixture (file source): 5 holds, 2 Esc cancels.

After every iteration it checked that exactly one app process ran, and it
compared the VM's `~/Library/Logs/DiagnosticReports` before and after.

Results (both runs): app alive after all 25 iterations; no `panicked` in the
app log; crash report count unchanged (1 before, 1 after, the one already in
the golden image, no `*Vocet*` or `*dictation*` report from the run);
23 No sound views and 23 Recording views in phase 1, 7 No sound in phase 2.
The 4 Esc iterations of phase 1 show 4 Recording phases with no Transcribing
after them (19 Transcribing for 23 Recordings), so Esc did cancel during No
sound. I could not reproduce the Developer's one exit; there is no crash
report to attach. The scenario's crash collection (report list, matching
report text, app log) never fired.

Limits: the Esc iterations log "the key job didn't end cleanly" because my
job-output check expected empty output; the cancels still happened (above).
Muting the input exercises No sound on the real device, not a hot-plug
or sleep, which I did not try.

## Gate and CI
- I ran no local gate. I ran `npx vitest run src/overlay` (60 pass),
  `cargo nextest run -p dictation-pipeline overlay u4 level` (34 pass).
- Hosted CI did not run on 1ada0cc or the later `d0807a8`: the PR shows
  `mergeable: CONFLICTING` (`DIRTY`), and GitHub doesn't start pull-request
  workflows on a conflicting PR. The newest CI I can see for the branch is
  green at d51627b, before round 2. So "hosted CI green" is not shown for
  round 3: the Developer must merge `main` into the branch and let CI run.

## Untested behaviour found
- No observable VM-side proof that polling stops (no counter); unit test only.
- No hot-unplug or device-change during No sound.

## Appendix: `e2e_u4_crash_hunt` (not committed as a scenario)

```sh
# U4 Tester run 2 (not committed as a scenario; quoted in the report).
# Crash hunt on the No sound path. Real mic with the VM input muted, then the
# silence fixture. After every iteration the app must still be running; on an
# exit, the VM's crash reports and the app log are saved before anything resets.

U4_CRASHES=0
U4_ITER=0

u4_reports_count() {
  vm "ls ~/Library/Logs/DiagnosticReports 2>/dev/null | wc -l" | tr -d ' '
}

u4_collect_crash() {
  local tag=$1
  U4_CRASHES=$((U4_CRASHES + 1))
  log "  CRASH #$U4_CRASHES ($tag): the app is gone; collecting reports"
  vm "ls -la ~/Library/Logs/DiagnosticReports/ /Library/Logs/DiagnosticReports/ 2>&1" >"$OUT_DIR/$tag.reports.ls" || true
  vm "sh -c 'for f in ~/Library/Logs/DiagnosticReports/*; do case \"\$f\" in *ocet*|*ictation*) echo \"=== \$f\"; head -c 80000 \"\$f\";; esac; done'" >"$OUT_DIR/$tag.crash.txt" || true
  app_log >"$OUT_DIR/$tag.app.log"
  log "  reports: $(wc -c <"$OUT_DIR/$tag.crash.txt") bytes of crash text saved to $OUT_DIR/$tag.crash.txt"
}

# u4_check <label>: the app is still there; otherwise collect and relaunch.
u4_check() {
  local label=$1
  if [ "$(app_count)" != 1 ]; then
    u4_collect_crash "u4crash.$U4_ITER.$label"
    return 1
  fi
  return 0
}

u4_wait_idle() {
  wait_for_idle_count $(($1 + 1)) 15
}

# Hold the real Right Option, wait for Recording, 2 s later (past No sound)
# press Esc, then release the key.
u4_hold_esc_nosound() {
  local key
  key="osascript -l JavaScript $(q "$VM_RIGHT_OPTION")"
  vm_terminal "down=0
    release() { if [ \$down = 1 ]; then $key up; fi; }
    trap release EXIT; trap 'exit 1' INT TERM HUP
    recordings() {
      sh -c 'cat \"\$0\"/\"\$1\".log* 2>/dev/null' $(q "$VM_LOG_DIR" "$APP_NAME") |
        grep -c 'state changed phase=Recording'
    }
    before=\$(recordings)
    down=1; $key down || exit
    i=0
    while [ \"\$(recordings)\" -le \"\$before\" ]; do
      i=\$((i + 1)); [ \$i -le 100 ] || { echo 'no Recording within 10 s' >&2; exit 1; }
      sleep 0.1
    done
    sleep 2.0
    osascript -l JavaScript $(q "$VM_KEYS") right-option+escape || exit
    sleep 0.3; $key up || exit; down=0"
}

u4_iterate() { # <kind> <n>
  local kind=$1 before
  U4_ITER=$((U4_ITER + 1))
  before=$(idle_count)
  case $kind in
    hold)
      launch --press
      sleep 2.2
      launch --release
      u4_wait_idle "$before" || log "  iteration $U4_ITER ($kind): no Idle after release"
      ;;
    back2back)
      launch --press
      sleep 2.0
      launch --release
      launch --press
      sleep 2.0
      launch --release
      wait_for_idle_count $((before + 2)) 20 || log "  iteration $U4_ITER ($kind): fewer than two Idle"
      ;;
    esc)
      job_says "" u4_hold_esc_nosound >/dev/null 2>&1 || log "  iteration $U4_ITER ($kind): the key job didn't end cleanly"
      u4_wait_idle "$before" || log "  iteration $U4_ITER ($kind): no Idle after Esc"
      ;;
    realkey)
      job_says "posted down
posted up" real_key_hold 2.2 >/dev/null 2>&1 || log "  iteration $U4_ITER ($kind): the key job didn't end cleanly"
      u4_wait_idle "$before" || log "  iteration $U4_ITER ($kind): no Idle after the hold"
      ;;
  esac
  if u4_check "$kind"; then
    log "  iteration $U4_ITER ($kind): app alive"
  else
    return 1
  fi
}

e2e_u4_crash_hunt() {
  local i reports0 reports1 phase_kinds
  arm_key_cleanup e2e_u4_crash_hunt
  reports0=$(u4_reports_count)
  log "  crash reports before: $reports0"

  # Phase 1: real mic, VM input muted.
  fresh_app_state
  launch --mock-text "$MOCK_TEXT"
  expect "the app starts and logs ready" wait_for_log 'INFO .*ready'
  expect "the real hotkey is on" hotkey_is_on
  vm_osascript 'set volume input volume 0' >/dev/null 2>&1 || log "  could not mute the input"
  textedit_fresh
  for i in 1 2 3 4 5 6; do u4_iterate hold || { fresh_app_state; launch --mock-text "$MOCK_TEXT"; wait_for_log 'INFO .*ready' >/dev/null; }; done
  for i in 1 2 3 4 5; do u4_iterate back2back || { fresh_app_state; launch --mock-text "$MOCK_TEXT"; wait_for_log 'INFO .*ready' >/dev/null; }; done
  for i in 1 2 3 4; do u4_iterate esc || { fresh_app_state; launch --mock-text "$MOCK_TEXT"; wait_for_log 'INFO .*ready' >/dev/null; }; done
  for i in 1 2 3; do u4_iterate realkey || { fresh_app_state; launch --mock-text "$MOCK_TEXT"; wait_for_log 'INFO .*ready' >/dev/null; }; done
  log "  overlay states logged in phase 1: $(app_log | grep -c 'overlay shown state="no_sound"') no_sound, $(app_log | grep -c 'overlay shown state="recording"') recording"
  log "  phase 1 panics: $(app_log | grep -ci 'panicked')"
  app_log >"$OUT_DIR/u4crash.phase1.app.log"
  screenshot u4crash.phase1

  # Phase 2: the silence fixture (the file source).
  fresh_app_state
  launch_full "$VM_SILENCE"
  expect "the app starts again and logs ready" wait_for_log 'INFO .*ready'
  for i in 1 2 3 4 5; do u4_iterate hold || { fresh_app_state; launch_full "$VM_SILENCE"; wait_for_log 'INFO .*ready' >/dev/null; }; done
  for i in 1 2; do u4_iterate esc || { fresh_app_state; launch_full "$VM_SILENCE"; wait_for_log 'INFO .*ready' >/dev/null; }; done
  log "  overlay states logged in phase 2: $(app_log | grep -c 'overlay shown state="no_sound"') no_sound"
  log "  phase 2 panics: $(app_log | grep -ci 'panicked')"
  app_log >"$OUT_DIR/u4crash.phase2.app.log"

  reports1=$(u4_reports_count)
  log "  crash reports after: $reports1 (before $reports0)"
  log "  ITERATIONS=$U4_ITER CRASHES=$U4_CRASHES"
  expect "no crash in $U4_ITER iterations" test "$U4_CRASHES" = 0
  expect "no new crash report" test "$reports1" = "$reports0"
  vm_osascript 'set volume input volume 100' >/dev/null 2>&1 || true
  key_scenario_end
}
```
