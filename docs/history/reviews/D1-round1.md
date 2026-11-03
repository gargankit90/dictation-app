Reviewer: CHANGES REQUESTED

PR: #194 at c5ec2430e457ea262ea4d516d758c31dcacc372f. Round 1; documentation/static review, no merge.

## Findings by severity

No blockers. One major and two minors.

| # | Severity | file:line | Finding | Suggested fix |
| --- | --- | --- | --- | --- |
| 1 | major | README.md:53 | The explicitly clean-Mac quickstart calls `brew` without installing Homebrew. Neither macOS nor Xcode Command Line Tools supplies it, so the advertised ordered sequence stops with `command not found: brew` before Node is installed. Mentioning nvm/fnm does not solve step 4's mandatory brew invocation. | Add Homebrew installation/setup (including its shellenv instruction) before the first brew command, or explicitly require an already installed Homebrew and stop calling this a clean-Mac sequence. |
| 2 | minor | docs/TESTERS.md:55 | Previously denied microphone recovery says only to enable it in System Settings; line 53 promises automatic progress. On macOS 15 this requires relaunch, as ENGINEERING's microphone rule documents. The later Settings recovery paragraph mitigates this, so it does not block. | Say to choose menu-bar > Relaunch dictation-app after enabling Microphone. Tracked in [#196](https://github.com/gargankit90/dictation-app/issues/196). |
| 3 | minor | README.md:56 | Brew installs current formula versions, while this comment references CI pins without showing their versions. D1 criterion 2 asks for the versions the gate uses; only machete/about show them. | List the CI pins or link the canonical ci.yml tool line and explain local Brew versions may differ. Tracked in [#197](https://github.com/gargankit90/dictation-app/issues/197). |

## Earlier public-review findings: areas A–C

Each row identifies a distinct finding from the two supplied reviews; shared findings are retained under both authors for traceability.

| Review | Area / finding | Resolution in D1 |
| --- | --- | --- |
| Codex | A: missing status/platform/provider/try link; confusing development requirements | Fixed: alpha, macOS 13+, aarch64-only DMG, both providers, planned Windows/local, and linked tester guide. Public download availability still depends on R1. |
| Codex | A: no demo, LinkedIn link or learning explanation | Fixed: actual GIF, disclosure of VM/synthetic speech, profile link and readable agent paragraph. |
| Codex | A: About description/topics/homepage | Not fixed; explicitly owner-owned metadata, not D2/D3/R1 or a D1 code change. Fine as a separate owner task. |
| Codex | B: Settings-first guide omits wizard/provider choice | Fixed for normal onboarding: four correct steps, both providers, per-provider key handling and Settings recovery. Minor #2 above qualifies denied-mic recovery. |
| Codex | B: no bug destination/current-version source/public privacy instructions | Fixed structurally: chooser, form, version dialog, review-before-attachment and SECURITY link. Version lookup requires a successful update check; failed/offline checks do not expose the version. R1 owns making the release/update endpoint usable. |
| Codex | C: clean-machine ordering and frontend preparation | Partly fixed: tool installation precedes hooks, CLT/rustup/Node/clone/npm ci included; remaining broken step is finding #1. `tauri dev` starts Vite and `tauri build` runs npm build automatically via tauri.conf.json, so a separate frontend build is unnecessary for those commands. |
| Claude | A: no GIF/status/Try it/LinkedIn | Fixed, with R1 dependency for an actual public download. |
| Claude | A: repository settings lack public context | Not fixed; separate owner metadata task, acceptable outside D1. |
| Claude | A: bare agent link dumps reader into rules | Fixed: short How it was built explanation comes before AGENTS link. |
| Claude | B: download points to empty releases repository | Not fixed here, intentionally R1-owned. `gh release list` for this repo is still empty at review time; README's download step promises a build that is not published yet. Coordinate R1/publication before advertising Try it. |
| Claude | B: no bug reporting destination | Fixed: issue chooser/form, environment/steps/logs and privacy/security directions. |
| Claude | B: Intel restriction absent | Fixed: explicit M1-or-newer requirement. |
| Claude | B/R1: release/updater references across TESTERS, engineering, scripts/config | Not fixed; expressly R1-owned, acceptable scope exclusion. Do not treat current updater/download claims as verified operationally. |
| Claude | C: prerequisites/tools appear after hook installation | Partly fixed: reordered correctly; finding #1 remains. Tool version documentation is minor #3. |
| Claude | C: layout omits scripts/e2e/vendors/docs/history | Fixed: each has a useful row. Actual history move remains D3-owned as stated. |
| Claude | C: For testers explains packaging instead of trying | Fixed: Try it precedes developer material; packaging moved down. Release availability remains R1. |
| Claude | C: stale private/billing engineering paragraph | Not fixed; not required by D1, covered by agent/engineering cleanup D2 or existing #184. |
| Claude | C: engineering implies Windows adapter exists | Not fixed; D2 engineering cleanup, acceptable outside D1. README correctly says planned. |

Area D history navigation/moves remain D3; area E identities, runbooks and policy cleanup remain D2. README's CI wording also fixes existing #181: hosted PRs and main pushes; local VM e2e after merges.

## Accuracy, demo and privacy

Status matches minimumSystemVersion 13.0, the aarch64 DMG naming, provider configuration and current UI. Node 24 matches .nvmrc/package engines; rustup reads toolchain 1.99.1 with clippy/rustfmt/llvm-tools-preview. Machete/about match CI pins. Signing/packaging descriptions match build-dmg.sh and tauri.conf.json. Tools installed by Homebrew are not guaranteed to equal CI versions.

Read Onboarding.tsx, provider/key flow, tray startup/relaunch and updater dialog strings. Normal first-run labels/order and key testing/storage match. Up-to-date and available dialogs contain current version; failed checks do not. R1 is needed for release availability and the still-old updater endpoint. No new claim of shipped Windows or local transcription.

Inspected sampled GIF frames including a full recording frame: TextEdit, Recording overlay, and the generic demonstration sentence; no private paths, keys or personal documents visible. Caption clearly discloses VM capture, macOS say audio and Groq, without claiming a real microphone recording. Static inspection cannot independently prove which input method/provider produced the capture. Replacing the spec's placeholder with a real demo is a justified improvement, not a scope problem.

Changed text contains generic application paths, public project/profile links and key prefixes only. Bug form asks for logs rather than audio/transcripts, warns against keys and asks users to inspect the zip; security contact is linked. Logging implementation has scrubbing/privacy tests; no new logging or product behavior is introduced.

A curious LinkedIn reader can understand hold/speak/paste, alpha limitations, cloud processing and how to try it within 30 seconds. The remaining public release dependency must be resolved before that download invitation works.

## Checklist and checks performed

1. Scope: D1 criteria mapped above; authorized Lead spec commit noted, no guard changes. Criterion 2 has finding #1; tool-version detail #3. Other criteria implemented, with release dependencies explicitly excluded by spec.
2. Correctness: documentation compared against wizard, providers, tray/updater, Tauri build hooks and CI triggers; findings above.
3. Security/privacy: OK for changed docs/template/media; no product/capability/network changes.
4. Architecture: OK, no code/dependency changes.
5. Rust/TypeScript practice: not applicable to docs-only diff.
6. Licences/reuse: no new dependencies or adapted code; no concerns in changed documentation/demo.
7. Tests: no new behavior needing unit tests; static documentation/source review. No clean-Mac install, live-provider dictation or VM permission experiment performed. Full gate runs through the push hook; not a substitute for clean-machine setup validation.
8. Git/PR: focused conventional docs commits, detailed PR notes, no protected guard edits, minors tracked above; no merge attempted.

Read the whole gh pr diff, both original public reviews, D1 spec, engineering and reviewer checklist; inspected README/TESTERS/CONTRIBUTING, templates, GIF frames, package/toolchain/config, gate/DMG scripts, CI workflow, onboarding/key and updater source. Approval requires fixing finding #1; minors do not block. Public release readiness remains R1's responsibility.
