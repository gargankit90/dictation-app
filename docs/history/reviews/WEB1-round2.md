# MWEB1 review, round 2

PR: #339 at bee7ccd9d94b0300678dde743c5c63da2804360a
Verdict: APPROVED

## Findings

No open blocker or major findings in the rechecked changes.

- Round 1, finding 1: fixed. Both Download anchors have `data-download` and releases-page fallback hrefs. A temporary jsdom harness verified both destinations with the current constant and with the documented DMG URL, including hiding both coming-soon lines after the switch.
- Round 1, finding 2: fixed. Both visible, keyboard-operable Pause/Play buttons control the shared paused state and expose `aria-pressed`. Pausing clears timers and displays finished examples; resuming restarts animation. Focus/hover holds the selected example, and a radio change permanently disables automatic tab selection. The harness reproduced keyboard focus on `t-ai` without a selection change at the cycle boundary, and a manual Mail selection remained selected after focus left. Reduced motion keeps controls hidden and schedules no animation timers. No JavaScript exceptions occurred.
- Q5 copy: the cleanup-only zero-data-retention wording matches the cleanup provider filter (`zdr: true`); it does not claim zero retention for speech. Both new privacy links target `privacy.html`.

## Deployment prerequisite

Merge Q5 PR #343 before deploying this PR, then update this branch with main as required by repository protection. `site/privacy.html` is supplied by #343 and is absent at this reviewed head; the live URL currently returns 404. Approval assumes this explicit dependency is respected, rather than publishing links before their target exists.

## Checklist

1. Scope: OK; rechecked only the two fixes, Q5 wording and privacy links.
2. Correctness: OK; four harness combinations (normal/reduced motion and releases/DMG constant) passed.
3. Security and privacy: OK; new wording matches the cleanup privacy filter; no added tracker or network request.
4. Architecture: OK; static site changes only.
5. Rust and TypeScript practice: OK; no new runtime exception in focused checks.
6. Licences and reuse: unchanged from round 1.
7. Tests: focused jsdom checks passed; token test and required gate results recorded in the PR comment. Browser/VM visual and screen-reader checks remain with the Tester/Lead.
8. Git and PR: OK; previous report is on the PR branch. No merge performed.
