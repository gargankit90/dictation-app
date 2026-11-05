Reviewer: APPROVED

# U10 review, round 4

PR: #279 at f4d011a564e636e111d1032bb96e9cdd8828c8dd
Verdict: APPROVED

Scope: only round 3's progress-file read-error finding, as requested.

## Findings

None. The outstanding finding is fixed.

## Re-check

- Onboarding::load now checks the original read result before discarding its error. NotFound retains the existing no-file behavior; every other error marks sharing_was_off=true, regardless of whether a settings file exists.
- This produces sharing_on_throughout=false. With sharing On, Ready selects On from now and cannot select On since setup began. The previously reviewed UI test covers this boolean-to-caption boundary; testing file loading separately remains sufficient.
- The expanded Rust test supplies actual invalid UTF-8 and an actual directory at PROGRESS_FILE, both with and without a settings file, then exercises load, open and status. Each of these assertions would fail with the old read_to_string(...).ok() implementation. Existing fresh-install, legacy JSON, malformed JSON, relaunch and new-walk checks remain.
- The fix adds no new data or sending behavior. No new areas reviewed.

## Validation

- `git diff --check f4d011a^..f4d011a`: passed.
- The focused unknown-history Rust test and normal pre-push gate are run before pushing this report; results accompany the PR comment.
- Only this review file modified by the Reviewer. Approval is not a merge: the current Tester result and Lead checks are still required.
