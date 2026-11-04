Reviewer: APPROVED

# F1 review, round 2

PR: #236 at 5c5484078360e5881a6c09c8a63db37d09ffc378
Verdict: APPROVED
Counts: 0 blocker, 0 major, 0 minor outstanding.

## Findings rechecked

1. **Fixed** — `.config/nextest.toml:22`, `crates/platform/src/reveal.rs:34`, `src-tauri/src/commands.rs:2610`: unsupported nextest environment entries are removed. The guard now also checks feature `testing`, which the app's dev-dependency enables. The cross-crate test exercises the real opener and real Keychain store and requires refusal. It therefore covers the dependency compiled without its own cfg(test). The gate's environment variable is no longer needed for this test's protection.
2. **Fixed** — `src-tauri/src/commands.rs:2536`, `src-tauri/src/commands.rs:2572`: the commands test explicitly builds a report, requires open_issue success, and asserts exactly one recorded URL at the expected repository's new-issue endpoint. Missing state, early errors and zero calls now fail. The new failing-launcher test requires the browser error to propagate and records the attempted call.

## Production feature check

- `cargo tree -p dictation-app -e normal,build,features -i dictation-platform` shows only the platform default feature; `testing` is absent. Including dev edges shows `testing` supplied only through dev-dependencies. Workspace resolver 3 separates those edges for normal builds.
- `src-tauri/Cargo.toml:24` uses the ordinary platform dependency; only its dev-dependencies section enables `testing` (`:45`). The pipeline's platform dependency is likewise dev-only. Platform has no default feature enabling `testing`.
- `package.json`, `src-tauri/tauri.conf.json`, `scripts/build-dmg.sh` and `scripts/release.sh` do not request `testing` or all features. Normal `npm run tauri dev` and release/debug bundle builds therefore retain real opening and Keychain access. No compile-time capture of the gate environment was introduced.
- Explicitly enabling `testing` in a custom build intentionally disables these host effects; this is outside the shipped/dev commands reviewed here. The existing runtime environment opt-out remains unchanged.

## Validation and scope

Focused nextest run with `DICTATION_NO_LAUNCH` unset: the settings-command test, failing-launcher test and cross-crate refusal test pass. The push hook also runs the required full gate.

Rechecked only the round 2 diff and the two prior major findings, including production feature selection. No new architecture, privacy, licence or guard changes in this diff. A real Keychain store accidentally substituted into the fixture is safely refused; it need not make the fixture test fail, as the coordinator clarified. Both prior review threads are ready to resolve. No merge performed.
