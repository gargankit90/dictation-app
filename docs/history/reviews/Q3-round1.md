# MQ3 review, round 1

PR: #338 at 1a6fcb22b87451fd8950a4448a435f1fcfefdc7f
Verdict: CHANGES REQUESTED

## Findings

| # | Severity | file:line | Finding | Suggested fix |
| - | -------- | --------- | ------- | ------------- |
| 1 | major | src-tauri/src/connect.rs:138 | Cancellation is dropped before the Keychain save, while the UI still offers Cancel/Done/Back as though waiting for the browser. A slow save can complete after cancellation; a failed save is hidden by the UI's blanket cancelled flag. | Expose a non-cancellable saving phase and block exit during it, or coordinate cancellation before committing the key. Test a gated slow/failing SecretStore. |
| 2 | major | src/onboarding/SpeechStep.tsx:318 | Acceptance criterion 4 is incomplete: native scenarios still select the removed radio label, wait for the old heading, and access the now-hidden paste field. No fake-provider Connect e2e or updated onboarding screenshots are included. | Tester updates all affected onboarding, Settings and DMG flows and captures screenshots; exercise Connect against the fake provider in the VM. |
| 3 | major | src/Onboarding.test.tsx:1125 | New explainer test races the initial saved-key lookup. The combined UI run failed here while the footer still said “Checking for a saved key…”; the isolated test passed. | Await the settled KEY_REASON text or settle the lookup in the helper before assertions. |

Inline threads: [1](https://github.com/gargankit90/dictation-app/pull/338#discussion_r4650348057), [2](https://github.com/gargankit90/dictation-app/pull/338#discussion_r4650348116), [3](https://github.com/gargankit90/dictation-app/pull/338#discussion_r4650348155).

## Checklist

1. Scope: finding 2. Primary features map to code and unit tests; criterion 4 remains incomplete. Real-account VM check belongs to the Lead.
2. Correctness: finding 1. Listener accepts only this attempt's state and callback path, exchanges once, and is dropped on completion/cancel/timeout. Requests are bounded and have a read deadline.
3. Security and privacy: OK in inspected paths. OS randomness supplies a 43-character verifier and independent 128-bit state; S256 matches the RFC vector. Sockets bind only 127.0.0.1 and ::1. Exchange errors discard response bodies; no key/code/verifier logging found. Key stays in Rust and Keychain; hint exposes only prefix and four characters. Disconnect uses existing bounded deletion and engine rebuild. Capabilities add only connection commands to the main window; no CSP expansion. Existing clipboard/audio handling unchanged.
4. Architecture: OK. Flow lives in transcribe; Tauri wraps it and reuses bounded secret writes. No migrations, new workspace dependency edges, unsafe code or OS-specific branches introduced. No unnecessary framework or balance/key-limit feature added.
5. Rust and TypeScript practice: finding 1. No new production panic/unwrap/expect or unexplained suppression found; typed IPC retained. Debug fake browser uses a raw background thread, reachable through the existing debug-only loopback provider override; release flag parsing ignores that override.
6. Licences and reuse: OK by inspection. No claimed copied code; new getrandom dependency is permissive and already in the lockfile. Full licence gate not rerun for this review.
7. Tests: findings 2 and 3. Targeted Rust tests pass; combined UI tests expose a timing-dependent failure. Coverage/full gate and native VM e2e are not attested by this review.
8. Git and PR: OK apart from outstanding validation. Conventional feature commits and detailed notes; no guard edits. Spec commit is the Lead's scope document. No minor findings requiring debt issues. Do not merge on this verdict.

## Validation and protocol reference

- `cargo nextest run -p dictation-transcribe connect::`: 15 passed.
- `cargo nextest run -p dictation-app connect::`: 8 passed, including fake-provider save and log capture.
- `npx vitest run src/Settings.test.tsx src/Onboarding.test.tsx src/Troubleshoot.test.tsx`: 198 passed, 1 failed (finding 3).
- Isolated `Q3: the explainer` test: passed, demonstrating the race rather than a missing product string.
- npm dependencies installed with `npm ci` before the UI run; initial attempt without dependencies could not load Vite.
- [OpenRouter OAuth documentation](https://openrouter.ai/docs/guides/overview/auth/oauth), checked 2026-11-20: confirms localhost callbacks on arbitrary ports, echoed state, key_label, S256 and the exchange body. Live account authorization remains a manual check.
- Overlay receives the shared OutOfCredits user text; tray maps it to AddCredits and credits URL; Troubleshoot invokes the same URL. Paste-key fallback retains validation/test/save and password input.

No merge performed. Product code and tests were not modified by the Reviewer.
