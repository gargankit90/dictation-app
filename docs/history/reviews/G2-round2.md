Reviewer: APPROVED

# G2 review, round 2

PR: #291 at d4a6eb1a9e5b2ac65965daa69461d3ae5a937ea8; fixes in 6fa66d4.

Scope: re-check only the three round-1 findings. All three are resolved; no remaining blocker or major finding in this re-check.

| Finding | Result | Independent evidence |
| - | - | - |
| Missing coverage/licence controls in receipt key | Fixed | Repeated the original clean-tree key probe with identical stubbed tool versions. Baseline, `COVERAGE_MIN_CORE=101`, and added `LICENCES_GEN=/nonexistent` produced three distinct hashes. The environment selection now includes both prefixes; self-tests exercise receipt invalidation for both. |
| Deleted installed npm tool/package not detected | Fixed | Repeated the isolated installed-tool deletion probe: removing `.bin/prettier` now changes the install hash. Gate reuse additionally requires executable prettier, eslint, tsc and vitest. The full self-test checks that deletion runs the full gate and reports the missing tool, and that deleting an installed package invalidates its receipt. |
| Inherited Git template hooks execute outside fixtures | Fixed | Repeated the original full-suite template-hook probe, changing only the expected outcome: a harmless executable inherited pre-commit hook did not run and its outside-fixture sentinel was absent. Separately ran the full self-test with this real linked worktree's `GIT_DIR` and `GIT_WORK_TREE` exported; it passed, and HEAD/tracked files stayed unchanged. |

Isolation now drops inherited `GIT_TEMPLATE_DIR`, supplies an empty private template, and disables hooks for throwaway Git commands through isolated environment config. Fixture commits, pushes and clones therefore cannot pick up the caller's executable template hooks. The new built-in hostile-environment regression covers pre/post-commit, post-checkout, pre-push and reference-transaction hooks and checks a decoy repository's refs/status remain unchanged. The independent full-suite probe above also checked successful completion, rather than relying solely on that regression's ignored child exit status. Actual project hooks remain enabled.

Both independent full self-test runs passed. The review commit goes through the normal required push hook; its gate outcome is recorded in the PR comment. No VM used; no product, test or guard files edited. The round-1 assessment of the narrow macOS timing-test exclusion is unchanged.
