# U4 test report, run 1

PR: #251 at 4bd9ddf (`u4-pill-meter`)
Result: PASS

**Scope:** everything below was tested at 4bd9ddf, the head named in the task.
While I tested, the Developer pushed round-1 fixes on top (3c2ac07 renames
the success state from "Pasted" to "Paste sent" and bounds level delivery;
a071d4f updates the e2e scenarios). Those are not retested here: the
"Pasted" results and screenshots below are for 4bd9ddf and need a run 2 on
the new head.

Tester: Claude Code (Sonnet 5.5), VM `dictation-vm-2`, 2026-11-03/04. The Mac was
heavily loaded (load average about 35 for most of the run), so several VM
steps were slow; see "Failures and flakes".

| # | Criterion | Test(s) | Result | Notes |
| - | --------- | ------- | ------ | ----- |
| 1 | Level per chunk, 15–30/s, only while recording; silence at floor; −20 dBFS sine ±2 dB | `dictation-ports level::`, `dictation-platform audio::macos::tests::criterion_u4_1_*`, `audio::file::tests::criterion_u4_1_*`, `dictation-pipeline criterion_u4_1_*` | PASS | 20 core/ports/platform tests and 29 pipeline u4/overlay tests pass. Mutation: see below. |
| 2 | Levels over the overlay channel; never logged, stored or sent | `overlay::tests` (shell), `Overlay.test.tsx`, code read | PASS | `emit_to(overlay, "overlay-level")`; no level in `UiEvent`; the overlay log lines name only the state (`no_sound`, `pasted`). The VM app logs hold no level or device name. |
| 3 | 7 bars from the level, Listening label and hint, no bar transition under Reduce Motion | `npx vitest run src/overlay` (55 pass) | PASS | Screenshots show 7 bars, flat when quiet, uneven with speech. |
| 4 | No sound after 1.5 s below the threshold, device name, back to Listening, named constant, fake clock | `dictation-core level::`, `dictation-pipeline criterion_u4_4_*`; VM `e2e_pill_states` and `e2e_u4_real_input` | PASS | Fixture: No sound 1560 ms after Listening by the log. Real input: see criterion 8. |
| 5 | Pasted for 800 ms ±100, then hides; not on failure | `dictation-pipeline criterion_u4_5_*`; VM `e2e_pill_states` | PASS | 802 ms by the log. No Pasted on the silent run, the bad-key run or the mic-denied run (`recording no_sound transcribing hidden`, `… error`). |
| 6 | Error: one sentence, no code, amber badge, no buttons or links | `Overlay.test.tsx` (no focusable element in any state); VM screenshots | PASS | "The provider rejected your API key. Check or re-enter it in Settings." with the amber "!" badge, no code, no buttons. |
| 7 | Existing overlay tests pass; `e2e_overlay_focus` passes | vitest, cargo, VM `e2e_overlay_focus` | PASS | The panel stayed a non-activating panel (`can_become_key=false`); TextEdit stayed frontmost throughout. |
| 8 | [manual] VM: speech moves the meter, silence then No sound, speech back to Listening; Pasted; bad-key error | `e2e_u4_real_input` (my scenario, below), light and dark | PASS | Real input, not the file source. See "Criterion 8". |

## Mutation checks (each restored with `git checkout`)

- **Level computation returned a constant (−30 dBFS)** in `LevelMeter::take`:
  9 of 13 tests failed, including `a_sine_reads_its_level`,
  `criterion_u4_1_a_sine_at_minus_20_dbfs_reads_minus_20`,
  `criterion_u4_1_the_level_survives_resampling` and the silence-floor tests.
- **No sound timer** `NO_SOUND_AFTER` set to 100 ms: 4 failures
  (`criterion_u4_4_silent_after_one_and_a_half_seconds_of_quiet`,
  `…sound_ends_silence_at_once…`, `a_short_pause_between_words_is_not_silence`,
  pipeline `criterion_u4_4_quiet_shows_no_sound_and_sound_brings_listening_back`).
  Set to 5 s: 6 failures. All on the fake clock.
- **Pasted time** `PASTED_VISIBLE_FOR` set to 1200 ms:
  `criterion_u4_5_run_shows_pasted_for_800_ms` fails.

## Gate
Hosted CI on 4bd9ddf: Quality gate (Linux) pass, Platform crate (macOS) pass,
DMG (macOS) pass. I ran no local gate (the Mac was loaded); I ran the
affected crates' tests (core, ports, platform with `testing`, pipeline,
`src-tauri overlay`) and `npx vitest run src/overlay`: all pass. Coverage is
CI's job and passed.

## VM e2e (`dictation-vm-2`)

