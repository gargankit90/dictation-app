Reviewer: CHANGES REQUESTED

PR #217, L1 round 1; reviewed head 2d206466b054095573475c71b9eb32e46979531e.

## Findings

### Major 1 — A same-size corrupt final model is treated as verified

`crates/transcribe/src/models.rs:242` (`ready_path`). Only the length is checked. After a verified download, change any byte without changing the length (for example disk corruption or a restored damaged file), then restart/select the local engine: the file is reported Ready and passed directly to the native loader. `download_from` also returns it immediately, so Download cannot repair it. This can produce bad transcription or native load failure while Settings still says Downloaded. This is distinct from the native crash isolation follow-up #218: the integrity boundary should reject damaged bytes before calling native code.

Suggested fix: verify the catalogue SHA-256 on the blocking pool before the first load of an existing final file in each process; cache successful verification for that file identity while it remains unchanged. Reject/quarantine damaged files and expose a re-download path. Add a test that changes one byte of a previously downloaded final file while preserving length and confirms it is never loaded or returned as verified. Current tests cover hash failure in `.part` and final size changes, but not this case.

### Major 2 — Two allowed downloads can fill the disk despite the space check

`src-tauri/src/models.rs:148` permits simultaneous downloads for different model IDs, while `crates/transcribe/src/models.rs:336` checks free space independently and only once. With 900,000,000 bytes free and neither model present, start both downloads before either receives substantial data: Parakeet needs 686,989,696 bytes including headroom and Qwen needs 799,275,680, so both pass. Together their bodies require 1,066,834,976 bytes, exceeding the actual free space; the disk can fill and both downloads can fail, affecting other applications too. The UI offers Download for both rows.

Suggested fix: serialize model downloads globally, or atomically reserve the remaining bytes of every active download against shared available space and retain the headroom. Test starting both models with space sufficient for either alone but insufficient for both, and require one to queue/refuse before writing. A per-file received-byte bound correctly exists, but does not prevent this aggregate failure.

### Major 3 — Switching local models keeps both native models allocated during load

`src-tauri/src/engine.rs:148` drops the warm-cache handle before loading the next model, but `crates/transcribe/src/engine.rs:163` builds the replacement before replacing `current`. Consequently `current` still owns an `Engine::Local` clone of the old model throughout the new native load, even when no dictation is in flight. Selecting downloaded Qwen after Parakeet therefore loads Qwen while Parakeet's weights/session buffers are still live. On a memory-constrained Mac this defeats the explicit release-before-load intent and can cause allocation failure or severe memory pressure when switching models.

Suggested fix: for a change of local model, replace the current engine with a loading/unavailable state and release its old handle before invoking the next loader, while retaining any references legitimately held by in-flight dictations. Preserve warm reuse for language/unrelated settings changes. Add a drop-probe test through `LiveEngine`/`Transcribers`, asserting the old model is dropped before the second loader starts when there are no in-flight calls; testing the cache alone misses the extra owner.

Blockers: 0. Majors: 3. New minors: 0; no new tech-debt issues.

## What I checked

- Scope: read L1 criteria 1–8, PR notes and the whole change by subsystem. Criteria 3 and 6 have the findings above. CI limitation is already tracked by #219; native process crash isolation is already tracked by #218. Neither is duplicated here.
- Correctness: traced partial resume, ignored Range responses, oversized/short bodies, checksum failure cleanup, rename, cancellation notification, per-model duplicate-start protection, deletion and background rebuild ordering. Incorrect resume bytes cannot pass the final hash. Existing final-file trust and aggregate space accounting need fixes above.
- Security/privacy: new Rust paths keep audio in memory, do not log transcript text or keys, and remove signed CDN URLs from network errors. Read the dependency's Rust ownership/FFI wrapper and inspected native logging sites. No new direct unsafe blocks, analytics or widened overlay command access found. This is source inspection, not a runtime privacy audit.
- Architecture/practice: model load and transcription run on blocking threads; engine snapshots do not hold their lock across transcription awaits; rebuilds serialize. Catalogue/core remain free of I/O. Reviewed typed IPC, local missing-model guidance and local-language filtering. Memory lifecycle has finding 3.
- Settings/onboarding: old settings default to cloud without changing provider/model; keys remain in the existing SecretStore. Local onboarding bypasses the key check and explains no key is needed. Reviewed form persistence and local download polling. No key migration loss found.
- Licences/reuse: inspected NOTICE and generated third-party notices, transcribe-cpp 0.4.1 dependency and Handy attribution. No apparent GPL-derived additions found.
- Catalogue: direct HEAD requests to both pinned Hugging Face resolve URLs returned HTTP 302 with `x-linked-size` and `x-linked-etag` exactly matching each catalogue entry (477274496/a8bf3de2… and 589560480/5b58f32a…). No model weights downloaded in this review.
- Tests: independently ran `cargo test -p dictation-transcribe --lib`: **55 passed, 0 failed**. Read model-store, local-model, shell lifecycle, migration and React test changes. The native real-model integration test is opt-in; I did not run real speech/model or VM tests. Tester owns acceptance/e2e evidence; full gate runs on push.
- Git/PR: reviewed Conventional Commit history and PR notes; no guard edits in the PR diff. Only this review file is committed by the reviewer. No merge performed.
