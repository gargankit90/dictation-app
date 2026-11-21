# MQ4 review, round 3

PR: #337 at 9531b844dc0bfed318135fa504ee78e88159f4d0
Verdict: CHANGES REQUESTED

Re-check limited to the shutdown changes, new regression tests and pinned OrukQwen URL since round 2. No merge.

## Findings

| # | Severity | file:line | Finding | Suggested fix |
| - | -------- | --------- | ------- | ------------- |
| 2 | major | crates/transcribe/src/llama.rs:385 | The timeout/error branch now awaits shutdown inside `wait_ready`, which is itself a cancellable arm of the outer `select!`. Once shutdown takes `supervisor`, cancellation drops that arm and its process handle. The cancellation arm's second `stop_and_wait` finds `None` and immediately returns, releasing startup serialization while the old helper still lives during its TERM grace. | Select only the startup outcome, then run one shared shutdown-and-reap step outside `select!` for every unsuccessful outcome. Keep `wait_ready` from consuming the process handle. Add cancellation-during-timeout-shutdown coverage; a short injected startup limit can keep that regression cheap. |

Independent reproduction through the public production `LlamaServer::start_until` API: a helper runs `trap '' TERM; echo $$ > <pid-file>; exec sleep 300` and never answers health. Cancel at **90.5 s**, after the 90 s startup timeout enters shutdown but before the supervisor's 2 s SIGKILL grace ends. Result: `Err(Cancelled)` at **90.511 s**, with `helper_still_alive=true` from `/bin/kill -0`. The reviewer explicitly killed the stand-in afterward. Local source/output: `/tmp/q4-review-repro/src/main.rs` and `/tmp/q4-review-repro/round3-timeout-race-output`. No product or test files were edited by the reviewer.

The previous immediate-cancellation reproduction now passes: cancellation returned after 2.040 s with `helper_still_alive=false`. The new escalation therefore fixes ordinary TERM-resistant cancellation, but the shared lifetime finding remains open for cancellation during timeout cleanup. A subsequent local selection can start the next helper before the old process exits, retaining the memory-overlap risk from finding 2.

## Other changes and validation

- The HTTP client is now built before spawning; startup errors and timeouts attempt shutdown and reaping. The supervisor escalates TERM to KILL. These changes address the earlier unchecked-timeout failure; the remaining issue is ownership across the nested cancellation boundary above.
- OrukQwen is pinned to `c947468edd421a9922afbbb7887056f793e5fd20`, and the catalogue test exemption is removed. Reviewer HEAD request to the pinned Hugging Face URL returned that commit, `x-linked-size: 1319830656`, and `x-linked-etag: 057471662f060800a6374708ad679d4abe51775e52ca591a88ea4ade78b273bb`, matching the catalogue.
- `cargo nextest run -p dictation-transcribe llama --no-fail-fast`: **17 passed**.
- `cargo nextest run -p dictation-core local_models`: **7 passed**.
- The new production-process keeper test (`review_2_a_sigterm_proof_helper_is_gone_before_the_next_starts`): **passed**, including release of the last helper.
- Findings 1 and 3 remain fixed as assessed in round 2. Scope and model integrity: OK for this diff. Process lifetime: finding 2 remains. Other checklist sections retain earlier assessments; no new minors or unrelated re-review.
- Lead intervention is required by the repository workflow after two fix rounds. Tester/current-head CI and Lead manual measurements remain merge requirements; no merge performed.
