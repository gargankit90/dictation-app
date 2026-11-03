Reviewer: APPROVED

# R1 review, round 1

PR: #190 at c16930ac55108f01c2e169182e57ddd7fd7bb005.

## Findings by severity

No blocker, major, or minor findings. No new tech-debt issues.

## What I checked

- **Scope:** OK. Read the whole diff, PR notes, R1 spec, engineering guide,
  reviewer role, and review checklist. Criteria 1–4 map to the implementation
  and focused tests. Criterion 5 remains an owner action after merge.
- **Correctness:** OK. The initial package URL must match the parsed endpoint's
  scheme, host, effective port and repository-specific `releases/download/`
  prefix. Old repository, lookalike repositories/hosts, wrong paths, insecure
  transport, encoded slashes/backslashes and traversal escaping the prefix are
  rejected. URL parsing resolves dot segments before comparison. Host case and
  explicit port 443 normalize to the same HTTPS origin; a trailing-dot host
  does not match. Userinfo cannot disguise the parsed destination host.
- **Security and privacy:** OK. Inspected `updater_for`, `install`, and the
  resolved tauri-plugin-updater 2.14.1 source. The custom redirect policy is
  applied to package requests as well as checks: HTTPS GitHub and GitHub asset
  hosts only, fewer than five previous hops. Redirects necessarily leave the
  initial repository URL for asset storage, but do not bypass signature or
  signed-version verification. `download_and_install` calls `download`, which
  verifies bytes before `install_inner`; direct plugin `install` also verifies.
  The shipped public key remains F6C80F173A96D78F and
  `requireSignedVersion` remains true. No key handling, permissions, CSP,
  transcript logging, audio handling or analytics changes.
- **Release safeguards:** OK. Publish checks a clean tree, fetches origin,
  requires HEAD reachable from the default branch, rejects local and remote
  version tags, captures the exact SHA, and rechecks HEAD and cleanliness
  before `gh release create --target <SHA>`. Version and format guards remain.
  Manifest URLs use this repository. Existing-tag errors and releasing docs
  explain using a new version after a partially failed publish. Read-only
  GitHub API inspection confirmed active ruleset `immutable-tags` covers all
  tags, blocks updates/deletions/non-fast-forward changes, and has no bypass
  actors. No real release or owner Keychain operation was run.
- **Architecture:** OK. No dependency, platform boundary, unsafe, runtime or
  coordinator changes.
- **Rust and TypeScript practice:** OK. No new production panic/unwrap,
  suppression, blocking callback, public API or frontend changes.
- **Licences and reuse:** OK. No dependency or copied-code changes.
- **Tests:** `bash scripts/test-release.sh` passed all six groups using stub
  security/npm/codesign/uname/gh, throwaway signer keys and a local bare origin.
  This includes dirty/unmerged trees, existing local/remote tags, exact target
  SHA, destination, signature/version/tamper refusals and redaction.
  `cargo nextest run -p dictation-app updater::` passed all 25 focused tests,
  including tampered payload, wrong key, garbled signature, redirects,
  shipped configuration and the stricter allowlist.
- **Docs and Git/PR:** OK. Tester download link, releasing guide and engineering
  guide use this repository. Repository-wide search found no active release
  destination left pointing at dictation-app-releases: remaining references
  are historical M5.4/R1 context, archive instructions or rejection tests.
  M5.4 is explicitly identified as historical in the PR notes. Conventional
  implementation commits and complete PR notes; no guard changes. Only this
  review is committed by the reviewer; pre-existing untracked files are kept.

No merge performed. Owner archive/first-publish actions and the independent
Tester result remain outside this review.
