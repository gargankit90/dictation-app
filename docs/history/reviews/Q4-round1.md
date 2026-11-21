# MQ4 review, round 1

PR: #337 at 1f15861456f4dc803f268ffa748758fced2dca80
Verdict: CHANGES REQUESTED

## Findings

| # | Severity | file:line | Finding | Suggested fix |
| - | -------- | --------- | ------- | ------------- |
| 1 | major | src-tauri/src/models.rs:279 | The automatic Gemma download uses an empty completion callback. The speech completion rebuild runs while Gemma is still missing, and nothing rebuilds when Gemma finishes. The normal first-install path therefore keeps pasting raw text despite showing cleanup Ready, until another settings change or restart. | On successful chained cleanup download, notify/rebuild the keeper using the current settings, just as the explicit download command does. Test the complete chained-download lifecycle, without an intervening settings save. |
| 2 | major | src-tauri/src/cleanup.rs:207 | Switching away or deleting a model only drops state.running; a helper inside starter().await remains owned by the detached start task. It continues loading for up to the 90 s startup timeout. Switching back starts another helper before the first finishes, potentially loading multiple 3 GB models concurrently on an 8 GB Mac. The generation check discards results only after startup completes. | Cancel the in-progress start when its generation is superseded, with one bounded startup task and process cleanup on cancellation. Test switching local → cloud → local while health never becomes ready, checking that the old PID exits before another helper loads. |
| 3 | major | crates/transcribe/tests/local_cleanup.rs:40 | The claimed criterion-2 cancelled-inference/next-job test is absent. Runner tests cover errors and process lifetime; the pipeline test waits on a fake pending future, then switches to cloud. Neither proves dropping the production HTTP request stops inference and frees the single slot for the next dictation. The real-helper smoke test only runs successful requests and stop. | Add a regression that cancels a deliberately long generation through the production runner and promptly completes another request on the same server; include a fake-runner test asserting cancellation and next-job readiness as required by criterion 2. |

## Checklist

1. Scope: findings 1 and 3. Cloud remains the default per the explicit owner decision; no migration added. OrukQwen size and SHA-256 match the dispatch. Its pending upload and revision pin remain a Lead/release dependency, acknowledged in PR notes.
2. Correctness: findings 1 and 2; finished-reply parsing, shared guard and raw fallback are reused. Cloud deadline/prompt defaults retain their previous values. Inference cancellation needs finding 3's evidence.
3. Security and privacy: reviewed loopback binding, random per-launch environment key, disabled redirects/proxy for loopback, disabled helper UI/logging, supervisor pipe and restricted new list command. No new transcript logging found. Startup lifetime issue is finding 2.
4. Architecture: OK for dependency direction, pure core and port reuse. Keeper coordination in the shell is consistent with the existing live-engine design. No unrelated migration or alternate cleaner introduced.
5. Rust and TypeScript practice: OK on inspection; asynchronous model verification uses the blocking pool and helper requests stay asynchronous. Existing poison recovery remains explicit.
6. Licences and reuse: OK on inspection of NOTICE and THIRD-PARTY-LICENSES additions for prompt provenance, llama.cpp and its bundled components, and model credits. No GPL-derived code identified.
7. Tests: finding 3. Reviewer ran `cargo nextest run -p dictation-transcribe llama --no-fail-fast`: 14 passed. The real-model smoke test and 16 GB/8 GB measurements were not run by this reviewer; Lead manual evidence remains required. CI was pending when inspected.
8. Git and PR: PR notes describe implementation and limitations. No guard edits. The milestone document was added in this diff, but records the supplied Lead scope; no independent spec change requested here. No minors raised. Do not merge this round.
