# dictation-app

Hold a hotkey, speak, release, and the text is pasted into the app that has
focus. Mac first, then Windows. Built with Tauri 2, Rust and React.

## Requirements

- Rust: installed by `rustup` from `rust-toolchain.toml`
- Node 24 (see `.nvmrc`)

## Develop

```sh
scripts/install-hooks.sh   # once per clone
npm ci
npm run tauri dev
```

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
