Reviewer: CHANGES REQUESTED

# K1 review, round 1

PR: #231 at d40c434
Verdict: CHANGES REQUESTED

## Findings

| # | Severity | file:line | Finding | Suggested fix |
| - | -------- | --------- | ------- | ------------- |
| 1 | blocker | src/HotkeyRecorder.tsx:40 | Key selection leaves the global listener live. With the default RightAlt, click Record, hold Right Option for more than 200 ms, then release: the native listener sends Pressed/Released and the coordinator starts recording and transcribes on release. Saving the same spec is Unchanged, so it cannot cancel that dictation. The pipeline remains busy while transcription/insertion runs, can send unintended audio to the provider, and can paste into a subsequently focused Settings field. DOM preventDefault cannot stop the listen-only native tap. | Pause delivery of dictation hotkey events before accepting recorder input, cancel any hold safely, and restore delivery on success, refusal/cancel, blur, unmount and errors. Add a pipeline/UI integration test proving selecting the current key produces no audio capture, transcription or paste, then a normal hold works after selection ends. |
| 2 | major | src/Settings.tsx:197 | The recorder's save never sets the parent saving flag; only its own buttons become busy. While saveHotkey is awaiting IPC, the form's Save button remains enabled and save() captures the old hotkey and settings. A delayed recorder save can overwrite a newer form save's language/provider, or a form save can restore the previous hotkey immediately after the recorder reports success. Rust serializes individual saves, not this read/save/reread transaction. | Share an in-flight save guard between recorder and form, disable/guard form submission until recorder persistence and reread finish, and prevent starting either operation during the other. Test deferred IPC replies with both operation orders and verify neither the new key nor other saved fields are lost. |
| 3 | major | src/HotkeyRecorder.tsx:48 | The handler discards modifier flags and only tracks keydowns observed since recording began. Holding Shift before clicking Record and then pressing F5 saves F5 rather than Shift+F5. The same issue happens after refusal: Ctrl down, unsupported CapsLock down, then K while Ctrl stays down is refused as a plain K because refused() erased the held Ctrl; Shift down, CapsLock, F5 silently saves F5. These outcomes do not match the combination actually pressed. | Reconcile current held modifiers from KeyboardEvent flags for non-modifier combinations (sides are only needed for modifier-only specs), or require a full release before accepting another attempt with a clear message. Preserve/reconcile physical held state across refusals and add tests using actual modifier flags, including already-held modifiers. |

## Checklist

1. Scope: recorder, Settings and onboarding match K1 scope; criteria 1–3 need findings 1–3 fixed. Manual criterion 6 remains with the Lead/Tester.
2. Correctness: findings 1–3. No evidence of a permanently stuck pipeline: an ordinary release transcribes, interruption cancels a hold, and transcription/insertion have their existing completion paths.
3. Security and privacy: finding 1 causes unintended capture; no new logging, destinations, capabilities or telemetry fields in this diff.
4. Architecture: OK; existing Rust save/switch logic is reused.
5. Rust and TypeScript practice: OK apart from shared save ownership in finding 2.
6. Licences and reuse: OK; no new dependency or copied-code evidence.
7. Tests: 117 tests passed across hotkeyKeys, HotkeyRecorder, Settings and Onboarding after npm ci. Existing tests do not exercise the native listener during selection, concurrent saves or modifier flags. Pure recorder reproduction confirms Ctrl is lost after a refusal. The Rust parser accepts the recorder vocabulary, including sided modifiers, F1–F20 and arrows; the new Rust test uses representative canonical examples.
8. Git and PR: Conventional Commits and detailed PR notes present. No guard changes. The spec was committed separately before implementation. No minor findings or tech-debt issues raised. Do not merge.

## Review limits and user-visible checks

Code uses physical KeyboardEvent.code, so Shift changing printable names does not change the stored key; physical letter/digit codes align with the native adapter's fixed macOS keycode mapping, including non-US layouts. Fn/Globe, numpad and other unmapped codes are refused, and Escape cancels. Blur cancels, listeners clean up, and status/error messages use live roles; no additional keyboard trap finding beyond deliberate key capture with Escape to exit. Existing Rust switch/write rollback retains the previous hotkey on ordinary failures and reports undo failures; this PR displays the returned error and rereads active settings.

No VM/computer-use check was performed in this reviewer role. Actual WKWebView MetaRight/AltRight delivery, screen-reader announcements, OS menu accelerators and the manual record/dictate/relaunch scenario still need VM evidence; synthetic DOM tests alone cannot establish those behaviors.
