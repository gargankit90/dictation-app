Reviewer: APPROVED

# Self-hosted CI review, round 2

PR: #145 at `793f22b89a6e4c9763ec8e872688c71e46376410`.
Scope: only the two round 1 findings and their fixes in the workflows,
`scripts/build-dmg.sh` and engineering documentation. Changes brought in by
merging the default branch were not re-reviewed. No merge performed.

## Findings

No open blocker, major or minor findings in this re-review.

1. **Round 1 major 1 — fixed.** All three jobs now run
   `git clean -ffdx -e /target/ -e /node_modules/` immediately after checkout.
   The root-anchored excludes preserve only the intended cache directories;
   ignored source-tree outputs, including generated command permissions,
   Tauri schemas, frontend output and environment files, are removed.
   A disposable Git checkout exercised this exact command: both root caches
   survived, ignored and ordinary untracked files and a nested Git repository
   were removed, a tracked file survived, and an untracked symlink to an
   external directory was removed without changing its external sentinel.
   Git cleans within its working tree, not the runner's parent `_work`
   directory or home; these workflows have no alternate working-directory
   setting. No cleanup was run against the owner's runner checkout.

   Retaining `target/` does not preserve the source-tree permission input
   that caused the finding. In pinned `tauri-build` 2.7.4,
   `app_manifest_permissions` builds the application manifest from current
   command definitions and current permission files, rather than loading
   the previous application's manifest from `OUT_DIR`. A changed build
   script is rebuilt by Cargo; without command generation and with the
   source-tree permissions removed, old generated manifests in `target/`
   cannot supply the missing command permissions. The remaining compiler
   and vendor build directories are Cargo-managed caches. Coverage clears
   its instrumentation data before each measurement.

   Packaging now explicitly removes `target/<profile>/bundle/dmg` before
   invoking Tauri. The profile is restricted to the script's literal
   `release` or `debug` choices. A failed build stops under `set -e`, and a
   successful build that creates no DMG fails the subsequent lookup, so an
   old DMG cannot satisfy it. `target/dmg-out` is recreated after successful
   validation; a failed build does not reach the upload step. The engineering
   documentation accurately describes the cleanup and retained caches.

2. **Round 1 major 2 — fixed.** Both workflows use a workflow-specific
   prefix plus `github.event.pull_request.number || github.run_id`.
   Updates to one PR retain the same group and
   `cancel-in-progress: github.event_name == 'pull_request'` remains enabled
   for them. Push and manual events have no PR number and receive distinct
   groups for distinct run IDs, preventing concurrency-based cancellation
   of running or pending default-branch runs. The single runner serializes
   jobs without requiring a shared concurrency group. This preserves runs;
   it does not promise FIFO scheduling. Checked the expression's mapping
   for repeated PR updates and three distinct non-PR run IDs in both
   workflow namespaces. Documentation matches the expressions.

## Validation and limits

- The disposable cleanup fixture and group-mapping assertions passed.
- `bash -n scripts/build-dmg.sh` passed.
- Verified both successful workflow runs report the reviewed SHA:
  [CI 41366821804](https://github.com/gargankit90/dictation-app/actions/runs/41366821804)
  (quality gate and platform checks) and
  [DMG 41366821806](https://github.com/gargankit90/dictation-app/actions/runs/41366821806).
- Missing command generation was assessed from the pinned dependency's
  source and cleanup fixture; no product mutation test was performed.
  No real default-branch queue experiment was attempted, since that would
  require unrelated pushes. The concurrency conclusion follows directly
  from distinct group keys.
- Checklist: scope and correctness OK for the fixes; security/privacy OK
  for cleanup boundaries; architecture, Rust/TypeScript practice and
  licences unchanged by these fixes; focused validation above; Git/PR
  changes limited to this report. Unchanged round 1 assessments stand.
- No product, test, spec or guard files changed. The normal pre-push gate
  remains enabled. Approval does not authorize merging in this task.
