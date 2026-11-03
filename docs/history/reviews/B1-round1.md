Reviewer: APPROVED

# B1 review, round 1

PR: #205 at 2a4cd8b2282ea063d276358b3777e9a3f4becf94.

## Findings

No blocker, major, or minor findings. No follow-up issues required.

## What I checked

- Scope: OK. Read B1's six acceptance criteria, issue #204, PR notes, and the whole seven-file diff. Criteria 1–3 map to the engine/pipeline fake-keychain tests; criterion 4 maps to onboarding Vitest tests; criterion 5 is covered by inspection and the gate. Criterion 6 remains a real prompt-flow check for the Tester/Lead, as disclosed in the PR.
- Correctness: OK. Only `Unavailable(KeychainUnavailable)` retries; missing keys, bad settings, startup pending, and pinned engines do not. Retry is once per transcription attempt, with the existing timeout and shared KeyReads. Successful stored reads heal via the same guarded rebuild. Both transcription entry points use the retry. The update mutex serializes builds with settings/key rebuilds, rechecks eligibility after acquiring the lock, and retains an Arc for each active transcription. Existing save invalidation prevents the subsequent save rebuild from accepting an older pending read. A failed retry preserves the keychain error.
- Security and privacy: OK. Added logs contain only engine names. No added key/transcript logging, secret IPC replies, audio persistence, endpoints, analytics, clipboard behavior, capability or CSP changes.
- Architecture: OK. OS/keychain work stays in the shell behind existing Config/SecretStore wiring; transcribe receives a callback and knows no Tauri types. No new crate dependency, target-specific code, unsafe code, or core I/O.
- Rust and TypeScript practice: OK. Blocking rebuilds run on spawn_blocking for dictation and already-blocking command paths for stored reads. The current-engine lock is not held across network awaits. No new panic/unwrap/expect, suppressions, or untyped IPC. The onboarding status is set only after a successful API test; failure returns before setting it.
- Licences and reuse: OK. No copied code or new packages; tokio only adds its runtime feature.
- Tests: OK. Ran `cargo nextest run -p dictation-app engine::`: 28 passed, including timeout/error recovery, stored-read recovery, read-count bounds, no-key/pinned paths, stale-read/save behavior, and in-flight request shutdown. Ran `npx --no-install vitest run src/Onboarding.test.tsx` after `npm ci`: 21 passed, including saved-key and typed-key confirmation. An initial frontend test invocation failed because this fresh worktree lacked installed dependencies; the rerun passed. Full gate also runs through the push hook; CI remains the required merge gate.
- Git and PR: OK. Small Conventional Commits and detailed PR notes. The milestone was added in a separate documentation commit; no guard changes. Review changes are confined to this file. No merge performed.

## Remaining validation

The fake-keychain tests prove recovery through the real pipeline with a local HTTP provider; they do not exercise a real macOS access prompt. The Tester/Lead must record criterion 6 and required CI results before merge.
