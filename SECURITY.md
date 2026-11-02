# Security policy

## Reporting a vulnerability

Please report vulnerabilities privately, not in a public issue or PR. Use
GitHub's private vulnerability reporting: open the repository's **Security**
tab and choose **Report a vulnerability**. I will acknowledge a report as soon
as I can; this is a one-person learning project, so there is no fixed response
time.

## Scope

In scope:

- the app in this repository (`src-tauri`, `crates/`, `src/`), including how
  it handles audio, transcripts, settings and API keys;
- the build, signing and update scripts under `scripts/`;
- dependencies we ship, when a vulnerability affects how the app uses them.

Out of scope: vulnerabilities in the transcription providers (Groq, OpenAI),
macOS itself, or upstream projects with no effect on this app (report those
upstream), and anything needing physical access to an unlocked machine.

## API keys

API keys live only in the user's macOS Keychain. They are never written to the
repository, settings files, logs or test fixtures. If you find a path where a
key or a transcript is stored or logged in plain text, treat it as a
vulnerability and report it privately.
