# MQ4 review, round 4

PR: #337 at 5ac2ce7ad65f2ce1d60042747856c3a998093160
Verdict: APPROVED

Re-check: shutdown fix fully; owner UI changes, Tester choice-persistence fix and Q3 integration briefly. No merge.

## Findings

No open blocker or major findings. No new minors raised.

Finding 2 is fixed: `wait_ready` reports only an outcome; the select decides ready/error/timeout/cancel without taking the process handle. Every non-ready result then calls one `shutdown_and_reap` outside the select. A cancellation arriving during timeout shutdown cannot discard that shutdown or bypass reaping. Startup serialization remains held until the result returns. The new short-limit test exercises cancellation during a SIGTERM-resistant helper's shutdown and requires the process already gone on return.

Findings 1 and 3 remain fixed as assessed in earlier rounds.

## Validation and checklist

- Independent rerun of the round-three production-API reproduction (cancel at 90.5 s while a never-ready, TERM-resistant helper shuts down): returned `Err(StartTimeout)` at **92.022 s**, with `helper_still_alive=false`. The exact previously reported race is fixed; local output is `/tmp/q4-review-repro/round4-timeout-race-output`.

- `cargo nextest run -p dictation-transcribe llama --no-fail-fast`: **18 passed**, including cancellation during timeout shutdown, TERM-resistant cancellation, disconnect and process lifetime tests.
- Focused Vitest run of onboarding, Settings, combined model presentation, model rows and onboarding logic: **224 passed in five files**.
- Brief UI review: summed speech/cleanup progress uses one row and Cancel; cancelling the speech id also cancels the active cleanup download; Settings presents OpenRouter first and the revised local title/caption; the menu is in document flow; clicking the preselected OpenRouter radio records the choice. Existing tests cover the combined progress strip and preselected-card regression.
- Q3 integration reviewed for the affected Q4 wiring and import changes; this is not a fresh independent review of the already-merged Q3 implementation.
- Checklist: scope, correctness, security/privacy, architecture, Rust/TypeScript practice, licences, tests and Git/PR retain the earlier assessments, with the final lifetime finding now closed. No guard or product edits by this reviewer.
- Approval is the code-review verdict. Current-head Tester PASS, CI and Lead manual measurements/checks remain separate merge requirements. No merge performed.
