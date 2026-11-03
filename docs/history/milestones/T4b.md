# T4b: native journeys: overlay text, login item across reboots, microphone denial

## Goal

Catch, in the real signed app, three things a user would see that today's
VM tests can't: an overlay that's visible but blank or shows the wrong state
or a stale error; launch at login that says "off" but still starts the app
after a reboot; and a first-time user who denies the microphone and gets
stuck. From the follow-up test audit (#103, T4 group), part 2 (part 1, T4a,
is PR #157).

## Scope

- **#119 `e2e_overlay_rendered_states_and_error`.** Read the overlay's real
  rendered text (Accessibility tree of the WebKit panel, or a test-build
  hook that reports the DOM text): Recording, Transcribing, and a known
  offline error from the T4a local provider. The panel is visible only when
  it should be; the error goes away after its time; a new recording replaces
  the error. Reuse T4a's local provider helpers (`_provider_helpers.sh`).
- **#123 launch at login: off survives a reboot.** Extend
  `e2e_launch_at_login.sh` (or add `e2e_launch_at_login_disable_survives_reboot`):
  turn it on through the real Settings checkbox, save, reopen Settings and
  read the OS state; reboot: exactly one auto-started instance, and it can
  dictate (hold the hotkey: Recording; Esc: Idle, no request, no paste). Then
  turn it off through Settings, reboot again: the app does not start.
- **#126 `e2e_microphone_denied_then_recover`.** In the test VM clone only
  (never `dictation-vm-golden`), reset the microphone permission for the
  app (e.g. `tccutil reset Microphone <bundle id>` inside the VM) so it
  starts ungranted; launch without the file-audio bypass; deny the real
  prompt; the user sees an actionable error, the app is Idle, nothing is
  pasted, the clipboard is unchanged, and Settings and Quit still work.
  Grant it (System Settings in the VM) and a dictation works, with or
  without a relaunch; write down which in `docs/ENGINEERING.md`. Restore the
  granted state at the end so later scenarios aren't affected.

## Out of scope

- VM e2e in CI (#104). Onboarding (M5.2). Real Groq.

## Acceptance criteria

1. `[auto]` The new or extended VM scenarios pass in a full
   `tests/e2e/run.sh` on `dictation-vm`: `e2e_overlay_rendered_states_and_error`,
   the launch-at-login reboot scenario, `e2e_microphone_denied_then_recover`.
2. `[auto]` Each fails when its bug is put back (the Tester checks at least
   one mutation each: overlay listener not ready or wrong event name; disable
   reports Disabled but leaves the login item; the microphone denial error
   hidden or the app left Recording).
3. `[auto]` The golden image is untouched, and after
   `e2e_microphone_denied_then_recover` the VM has microphone access again
   (a later `e2e_real_mic` passes).
4. `[auto]` The 39 key-handling checks and `test-runner.sh` still pass.

Every milestone also needs: the gate green, coverage at or above thresholds,
`Tester: PASS` and `Reviewer: APPROVED` on the PR.
