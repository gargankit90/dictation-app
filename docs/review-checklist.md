# Review checklist

The Code Reviewer works through every section for each PR and records each
one as OK or with finding numbers. Severity guide:

- **blocker**: wrong behaviour, a security or privacy leak, a licence
  violation, or a broken rule from `AGENTS.md`'s hard rules.
- **major**: a missing test for a criterion, an architecture rule broken, an
  unexplained suppression, an error path that loses or hides errors.
- **minor**: naming, comments, small simplifications.

`.claude/hooks/guard-git.sh` is a best-effort pattern list in front of the
real guard, git's pre-push hook with `guard-push.sh`. A way around
`guard-git.sh` is **major** only if the pre-push hook doesn't stop the same
action (e.g. disabling hooks, or merging or deleting refs through `gh`,
which never runs the hook); otherwise it's **minor**.

## 1. Scope

- [ ] The PR does what `docs/milestones/<id>.md` asks, and nothing outside its
      scope.
- [ ] Every acceptance criterion in the PR notes maps to code and a test.

## 2. Correctness

- [ ] Logic is right for normal input, empty input, errors and edge cases.
- [ ] State machines can't get stuck; every path returns to Idle.
- [ ] No races: shared state, channels and async tasks are used safely.
- [ ] Cancellation and timeouts are handled; nothing is left running.

## 3. Security and privacy

- [ ] No API key or transcript in logs at info level or above, in errors shown
      to users, in fixtures, or anywhere in the repo.
- [ ] Keys only through `SecretStore`/keychain; the UI can't read a key back.
- [ ] Audio stays in memory unless debug mode is on.
- [ ] No new network destinations beyond the transcription provider; no
      analytics.
- [ ] Tauri capabilities grant each window only what it needs; CSP unchanged
      or stricter.
- [ ] Clipboard writes carry the transient/concealed markers; restore only if
      our marker is still there.

## 4. Architecture

- [ ] Dependencies point inward; `core` depends on nothing in the workspace.
- [ ] `#[cfg(target_os)]` only in `platform`.
- [ ] `unsafe` only in `platform`, each block with a sound `// SAFETY:`.
- [ ] The coordinator stays pure (no I/O, time or threads in `core`).
- [ ] Tauri commands are thin; logic lives in the crates.
- [ ] No new dependency between crates unless this milestone uses it.

## 5. Rust and TypeScript practice

- [ ] No `unwrap`/`expect`/`panic!`/`todo!` outside tests.
- [ ] Library errors are `thiserror` enums; `anyhow` only in `src-tauri`.
- [ ] Nothing blocks the audio callback or the main thread.
- [ ] Every `#[allow]` / `eslint-disable` has a reason, and the reason holds.
- [ ] Public items have short doc comments; comments explain why.
- [ ] TS is strict with no `any`; UI talks to Rust only through typed
      commands/events.

## 6. Licences and reuse

- [ ] Nothing copied from FluidVoice or VoiceInk (GPLv3). Compare suspicious
      code against those repos in `~/Documents/Dictation App/`.
- [ ] Code adapted from MIT/Apache repos is credited in `NOTICE` and at the
      code.
- [ ] New dependencies have permissive licences (`cargo deny` passes).

## 7. Tests

- [ ] Each behaviour has a test at the cheapest level that catches it.
- [ ] Tests check behaviour, not implementation details; they would fail if
      the code were wrong.
- [ ] No sleeps for timing in unit tests; a `Clock` is injected.
- [ ] Coverage meets thresholds (`core`, `transcribe` ≥ 85%; `app` ≥ 70%).

## 8. Git and PR

- [ ] Small Conventional Commits; tests with or before the code.
- [ ] PR notes are complete (template in `.claude/agents/developer.md`).
- [ ] No changes to the guards, `AGENTS.md` or milestone specs by the
      Developer.
- [ ] Minors not fixed have a GitHub issue labelled `tech-debt`, linked from
      the PR notes.
