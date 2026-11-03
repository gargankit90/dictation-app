# AGENTS.md

This file instructs the maintainer's AI agents (Orca workers). Human
contributors: see `CONTRIBUTING.md`.

Rules for every agent working on dictation-app, Claude Code and Codex alike.
Read this first, then your role file:

| Role | Agent | Model | Role file |
| --- | --- | --- | --- |
| Lead / coordinator | the owner + Claude Code, Orca coordinator in the `main` worktree | Claude | this file |
| Developer | Claude Code, Orca worker | Sonnet 5.5 (Opus 5.5 for hard platform work) | `.claude/agents/developer.md` |
| Tester | Claude Code, Orca worker | Sonnet 5.5 | `docs/agents/tester.md` |
| Code Reviewer | Codex (OpenAI), Orca worker; also merges | gpt-6.1-sol | `docs/agents/reviewer.md` |

Claude Code runs unattended inside its guard hook and deny rules, so the
Tester needs no approvals. The Reviewer is always the other model family from
the PR's author, for independence: when Codex writes a PR (for example one
that edits Claude's own guard hook), a Claude worker (Opus 5.5) reviews it.
Codex runs as an Orca worker under the Lead's supervision; the Lead (or the
owner) approves its writes, pushes and merges.

## The app

Hold a hotkey, speak, release, and the text is pasted into the app that has
focus. Mac first, then Windows. Tauri 2 + Rust + React/TypeScript. Cloud
transcription first (Groq/OpenAI, testers bring their own key); local models
later.

How it is built: `docs/ENGINEERING.md`. What to build next:
`docs/history/milestones/<id>.md`.

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
scripts/make-dev-cert.sh               # once per Mac: the signing certificate
npm ci && npm run build
npm run tauri dev                      # run the app
npm run tauri build -- --debug --bundles app
```

The quality gate is `scripts/gate.sh`; it is the authority for what runs
(`docs/ENGINEERING.md` summarises it). git's pre-push hook runs it after the
push guard, and CI runs it on every PR. Run it before you push:

```sh
scripts/gate.sh                        # the whole gate, stops at the first failure
```

## Workflow: every change goes through a pull request

1. The Lead writes `docs/history/milestones/<id>.md` with numbered acceptance
   criteria tagged `[auto]` or `[manual]`.
2. **Developer** works on branch `m<id>-<slug>` in its own worktree, makes
   small Conventional Commits, runs the gate, pushes, and opens a PR to `main`
   with detailed notes (template in `.claude/agents/developer.md`).
3. As soon as the PR is open, the **Tester** and the **Code Reviewer** start
   in parallel, each time-boxed to 30–45 minutes (report what you have when
   time is up):
   - **Tester** checks each acceptance criterion and runs e2e tests, writes
     `docs/history/test-reports/<id>-run<k>.md`, and posts a PR comment starting
     `Tester: PASS` or `Tester: FAIL`. CI and lefthook already run the gate;
     write a custom test harness only when a criterion needs one.
   - **Code Reviewer** reviews the diff with `docs/review-checklist.md`,
     comments inline, writes `docs/history/reviews/<id>-round<k>.md`, and posts a PR
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
7. After the merge, the **Lead** runs `scripts/e2e-main.sh <merge sha>` on the
   owner's Mac and posts `VM e2e on <sha>: N passed, M failed` on the merged
   PR. The evidence stays local; no workflow reaches the owner's Mac.

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
- **Computer use happens only inside the Lume VMs**, never on the owner's
  own screen: Developers use `dictation-vm` (`cua-vm` MCP server), Testers
  use `dictation-vm-2` (`cua-vm-2`), and both go through `tests/e2e/run.sh`
  (`tests/e2e/README.md`).
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

The repository is public. GitHub rulesets enforce protection server-side for
all accounts, including the owner (bypass: never):

- `main-public` requires a PR (0 approvals), the required checks
  `Quality gate (Linux)` and `Platform crate (macOS)`, and a branch that is up
  to date with `main`; it prevents force pushes and deletion of the default
  branch. So update the branch ("Update branch" is enabled on the PR, or merge
  `origin/main` in) and wait for the checks again before merging.
- `immutable-tags` prevents tag updates, deletion and force pushes.
- `all-branches-no-force-no-delete` prevents force pushes to, and deletion
  of, every branch.

CI runs on GitHub-hosted runners. The hard rules above still apply to every
agent. Local checks add defence in depth:

- git's pre-push hook, installed by `scripts/install-hooks.sh` together
  with a copy of `scripts/guard-push.sh` next to it, runs that copy on every
  push from every worktree. It refuses pushes to `main`, non-fast-forward
  pushes, branch deletions and tag moves, and fails closed if the copy is
  missing. Then it runs the checkout's `scripts/pre-push` (the quality gate). Re-run `scripts/install-hooks.sh`
  whenever `scripts/guard-push.sh` changes. Not lefthook: lefthook skips
  pre-push jobs when it sees no changed files.
- Claude Code loads `.claude/settings.json`. Its `PreToolUse` hook
  (`.claude/hooks/guard-git.sh`) keeps role checks (the Code Reviewer merges),
  hook bypass checks, guard-edit checks (only the owner changes guards), and
  destructive repository settings checks. It does not inspect push commands
  or git aliases. Edit/Write deny rules protect guard files. Existing static
  push deny rules remain in settings.json pending the owner's decision.

## Reference repos

Cloned next to this repo, in the same parent folder: Handy, FluidVoice,
openwhispr, FluidAudio, transcribe.cpp, freeflow, muesli, VoiceInk. Licence
rules: `docs/ENGINEERING.md`; credits for adapted code: `NOTICE`.
