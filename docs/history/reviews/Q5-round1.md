# MQ5 review, round 1

PR: #343 at 30866418fa2c971dfd841730564bcf124cd295a6
Verdict: CHANGES REQUESTED

## Findings

| # | Severity | file:line | Finding | Suggested fix |
| - | -------- | --------- | ------- | ------------- |
| 1 | major | site/privacy.html:297 | The cited OpenRouter ZDR documentation says request-level `zdr: true` follows account/guardrail Safety Retention choices; self-serve accounts allow Safety Retention by default. It does not always exclude those endpoints. The unconditional zero-retention claims in the table, cleanup-host paragraph, docs/privacy.md and docs/TESTERS.md overpromise. | Describe the request filter and account-dependent retention accurately, including the ZDR-only account choice. Also disclose the separate OpenRouter inputs/outputs opt-in alongside logging in the retention paragraph. |
| 2 | major | site/privacy.html:219 | Usage reports are called anonymous, but PostHog receives a persistent random install ID linking events. docs/privacy.md itself correctly calls this pseudonymous. | Say reports use a random install ID and explain that it links one installation's events; remove unsupported anonymous wording in current policy/app/docs text. |
| 3 | major | site/privacy.html:340 | The promise that Sentry never gets file paths contradicts errors.rs: panic locations and stack frames include crate-relative source paths. The app's existing What's sent text repeats the same promise. | Say no personal/local filesystem paths; explicitly allow scrubbed source file names/paths and line numbers. Keep the technical notice consistent. |
| 4 | major | site/privacy.html:366 | Q5 explicitly requires the debug recordings' location and deletion instructions. “A folder on the Mac” supplies neither; the WAVs persist outside History and clearing History does not remove them. | Name the app-data debug-recordings folder (normally ~/Library/Application Support/com.dictationapp.desktop/debug-recordings), explain manual deletion, and distinguish it from History. |

## Evidence

- Read the whole PR diff, Q5 spec, engineering rules and review checklist.
- Speech uses the catalog's OpenAI transcription model and deny filter; cleanup uses GLM with deny plus zdr, and a 1.5-second deadline. No Vocet dictation receiver or training upload feature found. The OpenAI direct-endpoint retention caveat avoids inventing a period for the OpenRouter route.
- Checked the closed Event enum against the technical event list, PostHog's ID and properties, Sentry's allow-listed event builder and stack scrubbing, and shell usage startup before setup's switch. Setup-before-switch disclosure matches startup behavior.
- Checked History's default 25/allowed limits and 0600 storage, debug-only WAV saving, Keychain adapter, transcription/cleanup logging, GitHub updater endpoint and pinned Hugging Face model URLs. No new data destination or transcript/key logging introduced by this diff.
- Children, contact and dated changes sections exist. PR notes explicitly call privacy@vocetlabs.com a placeholder: Lead must confirm this is a working contact before publication. Footer coordination with WEB1 and the owner's manual policy read remain pending.
- Provider pages checked on 2026-11-20: [OpenRouter data collection](https://openrouter.ai/docs/guides/privacy/data-collection), [OpenRouter ZDR](https://openrouter.ai/docs/features/zdr), [OpenAI endpoint table](https://developers.openai.com/api/docs/guides/your-data), [Hugging Face privacy](https://huggingface.co/privacy). OpenRouter ZDR's Safety Retention section is the basis for finding 1; no retention period is inferred for this route.

## Checklist

1. Scope: findings 1–4; contact confirmation and manual acceptance remain.
2. Correctness: findings 1–3; mode-specific UI and typed policy link otherwise consistent.
3. Security and privacy: findings 1–4; no new collection implementation.
4. Architecture: OK; no dependency, unsafe or platform-boundary changes.
5. Rust and TypeScript practice: OK; thin closed-list link and simple UI copy.
6. Licences and reuse: OK; no new dependencies or adapted code.
7. Tests: all 72 Settings tests passed, including both new Q5 mode/link tests; Rust URL assertion covers the policy URL. Formatting checks passed; full gate result recorded below.
8. Git and PR: OK; Conventional Commits and detailed notes; spec commit is explicitly attributed to owner decisions. No guard changes. Do not merge this review round.
