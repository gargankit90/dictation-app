# CI self-hosted, test report, run 1

PR: #145 at 5aa4e68 (branch `ci-self-hosted`)
Result: PASS

No milestone spec; criteria are the 5 in the PR body plus the Lead's checks.

| # | Check | How | Result | Notes |
| - | ----- | --- | ------ | ----- |
| 1 | CI checks pass on the PR, on the self-hosted runner | `gh pr checks 145`; job log of run 41363282028 | PASS | "Quality gate (macOS, self-hosted)" 3m10s, "Platform crate (macOS)" 2m49s, "DMG (macOS)" 1m22s, all pass. Log says `Runner name: 'owner-mac-m4pro'`. |
| 2 | Every step of the old gate runs | Compared the `==>` lines in the log with `step` lines in `scripts/gate.sh`: 24 of 24 match, in order | PASS | fmt, prettier, clippy, deny, machete, no_tauri_in_pipeline, licence checks (+ their test), nextest (workspace, then platform with `testing`), doctests (workspace, platform), coverage.sh, test-coverage.sh, vendor-tests.sh, tsc, eslint, vitest, typos, test-guards.sh, test-key-handling.sh |
| 3 | Coverage gates enforced | log | PASS | core 98.93% (≥85), transcribe 98.61% (≥85), app 97.74% (≥70) |
| 4 | No big downloads; local caches kept | log | PASS | `actions/checkout` `clean: false`, then `git clean -ffd`; `npm ci` 2s; clippy 27s on warm `target/` |
| 5 | A failing change fails the self-hosted job | Branch `tester/ci-fail-probe` (a test in `crates/core/tests/ci_fail_probe.rs` that fails only when `GITHUB_ACTIONS` is set, so the local pre-push gate still lets the push through), draft PR #148 | PASS | "Quality gate (macOS, self-hosted)" failed after 41s: `FAIL dictation-core::ci_fail_probe`, `error: test run failed`, exit 100. PR #148 closed unmerged; **the branch `tester/ci-fail-probe` was left on the remote**. |
| 6 | `scripts/ci-check-tools.sh` fails clearly when a tool is missing | Ran with `PATH=/usr/bin:/bin` | PASS | Exit 1: `ci-check-tools: not installed on this runner: rustc cargo node npm typos cargo-nextest`. With the normal PATH it lists each tool's path and exits 0. Not exercised: the Node-major mismatch branch. |
| 7 | Safety: no `pull_request_target`, no forks | `grep -rn pull_request_target .github` | PASS | Only comments mention it. |

## Notes (not blockers)

- The first probe attempt (an unconditional failing `assert_eq!`) was stopped by the local pre-push gate, as designed. It never reached the remote.
- Not tested: the concurrency cancel (push twice, first run cancelled) and the Mac-asleep case.
- On the probe run the "Platform crate (macOS)" job was still pending when I closed #148; the Quality gate result is what the check needs.
