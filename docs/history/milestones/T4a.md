# T4a: native journeys with a local provider: key recovery, cancel, focus, Unicode, quit

## Goal

Catch, in the real signed app, the bugs a user would see when a request is
in flight or the key is wrong: fixing the key in Settings doesn't bring
dictation back, Esc or an extra press during a request starts a second
dictation, the text lands in the wrong window, emoji or newlines get damaged,
or quitting mid-request hangs or pastes later. From the follow-up test audit
(#103, T4 group), part 1. Part 2 (T4b: #119 overlay rendered states, #123
login-item disable across reboots, #126 microphone denial) follows.

## Scope

- **Local provider fixture and test-build seam (needed by everything
  below).** A loopback HTTP provider in the VM that speaks the
  OpenAI-compatible transcription API and can be told to reject a key, to
  hold a reply until released (a "gate"), and to count requests. A
  test-build-only endpoint override so the signed test build talks to it
  (not compiled into release builds; a test proves the release build ignores
  it). Reuse T2's VM helpers (`tests/e2e/lib_vm.sh`, `lib_full.sh`); they must
  keep propagating guest failures.
- **#114 `e2e_settings_key_provider_recovery`.** Start with a key the
  provider rejects: dictating shows the bad-key error and pastes nothing.
  Enter a good key in the real Settings window, save. Without a relaunch the
  next dictation pastes the transcript; after a relaunch too. The key is
  read from the keychain, never from a file or log (grep the logs for it).
- **#116 `e2e_cancel_and_busy_presses`.** Hotkey down, Recording, Esc,
  release: Idle, no request, no paste, clipboard unchanged; a fresh hold
  then works. With a gated reply pending: an extra press/release and Esc
  don't queue a second dictation; releasing the reply gives exactly the
  original paste; the next hold works.
- **#117 `e2e_focus_switch_during_transcription`.** Two harmless TextEdit
  documents A and B. While the gated reply is pending, switch focus from A
  to B, release the reply: B gets exactly one transcript and A is
  byte-for-byte unchanged (today's policy: the app with focus at paste time
  gets the text). Write that policy down in `docs/ENGINEERING.md` if it isn't
  already.
- **#118 `e2e_unicode_multiline_selection_paste`.** Fixture transcripts with
  emoji (non-BMP), combining marks, RTL text and embedded newlines; insert at
  a caret and replace a selected range inside existing text. Compare the
  exact document text (not trimmed), read back encoded (JSON or base64) so
  whitespace can't hide.
- **#115 quit during a real request.** Unit/integration:
  `pipeline_shutdown_closes_real_http_without_retry_or_insert`: with a raw
  local server that has received the request, call the real shutdown path;
  the connection closes, there's no retry and no insert. VM:
  `e2e_quit_during_request`: Quit from the tray while the gated reply is
  pending; the app exits within a bound, the provider saw one request, and
  releasing the reply afterwards pastes nothing.

## Out of scope

- #119, #123, #126 (T4b). VM e2e in CI (#104). Real Groq.

## Acceptance criteria

1. `[auto]` The endpoint override exists only in test builds: a test fails
   if a release build honours it.
2. `[auto]` `pipeline_shutdown_closes_real_http_without_retry_or_insert`
   passes, and fails when shutdown doesn't cancel the job.
3. `[auto]` The new VM scenarios pass in a full `tests/e2e/run.sh` on
   `dictation-vm`: `e2e_settings_key_provider_recovery`,
   `e2e_cancel_and_busy_presses`, `e2e_focus_switch_during_transcription`,
   `e2e_unicode_multiline_selection_paste`, `e2e_quit_during_request`.
4. `[auto]` Each new scenario fails when its bug is put back (the Tester
   checks at least one mutation per scenario, e.g. no engine rebuild on
   Settings save; busy press queued; text trimmed; shutdown not cancelling).
5. `[auto]` The 39 key-handling checks and `test-runner.sh` still pass; no
   key appears in any log or report.

Every milestone also needs: the gate green, coverage at or above thresholds,
`Tester: PASS` and `Reviewer: APPROVED` on the PR.
