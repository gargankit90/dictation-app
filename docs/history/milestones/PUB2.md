# PUB2: MIT licence and public-repo docs

Part of making the repository public for learning. The owner chose the MIT
licence (audit B3 and "After launch" items 9-10).

## Acceptance criteria

1. [auto] Root `LICENSE`: the standard MIT text, "Copyright (c) 2026 Ankit Garg".
2. [auto] Cargo workspace `[workspace.package] license = "MIT"`, inherited
   (`license.workspace = true`) by every original crate (`crates/*`,
   `src-tauri`); `third_party/` crates keep their own upstream licences
   untouched. `package.json` has `"license": "MIT"`. Keep `publish = false`
   and npm `"private": true`.
3. [auto] `deny.toml` and `about.toml` no longer describe the project as
   private or unlicensed; `cargo deny check` and the licence notice generation
   still pass, and the generated third-party notice is current.
4. [auto] `NOTICE` keeps every upstream credit, and the full MIT permission
   text plus copyright for code adapted from Handy
   (`crates/platform/src/insert/macos.rs`, see `NOTICE`) is available to
   recipients (in `NOTICE` or a referenced licence file), and
   `third_party/handy-keys` and `third_party/coreaudio-rs` keep their licence
   files.
5. [auto] `SECURITY.md`: how to report a vulnerability privately (GitHub
   private vulnerability reporting), what is in scope, and that API keys live
   only in the user's Keychain.
6. [auto] `CONTRIBUTING.md` for outside contributors: build and test commands
   (`scripts/gate.sh`), Conventional Commits, PRs to `main`, that VM e2e tests
   are run by the maintainer, and that `AGENTS.md` and `.claude/` describe the
   owner's own AI-agent workflow (contributors need not follow it). The
   no-GPL-copying rule (FluidVoice, VoiceInk) stated plainly.
7. [auto] `README.md`: a Licence section (MIT, see `NOTICE` for third-party
   code); the CI paragraph says CI runs on GitHub-hosted runners for every PR
   and push, and VM end-to-end tests are run by the maintainer on a local Mac;
   links to `SECURITY.md` and `CONTRIBUTING.md`; a one-line note that this is
   a learning project built with AI agents (link `AGENTS.md`).
8. [auto] Review `AGENTS.md` and `CLAUDE.md` for anything machine-specific
   that should not be public (absolute home paths, account names beyond the
   GitHub handle). Replace absolute paths like `~/Documents/Dictation App/`
   with a neutral description only if it does not break agent instructions;
   list what changed or was deliberately kept in the PR notes. No rule's
   meaning changes.
