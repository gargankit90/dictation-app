Reviewer: CHANGES REQUESTED

# U4 review, round 1

PR: #251 at 4bd9ddf6dade9f4c779eeb9b86cd8cca915ad837
Verdict: CHANGES REQUESTED

## Findings

| # | Severity | file:line | Finding | Suggested fix |
| - | -------- | --------- | ------- | ------------- |
| 1 | blocker | crates/app/src/overlay.rs:131 | Inserting → Idle proves only that the inserter returned success, not that text landed. The PR records two VM runs where TextEdit stayed empty while the pill said Pasted. This new confirmation overclaims and violates AC5; P1 #241 does not add delivery confirmation. | Agree honest wording such as “Paste sent” with the Lead, or show Pasted only with confirmed delivery. Preserve an empty-target regression rather than logging the failure as a successful pill case. |
| 2 | major | src-tauri/src/overlay.rs:166 | The watch bounds only the pipeline-to-overlay-model hop. Each emit_to enqueues another script without waiting for the page; a slow main thread/webview accumulates stale readings and stop/hide work can sit behind them. | Bound in-flight delivery at the dispatch boundary, retain only the latest pending level, invalidate pending levels on stop, and test a stalled consumer and stop/restart. Keep overlay capabilities minimal. |

Inline findings: [1](https://github.com/gargankit90/dictation-app/pull/251#discussion_r4464614816), [2](https://github.com/gargankit90/dictation-app/pull/251#discussion_r4464614905).

Finding 2 was checked against the installed dependency source: Tauri 2.12.5 `Webview::emit_js` calls `eval`; tauri-runtime-wry 2.12.3 `eval_script` queues `WebviewMessage::EvaluateScript` via `send_user_message`/the event-loop proxy. The Rust watch cannot coalesce scripts already handed to that queue. A five-minute recording can produce about 6,000 scripts. This is a source-level finding, not a measured VM memory regression.

## Checklist

1. Scope: findings 1 and 2. AC1–4 and 6–7 map to implementation and tests; AC5's success condition is insufficient. The milestone copy and e2e edits are explicitly documented as Lead-authorized. U1 supplies the eventual one-sentence error copy; U5 supplies its hint.
2. Correctness: findings 1 and 2. Recording exit clears the pipeline watch; source stop/discard ends its meter. The silence watch uses monotonic elapsed time, resets on sound/new recording, and treats a muted source as the floor. The documented −50 dBFS threshold matches the transcription gate; it is a heuristic, not a guarantee that all quiet speakers cross it.
3. Security and privacy: OK. Levels are absent from UiEvent/telemetry, logs and persistence. Device names are sent only to the overlay. No added audio disk path outside the existing opt-in debug wrapper, no network destination, secret exposure, capability or CSP expansion.
4. Architecture: OK. Dependencies remain inward, pure level maths stays in core, platform owns OS code, shell bridges the overlay. LevelMeter's shared synchronization belongs in ports alongside existing shared handles.
5. Rust and TypeScript practice: finding 2. Collector runs on handy-recorder's delivery thread (confirmed in its Sink contract), not the real-time capture callback. Meter accumulation is allocation-free per chunk; the short mutex section shares aggregate state with the reader. Device cloning occurs on the pipeline reader. Typed payloads and parser tests agree; no new panic path or unexplained suppression found.
6. Licences and reuse: OK. No new dependency or copied implementation identified.
7. Tests: findings 1 and 2. Existing model tests cover returned insertion errors, dropped transcripts, timer expiry and new recordings; they do not establish target delivery or bounded downstream dispatch. UI tests cover seven bars, hidden meter accessibility and reduced-motion CSS. Real microphone, VoiceOver announcement and focus behavior remain VM/manual checks; no owner-screen interaction performed.
8. Git and PR: OK. Conventional commits and detailed criterion mapping/risk disclosure. No guard edits. No minor findings, so no tech-debt issues needed. No merge performed.

## Validation

- `npm ci` completed; `npx vitest run src/overlay`: 55 passed across 2 files.
- `cargo test -p dictation-core -p dictation-ports -p dictation-pipeline`: passed, including integration and doc tests (160 core, 140 pipeline and 27 ports unit tests).
- `cargo test -p dictation-platform u4`: 8 passed, including 48 kHz resampling, failed start and recording exit.
- PR-head CI: Quality gate (Linux), Platform crate (macOS) and DMG (macOS) passed. Local workspace clippy and formatting passed; full gate also runs through the mandatory push hook.
- Inspected all changed production paths and e2e changes, including the removed target-text assertion. No custom harness or product/test edits made.
- Existing VM evidence is the Developer's, not independently rerun here. Lead still owns AC8 and actual VoiceOver validation; status roles/aria-hidden and lack of focusable children are present, and native non-activating panel construction is unchanged.
