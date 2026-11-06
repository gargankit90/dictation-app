# Contributing

Thanks for taking a look. This is a learning project; small, focused changes
are welcome.

## Build and test

On a Mac, follow the README's
[Build it on a Mac](README.md#build-it-on-a-mac) steps in order (Xcode Command
Line Tools, Rust, Node 24, the gate tools, `npm ci`, the git setup and the dev
certificate). Then:

```sh
cargo build --workspace
scripts/gate.sh            # the full quality gate; run it before you push
```

`scripts/gate.sh` runs formatting, clippy, `cargo deny`, tests, coverage,
TypeScript checks and the licence notice check. The tools it needs are listed
in the README's steps. Add unit tests with your change.

On a clean tree the gate skips itself when the same input already passed a
full run in the last 4 hours, and a change that only touches plain prose docs
runs just the format, spelling and link checks. `scripts/gate.sh --full` runs
everything; CI always does.

## Pull requests

- Branch from `main` and open a pull request to `main`. Do not push to `main`.
- Use [Conventional Commits](https://www.conventionalcommits.org/) for commit
  messages (`feat(hotkey): ...`, `fix: ...`, `docs: ...`).
- Keep each PR to one change and say how you tested it.
- CI runs the gate on GitHub-hosted runners.
- The VM end-to-end tests (`tests/e2e/`) need a local Mac with test VMs. The
  maintainer runs them; you do not need to.

## Licensing: no copying from GPL code

The project is MIT. Only adapt code from MIT or Apache-2.0 projects, and credit
it in `NOTICE`. Do not copy code from GPL projects, in particular FluidVoice
and VoiceInk (both GPLv3). Reading them for ideas is fine; copying their code
is not. By contributing you agree your work is released under the MIT licence.

## The maintainer's AI-agent workflow

`AGENTS.md` and `.claude/` describe how the maintainer runs their own
AI-agent workflow (developer, tester and reviewer agents). Contributors do not
need to follow it, and the milestone specs in `docs/history/milestones/` are the
maintainer's planning notes.

## Security

Report vulnerabilities privately; see [SECURITY.md](SECURITY.md).
