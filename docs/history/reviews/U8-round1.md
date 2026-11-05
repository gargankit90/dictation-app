Reviewer: CHANGES REQUESTED

# U8 review, round 1

PR: #269 at 67795bd2af320b262b7c58b9af8419c8f36faac0
Verdict: CHANGES REQUESTED

## Findings

| # | Severity | file:line | Finding | Suggested fix |
| - | -------- | --------- | ------- | ------------- |
| 1 | blocker | src/onboarding/WelcomeStep.tsx:50 | Continue remains enabled while the sharing command is pending. A user can switch sharing off and immediately Continue: `set_share_usage_events` runs on the blocking pool, whereas `set_onboarding_step` calls `left_welcome` before its blocking work. If the latter wins, PostHog/Sentry are enabled and events can leave before opt-out executes. The existing UI test waits for the save before clicking Continue, and the fake-port tests only exercise sequential calls. | Block departure until the latest sharing write succeeds, keep departure blocked on a failed opt-out, and serialize rapid switch writes. Test Continue with a deferred opt-out and assert that no event or adapter enablement occurs. |
| 2 | major | src/onboarding/logic.ts:93 | Download provenance is inferred from the current byte count rather than the HTTP response. If a server ignores Range and starts over, once its new download exceeds the old partial size the strip changes to “Continued from”; a fast download may already exceed it at the first one-second poll. `models.rs` explicitly handles HTTP 200 by truncating the part, but exposes no retained restart fact to this UI. This violates criterion 7's honest restart wording. | Expose and retain the accepted resume offset or restart outcome from the downloader; render that fact throughout the download. Test HTTP 200 after a partial with progress that has already overtaken the old offset. |
| 3 | major | src/onboarding/SpeechStep.tsx:65 | The engine radio choice is component state only. On a fresh install choose “Use my own Groq or OpenAI key”, then quit while still on Speech: reopening defaults back to local because no saved key exists. Conversely choosing local while an old cloud key exists reopens on the key path. Language and committed engine choices persist, but this visible choice does not, contrary to the spec's kept choices requirement. | Persist the non-secret onboarding engine selection as it changes and restore it independently of whether a key was saved; add quit/reopen tests before pressing Continue. Never persist unsaved key text. |
| 4 | major | src/Onboarding.test.tsx:342 | The pseudo-long layout test only checks that the title has an `.ob-content` ancestor. It passes with clipped or overlapping controls and cannot prove the required 680×520 layout. The PR explicitly says no rendered pseudo-long check ran, so criterion 1 remains unverified. | Add a rendered browser/VM check with pseudo-long labels at the minimum size that verifies the primary action, reason and scrolling content remain usable; keep the lightweight structural tests as supplementary checks. |

## Checklist

1. Scope: findings 2–4; five steps and the other specified surfaces are present. History is correctly omitted because U7 is absent.
2. Correctness: findings 1–3. Setup step and try/download-pause progress are persisted; permission regression intentionally brings Permissions back. Persistence write failures are logged rather than surfaced.
3. Security and privacy: finding 1. Closed event properties contain enums, buckets and versions, not text, keys or app names. PostHog and Sentry start disabled before Welcome; sequential fake-port opt-out cases are covered. Practice words stay in memory and are separate from event payloads. Capabilities remain scoped to the main window; no added destination or CSP relaxation found.
4. Architecture: OK; core state remains pure, adapters retain OS/network responsibilities, and generated IPC covers the new commands.
5. Rust and TypeScript practice: OK on inspection; no new unsafe or production panic found. Full gate result recorded below.
6. Licences and reuse: OK on inspection. OwnKeyPanel/useOwnKey, LanguagePicker, HotkeyRecorder, MicMeter and A1's sharing command are reused. Onboarding adds a different model selection presentation using the existing catalogue/download commands. No new dependency or copied external implementation found.
7. Tests: findings 1–4. Real `say` local-model scenario, paste-miss recovery and fake-port tests are useful; no removed security assertion identified. VM scenarios were read, not executed by this reviewer.
8. Git and PR: PR notes are detailed and disclose limitations. No guard changes. The milestone file was added on this branch as the supplied review target; authorship/approval of the spec is the Lead's responsibility.

## Launch paths and event documentation

- Fresh nonscripted installs now open Welcome regardless of existing permissions; completed legacy settings without a progress file migrate as already set up. Settings is loaded before any new settings write determines that legacy status.
- DMG mock launches and provider launches remain scripted through the existing flags. Settings can still be opened from the tray. Launch-at-login's first test launch is scripted, but its reboot launch is not: unfinished setup can now appear there and must be checked by the Tester for focus interference. Update fixtures with existing settings retain the legacy migration path. These three VM scenario groups were not run by this reviewer, so there is no runtime PASS claim for them.
- There is no docs/analytics directory. docs/privacy.md documents the five new step values and completion at ready; the closed enum's serialization tests match. PostHog serializes the enum payload and Sentry consumes error reports rather than onboarding-step values, so no adapter schema mismatch was found. Dashboard changes remain an operational follow-up noted in the PR.

## Validation

Focused frontend validation: 69 tests passed across src/Onboarding.test.tsx and src/onboarding/logic.test.ts after npm ci. The full quality gate is being run locally and runs again through the required push hook; its final result will accompany the PR comment. VM e2e was not run by this reviewer. Lead manual checks and Tester evidence are still required before merge.
