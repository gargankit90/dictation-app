# MWEB1 test report, run 2

PR: #339 at 0228b33
Result: PASS

Served `site/` on 127.0.0.1 and drove it with headless Chromium (Playwright) at 1280 px and 390 px (dark scheme), with JS off, and with reduced motion.

| # | Check | Result | Notes |
| - | ----- | ------ | ----- |
| 1 | Earlier FAIL (closing Download pointed at a 404 DMG) | PASS | Both Download buttons resolve to the releases page via `DOWNLOAD_URL`. |
| 2 | Owner changes present | PASS | Dictation badge, heading "Talk instead of type in every app you use.", only "Free · macOS 13 or later · Apple silicon" under the button, no "signed and notarized" or "coming soon", no pause controls (0 `aria-pressed` elements), two-line privacy section linking `privacy.html`. |
| 3 | Download works without JS | PASS | The default `href` is already the releases page; same value with JS off. |
| 4 | Phone width | PASS | `scrollWidth` equals `clientWidth` (390); full-page screenshot read. |
| 5 | Light and dark | PASS | The page is light-only by design (same colours under dark scheme); text readable. |
| 6 | Reduced motion and JS off | PASS | Demos sit in the `pasted` state and do not change. |
| 7 | Animation runs by default | PASS | Hero composer moved listening to pasted within 6 s. |
| 8 | Links | PASS, one expected 404 | `privacy.html` returns 404 on this branch (arrives with Q5 #343, expected). Other links are GitHub repo, releases, issues and `eval/`; not fetched over the network. |
| 9 | Console errors, third-party requests | PASS | None. |
| 10 | Banned words (open source, Groq, Parakeet/Qwen3, Alpha, percentages, retention period, "signed", "notarized") | PASS | Page text read in full; none present. Spec criterion 2 holds. |
| 11 | Claims true for 0.2.0 | PASS | Right ⌥ default (`hotkey_spec.rs`), last 25 history (`settings.rs`, CHANGELOG), Copy last dictation in the menu (CHANGELOG). The $0.70 per 1,000 figure and "zero-data-retention host" come from the spec and owner; I did not re-measure them. |
| 12 | Meta | PASS | Title, canonical, og:title/url/image, `SoftwareApplication` JSON-LD present. |

## Gate
`scripts/gate.sh` (full, foreground): all checks passed.

## Notes (not failures)
- The data section now lists two modes only. The spec's "Both: history stays on your Mac, anonymous usage reports on by default, off in Settings → Privacy" line is gone; the owner approved the shorter page, but the Lead may want that line back, or it should live in privacy.html.
- The page says "no Vocet server" while analytics reports are on by default (`docs/privacy.md`); the sentence is about dictations, so it is accurate as written.
- Download still points at the releases page; on release day set `DOWNLOAD_URL` (see `site/README.md`).
- Not checked: og.png image content, and links over the network.
