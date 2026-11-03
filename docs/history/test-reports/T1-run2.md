# T1 test report, run 2

PR: #107 at ddbbf23
Result: PASS

Re-check of what changed since run 1 (37489c0): Codex M1 fix, #109, merge of main.

| Check | Result | Notes |
| --- | --- | --- |
| Overflow-only mutant (skip `AudioCommand::Discard` on `DiscardRecording`, local, reverted) | PASS | `overflow_outcome` and `m2_3_edges` criterion 7 both fail ("overflow never discarded the recording"); restored, tree clean |
| Redirect advice ("Check the provider selected in Settings") | PASS | reads well; pinned by exact-message tests in `ports/transcribe.rs` and `src-tauri/commands.rs` |
| Flakiness: changed tests x20 | PASS | 20/20 runs, 4/4 passed each |
| ports + pipeline tests | PASS | 71/71 |
| Gate (`scripts/gate.sh`) | PASS | all checks passed |

No VM used. No product code committed.
