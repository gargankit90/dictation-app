# LAL (e2e_launch_at_login race fixes) test report, run 1

PR: #237 at 1b9c78d
Result: PASS (with two infra flakes noted)

| # | Criterion | Test(s) | Result | Notes |
| - | --------- | ------- | ------ | ----- |
| 1 | 3/3 pass on dictation-vm | `E2E_VM=dictation-vm tests/e2e/run.sh e2e_launch_at_login` | PASS | After waiting for other workers' locks: 3 consecutive passes (about 4-5 min each). |
| 2 | Test still fails when it should | line 145 edited locally to expect `disabled` after enabling | PASS | Failed with "FAIL: the OS says enabled"; edit reverted. |
| 3 | Hosted CI | `gh pr checks 237` | PASS | Platform crate (macOS) and Quality gate (Linux) both pass. |

## Flakes observed (not in the PR's changed lines)

Two other runs of the unmodified PR branch failed:
- 16:13 run: failed; I did not capture the failing step (output was tail-truncated).
- 16:45 run: scenario exited with status 255 after "its hotkey is on", i.e. in `textedit_fresh` / `set_clipboard` (about 2 min stall) in the first-boot section, which the PR doesn't touch. The changed Settings/Save steps all passed in that run.

Other workers also run this scenario on shared VMs, so load may be a factor. Suggest treating these as a separate flake (tech-debt), not a blocker for this PR.
