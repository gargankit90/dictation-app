Reviewer: APPROVED

PR #217, L1 round 2; reviewed head 89e1e7b7a67d0c6a8a60d0e0c76b98b7dbd7c9c0.

## Findings

No open blockers or majors. All three round 1 majors are fixed; no new minors or tech-debt issues.

| Round 1 finding | Resolution | Evidence checked |
| --- | --- | --- |
| Major 1: same-size corrupt final model bypasses verification | Fixed in 85d513d and wired into loading in 72ecd3f | `crates/transcribe/src/models.rs` now hashes an existing final file before first use in a process, caching successful checks against size/mtime. Corruption removes the file; Download fetches it again. `src-tauri/src/engine.rs` uses `verified_path` before invoking the loader. New tests cover in-place same-size corruption, re-download, fresh-process verification and never invoking the loader on a damaged model. |
| Major 2: concurrent model downloads can exhaust disk | Fixed in 7080e0d | `src-tauri/src/models.rs` checks and inserts the running model under the same shared lock, rejecting a different model until completion/cancellation cleanup. The UI also disables other Download/Resume buttons and explains the limit. The regression test checks the second request is refused without an HTTP request or part file, and permits it after cancellation. |
| Major 3: old native model remains live during replacement load | Fixed in 72ecd3f | `Transcribers::update_staged` publishes ModelLoading and drops the previous engine outside the current-engine lock. `LocalModels::engine` then drops its cache handle before verification/loading. The shell drop-probe test asserts the old model drops before the second loader starts and checks ModelLoading is visible; unrelated/language saves still reuse the warm model. In-flight dictations intentionally retain their own handle. |

## Focused re-review checks

- Reviewed the three fixes and their regression tests, the B1 merge resolution against current `origin/main`, and the added Tester e2e scenario. Did not repeat unrelated round 1 review work.
- B1 behavior is retained: startup installs `retry_on_dictation`; keychain failures alone trigger bounded retry on the blocking pool, serialized with other rebuilds. Successful stored-key reads still rebuild a failed engine. Missing keys, working engines and pinned mocks do not gain retries. The retry and stored-key recovery builders now call local-aware `engine_for`; the regular rebuild uses staged replacement. Existing B1 regression tests remain present. No new engine/config reference cycle is introduced by the retry closure.
- Checked the new ModelLoading error's user message and IPC error mapping, the cancellation-to-next-download transition, hash error cleanup and the test-only verification seam (`trusting` remains behind `cfg(test)`). No new user-impacting regression found.
- Read `e2e_local_model.sh`, its suite registration, accessibility radio helper and fixture addition. It exercises Settings download/save and real local dictation into TextEdit, with conditional offline verification and latency logging. The documented lack of passwordless sudo/offline proof remains tracked in #222; I did not run VM tests or independently claim offline acceptance.
- Independently ran `cargo test -p dictation-transcribe --lib`: **59 passed, 0 failed**. Full required gate runs through the push hook; Tester/CI supply broader acceptance evidence. Existing follow-ups #218 (native crash isolation) and #219 (macOS transcribe CI coverage) remain outside this re-review's fixes.
- Only this review file is committed by the reviewer. No merge performed; Lead manual checks and Tester approval remain required before merge.
