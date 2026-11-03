Reviewer: APPROVED

PR: #160, round 1, reviewed at `0c138c8ea9b5eea3bb09aa5956fc1d43965f2ce4`.

## Findings by severity

- Blocker: 0.
- Major: 0.
- New minor: 0. No new tech-debt issues or inline finding comments required.

## Product behaviour

**Overlay handshake** (`src-tauri/src/overlay.rs:50`, `:129`, `:138`; `src/overlay/Overlay.tsx:24`; `src/overlay/view.ts:29`): the page announces only after its listener resolves, and an unmounted subscription does not announce. The relay stores the latest view and sends/replays it under the same mutex. A late or duplicate title reads the current view, rather than retaining the state at subscription time. Hidden replaces an expired error in that same relay; replay neither shows the native window nor restarts the error timer. A reload registers a fresh listener and changes the document title from the HTML's initial title, so the per-document counter restarting is safe. I found no path leaving a stale error or blank overlay after a successful subscription and replay.

Checked the installed Tauri 2.12.5 / runtime-wry 2.12.3 sources as well: emission targets registered JS listeners; non-main-thread evaluation posts to the event loop and does not synchronously wait for it in the enabled configuration. Thus the new mutex does not introduce the main-thread/overlay-task wait cycle suggested by holding a lock around emission. Main-thread evaluation can run inline, but replay carries the latest stored view and the queued state emissions still end on the latest view.

**Security of the title** (`crates/platform/src/overlay/macos.rs:84`, `other.rs:36`; `src-tauri/src/overlay.rs:138`): only this overlay's callback receives the signal. The broad prefix accepts additional suffixes, but every accepted signal has the same bounded effect: resend the app's current overlay payload to the overlay itself. It accepts no command, view, window label, key or transcript from the page, changes no native visibility, and grants no Tauri permission. Remote content receives no listening capability; CSP and capabilities are unchanged. A title alone does not register an event listener or add authority. No concrete new remote-origin exploit found.

**Microphone text** (`crates/core/src/error.rs:82`): the shortened instruction names the settings destination and the required reopen after denial, consistent with the documented macOS observation. The Tester reports the whole message visible on two lines in the screenshot; AX text alone would not establish absence of clipping. While the first macOS prompt is still open, the expanded instruction can incorrectly direct someone to Settings/reopening even though Allow in the prompt suffices. This is the explicitly tracked [#161](https://github.com/gargankit90/dictation-app/issues/161), not a new finding; this PR changes the wording but does not change the permission state or prompt timing. Resolve there by distinguishing undetermined permission from an actual denial.

## Journey coverage

- **Overlay:** `tests/e2e/fixtures/ax.js:389` walks the real overlay window's AXStaticText, rather than reporting the intended event payload. The scenario holds the provider response to prove Transcribing, checks actual paste and native on/off-screen state, samples error expiry, and holds a replacement recording beyond the old error deadline. The cold-start handshake mutation is reported to fail the rendered Recording assertion.
- **Login item:** `tests/e2e/scenarios/e2e_launch_at_login.sh:136` uses the real checkbox and Save, reopens Settings and reads OS state. Both reboots verify changed boot time. The enabled boot checks a single instance, startup log, real hotkey/Esc behaviour and unchanged paste/clipboard; the disabled boot checks a live GUI session, no process and no log for at least 60 seconds, then proves manual launch still works. The retained-login-item mutation fails, including the second-boot check.
- **Microphone:** `tests/e2e/scenarios/e2e_microphone_denied_then_recover.sh:167` requires the actual denial callback and disabled OS switch, then checks rendered error, tray, Idle, unchanged clipboard, no paste, Settings and Quit. Recovery uses the real microphone with no file-audio bypass; silence is accepted only with the expected phase sequence and silence verdict. The cleanup is armed before reset, runs after quitting the app, and resets/grants/relaunches/verifies recording if the normal path did not verify the grant. Cleanup failure fails the scenario. The Tester reports both the injected denial failure followed by successful restoration and a subsequent passing `e2e_real_mic`.
- **Golden image:** reset explicitly accepts only the two clone names; the existing VM helper validates the same allowlist before VM operations. No added path starts or changes `dictation-vm-golden`. The Tester separately records unchanged golden disk/NVRAM mtimes.

## Checklist

| Section | Assessment |
| --- | --- |
| 1. Scope | OK: all three T4b journeys and the two product fixes they exposed; acceptance criteria map to the scenarios and the test report. |
| 2. Correctness | OK: late/repeated listening, reload, expiry/replacement, denial and recovery reviewed as above. |
| 3. Security and privacy | OK: unchanged capabilities/CSP, no new network destinations or secret/transcript logging, clone-only permission reset; clipboard checks retained. |
| 4. Architecture | OK: native title callbacks remain in platform adapters; relay is shell wiring; no new crate dependencies, OS cfg or unsafe outside platform. |
| 5. Rust and TypeScript | OK: typed overlay payload unchanged, guarded listener lifecycle, poisoned mutex recovery, no new production panic/unwrap or unexplained suppression. |
| 6. Licences and reuse | OK: no new dependencies, vendored changes or apparent external code adaptation. |
| 7. Tests | OK: local checks below; native journeys and mutations independently reported by Tester. Coverage enforced by the successful CI gate. |
| 8. Git and PR | OK: scoped Conventional Commits, detailed PR notes, milestone present, no guard edits, existing #161 linked. No merge performed. |

## Validation and limits

- `cargo nextest run -p dictation-core -p dictation-pipeline -p dictation-app`: 561 passed, 1 skipped, including relay, permission/error, overlay expiry and ACL tests.
- `npx vitest run src/overlay`: 26 passed across 2 files, including capability constraints and listener lifecycle.
- `bash tests/e2e/test-key-handling.sh`: 39 passed.
- `bash tests/e2e/test-runner.sh`: 67 passed.
- Shell syntax checks for the three scenarios and `git diff --check`: passed.
- PR CI at the reviewed head: quality gate, platform crate and DMG checks successful. The full gate/coverage is not independently repeated here; the required push hook runs normally for this report.
- Read the entire PR diff, milestone, PR notes, engineering rules, review checklist, `test-reports/T4b-run1.md`, Tester PASS comment and #161. VM journeys/mutations, screenshot clipping assessment and golden mtimes rely on that Tester evidence and the Developer's documented full run. I did not use a VM or drive the owner's screen.
