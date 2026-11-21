# MQ5 test report, run 1

PR: #343 at 3086641
Result: PASS

| # | Criterion | Test(s) | Result | Notes |
| - | --------- | ------- | ------ | ----- |
| 1 | [auto] No page/doc claims what 0.2.0 contradicts | grep of site, README, docs, src, crates for "transcripts never sent", "open source", "app names never", Groq, "never through a server" | PASS | Only hits: `site/index.html` "Never through a server of ours" (true for 0.2.0), README caption about the old Groq recording (labelled as pre-OpenRouter), "open-source software" licence credits (accurate), code comments. |
| 2 | [auto] Gate passes, Privacy UI test | `npx vitest run src/Settings.test.tsx` 72/72; CI `Quality gate (Linux)`, `Platform crate (macOS)`, `DMG (macOS)` all pass on 3086641 | PASS | Local `scripts/gate.sh` exceeded 10 min and was stopped; CI is the evidence. |
| 3 | [manual] Lead/owner read policy | n/a | Lead's | I did not verify provider claims against sources. All 5 external links return HTTP 200. |

## Page checks
- `site/privacy.html` at 390 px (iframe, dark): no horizontal overflow, summary and table readable (table has its own overflow-x wrapper).
- Light mode: default styles plus a `prefers-color-scheme: dark` block exist; I did not take a separate light screenshot.
- Links: OpenRouter data-collection and ZDR, OpenAI your-data, Hugging Face privacy, GitHub `docs/privacy.md` all 200. `mailto:privacy@vocetlabs.com` present.
- `docs/privacy.md` links to the policy and states the same retention wording and the same no-own-server claim. `docs/TESTERS.md` is consistent.
- App: `Settings.tsx` Privacy shows a per-mode summary and a "Read the privacy policy" link (`Link::Privacy` -> `https://vocetlabs.com/privacy`, Rust test covers the URL).

## Untested behaviour found
- `site/index.html` footer still links to the GitHub `docs/privacy.md`, not `/privacy` (spec: wait for WEB1). Not a blocker.
- `site/index.html` says the cleanup host "keeps no data"; the policy is more careful (ZDR host, CSAM screening). Minor wording drift.
- `vocetlabs.com/privacy` returns 404 until the site deploys. Check it after deploy.

## Failures
None.
