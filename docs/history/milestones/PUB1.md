# PUB1: CI off the owner's Mac

Part of making the repository public. Today every CI job runs on a
self-hosted runner on the owner's Mac, in the owner's logged-in session with
the login keychain unlocked. On a public repo a pull request can edit
workflow files and reach that runner. GitHub-hosted runners (Linux and macOS)
are free on public repos. Audit: section B1 of the public-readiness audit.

## Acceptance criteria

1. [auto] No workflow under `.github/workflows/` uses a self-hosted runner: no
   `self-hosted`, `owner-mac` or `macOS` self-hosted label anywhere (a grep
   check added to `scripts/test-guards.sh` or a small new gate check script,
   wired into `scripts/gate.sh`).
2. [auto] `ci.yml`: the `gate` job runs on GitHub-hosted `ubuntu-24.04` with
   the Tauri Linux prerequisites and the pinned gate tools (see
   `docs/ENGINEERING.md` and the history of `ci.yml` before commit
   `87aeddc`); the `macos` job runs on a GitHub-hosted Apple Silicon macOS
   image (e.g. `macos-15`). Both run for pull requests from forks and from
   this repo, and on push to `main`. No job reads secrets.
3. [auto] `dmg.yml` runs on hosted macOS, ad-hoc signed (no owner certificate,
   no secrets).
4. [auto] Every workflow: top-level `permissions: contents: read`; actions
   pinned to full commit SHAs; `actions/checkout` with
   `persist-credentials: false`; no `pull_request_target`; no `workflow_run`;
   `workflow_dispatch` only where harmless on hosted runners.
5. [auto] `.github/workflows/e2e.yml` is removed. Replaced by
   `scripts/e2e-main.sh`, which the Lead runs locally after each merge: it
   takes an `origin/main` commit (default: current `origin/main` tip),
   refuses any SHA not reachable from `origin/main`, checks it out into a
   fresh temporary worktree, runs `tests/e2e/run.sh` with the existing VM
   pool lock, and writes evidence the same way the workflow did (keep #174's
   size concerns in mind). shellcheck-clean. Closes #168.
6. [auto] `docs/ENGINEERING.md` (CI section) and `tests/e2e/README.md`
   describe the new setup: hosted CI for every PR and push; VM e2e run
   locally by the Lead via `scripts/e2e-main.sh`; why (public repo, no
   workflow may reach the owner's Mac).
7. [manual] After the repo is public: the first hosted CI run on `main` is
   green, and a test PR from a fork runs only on hosted runners.
