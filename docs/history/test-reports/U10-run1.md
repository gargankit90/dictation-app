# U10 test report, run 1

PR: #279 at 7253613
Result: FAIL

One blocker: no screenshot of the expanded "What's sent" on Ready exists, and
my real clicks did not open it (details below). Everything else I could check passed.

| # | Criterion | Test(s) | Result | Notes |
| - | --------- | ------- | ------ | ----- |
| 1 | Welcome has no switch; Ready has the only one, on, with What's sent | VM `e2e_onboarding_first_run`, `_layout`, `_screenshots`; probe: AX dump of Welcome (stored Off) has no share/usage/events/"What's sent" text; Ready light and dark: switch checked, caption "…have been on since setup began…" | PASS | Screenshots in `/Users/gauri/vocet-screens/u10/` |
| 1b | Expanded "What's sent" on Ready has a screenshot, light and dark, opened by a real click | probe: CGEvent mouse click at the element's computed position (twice, after activating the app), AX press | FAIL | Collapsed shots only. AX press fails (`no button "What's sent"`; the element is `AXDisclosureTriangle`/`AXSummary`). The real clicks left the section collapsed in both modes; the shots are byte-identical to the collapsed ones. I could not tell a missed click from a non-responding control, so this needs the Developer or Lead to open it with a verified click. |
| 2 | Fresh install sends from the first screen, with the fake port | `cargo nextest run -p dictation-app u10_`: 18/18 | PASS (Rust tests) | The VM build has no PostHog key (and scripted launches send nothing), so a send can't be seen in the VM. |
| 3 | Stored Off: nothing sent at any step, no install ID | Rust `u10_stored_off_*`; VM probe: stored `share_usage_events:false` at Welcome and at Ready: no `install-id` file, Ready switch unchecked, Off caption shown | PASS | VM no-ID check is weak without a key; the Rust test is the real proof. |
| 4 | Closed `SetupProblem` values, listed in `docs/privacy.md` | `cargo nextest run -p dictation-ports setup_problem`: 2/2 | PASS | |
| 5 | Off on Ready, then Start dictating at once: nothing further sent, ID gone | Rust `u10_off_right_before_start_dictating_stops_and_deletes_everything`, `u10_off_drops_a_request_under_way`; vitest `src/Onboarding.test.tsx`: 80/80 | PASS (unit/integration) | Not driven live in the VM (not run: time box, and the VM has no key). |
| 6 | Error reports follow the switch | Rust `u10_error_reports_follow_the_same_switch` | PASS | |
| 7 | Privacy page and What's sent text | vitest, `docs/privacy.md` check in the ports test | PASS | I did not read the expanded text in the app (see 1b). |
| – | Run setup again as an existing user with sharing off stays off | not run in the VM | NOT TESTED | Only the stored-Off launch at Ready/Welcome was run. Rust `u10_the_stored_choice_decides_at_launch` covers the launch rule. |

## VM e2e (dictation-vm-2, reset, 7253613)

`e2e_onboarding_first_run`, `e2e_onboarding_five_steps`, `e2e_onboarding_layout`,
`e2e_onboarding_round2`, `e2e_onboarding_screenshots`: 5 passed, 0 failed, 0 skipped.
(`npm ci` was needed first: the fresh worktree had no `tsc`.)

## Gate
Not rerun (CI and the Developer ran it). Tests I ran: `cargo nextest` u10_ 18/18, ports setup_problem 2/2, vitest Onboarding 80/80. Coverage not measured.

## Untested behaviour found
- A real click on "What's sent" in the VM (the AX tree exposes it only as a disclosure triangle; no existing helper can click it).
- Run setup again with sharing off, and Off then Start dictating, in a live VM app (needs a key or a local server; not wired in the e2e harness).

## Failures
1b. Steps: seed `{"step":"ready","welcome_passed":true,"try_outcome":"passed"}` and a default settings file, launch with `--debug-onboarding`, click the "What's sent" link on Ready at its on-screen position (about 437,804 in a 1366-px window shot). Expected: the section opens, showing "Setup problems". Actual: unchanged window, in light and dark.
The probe scenario was temporary and is not committed.
