Reviewer: APPROVED

# SITE-235 review, round 1

PR: #235 at 78208760f60c337113e01fa7c9c9440e2635a13c, stacked on #234.
App reference: origin/main b7ae67e6ad2dc807b38b3ce12830a4a26834e6a7.
Reviewed the incremental copy diff, the new fix, and the combined page. No merge performed.

## Findings

No open blocker, major or minor findings. Two inherited majors were identified in the initial combined page at e097cc6: setup promised automatic model downloads during permission setup, and og.png claimed offline operation without a local-model qualification. Both are fixed in 7820876: setup now directs model downloading in Settings and describes the key/model preparation step; the social image now says “On your Mac or with your own key”. I inspected the changed image and re-ran the 390 px/all-tabs and initial axe checks successfully after the fix.

Also verified: false app-name telemetry and first-screen privacy-switch claims removed; actual Settings Privacy opt-out retained. All three download buttons say coming soon and target the repository on the current latest-release 404. HTML distinguishes local/offline and cloud/BYOK operation. Two majors found, two fixed, zero open; zero minors or tech-debt issues.

## Fact check and external requests

- Local catalogue: Parakeet English, 477,274,496 bytes (approximately 477 MB); Qwen3-ASR supports 30 languages including Hindi. Offline dictation after downloading is supported. Cloud options are Groq/OpenAI with the user's own key, in macOS Keychain; audio goes directly to the selected provider. Checked core/local_models.rs, core/settings.rs, src/providers.ts and platform/secrets.rs on origin/main.
- Apple Silicon and macOS 13+ match the build configuration and local-engine requirements. MIT matches root LICENSE; model weights have their own licences. No public release exists; notarization is not claimed. Windows is described as roadmap only.
- App telemetry uses PostHog US and Sentry US, defaults on, one Settings Privacy switch; excluded content matches docs/privacy.md after #235. “Never sent” is read in the usage/error-events context, not as a denial of cloud-provider uploads. The install ID belongs to PostHog events (pseudonymous, as the full privacy document explains); Sentry reports have their own allow-list.
- Automatic third-party requests on BOTH pages: Google Fonts stylesheet at https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600;700&family=Inter+Tight:wght@600;700;800&display=swap; Inter and Inter Tight font files at fonts.gstatic.com; GET https://api.github.com/repos/gargankit90/dictation-app/releases/latest. Preconnects target fonts.googleapis.com and fonts.gstatic.com. No external script, tracker, PostHog or Sentry SDK is included. Google/GitHub receive ordinary visitor network metadata; the app's telemetry switch does not govern these website requests.
- Observed font URLs in Chrome: https://fonts.gstatic.com/s/inter/v20/UcC73FwrK3iLTeHuS_nVMrMxCp50SjIa1ZL7W0Q5nw.woff2 and https://fonts.gstatic.com/s/intertight/v9/NGSwv5HMAFg6IuGlBNMjxLsH8ahuQ2e8.woff2. Google may return different font URLs by client/cache. The SVG favicon is a data URL. og.png is same-origin metadata for social crawlers, not an ordinary page-load fetch. GitHub repository/issues/releases/privacy links make navigation requests only on activation; successful future lookup will link to GitHub's installer asset.

## Verification and checklist

1. Scope: static snapshot matches PR scope; copy must still be accurate before publication (findings above).
2. Correctness: real links/buttons; all local fragment targets resolve. Both heads rendered at 390 px with document scrollWidth 390; all four tabs worked without overflow. #234's original broken download fallback reproduced; #235's three coming-soon links verified.
3. Security/privacy: OK after offline qualification fixes; external requests inventoried above. No secrets or external scripts introduced.
4. Architecture: OK; static site only, no crate changes.
5. Rust/TypeScript practice: not applicable; inline JavaScript uses textContent for release tags, no HTML injection.
6. Licences/reuse: no suspicious copied code or distinctive borrowed paragraphs identified; README credits internal design. Spot searches of the headline/setup phrase found no evidence of verbatim third-party copying; this is not proof of provenance.
7. Tests: check-doc-links passed (43 relative Markdown links, including site/README.md; HTML/external links are outside its coverage). HTML fragment checks passed; initial mobile axe scan returned no violations on either head. No app tests needed for this static diff. Live site and linked full privacy document returned HTTP 200; GitHub repo API access works; latest release correctly 404. Social image inspected visually. No native app or owner's screen used.
8. Git/PR: small scoped commit; no guards changed. No minor findings or tech-debt issues. No merge authorized.

Limitations: mobile browser checks were headless, with representative initial axe coverage, not a complete assistive-technology audit. No hosting configuration changed or assessed. The full gate runs through the push hook for this report.
