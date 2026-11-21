# MWEB2 test report, run 1

PR: #351 at 35869c8 (includes the Lead's ZDR wording fix 35869c8)
Result: PASS after run 1 fixes (run 1 was FAIL: two unverified figures; re-checked at 0b1a0dd)

| # | Check | Result | Notes |
| - | ----- | ------ | ----- |
| 1 | Claims vs `cleanup.txt` (Engineers/Students bullets) | PASS | filler removal, get hub→GitHub, jason→JSON, pie test→pytest, code-style names, "docs slash setup"→docs/setup, greetings/sign-offs kept: all in the prompt. No lecture-recording claim. |
| 2 | "4.4 GB download" vs `local_models.rs` | PASS | 1,319,830,656 + 3,106,738,272 B = 4.43 GB. |
| 3 | Privacy claims vs `site/privacy.html` | PASS | "never leave your Mac", no Vocet server/account, OpenRouter→OpenAI audio. New sentence ("Vocet asks for zero-data-retention hosts; your OpenRouter privacy settings decide the rest") matches privacy.html (zdr:true is a request; account setting decides). |
| 4 | "about 1 second from key-up to paste on Apple silicon (measured on 16 GB)" | FAIL | No recorded measurement for 0.2.0's local pair (OrukQwen + Gemma). Q4 criterion 5 ([manual], p50/p95 on 16 GB/8 GB) has no result in the repo. The only 16 GB mention is the voicearena whitepaper; the repo's own on-device numbers are other models (Parakeet 1.15 s median) in a 6 GB VM, and L1 VM runs took 3.6-4.8 s including a 2 s hold. Needs the Lead's Q4 measurement or a rewording without the number. |
| 5 | No unverified cost figure | FAIL | Cards are clean, but the FAQ "What does it cost?" still says "about $0.70 per 1,000 dictations, with a $5 minimum top-up" (unchanged from main, so pre-existing). Nothing in the repo or PR verifies it (the PR itself says it could not verify an hourly figure). Remove or source it. |
| 6 | Phone width, no horizontal scroll | PASS | Chrome headless, 375 px: scrollWidth = clientWidth = 375. Cards stack; screenshot read fine. |
| 7 | Light and dark | PASS (static for dark) | Light screenshot checked. New `.ticks` uses the themed token `--vc-ink-2`, and the page has `prefers-color-scheme: dark` token overrides; headless Chrome would not apply dark in my harness, so dark was checked from CSS only. |
| 8 | Links | PASS | `privacy.html` (x2) and `/` exist; releases and repo URLs return 200. `#data` anchor kept. |
| 9 | Console errors | PASS (weak) | No script error captured in the 375 px headless load; no browser-console tool run. |
| 10 | No-JS download | PASS | Both `data-download` buttons have a static `href` to the releases page; JS only rewrites it. |
| 11 | Gate | PASS | `scripts/gate.sh` in the foreground: "all checks passed". |

## Findings
- **Major** #4 and #5 above: two numbers on the page have no evidence. Fix: reword #4 (for example "no network round trip", no figure) and drop or source the $0.70 FAQ figure, then re-check only those lines.
- Minor: "The most accurate option" is a comparison with no citation on the page; eval/reports/voicearena.md supports Groq/AssemblyAI vs local ranks only loosely. Consider "highest accuracy of the two".

## Re-check at 0b1a0dd
- Latency figure: gone. Local card now reads "Fast, with no network round trip." (no number). PASS.
- Cost figure: gone. FAQ now says you add credit to your own OpenRouter account and pay only for what you dictate; no price, no minimum. grep for `1 second`, `16 GB`, `0.70`, `minimum`, `top-up`, `$` finds nothing. PASS.
- Diff since the reviewed head touches only those two spots; at 375 px scrollWidth = clientWidth = 375 still. The "most accurate" minor stays a note.
- Gate: ran on the push (pre-push hook), plus the full run from run 1.
