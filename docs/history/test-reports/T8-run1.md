# T8 test report, run 1

PR: #227 at e5f421b
Result: FAIL

| # | Criterion | Test(s) | Result | Notes |
| - | --------- | ------- | ------ | ----- |
| 1a | Wait is bounded, logs frontmost app, doesn't hide paste failures | code read of lib_full.sh, e2e_error_bad_key.sh | PASS | 10 s loop; logs frontmost app on timeout and after a failed paste check; the paste check itself is unchanged |
| 1b | e2e_error_bad_key stable (3 runs, dictation-vm-2) | E2E_VM=dictation-vm-2 tests/e2e/run.sh e2e_error_bad_key | FAIL | Run 1 FAIL, runs 2 and 3 PASS. Extra runs 4-6 PASS (5 of 6 overall) |
| 2 | macOS job builds, lints, tests dictation-transcribe; read-only permissions; caching | ci.yml diff, gh pr checks 227 | PASS | build/clippy/nextest steps added after rust-cache; no permission change; both checks green (macOS 4m39s) |

## The failure (run 1)

"TextEdit is frontmost with an empty document" PASSED, the mock dictation ended Idle,
but "the mock's text is in TextEdit" FAILED. Frontmost after the paste: TextEdit.
The final screenshot shows an empty Untitled document and the tray "Dictation failed".
So the new wait does not cover this flake: TextEdit was in front and empty, yet
the pasted text did not land. The added logging correctly ruled out focus as the
cause in this run; the root cause (probably paste timing, or the mock run itself
failing) is still open for #203. The saved app log was taken before the mock
restart, so it does not show the mock run; saving the post-restart log would help.

Output: tests/e2e/out/20261103-122849-dictation-vm-2 (local, git-ignored).
