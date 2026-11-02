# AGENTS.md

Rules for every agent working on dictation-app: Claude Code and Antigravity
alike. Read this first, then your role file:

| Role | Agent | Role file |
| --- | --- | --- |
| Lead / coordinator | the owner + Claude Code, Orca coordinator in the `main` worktree | this file |
| Developer | Claude Code, Orca worker | `.claude/agents/developer.md` |
| Tester | Claude Code, Orca worker | `docs/agents/tester.md` |
| Code Reviewer | Antigravity (Gemini), Orca worker; also merges | `docs/agents/reviewer.md` |

Claude Code runs unattended inside its guard hook and deny rules, so the
Tester needs no approvals. The Reviewer is a different model family on
purpose, for independence; its read-only commands are pre-approved and only
writes, pushes and the merge need the owner's (or the Lead's) approval.

## The app

Hold a hotkey, speak, release, and the text is pasted into the app that has
focus. Mac first, then Windows. Tauri 2 + Rust + React/TypeScript. Cloud
transcription first (Groq/OpenAI, testers bring their own key); local models
later.

How it is built: `docs/ENGINEERING.md`. What to build next:
`docs/milestones/<id>.md`.

## Layout and dependency rules

```
crates/core        dictation-core        pure logic, no I/O, no OS, no Tauri, no async runtime
crates/ports       dictation-ports       traits only
crates/platform    dictation-platform    OS adapters + fakes (feature "testing")
crates/transcribe  dictation-transcribe  speech-to-text engines
crates/app         dictation-pipeline    runs the coordinator against the ports; no Tauri types
src-tauri          dictation-app         thin Tauri shell
src                React UI
```

- Dependencies point inward: `src-tauri → app → ports ← platform/transcribe`;
  `core` depends on nothing in the workspace.
- `#[cfg(target_os)]` only in `platform`.
- `unsafe` only in `platform`, each block with a `// SAFETY:` comment.

## Commands

```sh
scripts/install-hooks.sh               # once per clone: git hooks incl. the push guard
cargo build --workspace
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --workspace
npm ci && npm run build
npm run tauri dev                      # run the app
scripts/make-dev-cert.sh               # once per Mac: the signing certificate
npm run tauri build -- --debug --bundles app
```

The quality gate (`docs/ENGINEERING.md`) is `scripts/gate.sh`. git's
pre-push hook runs it after the push guard, through
`lefthook run pre-push-gate`, and CI runs it on every PR. Run it before you
push:

```sh
lefthook run pre-push-gate             # the whole gate, as pre-push runs it
scripts/gate.sh                        # the same, without lefthook
```

It runs, stopping at the first failure:

```sh
cargo fmt --all --check
npx prettier --check .
cargo clippy --workspace --all-targets -- -D warnings
cargo deny check
cargo machete
cargo nextest run --workspace
cargo test --workspace --doc
scripts/coverage.sh                    # core, transcribe >= 85%; app >= 70%
npx tsc -b --noEmit
npx eslint . --max-warnings 0
npx vitest run
typos
scripts/test-guards.sh                 # push guard and Claude hook cases
```

## Workflow: every change goes through a pull request

1. The Lead writes `docs/milestones/<id>.md` with numbered acceptance
   criteria tagged `[auto]` or `[manual]`.
2. **Developer** works on branch `m<id>-<slug>` in its own worktree, makes
   small Conventional Commits, runs the gate, pushes, and opens a PR to `main`
   with detailed notes (template in `.claude/agents/developer.md`).
3. As soon as the PR is open, the **Tester** and the **Code Reviewer** start
   in parallel, each time-boxed to 30–45 minutes (report what you have when
   time is up):
   - **Tester** checks each acceptance criterion and runs e2e tests, writes
     `test-reports/<id>-run<k>.md`, and posts a PR comment starting
     `Tester: PASS` or `Tester: FAIL`. CI and lefthook already run the gate;
     write a custom test harness only when a criterion needs one.
   - **Code Reviewer** reviews the diff with `docs/review-checklist.md`,
     comments inline, writes `reviews/<id>-round<k>.md`, and posts a PR
     comment starting `Reviewer: APPROVED` or `Reviewer: CHANGES REQUESTED`.
