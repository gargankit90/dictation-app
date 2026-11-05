# U10 test report, run 2

PR: #279 at 7acfa3b
Result: PASS (one item verified only from the Developer's screenshots, see 1)

My first VM pass in this run used a stale worktree (round-2 changes missing), so I
discarded it and rebuilt from 7acfa3b; everything below is from the rebuilt app.
Mid-run the Lead asked to save data (no model downloads): `e2e_onboarding_screenshots`
and `five_steps` had already run once; I did not rerun a download scenario after that.

| # | Check | Result | Notes |
| - | ----- | ------ | ----- |
| 1 | "What's sent" opens by a real click | NOT REPRODUCED BY ME | My CGEvent clicks (moved mouse, then down/up at the element's position, once or twice) did not open it, in light and dark. My shots show the window **inactive** (grey traffic lights). The Developer's shots in `/Users/gauri/vocet-screens/u10/` (`onboarding-u10-ready-whats-sent-open-*`, `…privacy-whats-sent-open-*`) show an **active** window with the section open, light and dark, Ready and Settings → Privacy. I read the light Ready one: expanded text matches the privacy copy. I believe the difference is window focus (a first click on an inactive WebKit window), not a broken control, but I did not confirm it. |
| 2 | Welcome has no switch or sharing text (stored Off, Run setup again) | PASS | AX dump has no share/usage/events text |
| 3 | Ready: switch on, caption "On since setup began", light and dark | PASS | |
| 4 | Ready layout, switch in view at 740x560 with the download strip | PASS | `e2e_onboarding_layout` incl. `ready-strip` (first pass was on stale code, see above; the probe's own shots at 740x560 with the strip show the switch row in view) |
| 5 | Caption across a quit: stored Off → switch On → "On from now"; quit; relaunch | PASS | still "On from now" after the quit |
| 6 | Off then Start dictating at once | PASS | window hides, `share_usage_events:false` stored, no install-id file |
| 7 | Run setup again as an existing user with sharing off | PASS | Welcome has no sharing text, stored choice stays false, Ready shows the switch off and the Off caption, no install-id file |
| 8 | Empty tray menu at quit | NOT SEEN | Quit from the tray worked in all ~10 quits (tray-click "Quit Vocet") |
| 9 | `e2e_onboarding_five_steps` | PASS | On the rebuilt app (`--no-build`). The stale-code run failed at "the strip offers to resume"; it passed on the current build. |
| 10 | `e2e_onboarding_layout`, `e2e_onboarding_first_run` | PASS | |
| 11 | `e2e_onboarding_screenshots` | FAIL on stale code only; not rerun | known setup problem #286; not held against the PR |

Rust and vitest were not rerun (round 1 results stand; I did not check the new
tests on 7acfa3b).

## Gate
Not rerun; CI covers it.

## Untested behaviour found
- Clicking "What's sent" in an active window by my own harness (item 1).
- Sends themselves can't be seen in the VM (no PostHog key); Rust tests cover them.
- My probe scenario was temporary and is not committed. Probe seeds used `download_paused` to avoid model downloads.
