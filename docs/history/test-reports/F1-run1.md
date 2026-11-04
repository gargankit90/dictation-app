# F1 test report, run 1

PR: #236 at b85fedd
Result: FAIL

Host: owner's Mac, `open`/`osascript`/`xdg-open`/`say`/`afplay` wrappers first on PATH (log each call, exit 0).

| # | Criterion | Test(s) | Result | Notes |
| - | --------- | ------- | ------ | ----- |
| 1 | Injectable opener, recording fake, commands test asserts URL | `Opener` port, `FakeOpener`, `the_settings_window_may_invoke_all_app_commands` | PASS with a caveat | Port and fake exist; production wires `SystemOpener` (`src-tauri/src/lib.rs:341`). The URL assertion is a `for url in fixture.opener.urls()` loop with no minimum, so it passes when nothing was recorded (see finding 2). |
| 2 | Real opener refuses under test | `reveal::tests::nothing_is_launched_under_test` | PASS | `open_url`/`reveal_folder` return `PermissionDenied` when `DICTATION_NO_LAUNCH` is set or `cfg!(test)` holds. |
| 3 | Other host side effects checked | review of PR notes | not independently audited | Time box. No `say`/`afplay`/`osascript` call seen during the gate (wrapper log empty). |
| 4 | Full gate with wrapper `open` on PATH: log stays empty | `PATH=wrappers scripts/gate.sh` | PASS | Gate "all checks passed" (nextest 1191 + 328 + 225 + 124 passed); wrapper log 0 bytes. |
| 5 | Gate and hosted CI | CI on b85fedd | PASS | Quality gate (Linux), Platform crate (macOS), DMG (macOS) all SUCCESS. |
| 6 | Fixture uses fake store and temp dir | `app_with_windows` | PASS | `KeychainSecretStore::disabled()` and a scratch dir. |
| 7 | Keyring store refuses in test builds / with the var | `secrets::tests::a_disabled_store_and_test_builds_refuse_without_asking_the_os` | PASS | |
| 8 | No Keychain prompt; app support dir and settings.json unchanged | before/after `stat` of `~/Library/Application Support/com.dictationapp.desktop` | PASS | mtimes identical (dir, `settings.json`, `debug-recordings`); no Keychain prompt appeared during the gate. |

## Extra checks asked by the Lead

- Mutation A (fixture uses `SystemOpener` instead of `FakeOpener`), `DICTATION_NO_LAUNCH=1`: test PASSES (it should fail), nothing was opened. The refusal error is swallowed.
- Mutation B (fixture uses `KeychainSecretStore::new()`), `DICTATION_NO_LAUNCH=1`: test PASSES, no prompt. Not run with the variable unset (would risk a real Keychain prompt).
- `DICTATION_NO_LAUNCH` unset, `dictation-platform` own tests (cfg(test)): refused, wrapper log empty (PASS).
- `DICTATION_NO_LAUNCH` unset, `dictation-app` tests: NOT refused; see finding 1.
- Production wiring: `lib.rs:341` manages `AppOpener(SystemOpener)`; `open_issue` (`diagnostics.rs:615-622`) calls `opener.open_url` on it, so production still opens. No click made on the host; app build not repeated beyond the gate's compile.

## Findings

1. **Major: `.config/nextest.toml` `[profile.default.env]` is not honoured by nextest 0.9.149.** Running `cargo nextest run -p dictation-app the_settings_window_may_invoke_all_app_commands` with the variable unset (and the Mutation A fixture) called the wrapper `open` with the real `github.com/.../issues/new?...` URL, i.e. it would have opened a browser tab. Only `scripts/gate.sh` (which exports the variable) protects, so any direct `cargo nextest run`/`cargo test` of `src-tauri` (Developers' and Testers' common iteration) is unprotected. `cfg!(test)` does not help there: it is only true inside `dictation-platform` itself, not when `dictation-app` tests call it as a dependency. Reproduce: unset the var, put a logging `open` on PATH, apply Mutation A, run the test. Fix idea: set the variable another way (nextest setup script / `.cargo/config.toml` `[env]`), or add a `testing` feature/runtime flag the test fixtures set.
2. **Major: the safety net is silent, so the commands test cannot catch a forgotten fake.** With either mutation (and the variable set) the test still passes. The `open_issue` result in the loop is tolerated, and the opener assertion has no lower bound ("may have found no report"). Spec check "test must fail with the refusal error" is not met. Suggest asserting `fixture.opener.urls().len() == 1` after a call that is made with valid args, or asserting that a refusal from the opener fails the test.

Original mtimes: dir 1793642738, settings.json 1793642738, debug-recordings 1793507465 (unchanged after the run).
