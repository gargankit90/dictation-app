Reviewer: CHANGES REQUESTED

# U6 review, round 1

PR: #262 at a46d812fb0121334c0cd4539180d002bd2a5cd81
Verdict: CHANGES REQUESTED

## Findings

| # | Severity | file:line | Finding | Suggested fix |
| - | -------- | --------- | ------- | ------------- |
| 1 | major | src/settings/Speech.tsx:222 | An obsolete successful test still writes its key. Start Test & Save with key A while a key is already stored; edit the field while the provider request is pending (this increments keyRun and clears keyBusy), then press Remove key. Removal completes, but when A's test succeeds it unconditionally calls setApiKey and reinstates A. Alternatively test/save B before A returns: A replaces B after the UI said B was saved. The generation check at line 231 runs after the destructive write. | Check the generation immediately after the test resolves and before starting setApiKey; serialize writes/removal or keep them disabled while a save is in flight. Cover delayed test completion after removal and after a newer successful save. |
| 2 | major | src-tauri/src/preferences.rs:165 | Failed persistence leaves a different live local model from the stored selection. With local A active, choose downloaded B: preload_local uses update_staged and publishes B (engine.rs:448–452) before this write. If settings persistence then fails (e.g. disk full), write_change returns the error here without rebuilding A. engine::changed returns early on the failed file write, so no rebuild occurs there either. Settings still chooses A and reports A as Active because get_engine_status reports only Ready, while actual dictations use B, potentially an English-only model instead of the user's multilingual engine. | Restore the engine from stored settings on any post-preload write failure, or make preload and publication transactional. Add a local-to-local test injecting a failed settings write and assert both stored and live model remain A. |

## Checklist

1. Scope: reviewed against U6; criteria 7 and 7b integration remain the explicitly disclosed U5/U1 dependencies. History omission is allowed. Manual criterion 9 remains with Lead.
2. Correctness: findings 1–2. Per-control settings mutations read/write under the shared lock; language changes are rebuilt from stored settings. Local-to-local preloading also deliberately interrupts new dictations while loading, as disclosed in PR risks; criterion 4's strict previous-engine guarantee is not met for that transition.
3. Security and privacy: finding 1 affects removal semantics. Keys stay in Rust on reads; status replies contain only presence. New links are a closed enum of HTTPS destinations through the Opener port. New commands are restricted to main, not overlay; CSP is unchanged. Mic test starts on request, discards on stop and has a seven-second backend limit. Update metadata reads do not initiate checks; existing launch checks are unchanged. Clipboard and telemetry payloads unchanged.
4. Architecture: OK; no added workspace dependency, unsafe, or platform cfg. Mic test uses the AudioSource port.
5. Rust and TypeScript practice: OK in changed production paths; S1 asynchronous launch-at-login command reused, typed IPC used, U3 colour tokens used. Blocking engine/key work stays on the blocking pool.
6. Licences and reuse: OK; no dependency or external copied implementation added; U4 meter reused internally.
7. Tests: targeted Vitest run passed 56/56 (Settings, App, modelRow); existing tests do not cover the two adverse interleavings/error paths above. Initial run lacked local npm dependencies; npm ci installed them and the rerun passed. VM/manual checks left to Tester/Lead. Full gate runs via mandatory pre-push hook.
8. Git and PR: detailed PR notes and test mappings reviewed. No guard edits. Milestone spec is the supplied Design Lead document. No minor findings raised.

## Disposition

0 blockers, 2 majors, 0 minors. Do not merge until findings are fixed and re-reviewed, pending integration is complete, and Tester/Lead evidence is present.
