# U8 test report, run 4

PR: #269 at 76642b0 (re-check of rounds 4 and 5 since run 3 at c36e60c)
Result: PASS

Everything ran in `dictation-vm-2` (VM reset before each run, lock held by `run.sh`).

| # | Check | Test(s) | Result | Notes |
| - | ----- | ------- | ------ | ----- |
| 1 | First run, onboarding to Ready, events switch on Ready | `e2e_onboarding_first_run` | PASS | includes the new `ob-share-ready` checked check |
| 2 | Five steps | `e2e_onboarding_five_steps` | PASS on rerun | first run failed 4 checks (see Notes); rerun passed |
| 3 | Layout at minimum size, light and dark | `e2e_onboarding_layout` | PASS | all 12 step/appearance checks |
| 4 | Try it title reads "Try it" | `e2e_onboarding_screenshots` (window screenshot read by eye) | PASS | heading and rail both say "Try it" |
| 5 | Failed paste shows the whole fix panel at 680x520 | same, `4-try-paste-missed-680x520-{light,dark}.png` | PASS | message, transcript, Copy and the paste hint all visible above the footer; the panel is scrolled into view |
| 6 | Ready: flip sharing off, press Start dictating at once | throwaway harness, 3 rounds | PASS | each round: window hides, saved settings say `share_usage_events:false`, no install ID, no usage-event line in the log |
| 7 | Download strip's "Use my own key", same race | same harness | PASS | seeded Ready with no model, Cancel, switch off, press Use my own key at once: moves to Speech, settings off, no install ID, no event line |

## Notes

- **five_steps first run:** 4 checks failed (strip text, "Download running", part on disk, resume answer). The app log shows the model downloaded in about 10 s on that run, so the part was gone before the scenario looked (`resume=None`). The rerun passed. It is the fast-download race that the last commit (76642b0) already tries to narrow; not a product fault.
- **Limit of the "no event sent" evidence:** the debug build has no PostHog key, so no event is sent whatever the switch says. A control round with the switch left on also showed no install ID and no log line, so those checks can't tell on from off here. What the harness does prove is the order: by the time the window hid (or Speech opened), the saved settings already said off. The unit tests in `src/Onboarding.test.tsx` carry the rest. This is the same limit as `e2e_onboarding_round2`.
- The harness (`e2e_onboarding_u8run4`) was throwaway and is not committed. It seeded setup progress at `ready` with a local engine, flipped `ob-share-ready` off and pressed Start dictating (or the strip's Use my own key) at once, retrying the press while held back, then read the settings file, the install-ID files and the app log.

## Gate

Not re-checked in this run (run 3 covered it; only the targeted re-checks above were asked for).

## Untested behaviour found

None new beyond the note on event-sending evidence above.

## Failures

None open.
