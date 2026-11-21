# MQ5 review, round 2

PR: #343 at 4215484 (fix commit 46dbd6f)
Verdict: CHANGES REQUESTED

## Findings

| # | Severity | file:line | Finding | Suggested fix |
| - | -------- | --------- | ------- | ------------- |
| 3 (round 1) | major | site/privacy.html:347; docs/privacy.md:89; src/settings/Privacy.tsx:33; docs/TESTERS.md:170 | Sentry wording is partly fixed, but now incorrectly limits source details to Vocet's own code. errors.rs retains dependency-relative paths, Rust standard-library paths and function names too; paths_become_crate_relative and the stack test cover these. TESTERS still promises no file paths at all. | Say scrubbed source file paths/names, line numbers and functions from Vocet and its dependencies (including Rust), with no paths to the user's files. Apply consistently to all four current notices. |

## Re-check of round 1

1. **Fixed:** policy and technical docs now explain that request-level ZDR follows the account's Safety Retention choice, self-serve defaults allow it, and strict ZDR requires the account's ZDR-only choice. The policy disclaims a zero-retention guarantee and includes OpenRouter's separate inputs/outputs opt-in. Matches the cited [OpenRouter ZDR documentation](https://openrouter.ai/docs/features/zdr), rechecked this round.
2. **Fixed:** current policy, docs, README and setup captions describe a random install ID; the policy explains that it links one installation's events. The technical notice retains the correct pseudonymous explanation.
3. **Still open:** personal paths are now distinguished from source paths, but the new own-code-only limitation and the remaining TESTERS promise are false. See finding above; no product change is needed.
4. **Fixed:** policy names ~/Library/Application Support/com.dictationapp.desktop/debug-recordings, explains quitting and deleting that folder, and distinguishes recordings from History deletion. Matches the app-data directory plus debug-recordings implementation.

## Checklist

1. Scope: re-check limited to the four requested fixes; finding 3 remains.
2. Correctness: finding 3.
3. Security and privacy: finding 3.
4. Architecture: OK; copy-only changes.
5. Rust and TypeScript practice: OK; no logic changes.
6. Licences and reuse: OK; no new dependencies or reused code.
7. Tests: existing scrubber tests establish the remaining contradiction; full pre-push gate supplies validation for this report commit.
8. Git and PR: OK; no guard edits, no merge. After two review rounds the Lead should coordinate the remaining wording correction per workflow.
