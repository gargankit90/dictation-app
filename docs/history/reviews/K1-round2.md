Reviewer: CHANGES REQUESTED

# K1 review, round 2

PR: #231 at 9f6d636
Verdict: CHANGES REQUESTED

## Re-check of round 1

- Finding 1 (global dictation during key selection): improved, but still open through the form/recorder interaction below. The native delivery pause drops presses, releases, Escape and shortcuts, interrupts an active hold, and swallows its later release. A normal completed selection now resumes correctly.
- Finding 2 (competing persistence operations): fixed for the original race. Recorded-key persistence sets the parent saving flag, and both Rust writers hold SETTINGS_WRITE across read/write. set_hotkey rereads stored settings under that lock; save_settings preserves the stored key. The tests cover delayed key persistence and a stale form save. The separate listening state still needs the new fix below.
- Finding 3 (held modifiers): fixed. Non-modifier combinations use event flags, refusals preserve held state, and releasing modifiers after a refused non-modifier does not accidentally save a modifier-only key. New tests cover already-held Shift and retry cases.

## Findings

| # | Severity | file:line | Finding | Suggested fix |
| - | -------- | --------- | ------- | ------------- |
| 1 | blocker | src/Settings.tsx:223; src-tauri/src/commands.rs:353 | The form remains enabled while the recorder is listening (before it has selected a key). Type F6 into "Or type it", click Record, then mouse-click Save. save() permits this because neither saving nor testing is true; it calls set_hotkey(F6), which unconditionally resumes native delivery. The recorder still shows "Press a key…" and continues listening, but the global F6 now dictates until the next 10-second renewal. Holding F6 at this point can start capture/transcription during key selection, reopening round 1's blocker. This also permits a normal save to start first and the recorder to consume a key while saving, only to reject the change as busy. | Track the recorder's listening state in the parent and disable/guard form submission and conflicting actions while it is active, or cancel and finish the recorder session before any form save. Keep pause ownership consistent so an unrelated set_hotkey cannot resume a live recorder session. Add a UI test for a pretyped key, Record, mouse Save, then holding that key; verify no form save occurs while listening, no dictation occurs, and normal dictation works after cancellation. |
| 2 | major | tests/e2e/scenarios/e2e_hotkey_recorder.sh:68 | The new current-key regression check cancels its own recorder. textedit_fresh explicitly activates TextEdit and waits for it to be frontmost; HotkeyRecorder cancels on window blur. Thus the later post_key_hold brings the app back with recording already ended, so this is an ordinary hotkey hold rather than a hold during key selection. With the fixed cleanup it can legitimately produce Recording/Transcribing phases and fail the assertion. The old ready log at line 76 does not establish current Idle state. | Prepare the TextEdit document before starting the recorder, then activate Settings, start recording and post the hold without intervening focus loss. Assert the recorder is active immediately before keydown within the same Terminal job (the scenario already explains why job boundaries matter). Assert no new phases/insertions and then prove normal dictation resumes. Have the Tester run this amended scenario on the current head; the run-1 report is for d40c434 and is not evidence for this new check. |

## Pause lifecycle and validation

The effect sends resume and clears renewal on Escape, Cancel, successful selection, blur, visibility hidden and unmount. A vanished webview has a bounded 30-second lease rather than an indefinite pause; app exit drops process state. Repeated native pause renews the lease and repeated resume is idempotent. set_hotkey resumes after both success and ordinary error results, including failures reading or writing settings. No permanently paused path was found in these changes. The UI's pause IPC is fire-and-forget, so no end-to-end evidence of pause acknowledgment is provided by the DOM tests.

129 tests passed across hotkeyKeys, HotkeyRecorder, Settings and Onboarding. Reviewed native pause integration tests for normal resume, interruption of an existing hold, release after resume and lease expiry; the lease test uses real time. No VM/computer-use run was performed in this reviewer role. The round-1 Rust/parser and unrelated checklist conclusions remain unchanged; this round checked only the fixes, their tests and resulting interactions. No minor findings; no tech-debt issues created. Do not merge; leave the original blocker thread open until finding 1 is fixed.
