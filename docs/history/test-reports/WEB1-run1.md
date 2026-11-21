# MWEB1 test report, run 1

PR: #339 at b8d80f3
Result: FAIL

Served `site/` with `python3 -m http.server` on 127.0.0.1 and drove it with headless Chromium (Playwright) at 1280 and 390 px.

| # | Check | Result | Notes |
| - | ----- | ------ | ----- |
| 1 | Site gate checks (`src/styles/tokens.test.ts`) | PASS | 10/10 |
| 2 | Download buttons go to the releases page until v0.2.0 | **FAIL** | Hero button is `https://github.com/gargankit90/dictation-app/releases`. The closing-section button (`site/index.html` ~line 2118) is hard-coded to `.../releases/download/v0.2.0/Vocet_0.2.0_aarch64.dmg` and has no `data-download`, so the script never rewrites it. That URL 404s today, with or without JS. |
| 3 | Other links and anchors | PASS | No `#` anchors; all other links are GitHub URLs (repo, `tree/main/eval`, releases, issues). Not fetched over the network. |
| 4 | Banned words (open source, Groq, OpenAI-direct, Parakeet/Qwen3, Alpha, v0.1.0, percentages, retention, "anonymous") | PASS, one note | None found. "OpenAI" appears once, as the OpenRouter-to-OpenAI audio route. Note: the model name "OrukQwen 1.7B" (On this Mac card) contains "Qwen", though not "Qwen3". Lead to decide. |
| 5 | GitHub only in proof line, source FAQ and footer | PASS with note | Four places: the proof line (`eval/`), the source FAQ, the footer, and the Download button in the hero (releases page, required until 0.2.0). The closing Download button is also a GitHub URL. |
| 6 | Hero animation runs | PASS | Cycles pasted, listening, transcribing, pasted, with the app changing each loop (Slack, then Mail). |
| 7 | Tabs auto-advance and jump on click | PASS | Advanced ai → chat → doc → mail about every 9 s. Clicking Mail jumped to it at once. |
| 8 | `prefers-reduced-motion` | PASS | All demos sit in the `pasted` state and the tab stays put after 6 s. |
| 9 | JS off | PASS | Final state shown (screenshot checked): the clean text is visible, with no animation. |
| 10 | Light and dark readable at 1280 and 390 px | PASS | No horizontal overflow. Dark scheme renders identically to light (light-only override, as the PR notes say), and the text is readable. |
| 11 | Console errors, failed requests, third-party requests | PASS | None. |

## Gate
`tokens.test.ts` passes. The full gate and CI were not rerun.

## Failures
1. Closing Download button points at the v0.2.0 DMG. Expected: the releases page, like the hero button, until v0.2.0 exists. Repro: open the page, scroll to "Free beta, source available" and inspect the button `href`. Fix: add `data-download` to that anchor and point its default `href` at the releases page.

## Untested
The Download link was not fetched, and the og.png and JSON-LD were not inspected. The visual comparison was by screenshot only.