| Scenario | Result |
| --- | --- |
| `e2e_pill_states` | PASS (Pasted 802 ms; No sound 1560 ms) |
| `e2e_overlay_focus` | PASS (TextEdit frontmost throughout; non-activating panel) |
| `e2e_microphone_denied_then_recover` | PASS |
| `e2e_mock_paste` | PASS |
| `e2e_silence` | PASS |
| `e2e_overlay_rendered_states_and_error` | one check FAILED twice under load, all others PASS; see below |
| `e2e_u4_real_input` (mine, uncommitted; source below) | PASS in light and in dark (after two harness fixes) |

## Criterion 8: real input (screenshots in `U4-screens/`)

The VM's input device is "Apple Virtual Sound Device"; it carries the host
microphone, so `say` on the host is heard through it. The pill names that
device in its hint. Steps in `e2e_u4_real_input`, in light and in dark mode
(the pill is the same dark capsule in both, as the spec says):

1. Press: Listening, bars flat and low (`*-1-listening-quiet`).
2. `say` speech: the bars go uneven and taller (`*-2-speech.2`, `*-2-speech.3`).
3. The VM's input gain is set to 0 (a muted mic): after about 1.5 s the pill
   reads "No sound" and "Check your microphone (Apple Virtual Sound Device)"
   with the idle-colour flat meter (`*-3-no-sound`).
4. Input gain back to 100 and `say` again: back to Listening with moving bars
   (`*-4-listening-again`). Log: `recording no_sound recording transcribing pasted hidden`.
5. Release: Transcribing, then Pasted with a green check (`light-6-pasted`;
   in the dark run the 0.3 s screenshot grid missed the 800 ms pill, but the
   log shows `pasted` then `hidden`), then hidden.
6. Bad key (`gsk_invalid_test_key`, not a real key) with the tone fixture:
   one sentence, amber badge, no code, no buttons (`*-5-error`). The panel was
   on screen at 420x72 right after the error appeared.

Two things about the setup, not defects:

- With the unmuted input, the room's own noise kept the bars at about 10 px
  (about −35 dBFS), above the −50 dBFS threshold, so No sound never appeared
  in the first attempt. The meter works; the threshold is the same as the
  transcription gate's. Real quiet needs a muted or really quiet input, which
  is why I muted the VM's input gain for step 3. A noisy room never shows No sound.
- With a long device name the hint is cut with an ellipsis
  ("…(Apple Virtu…", `*-3-no-sound`). Minor: the name is hard to read in full on
  a 420 px pill.

## Overlay never takes focus; typing continues
- `e2e_overlay_focus` PASS: the `lsappinfo` samples were always TextEdit.
- During Listening in `e2e_u4_real_input`, TextEdit was frontmost and text typed
  with cua-driver `type_text` reached TextEdit (`typing-while-pill`), light and
  dark. (`osascript keystroke` is not allowed in the VM, so the first attempt
  used cua-driver.)

