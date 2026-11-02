# ci-self-hosted test report, run 2

PR: #145 at 793f22b
Result: PASS

Scope: only the fixes in 793f22b (git clean, build-dmg.sh bundle clearing, concurrency keys).

| # | Check | Test | Result | Notes |
| --- | --- | --- | --- | --- |
| 1 | build-dmg.sh ignores an older DMG | Scratch repo with the real script, stubbed `npm`/check scripts, `app_1.0.0` DMG already in `target/debug/bundle/dmg`; stub builds `app_2.0.0`; ran `DMG_ADHOC=1 build-dmg.sh --debug` | PASS | `check-dmg.sh` received `app_2.0.0_aarch64.dmg`; the old DMG was gone from the bundle dir; `target/dmg-out` holds only 2.0.0 |
| 2 | git clean ran and removed ignored files in CI | Logs of run 41366821806 (DMG job) | PASS | `git clean -ffdx -e /target/ -e /node_modules/` ran, printed `Removing src-tauri/gen/` and `Removing src-tauri/permissions/`; the build then passed (regenerated). Quality gate job's clean ran with nothing to remove (fresh state), no error |
| 3 | Concurrency keys | ci.yml, dmg.yml | PASS | `ci-`/`dmg-` + `pull_request.number \|\| run_id`; cancel-in-progress only when event is pull_request. PR events share a group per PR; push events get a unique group per run |
| 4 | CI at 793f22b | `gh pr checks 145` | PASS | DMG, Platform crate, Quality gate all pass |

Limits: the build in check 1 was stubbed (no real Tauri build); the logic under test is the clear-then-find step. Push-event concurrency was verified by reading the expression, not by a push run.