4. On a fail or changes requested, the Developer fixes **all** blocker and
   major findings from both in one round and pushes, then the Tester and
   Reviewer re-check only what changed. After 2 rounds, the Lead steps in.
   Minors never block: each becomes a `tech-debt` issue.
5. The **Lead** does the `[manual]` checks and notes them on the PR. The next
   milestone's Developer may start while this PR is in review.
6. The **Code Reviewer merges** the PR (merge commit) once `Tester: PASS`,
   its own `Reviewer: APPROVED` and the Lead's manual checks are all on it,
   then tags `m<id>`.

The Lead sends the owner one status line per stage change (PR opened,
PASS/FAIL, APPROVED/CHANGES REQUESTED, merged). The owner's to-dos are GitHub
issues labelled `owner`.

All agents use the owner's GitHub account through `gh`. GitHub won't let an
account approve its own PR, so the `Tester:` and `Reviewer:` comments are the
record of approval.

## Hard rules

- **Never push to `main`**, never force push (`--force`, `-f`,
  `--force-with-lease`, `+ref`), never delete remote branches or move tags.
  `main` changes only by merging a PR.
- **Never skip hooks**: no `--no-verify`, no `git commit -n`, no changing
  `core.hooksPath`, no `LEFTHOOK=0`.
- **Never edit the guards**: `scripts/guard-push.sh`, `scripts/pre-push`,
  `scripts/install-hooks.sh`, `lefthook.yml`, `.claude/settings.json`,
  `.claude/hooks/`. Only the owner changes them.
- Only the Code Reviewer merges, and only when step 6's conditions hold.
- The guards change only when they block legitimate work or miss a
  dangerous case that git's pre-push guard doesn't also stop. A way around
  `.claude/hooks/guard-git.sh` that the pre-push guard stops is a
  `tech-debt` issue, not a reason to hold a PR.
- Stay inside your role's "may edit" list.
- **Computer use happens only inside the Lume VM** (`cua-vm` MCP server),
  never on the owner's own screen.
- **Secrets:** API keys live only in the OS keychain. Never put a key in the
  repo, a log, a test fixture, a PR comment or a report. `GROQ_API_KEY` enters
  the VM only for the opt-in real-API e2e test.
- **Privacy:** never log API keys or transcripts at info level; audio stays in
  memory unless debug mode is on; analytics stay off.
- **Licences:** adapt code only from MIT/Apache repos (Handy, OpenWhispr,
  FreeFlow, Muesli, transcribe.cpp, FluidAudio) and credit them in `NOTICE`.
  FluidVoice and VoiceInk are GPLv3: read for ideas, never copy.
- Ask the Lead before anything outward-facing beyond pushing your branch,
  commenting on PRs, and opening or commenting on GitHub issues for
  follow-ups (labels: `tech-debt`, `owner`, `guard`, `spec`, `ci`,
  `process`, `manual-check`).

These rules are enforced locally because GitHub Free can't protect branches
on a private repo:

- git's pre-push hook, installed by `scripts/install-hooks.sh` together
  with a copy of `scripts/guard-push.sh` next to it, runs that copy on every
  push from every worktree. It refuses pushes to `main`, non-fast-forward
  pushes, branch deletions and tag moves, and fails closed if the copy is
  missing. Then it runs the checkout's `scripts/pre-push` (the quality gate,
  from M0.3) if that checkout has one. Re-run `scripts/install-hooks.sh`
  whenever `scripts/guard-push.sh` changes. Not lefthook: lefthook skips
  pre-push jobs when it sees no changed files.
- Claude Code loads `.claude/settings.json`. Its `PreToolUse` hook
  (`.claude/hooks/guard-git.sh`) blocks force pushes, pushes to `main`,
  skipping hooks, `gh pr merge` and edits to the guards.
- Antigravity runs with Manual permissions, so the owner approves its
  commands.

## Reference repos

Cloned next to this repo, in the same parent folder: Handy, FluidVoice,
openwhispr, FluidAudio, transcribe.cpp, freeflow, muesli, VoiceInk. Commits
and what each is good for: the plan's "Reference repos" section and
`docs/ENGINEERING.md`.