## Pasted with TextEdit empty (#239, fix in P1 #241)
Pasted showed on every successful paste. Across five dictations in my runs
(`e2e_pill_states` once, `e2e_u4_real_input` four times), TextEdit held the
pasted text 3 times and was empty 2 times (both after the scenario read the
overlay with Terminal jobs mid-recording, the known #239 pattern). So Pasted
did show twice with TextEdit empty. Not a FAIL for U4, per the task.

## Failures and flakes
- `e2e_overlay_rendered_states_and_error`, check "the panel is on screen with
  the error", failed in both runs. In each the check ran at the boundary of the
  error's 4 s life: the log shows the error shown at 07:32:31.23 and hidden at
  07:32:35.23, and the check's window list (just after a slow screenshot)
  held only the tray item. The screenshot taken just before shows the error pill.
  In my scenario, on a quieter Mac, the same window list taken right after the
  error appeared held the 420x72 panel. The rest of the scenario passed
  (error lifetime 3907 ms on screen and 4003 ms by the log). I consider it a
  load flake in the check, not a product defect; I did not edit it.
- My first `e2e_u4_real_input` attempts failed on my own harness: `osascript`
  keystrokes are blocked; the No sound text lacks the device name in the
  expected string; `say` started before the unmute took effect. All fixed in
  the version below.
- Process note: while cleaning up, I ran `pkill -f tests/e2e/run.sh`, which
  matches every worker's runs, not only mine. Other runs were still alive
  afterwards, but I can't prove I didn't interrupt one; the Lead should check
  the other workers' e2e results around 23:56 PST on 2026-11-03.

## Untested behaviour found
- No test checks the hint's truncation with a long device name (minor, above).
- The meter's bar heights in a real browser are covered only by the jsdom
  test and screenshots, not by a numeric e2e check; acceptable for U4.

## Appendix: `e2e_u4_real_input` (not committed as a scenario)

```sh
# U4 Tester run 1 (not committed as a scenario; quoted in the report).
# Real input: the VM's microphone is the host's, so `say` on the host is heard.
# U4_APPEARANCE=light|dark.

u4_say() { say -v Samantha -r 150 "$1"; }

# u4_shots <prefix> <count>: sequential screenshots, in the background.
u4_shots() {
  local n
  (for n in $(seq 1 "$2"); do screenshot "$1.$n"; done) &
}

e2e_u4_real_input() {
  local mode=${U4_APPEARANCE:-light} mark say_pid
  if [ "$mode" = dark ]; then
    vm_osascript 'tell application "System Events" to tell appearance preferences to set dark mode to true' >/dev/null || log "  could not set dark mode"
  else
    vm_osascript 'tell application "System Events" to tell appearance preferences to set dark mode to false' >/dev/null || true
  fi
  sleep 2
  log "  appearance: $(vm 'defaults read -g AppleInterfaceStyle 2>/dev/null || echo Light')"
  fresh_app_state
  launch --mock-text "$MOCK_TEXT"
  expect "the app starts and logs ready" wait_for_log 'INFO .*ready'
  textedit_fresh
  mark=$(pill_log_mark)

  launch --press
  expect "Listening on the real mic" overlay_shows "$OVERLAY_RECORDING"
  expect "TextEdit in front" textedit_front
  screenshot "u4.$mode.1-listening-quiet"
  # Typing into TextEdit while the pill shows (Listening).
  log "  frontmost before typing: $(frontmost_app)"
  log "  type_text: $(vm "~/.local/bin/cua-driver call type_text '{\"pid\":'\"\$(pgrep -x TextEdit | head -1)\"',\"text\":\"typing-while-pill\"}'" 2>&1 | head -c 200)"
  sleep 1
  log "  TextEdit holds: $(textedit_text | head -c 60)"
  expect "typing reached TextEdit while the pill showed" \
    sh -c "printf '%s' \"\$1\" | grep -q 'typing-while-pill'" _ "$(textedit_text)"
  log "  overlay still: $(overlay_shows "$OVERLAY_RECORDING" 2 && echo Listening)"
  expect "TextEdit in front again" textedit_front
  # Speech: the meter moves.
  vm_osascript 'set volume input volume 100' >/dev/null 2>&1 || true
  (u4_say "Testing the level meter. One two three four five six seven eight nine ten.") &
  say_pid=$!
  sleep 0.8
  u4_shots "u4.$mode.2-speech" 4
  wait
  # Silence: the VM's input gain is muted (the room's own noise reads about
  # -35 dBFS, above the -50 dBFS threshold, so a quiet room can't be had).
  log "  input volume before mute: $(vm_osascript 'input volume of (get volume settings)' 2>&1 | head -1)"
  vm_osascript 'set volume input volume 0' >/dev/null 2>&1 || log "  could not mute the input"
  log "  input volume after mute: $(vm_osascript 'input volume of (get volume settings)' 2>&1 | head -1)"
  expect "No sound after silence (muted input), with the device name" \
    overlay_shows "$OVERLAY_NO_SOUND (Apple Virtual Sound Device)" 8
  screenshot "u4.$mode.3-no-sound"
  # Speech again: back to Listening (unmute first, then speak a long phrase).
  vm_osascript 'set volume input volume 100' >/dev/null 2>&1 || true
  (u4_say "Speaking again now. Back to listening. One two three four five six seven eight nine ten.") &
  say_pid=$!
  sleep 0.5
  expect "Listening returns with speech" overlay_shows "$OVERLAY_RECORDING" 6
  screenshot "u4.$mode.4-listening-again"
  wait "$say_pid" 2>/dev/null || true
  expect "TextEdit is in front again for the paste" textedit_front
  launch --release
  pill_pasted_screenshots
  expect "back to Idle" wait_for_idle_count 1
  wait
  log "  overlay log: $(pill_log_since "$mark")"
  if textedit_text | grep -q "hello world"; then log "  PASTED_LANDED=yes"; else log "  PASTED_LANDED=no"; fi
  log "  pasted_shown=$(app_log | grep -c 'overlay shown state="*pasted')"
  collect_log "e2e_u4_real_input.$mode"

  # Error: a bad key.
  groq_guard_start
  fresh_app_state
  groq_key_store gsk_invalid_test_key
  groq_textedit_new
  E2E_TRANSCRIBER=settings launch --debug-audio-file "$VM_TONE"
  expect "app ready (settings engine)" wait_for_log 'INFO .*ready'
  launch --press
  sleep 2
  launch --release
  expect "the error is shown" wait_for_log 'shown to the user error=' 30
  log "  windows right after the error: $(app_windows_on_screen | paste -sd ' ' -)"
  screenshot "u4.$mode.5-error"
  sleep 0.5
  screenshot "u4.$mode.5-error-b"
  log "  error overlay: $(overlay_watch 1 2>&1 | head -3)"
  groq_key_remove
  fresh_app_state
}
```
