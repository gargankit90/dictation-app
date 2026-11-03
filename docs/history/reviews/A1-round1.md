Reviewer: CHANGES REQUESTED

# A1 review, round 1

PR: #220 at f1470be5d4ab0130b138b53a585ab63860b0900f

## Findings

### 1. Blocker — an extracted batch survives opt-out

`crates/telemetry/src/lib.rs:164` drains events into a local batch and releases the state lock before constructing and sending the request at lines 169–176. `set_enabled(false)` (lines 149–151) clears only the queue and ID, without invalidating this batch or cancelling the request. Concrete scenario: a full batch wakes the sender, which drains it; the settings task turns sharing off while the sender is descheduled or awaiting connection establishment; the sender subsequently sends those events with the old ID. This contradicts A1 criterion 5 and the immediate-stop promises in Settings, privacy.md and ENGINEERING.md. Events already delivered cannot be recalled, but batches not yet transmitted must not continue after opt-out.

Suggested fix: coordinate sending with an enable-generation/cancellation mechanism; invalidate extracted batches and cancel pending sends on disable, without holding a blocking mutex across network I/O. Cover opt-out after extraction/before transmission and during connection setup with deterministic synchronization, not just the existing disable-before-flush test.

### 2. Major — Save can silently undo a pending opt-out

`src/Settings.tsx:277` starts an asynchronous sharing save without setting a busy state or excluding a normal form save. While it awaits getSettings/saveSettings, `loaded` still contains `share_usage_events: true`; Save remains enabled. Concrete scenario: click the sharing checkbox off, then immediately click Save (or press Enter); the normal save builds its payload from that old `loaded` at line 226. If its write follows the opt-out write, it persists true and `settings_saved` re-enables analytics and records an event, although the user only asked to stop sharing. The toggle's eventual local update can also disagree with backend state. The existing later-Save test waits for the checkbox to become false and misses this interleaving.

Suggested fix: serialize all form/toggle writes using a shared synchronous in-flight guard and busy state, or give sharing a dedicated command and ensure ordinary saves cannot overwrite a newer sharing choice. Add a deferred-response UI test that starts opt-out, submits the form before it resolves, and verifies persisted sharing remains off.

No minor findings; no tech-debt issues opened.

## What I checked

- Scope: compared the whole PR diff, A1 criteria and PR notes. The analytics default and US destination match the owner decisions. The documentation changes are explicitly required by A1; no guard edits.
- Correctness: findings 1–2. Checked tracker phase/error handling, cancellation/reset, save integration, startup and shutdown (quit flush is bounded to two seconds).
- Security/privacy: finding 1. Traced all Event constructors and serialization through the adapter. Error reasons, transcripts, panic strings and Debug output are not converted into event properties; error_type is a closed enum. Provider/setting/onboarding values are enums; timings are buckets. Version sources are the package version and the local version marker. No new webview sender or SDK, no capability/CSP expansion. Random OS-generated UUID, no hardware ID; no $ip/$set, explicit false person profiles and geo-IP disable. Project-side IP discard remains an owner setup prerequisite, as documented. No-key builds queue/send no events or ID.
- Architecture: OK. Inward dependencies preserved; no new unsafe or target_os conditional. Network delivery runs on a runtime task; record does no network I/O. Queue is capped at 200; one timed request attempt per batch, failure drops queue, no unbounded retries.
- Rust/TypeScript practice: findings 1–2 for concurrency. Typed IPC preserved; no new production unwrap/expect/panic, no unexplained lint suppression.
- Licences/reuse: OK on inspection; no attributed external adaptation; dependencies reuse existing versions. Full dependency checks run through the push gate.
- Tests: existing port, adapter, tracker and planted-secret tests pass, but do not cover the two interleavings above. Ran `cargo test -p dictation-telemetry -p dictation-ports -p dictation-pipeline --lib --test usage_privacy`: 159 passed (123 pipeline, 2 privacy integration, 20 ports, 14 adapter).
- Git/PR: Conventional Commits and detailed criterion mapping present. No merge performed. Tester/e2e validation remains independent.
