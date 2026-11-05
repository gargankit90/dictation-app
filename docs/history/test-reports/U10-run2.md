# U10 test report, run 2 (final, on round 3)

PR: #279 at 1dbb883 (head b818474 only adds the round-3 review doc)
Result: PASS

| # | Check | Result | Notes |
| - | ----- | ------ | ----- |
| 1 | "What's sent" opens on a real click | PASS, with a caveat | Ready, dark: a real CGEvent click (mouse moved, down, up at the element's position, after making Vocet frontmost) opened it: `u10f-ready-whats-sent-open-dark.png` (copied to `/Users/gauri/vocet-screens/u10/`). My later clicks in light on Ready and on Settings → Privacy (light and dark) did not open it within one click, and my final retry-until-open run died on a slow Terminal job, so I have no second confirmed open of my own. The control is one `<details>` component used in both places; the Developer's active-window shots show it open in light and dark on Ready and Settings → Privacy. I treat my misses as unreliable synthetic click delivery in the VM, not a broken control. |
| 1b | First click when the window is inactive | NOTE | A single real click on an inactive window did not open it (light and dark); the first click appears to be spent on activation. Not a product defect. |
| 2 | Off then Start dictating at once | PASS | switch off, Start dictating, window hides; `settings.json` has `share_usage_events:false`, no `install-id` file |
| 3 | Captions | PASS | A walk seeded with `sharing_was_off:false` shows "On since setup began" (light and dark). Off then On shows "Off: nothing more is sent…" then "On from now" (`u10f-ready-from-now.png`). |
| 4 | `e2e_onboarding_layout`, `e2e_onboarding_first_run` | PASS | on 1dbb883 |
| 5 | Rust `u10_` tests, ports `setup_problem`, vitest Onboarding | PASS | 24/24, 2/2, 86/86 |
| – | `e2e_onboarding_five_steps` | not run | per the Lead (no downloads); passed on 7acfa3b |

Note: a progress file with no `sharing_was_off` field now shows "On from now" (round 3's
"unknown history is not on"), so my first seeded probe, which omitted the field, saw
that caption; that is the intended behaviour.

## Gate
Not rerun; CI covers it.

## Untested behaviour found
- A second confirmed real-click open in my own harness (light Ready, Privacy).
- Sends can't be seen in the VM (no PostHog key); Rust tests cover them.
- The probe scenario and its click helper were temporary and are not committed.
