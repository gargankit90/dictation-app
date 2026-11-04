# F1 test report, run 2

PR: #236 at 5c54840
Result: PASS

Re-check of run 1's two majors (docs/history/test-reports/F1-run1.md).

| # | Check | Result | Notes |
| - | ----- | ------ | ----- |
| 1 | `DICTATION_NO_LAUNCH` unset, logging `open`/`osascript`/`xdg-open`/`say`/`afplay` wrappers first on PATH: `cargo nextest run -p dictation-app`, then `cargo test -p dictation-app` | PASS | 220 passed in each; wrapper log 0 bytes; app-support dir, `settings.json` and `debug-recordings` mtimes unchanged; no Keychain prompt. Finding 1 fixed: `launching_disabled()` is also true under `cfg!(feature = "testing")`, which `src-tauri` and `crates/app` enable only as dev-dependencies; the dead nextest.toml env block is removed. |
| 2a | Fixture on `SystemOpener` (env unset) | PASS | `the_settings_window_may_invoke_all_app_commands` fails (`main couldn't call open_issue`) and so does `open_issue_reports_a_launcher_that_fails` (0 URLs recorded, expected 1). Wrapper log empty: nothing opened. Finding 2 fixed. Restored. |
| 2b | Fixture on real `KeychainSecretStore::new()` (env unset) | ACCEPTABLE | All 220 pass; no prompt, because the store refuses under the `testing` feature. No fixture command reads a key, so nothing fails loudly, but `the_real_launcher_and_keychain_refuse_in_this_crates_tests` asserts the refusal directly (`get` returns an error). That is enough: the failure mode is a refusal, never a prompt. Restored. |
| 3 | Production still uses the real opener | PASS | `lib.rs:341` manages `AppOpener(SystemOpener)`; Report a problem → `open_issue` (`diagnostics.rs:615-622`) and Reveal log (`lib.rs:817-820`) both go through it. `cargo build -p dictation-app` (debug, no tests) succeeds and `cargo tree -e normal -i dictation-platform` shows no `testing` feature on the shipped dependency path (it is a dev-dependency only in `src-tauri/Cargo.toml:45` and `crates/app/Cargo.toml:22`), so `launch_allowed` only refuses when the env var is set. I did not run the built app or click anything on the host. |
| 4 | Hosted CI | PASS | Quality gate (Linux), Platform crate (macOS), DMG (macOS) all SUCCESS at 5c54840. |

Minor note: `launching_disabled` is now `pub`; any build of `dictation-platform` with feature `testing` refuses real launches, so nobody may enable `testing` on a shipped path. Nothing does today.
