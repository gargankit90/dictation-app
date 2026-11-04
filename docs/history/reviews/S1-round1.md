Reviewer: CHANGES REQUESTED

# S1 review, round 1

PR: #255 at be14614f4903fa0c79061e5251737c80e5e9fd73

Counts: 1 blocker, 1 major, 0 minors. No merge performed.

## Findings

| # | Severity | file:line | Finding | Suggested fix |
| - | -------- | --------- | ------- | ------------- |
| R1 | blocker | crates/app/src/launch.rs:321 | `work` calls `login.state()` while holding the pending mutex. The PR measures this call taking 5–19 seconds. Both `request()` and `shown()` first take that mutex, so another toggle blocks an async command executor and `get_settings` can wait indefinitely before reaching its 500 ms timeout. Save awaits a settings reread before showing its notice, so this also reintroduces the reported Save delay. | Never read the OS under the pending lock. Cache the state obtained by the change, or perform a versioned read outside the lock. Gate the final read in a deterministic test and prove a second request and `shown()` return promptly while it is blocked. |
| R2 | major | src-tauri/src/commands.rs:67 | A settings-file write failure after the OS changes is only logged; the command still reports success. An unwritable settings directory leaves the OS enabled and the stored flag disabled, with no user warning. After relaunch, a slow OS read uses the incorrect stored flag. This hides a partial failure and violates criterion 3's agreement after completion. | Propagate a persistence failure with an actionable partial-success message while preserving the actual OS outcome. Test a failed write and the subsequent slow-read/relaunch fallback. |

Inline threads: [R1](https://github.com/gargankit90/dictation-app/pull/255#discussion_r4464982699), [R2](https://github.com/gargankit90/dictation-app/pull/255#discussion_r4464982935).

## Checklist

1. Scope: criterion 1 has timing measurements in the PR; criteria 2–3 have R1/R2. The Lead-authored milestone is the only spec change. Criterion 4 has Developer-reported VM 3/3 evidence; no independent VM run by this reviewer.
2. Correctness: R1/R2. Request coalescing and storing before delivering outcomes otherwise serialize the latest choice correctly. Save preserves the stored login choice, so a stale form cannot write it back to the OS. Outside changes still update the displayed value when the OS answers. Read threads are detached and can accumulate if status never returns; normal process exit terminates them, with no join blocking quit. No additional shutdown blocker raised.
3. Security and privacy: OK. Duration/state logs contain no secrets or transcripts; the capability addition is scoped to the existing settings window capability. No new network destination or analytics property.
4. Architecture: OK. Synchronization lives in app, OS calls remain behind the port, and platform retains OS-specific code. No new dependency.
5. Rust and TypeScript practice: R1 blocks an async command path; otherwise typed bindings and error mapping checked. Approval-required errors include the System Settings next step. No new unsafe blocks or unexplained suppressions.
6. Licences and reuse: OK. No new dependencies or apparent copied implementation.
7. Tests: R1/R2 are untested paths. Delayed changes cover basic coalescing, but slow reads are tested only through `shown`, not the final read in `work`. Several new timing tests use real sleeps rather than explicit synchronization; use barriers/channels for the regression coverage. Focused test results recorded below.
8. Git and PR: OK. Conventional commits and detailed PR notes; no guard edits. No minors requiring issues. Hosted macOS platform and DMG checks passed when inspected; Linux quality gate was pending.

## Validation

- `cargo test -p dictation-pipeline launch:: -- --nocapture`: 17 launch tests passed.
- `npx --no-install vitest run src/Settings.test.tsx`: 77 passed, after `npm ci` (initial attempt could not load Vite because dependencies were absent).
- Hosted checks subsequently all passed at the reviewed product head (Linux quality gate, macOS platform, DMG).
- Findings are source-level interleavings/error-path analysis; no product code or tests edited, and no new reproduction harness added under the reviewer's restricted edit scope.
