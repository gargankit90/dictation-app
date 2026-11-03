# D1 test report, run 1

PR: #194 at c5ec243
Result: PASS (one minor, no blockers)

| # | Criterion | Test(s) | Result | Notes |
| --- | --- | --- | --- | --- |
| 1 | README top: description, status, Try it, How it was built, demo | Read README.md; checked link targets | PASS | All pieces present. The spec asked for a `<!-- demo GIF goes here -->` comment and no image; the PR (commit c5ec243) instead embeds the real `docs/media/demo.gif`. That is the intended end state and the GIF is real (see Demo GIF), so I count it as met. |
| 2 | One ordered Mac quickstart; "Development prerequisites" | Dry review against scripts/gate.sh, rust-toolchain.toml, .nvmrc, package.json engines, ci.yml; ran install-check commands | PASS | Order is right: CLT, rustup, Node 24, gate tools, clone, `npm ci`, hooks, dev cert, `tauri dev`. lefthook is installed before the hook installer (the reviews' complaint). Heading renamed. Minor m1 below. |
| 3 | Layout table rows | Read table; checked the paths exist | PASS | `scripts/`, `tests/e2e/`, `third_party/` (both PATCHES.md exist), `docs/`, paper trail with "moving to docs/history/ soon". |
| 4 | TESTERS.md: Apple Silicon, macOS 13+, wizard steps, Settings recovery, Groq/OpenAI, Report a bug | Compared with src/Onboarding.tsx and src/Settings.tsx | PASS | See "Onboarding comparison". |
| 5 | Issue template + config | Parsed both with PyYAML; checked the form schema | PASS | See "Issue templates". |
| 6 | CI paragraph exact | Read README "Quality gate" | PASS | "hosted runners for every pull request and on pushes to `main`"; VM e2e run by the maintainer locally after each merge. Closes #181 as intended. |

## Links (README.md, docs/TESTERS.md, CONTRIBUTING.md)

- Relative links all resolve: docs/media/demo.gif, docs/TESTERS.md, AGENTS.md, CONTRIBUTING.md, SECURITY.md, LICENSE, NOTICE, ../SECURITY.md. The anchor `README.md#build-it-on-a-mac` matches the heading "Build it on a Mac".
- External, logged out (curl): 200 for console.groq.com/keys, the issue chooser (issues/new/choose), v2.tauri.app/start/prerequisites/, sh.rustup.rs, conventionalcommits.org, linkedin.com/in/ankitrgarg/, the repo .git URL and SECURITY.md on the default branch. `platform.openai.com/api-keys` answers 403 to curl (bot protection on a login-gated page; not a dead link, a browser reaches it).
- `https://github.com/gargankit90/dictation-app/releases/latest`: HTTP 302 to `/releases`. With no release yet, a visitor lands on the Releases page showing no releases. Fine until 0.1.0 is published.
- `.../dictation-app-releases/releases/latest` returns 404; it appears on the PR branch only because the branch predates R1. See "R1 and merge".

## Quickstart check

Versions: .nvmrc 24, package.json engines `>=24 <25`, rust-toolchain.toml 1.99.1 (rustup reads it). CI pins cargo-about 0.9.3, deny 0.20.2, machete 0.9.2, nextest 0.9.149, llvm-cov 0.9.1, typos 1.51.1. README pins machete 0.9.2 and about 0.9.3 with `--locked`: matches CI. Gate tools used by scripts/gate.sh: fmt, prettier, clippy, deny, machete, nextest, llvm-cov (via coverage.sh), jq (coverage.sh), typos, cargo-about (licence check); step 4 installs all of them. `xcode-select`, rustup and `brew install node@24` (keg-only, hence the PATH line) are right. Homebrew has node@24 (24.22.2), lefthook, cargo-deny, cargo-nextest, cargo-llvm-cov, typos-cli and jq.

Safe commands run here: lefthook 2.2.2, cargo-nextest 0.9.149, cargo-deny 0.20.2, cargo-machete 0.9.2, typos-cli 1.51.1, cargo-about 0.9.3, jq 1.7.1, node v24.22.1. (cargo-llvm-cov has no direct `--version`; it is a cargo subcommand.) I also confirmed `cargo check -p dictation-app` works with `dist/` absent, so CONTRIBUTING's `cargo build --workspace` doesn't need a prior `npm run build` on a fresh clone (a check, not a full build).

I could not run the sequence on a clean Mac: this is a dry review plus the checks above.

Minor:

- m1: "CI pins these versions" is true of CI, but `brew install` gives the latest. Today brew's cargo-nextest is 0.9.150 (CI 0.9.149) and jq is 1.8.2; the rest match. Harmless now, may drift. Suggest naming the CI versions or saying the latest is fine. (Not blocking; for a `tech-debt` issue if wanted.)

## Onboarding comparison (TESTERS.md section 3 vs src/Onboarding.tsx)

Matches: window title "Set up dictation", "Step N of 4", Back/Continue, order microphone, Accessibility, key, try. Labels: "Allow microphone"; "Open System Settings" (shown when denied, and the Accessibility button); "Relaunch" (appears only after Open System Settings was clicked, as the guide's order implies); Provider select with Groq/OpenAI; "Test and continue"; key hint "Get one at console.groq.com/keys"; key tested before it is saved and kept in the keychain; "Hold the right Option key"; "It works."; "Done". Steps 1 and 2 move on by themselves (polling). Settings labels ("Test key", "Key saved", "Launch at login", Hotkey, Language) exist in src/Settings.tsx; the tray "Check for updates…" and "dictation-app x.y.z is the newest version." exist in src-tauri/src/updater.rs; "Reveal log" is in lib.rs. No mismatch found.

## Demo GIF

docs/media/demo.gif: 247,718 bytes (0.24 MB, limit 3 MB), 900x618, 10.1 s, 121 frames. I extracted frames 0, 40, 80, 115 and an overlay strip with ffmpeg and viewed them: empty TextEdit; "Recording" overlay; "Transcribing..." overlay; the sentence "dictation that just works, hold the key, speak, and let go." in TextEdit. The menu bar shows only TextEdit, the clock and system icons: no username, no other apps, no notifications; the wallpaper is the stock one. Nothing personal. The caption says VM, macOS `say` audio and Groq Whisper, which fits the e2e setup. I can't verify "a real Right Option hold" or Groq from the pixels, only that the overlay states are real.

## Issue templates

bug.yml: top-level name, description, labels, body. The body has a markdown intro, 3 inputs (macos, mac, version) and 4 textareas (steps, expected, actual, log). Ids are unique, each field has a label, `required` is set (log optional). It covers every field criterion 5 lists. config.yml: `blank_issues_enabled: true` and one contact link to the repo's SECURITY.md (returns 200). I could not see GitHub's rendering before the template is on the default branch; the schema is valid by inspection.

## R1 and merge

The branch base predates R1 (#190). The TESTERS.md hunks are all outside section 1 (lines 12-24 untouched). `git merge-tree --write-tree origin/main HEAD` completes with exit 0 (no conflicts), and in the merged tree section 1 reads `https://github.com/gargankit90/dictation-app/releases/latest`, so R1's wording is kept. Nothing from the test merge was pushed.

## CI

`gh pr checks 194`: Quality gate (Linux) pass 8m39s; Platform crate (macOS) pass 1m57s.

## Not covered

No clean-Mac run of the quickstart; no GitHub rendering of the form; links checked once, logged out, from this machine.
