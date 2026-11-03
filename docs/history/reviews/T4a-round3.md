Reviewer: APPROVED

PR: #157 at `79029c54ad392eacb9b59d1d7c7f1e05b952d19c`, round 3.
Scope: merge resolution `2137c72` and scenario correction `79029c5` only.
No blocker, major or minor findings. Unchanged T4a code was not re-reviewed.

## Findings

None. The resolution preserves both branches' user-visible behavior:

- A dictation error shows `⚠ Dictation failed`, with its full message in the
  tooltip, until the next Recording event. Idle does not clear it.
- Hotkey failure shows `Hotkey unavailable` until HotkeyRestored; Recording
  alone does not clear an ongoing outage. A newer dictation error temporarily
  takes precedence; Recording clears that error and reveals the ongoing outage.
  Recovery clears the outage without dismissing a newer dictation error.
  Conversely, a new hotkey failure replaces an older dictation error.
- The only nonempty titles produced by TrayStatus are the two constants, each
  18 characters and tested against MAX_TITLE_CHARS (20). Arbitrarily long error
  messages reach the tooltip, never the title. The shell applies that view and
  continues forwarding events to the overlay.
- The scenario now uses existing tray_shows/tray_is_clear helpers with correct
  arguments, checking exact title and tooltip. All scenario files are sourced
  before execution, so the helpers exist when the hotkey scenario runs.

## Preservation checks

- The proxy client, OpenAiCompat construction, engine and Settings command
  paths have no diff from the previously approved T4a parent. Loopback HTTP
  still bypasses proxies for dictation and Test key.
- Both provider URL flag forms retain test_flags gating; TEST_FLAGS follows
  debug_assertions, and the release with_provider_url remains a no-op.
- hotkey_setting.rs, pipeline.rs and ui_events.rs have no diff from the
  default-branch merge parent: the hotkey supervisor and recovery events remain.
  The runner registers the recovery scenario alongside T4a's new scenarios.

## Verification

- Pipeline tray unit tests: 6 passed, including precedence and clearing.
- Shell tray tests: 2 passed, including every error's title and full tooltip.
- Loopback proxy regression: 2 passed; dictation and Test key reach the local
  provider while the configured proxy receives zero connections.
- Runner harness: 67 passed, 0 failed.
- Bash syntax checks for both affected scenarios: passed.
- git diff --check from the round 2 parent: passed.
- The Developer reports VM PASS for tray clearing, Settings provider recovery
  and hotkey recovery in the PR comment. Those VM runs were not repeated here.

## Checklist for the changed scope

Scope, correctness, security/privacy, architecture, Rust/TypeScript practice,
licences/reuse, tests and Git/PR: OK. No new dependency, capability, unsafe code,
secret storage or logging change in the resolution. Tests accompany the changed
behavior; full quality checks remain enforced by the push hook.

Only this review file is changed. No merge was performed or authorized by this
review; the workflow's other approval conditions still apply.
