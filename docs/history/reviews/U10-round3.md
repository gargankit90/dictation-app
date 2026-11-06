Reviewer: CHANGES REQUESTED

# U10 review, round 3

PR: #279 at 1dbb883262d6887e12d83aac9a078ca98797d554
Verdict: CHANGES REQUESTED

Scope: only the two round 2 findings, as requested by the Lead.

## Findings

| # | Severity | file:line | Finding | Suggested fix |
| - | -------- | --------- | ------- | ------------- |
| 1 | major | src-tauri/src/onboarding.rs:193 | Actual progress-file read failures still become None through read_to_string(...).ok(). Both None branches set sharing_was_off=false. With sharing On, this reports sharing_on_throughout=true and can still select On since setup began. The new unreadable test uses malformed JSON, which successfully reads and exercises only the Some branch. Invalid UTF-8, PermissionDenied, or a directory at PROGRESS_FILE exercise the unfixed path. | Distinguish NotFound from other read errors; count the latter as unknown history and test a deterministic read failure, for example invalid UTF-8 or a directory at the progress path. |

Inline: https://github.com/gargankit90/dictation-app/pull/279#discussion_r4486619721

## Round 2 findings

- Finding 1, opt-out before persistence: fixed. choose_sharing disables both adapters before taking the settings lock or writing settings/progress. The real FIFO test holds the settings lock, verifies both adapters are disabled and the queue/ID are gone, then stalls the progress write and checks again. On the old order, the FIFO blocks before opt-out and the first assertions fail. The failed-save test also confirms Off is noted when saving fails.
- Finding 2, unknown history: fixed for old JSON missing the field and for malformed JSON, but actual read errors remain as above. Fresh installs and Run setup again intentionally start a new walk; stored Off is still noted when setup opens.
- Separate tests are sufficient here. Rust exercises loading through OnboardingStatus.sharing_on_throughout, and the UI test sets that same typed boolean false with sharing On and asserts the from-now caption rather than the since-setup caption. A combined file-to-webview test would duplicate that simple boundary. The missing coverage is a distinct Rust read-error input, not integration between the two test layers.

## Validation

- `npx vitest run src/Onboarding.test.tsx`: 86 passed.
- `git diff --check 1dbb883^..1dbb883`: passed.
- Focused Rust U10 tests and the normal pre-push gate are run before pushing this report; results accompany the PR comment.
- Only this review file modified by the Reviewer. No merge performed.
