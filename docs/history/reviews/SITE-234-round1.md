Reviewer: CHANGES REQUESTED

# SITE-234 review, round 1

PR: #234 at 3ba9fa1d2541c9c009dd90013038447d381b1a4b.
App reference: origin/main b7ae67e6ad2dc807b38b3ce12830a4a26834e6a7.
Reviewed #234 alone, #235's diff, and their combined result. No merge performed.

## Findings

| # | Severity | Location | Finding and suggested fix |
| - | - | - | - |
| 1 | major | site/index.html:2147 | App-name telemetry and the first-setup-screen opt-out are false: docs/privacy.md excludes app names and src/Onboarding.tsx has no sharing switch. #235 fixes both; retain those changes. |
| 2 | major | site/index.html:1662 | Download buttons advertise an available installer but releases/latest returns 404 and gh release list is empty. #235 correctly changes all three fallbacks to coming soon. |
| 3 | major | site/index.html:1654; site/og.png | Offline operation requires a downloaded local model. #235 qualifies the HTML by contrasting local and cloud operation, but the 1200×630 social image still says “Works offline” without qualification. Update the image to say offline with a local model. |
| 4 | major | site/index.html:2125 | “The model downloads meanwhile” promises automatic downloading during permission setup. origin/main src/Onboarding.tsx:470 instead tells local users to download in Settings; default transcription is cloud and setup requests an API key. Replace this with the actual engine/key/model preparation step; do not advertise the planned onboarding as shipped. This remains in #235 at line 2130. |

Conditional approval is insufficient: #235 addresses findings 1 and 2 and the HTML part of 3, but leaves the social image and finding 4. Fix those before approval of the stack.

## Fact check and external requests

- Local catalogue: Parakeet English, 477,274,496 bytes (approximately 477 MB); Qwen3-ASR supports 30 languages including Hindi. Offline dictation after downloading is supported. Cloud options are Groq/OpenAI with the user's own key, in macOS Keychain; audio goes directly to the selected provider. Checked core/local_models.rs, core/settings.rs, src/providers.ts and platform/secrets.rs on origin/main.
- Apple Silicon and macOS 13+ match the build configuration and local-engine requirements. MIT matches root LICENSE; model weights have their own licences. No public release exists; notarization is not claimed. Windows is described as roadmap only.
- App telemetry uses PostHog US and Sentry US, defaults on, one Settings Privacy switch; excluded content matches docs/privacy.md after #235. “Never sent” is read in the usage/error-events context, not as a denial of cloud-provider uploads. The install ID belongs to PostHog events (pseudonymous, as the full privacy document explains); Sentry reports have their own allow-list.
- Automatic third-party requests on BOTH pages: Google Fonts stylesheet at https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600;700&family=Inter+Tight:wght@600;700;800&display=swap; Inter and Inter Tight font files at fonts.gstatic.com; GET https://api.github.com/repos/gargankit90/dictation-app/releases/latest. Preconnects target fonts.googleapis.com and fonts.gstatic.com. No external script, tracker, PostHog or Sentry SDK is included. Google/GitHub receive ordinary visitor network metadata; the app's telemetry switch does not govern these website requests.
- Observed font URLs in Chrome: https://fonts.gstatic.com/s/inter/v20/UcC73FwrK3iLTeHuS_nVMrMxCp50SjIa1ZL7W0Q5nw.woff2 and https://fonts.gstatic.com/s/intertight/v9/NGSwv5HMAFg6IuGlBNMjxLsH8ahuQ2e8.woff2. Google may return different font URLs by client/cache. The SVG favicon is a data URL. og.png is same-origin metadata for social crawlers, not an ordinary page-load fetch. GitHub repository/issues/releases/privacy links make navigation requests only on activation; successful future lookup will link to GitHub's installer asset.

## Verification and checklist

1. Scope: static snapshot matches PR scope; copy must still be accurate before publication (findings above).
2. Correctness: real links/buttons; all local fragment targets resolve. Both heads rendered at 390 px with document scrollWidth 390; all four tabs worked without overflow. #234's broken download fallback reproduced; #235's three coming-soon links verified.
3. Security/privacy: findings 1 and 3; external requests inventoried above. No secrets or external scripts introduced.
4. Architecture: OK; static site only, no crate changes.
5. Rust/TypeScript practice: not applicable; inline JavaScript uses textContent for release tags, no HTML injection.
6. Licences/reuse: no suspicious copied code or distinctive borrowed paragraphs identified; README credits internal design. Spot searches of the headline/setup phrase found no evidence of verbatim third-party copying; this is not proof of provenance.
7. Tests: check-doc-links passed (43 relative Markdown links, including site/README.md; HTML/external links are outside its coverage). HTML fragment checks passed; initial mobile axe scan returned no violations on either head. No app tests needed for this static diff. Live site and linked full privacy document returned HTTP 200; GitHub repo API access works; latest release correctly 404. Social image inspected visually. No native app or owner's screen used.
8. Git/PR: small scoped commit; no guards changed. No minor findings or tech-debt issues. No merge authorized.

Limitations: mobile browser checks were headless, with representative initial axe coverage, not a complete assistive-technology audit. No hosting configuration changed or assessed. The full gate runs through the push hook for this report.
