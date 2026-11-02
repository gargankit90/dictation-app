# PUB1 test report, run 1

PR: #182 at 2227b56
Result: PASS (criteria 1-6; 7 is a manual check after the repo is public)

| # | Criterion | Test(s) | Result | Notes |
| - | --------- | ------- | ------ | ----- |
| 1 | No self-hosted runner; check wired into the gate | `scripts/check-workflows.sh` run clean, then 14 local mutations of `ci.yml` (all restored, tree clean) | PASS | See "Mutations". `gate.sh` runs `check-workflows.sh` and `test-e2e-main.sh`. |
| 2 | `ci.yml` hosted, fork and same-repo PRs, push to the default branch, no secrets | Read the file; diffed against `ci.yml` at `87aeddc^`; `actionlint` clean | PASS | `gate` is `ubuntu-24.04`, `macos` is `macos-15` with a `uname -m = arm64` check. Triggers: `pull_request` (forks included), `push` to `main`, `workflow_dispatch`. No `secrets.` anywhere. Linux gate not yet run on a hosted image, see below. |
| 3 | `dmg.yml` hosted macOS, ad hoc | Read the file and `build-dmg.sh` | PASS | `macos-15`, `DMG_ADHOC=1` forces the ad-hoc path (`signingIdentity "-"`), `CI=true` skips Finder scripting; `release.sh` is never called. |
| 4 | permissions, SHA pins, persist-credentials, no `pull_request_target` / `workflow_run` | `gh api` check of every SHA; mutations | PASS | All 5 pins verified (below). One gap, see minor 1. |
| 5 | `e2e.yml` removed; `scripts/e2e-main.sh` | Refusal cases, real run, SIGINT / SIGTERM, `shellcheck`, grep | PASS | See "e2e-main.sh". |
| 6 | Docs | Read the `ENGINEERING.md` "CI" section and `tests/e2e/README.md` against the code | PASS | Accurate. Stale references in minors 2 and 3. |

## Criterion 2: will the Linux gate pass on a clean hosted image?

It can't run yet (the PR's own checks fail in 1-2 s: the repo is private and billing is blocked, as `ENGINEERING.md` says), so I compared the setup by hand.

