# MQ4 review, round 2

PR: #337 at 9d6c1c0c950bf82c0fb3d765afe5bb33444b1cf1
Verdict: CHANGES REQUESTED

Re-check limited to changes since round 1 (`84f50d5`, `9d6c1c0`). No merge.

## Findings

| # | Severity | file:line | Finding | Suggested fix |
| - | -------- | --------- | ------- | ------------- |
| 2 | major | crates/transcribe/src/llama.rs:339 | The superseded-start fix serializes startup but silently discards the shutdown wait's five-second timeout. It can return `Cancelled` and release the keeper's startup lock while the old helper is alive, allowing the next helper to overlap. Startup error/timeout paths also still call the non-waiting stop. | Bound graceful shutdown, force termination if necessary, and reap before permitting the next startup; apply that invariant to unsuccessful startup exits. Add a slow or TERM-resistant shutdown test. |

Independent reproduction using the public production `LlamaServer::start_until` API and an executable stand-in (`trap '' TERM; echo $$ > <pid-file>; exec sleep 60`): cancellation returned `Err(Cancelled)` after **5.451 s**, and `/bin/kill -0 <pid>` still succeeded. The reviewer explicitly killed the stand-in afterward. The existing new process test uses ordinary `sleep`, which exits promptly on TERM and therefore misses the unchecked timeout. Reproduction source and output are local under `/tmp/q4-review-repro/`; no product files were changed.

## Earlier findings

- **1 fixed:** chained download success now invokes `cleanup_downloaded` and `LocalCleanup::refresh`; tests cover the model arriving after the speech rebuild, without a settings save/restart, and cloud remaining off.
- **2 remains open:** generation notification and startup serialization address normal cancellation, but do not guarantee the old process has exited before a new start (see above).
- **3 fixed:** production request cancellation now has a connection-close regression; the fake pipeline asserts its hung request was dropped and the next dictation is cleaned promptly; the opt-in real-helper test cancels a long generation then bounds the next cleanup. Developer evidence reports 635 ms for that next real request; reviewer did not rerun the real model.

## Validation and checklist

- Reviewer ran `cargo nextest run -p dictation-transcribe llama --no-fail-fast`: **16 passed**.
- Reviewer ran the shell cleanup tests and the successful-download activation regression: **10 passed**.
- Correctness/process lifetime: finding 2 remains. Scope, port reuse, download activation and cancellation coverage: addressed within this re-check. Other checklist sections retain round 1's assessment; no unrelated scope re-review or new minors.
- Replied in all three original inline threads and posted an inline comment on the unchecked wait. Lead manual measurements, Tester evidence and current-head CI remain merge requirements.
