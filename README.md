# dictation-app

Hold a hotkey, speak, release, and the text is pasted into the app that has
focus. Mac first, then Windows. Built with Tauri 2, Rust and React.

## Requirements

- Rust: installed by `rustup` from `rust-toolchain.toml`
- Node 24 (see `.nvmrc`)

## Develop

```sh
scripts/install-hooks.sh   # once per clone
scripts/make-dev-cert.sh   # once per Mac: "dictation-app Dev" signing certificate
npm ci
npm run tauri dev
npm run tauri build -- --debug --bundles app   # signed .app; needs the certificate
```

Builds are signed with the self-signed `dictation-app Dev` certificate so
macOS keeps the Accessibility and Microphone permissions across rebuilds.
Without it, `tauri build` stops with "The specified item could not be found
in the keychain".

## Quality gate

Every push and every PR runs the same gate (`scripts/gate.sh`): git's
pre-push hook runs it through `lefthook run pre-push-gate`, and GitHub Actions
runs it on Linux (Ubuntu 24.04; a macOS job is added once the gate has
Mac-specific code). Run it yourself with:

```sh
scripts/gate.sh
```

| Check | Command |
| --- | --- |
| Format | `cargo fmt --all --check`, `npx prettier --check .` |
| Rust lint | `cargo clippy --workspace --all-targets -- -D warnings` |
| Dependencies | `cargo deny check`, `cargo machete` |
| Rust tests | `cargo nextest run --workspace`, `cargo test --workspace --doc` |
| Coverage | `scripts/coverage.sh` (`core`, `transcribe` ≥ 85%, `app` ≥ 70%) |
| TypeScript | `npx tsc -b --noEmit`, `npx eslint . --max-warnings 0`, `npx vitest run` |
| Spelling | `typos` |
| Guards | `scripts/test-guards.sh` |

Tools: `brew install lefthook cargo-deny cargo-nextest cargo-llvm-cov typos-cli`
and `cargo install cargo-machete`. pre-commit (lefthook) runs the fast checks
on staged files: `cargo fmt --check`, `prettier --check`, `eslint`, `typos`.

## Layout

| Path | What it holds |
| --- | --- |
| `crates/core` | Pure logic: coordinator state machine, text, audio maths, settings. No I/O. |
| `crates/ports` | Traits only: the boundaries to the OS and the network. |
| `crates/platform` | OS adapters (hotkey, audio, paste, permissions, secrets) and test fakes. |
| `crates/transcribe` | Speech-to-text engines behind one trait. |
| `crates/app` | Runs the coordinator against the ports (package `dictation-pipeline`). No Tauri types. |
| `src-tauri` | Thin Tauri shell: commands, events, tray, windows. |
| `src` | React UI. |
| `docs/milestones` | One spec per milestone. |
