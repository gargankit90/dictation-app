# U11 review, round 1

PR: #300 at 463d873947f99e882ff9f1e364cbf59eba271c96
Verdict: CHANGES REQUESTED

## Findings

| # | Severity | file:line | Finding | Suggested fix |
| - | -------- | --------- | ------- | ------------- |
| 1 | major | src/onboarding/SpeechStep.tsx:181 | Continue with a saved key awaits a network test, but only the primary action observes `working`. Back remains enabled (the frame's busy/exitBlock do not include working), and the Provider select also remains enabled (keyBusy is null). Press Continue, then Back: the choices screen appears, but resolving the old test still calls useCloudEngine and onContinue, unexpectedly advancing to Permissions. Alternatively change Provider to OpenAI while Groq is testing: the old closure still selects Groq and advances. A failed old test can also be shown against the new provider. The keyRun cancellation used by Test & Save does not cover this path. | Treat Continue as a transaction: either disable Back, provider changes and edits until completion, or invalidate it on Back/provider/edit/unmount and verify the generation before switching engine and advancing. Add deferred-promise tests for both Back and provider change, asserting no stale engine switch, advancement or error. |

## Checklist

1. Scope: finding 1. Otherwise the Welcome, separate key screen, Try it copy and shared model naming meet the requested scope. Owner wording decisions accepted.
2. Correctness: finding 1. The rail stays on Speech for both pages. Back preserves language/choice while clearing unsaved secret input. Quit resumes at Speech with the persisted engine choice; a further Continue reopens Connect your key. U8 backend guards and U10 sharing queue/exit code are unchanged.
3. Security and privacy: OK on inspected paths. Saved secrets are queried only by existence; typed keys remain in the password input and command arguments, not explanatory DOM text or logs. No new network destination, telemetry, capability or transcript handling. Test & Save generation and committed-write protections remain present.
4. Architecture: OK. Frontend changes only; no new crate dependencies or platform code.
5. Rust and TypeScript practice: OK. No new unsafe code, suppressions or untyped IPC.
6. Licences and reuse: OK. Internal pill/form reuse, no dependencies or external copied implementation.
7. Tests: finding 1. All 447 Vitest tests passed locally after npm ci; initial run failed because dependencies were absent. Existing tests meaningfully check labels, deterministic levels, actual pill states, Reduce Motion timer cancellation, unmount cancellation, key gating/failure and unchanged Ready/Permissions expectations. They do not exercise pending saved-key Continue interactions. No VM e2e or Rust build was run by this reviewer. CI was pending when checked.
8. Git and PR: OK for product scope and detailed PR notes. The milestone file in this diff appears to be the supplied build spec; no guards are changed. This review does not authorize merge.

## Requested focused checks and observations

- Overlay: the page selectors were moved verbatim to overlay-window.css and imported only by overlay/main.tsx. All actual pill rules remain unchanged. Welcome uses OverlayContent and its stylesheet, not Overlay's event/microphone hook. Its animation frame is cancelled on cleanup; reduced motion schedules none and disables the caret animation. The demo-only outline/max-width do not affect the real overlay window.
- Display names: Speech and the download strip use modelTitle/modelLabel, whose language count derives from language_codes; Settings uses the same underlying title and size formatter. Ready still uses spokenName and says Parakeet English. For existing issue #299, I would preserve the explicit Ready-unchanged decision in this PR, then ask the Lead to approve changing Ready to `On this Mac · English · ✓ Ready` in a follow-up. This documented spec conflict is not another blocker.
- Hotkeys: long labels share the existing alias table, preserve side and cover combinations, function keys and arrow names without confusing Left/Right with modifiers. The tests omit individual LeftCtrl and RightShift examples, but those follow the same tested mapping; no distinct failure found.
- E2e: reviewed all changed scenarios. Expectations are updated to the new page sequence and saved-key label; existing provider request and downstream state checks remain. Layout adds real WebKit checks rather than replacing old ones. The new screenshot scenario is evidence capture and makes no claim to replace functional checks.
- Screenshots: 38 PNG files total about 7.5 MiB (du). Finite, window-only milestone evidence can reasonably live here, though PR attachments or external retained artifacts would avoid permanent clone growth for future rounds. This is a repository storage preference, not a correctness finding. The PR acknowledges missing default-size Hold frames; Design Lead must assess manual criterion 10 and the mock comparison.
- No additional blocker/major found in the remaining diff. No style-only changes requested, and no new minor issues opened.