- `ci.yml` at `87aeddc^` (the last hosted version) against now. The `gate` job keeps the same steps: free disk space, the same five apt packages (`libwebkit2gtk-4.1-dev`, `libjavascriptcoregtk-4.1-dev`, `libsoup-3.0-dev`, `libgtk-3-dev`, `pkg-config`), `rustup toolchain install` (reads `rust-toolchain.toml`: 1.99.1 with clippy, rustfmt, llvm-tools-preview), `setup-node` from `.nvmrc` (24), rust-cache, and `install-action` with the same pinned tools (cargo-about 0.9.3, cargo-deny 0.20.2, cargo-machete 0.9.2, cargo-nextest 0.9.149, cargo-llvm-cov 0.9.1, typos 1.51.1). The actions have the same SHAs.
- Differences are only additions: `persist-credentials: false`, per-run concurrency, a `tee` of the gate log plus evidence upload (the step uses `shell: bash`, so `pipefail` holds and `tee` can't hide a failure), the platform-crate `clippy` / `doc` runs without feature `testing`, `macos-latest` pinned to `macos-15`.
- Tools `gate.sh` needs that the workflow does not install: `npm` (setup-node), `jq` and `openssl` (both on `ubuntu-24.04`), `shasum` (perl, on the image), `git`. `prettier`, `eslint`, `vitest` come from `npm ci`. Nothing else is missing that I could find.
- The risk that remains: since `87aeddc` (Nov 1) the gate has only run on the owner's Mac. About 70 files of new gate content (`test-release.sh`, `test-vm-lock.sh`, `test-runner.sh`, `test-ci-run.sh`, `single_instance_launches.rs`, `loopback_no_proxy.rs`, coverage thresholds) were never run on Linux. I have no Linux or Docker here, so I ran the shell parts of the gate on this Mac with GNU userland first on `PATH` (coreutils, gnu-sed, grep, gawk, findutils, bash 5.3) as a proxy: `check-workflows.sh` ok, `test-e2e-main.sh` 26/26, `test-guards.sh` 63/63, `test-release.sh` ok, `test-key-handling.sh` 42/42, `test-vm-lock.sh` 271/271, `test-runner.sh` 94/94 and ci-run 32/32. The Rust side on Linux (nextest, coverage >= 85/70, `cargo deny`, the new unix single-instance tests) is **not verified**. Criterion 7 (first green hosted run) is where that is settled; expect a possible first-run fix.

## Mutations of `check-workflows.sh` (criteria 1 and 4)

Each was applied to `ci.yml` (or a new file), the script run, then the file restored (`git status` clean at the end).

| Mutation | Result |
| -------- | ------ |
| `runs-on: [self-hosted]` | fails (two messages) |
| `runs-on: ${{ matrix.os }}` | fails (matrix labels are refused outright) |
| `runs-on:` as `group:` / `labels: owner-mac` | fails |
| `runs-on:` as a multi-line list | fails |
| quoted `"macos-15"`, or `macos-15-xlarge` | fails (strict allowlist; see minor 4) |
| new file with a job `uses:` a reusable workflow (no `runs-on`) | fails ("has no runs-on") |
| job calling `./.github/workflows/ci.yml` | fails (same reason) |
| `pull_request_target`, `workflow_run` | fails |
| `${{ secrets.X }}` | fails |
| action `@v2`, `docker://alpine:3` | fails (not a full SHA) |
| `persist-credentials: true` | fails |
| top-level `contents: write` | fails |
| a job-level `permissions: contents: write` | **passes** (minor 1) |
| `uses: ./local-action`, job `environment: prod` | passes (the latter only matters if it brings secrets, and `secrets.` is grepped) |

Comments mentioning `self-hosted` are ignored, by design (`test-e2e-main.sh` covers it).

## Action pins (criterion 4)

Checked with `gh api repos/<action>/git/ref/tags/<tag>`:

- actions/checkout v7.0.1 = 3d3c42e5 (commit)
- actions/setup-node v7.1.0 = 949feb24 (commit)
- actions/upload-artifact v7.0.2 = cf430e03 (commit)
- taiki-e/install-action v2.88.0 = 86f7a0ff (commit)
- Swatinem/rust-cache v2.9.3 = d1527c8f (annotated tag; dereferenced with `git/tags/` to d1527c8f, matches)

All workflows: top-level `permissions: contents: read`, every checkout has `persist-credentials: false`, no `pull_request_target`, no `workflow_run`, `workflow_dispatch` only in `ci.yml` and `dmg.yml` (hosted, no secrets, read-only token). `actionlint` 1.7.12: clean.

## e2e-main.sh (criterion 5)

- **Refuses** (exit 2, no worktree, no build): `HEAD` and `origin/pub1-hosted-ci` ("not reachable from origin/main"), `deadbeef`, an all-zero SHA and `main..HEAD` ("not a commit"). Too many arguments gives usage, exit 2.
- **Fresh temp worktree** under `$TMPDIR/e2e-main.XXXXXX/wt`, detached at the SHA; the caller's tree is untouched.
- **Real run** against the origin/main tip `e31cd9f`, pool narrowed with `CI_VM_POOL=dictation-vm-2`, and `E2E_RUN` pointed at a wrapper that runs only `e2e_hold_release` and `e2e_startup` (`ci-run.sh` and `e2e-main.sh` unchanged, so the real lock, build and evidence code ran). It built, took the `dictation-vm-2` lock, and both scenarios PASSED. Evidence: `about.txt` and `results.txt` only, 8 KB (the #174 size goal). Afterwards no lock remained in `~/.cache/dictation-e2e`, no worktree or temp dir was left, and `dictation-vm-golden` was not touched. Not run: the full suite (about 40 scenarios), because the point was the script, not the app, and it would hold the Testers' VM for a long time.
- **Cleanup on Ctrl-C and kill:** SIGINT and SIGTERM sent to the process group of a run in progress: exit 130, worktree and temp dir removed. A failing child (exit 1) returns 1 and removes the worktree (`test-e2e-main.sh` covers the evidence size cases). (When a signal went to the script alone, bash ran its trap only after the child finished; a real Ctrl-C signals the whole group, so this does not matter in practice.)
- **Uploads nothing:** no `gh`, `curl` or `wget` in `e2e-main.sh` or `ci-run.sh`; evidence goes to `target/e2e-main-evidence/` (or `E2E_MAIN_EVIDENCE_ROOT`).
- `shellcheck` 0.11 at warning level on `e2e-main.sh`, `check-workflows.sh`, `test-e2e-main.sh`, `gate.sh`: clean.
- Note: `E2E_VM` is ignored by `e2e-main.sh` (the pool is `dictation-vm-2`, then `dictation-vm`); use `CI_VM_POOL` to narrow it. Not a defect: the docs say so.

## Gate

I did not run the full gate (the push hook does). Ran: `actionlint`, `shellcheck`, the shell gate steps above. Coverage not run.

## Untested behaviour found / minors

1. `check-workflows.sh` only checks the top-level `permissions:` block. A job-level `permissions: contents: write` (or `id-token`, `actions: write`) passes. Fork PRs always get a read-only token, and PRs from this repo need the owner's push access, so the risk is low, but criterion 4 reads "every workflow" and a small check for job-level `permissions:` would close it.
2. `scripts/ci-check-tools.sh` is dead self-hosted code ("Checks the self-hosted CI runner ..."): no workflow or doc uses it any more. Delete it.
3. `tests/e2e/run.sh` header (lines 3-4) still says "the T6 job runs it on every push to main"; it is now `scripts/e2e-main.sh` after a merge.
4. The `runs-on` allowlist rejects quoted values and larger runners (`macos-15-xlarge`): safe, but a future legitimate edit will have to change the script. Fine as is.
5. `e2e-main.sh` hard-codes `main` as the default branch, so a rename needs a script edit.

None blocks. No `tech-debt` issues opened yet; the Lead can say if wanted.

## Failures

None.
