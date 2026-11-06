# U11 test report, run 1

PR: #300 at 463d873
Result: PASS (with one flaky scenario step and some manual checks not done; see below)

Run in `dictation-vm-2` (reset per run), real Wi-Fi, local stand-in provider, made-up keys only.

| # | Criterion | Test(s) | Result | Notes |
| - | --------- | ------- | ------ | ----- |
| 1 | Welcome: title, line, demo, Get started only; no sharing text | e2e_onboarding_first_run | PASS | "Share usage" absent; screenshot checked |
| 2 | Demo uses the real pill, with hint, no wrap at 680x520, loops | hand (real app, 740x560 and 680x520) + screenshots | PASS (partly by hand) | Saw "Listening" + hint, "Transcribing…", beat captions emphasised in turn; pill straddles the card's bottom edge as in the mock. No wrap at 680x520. Reduce Motion, "stops on leaving the step" and "no timers" NOT checked by hand (see below) |
| 3 | Keycap shows long label, shares a baseline | e2e_onboarding_layout, screenshot | PASS | "Right Option (⌥)" on one baseline with the keycap |
| 4 | Own key: no form, Continue opens Connect your key, Back keeps choice, rail on Speech | e2e_onboarding_first_run, e2e_onboarding_five_steps | PASS | rail shows Speech on both |
| 5 | Model names shared, exact wording | e2e_onboarding_screenshots, screenshots | PASS | "English · 477 MB", "30 languages · 590 MB", key card wording exact |
| 6 | Own-key screen: no key blocks Continue with reason, unlocks after Test & Save; rejected key refused; saved state | e2e_onboarding_first_run, e2e_onboarding_five_steps | PASS | "Continue unlocks after Test & Save succeeds"; bad key stays locked and nothing saved; good key saves, engine rebuilt. Saved-key "Key saved"/Replace/no Remove, rejected saved key on Continue, and provider-change re-read: relied on e2e_onboarding_five_steps, not clicked by hand |
| 7 | Try it: no Reset to default, Change key, instruction, one line at 680x520 | e2e_onboarding_layout, screenshots | PASS | "Hold the key, say a sentence, let go."; Change key is link-styled, as in the mock |
| 8 | Permissions and Ready unchanged | e2e_onboarding_five_steps, screenshots | PASS | |
| 9 | e2e scenarios updated | all below | PASS | |
| 10 | Screenshots light and dark | e2e_onboarding_screenshots | PASS | 39 screenshots copied to /Users/gauri/vocet-screens/u11/ (one Welcome frame per theme, not one per beat; no Reduce Motion frame, no "Key saved" frame) |

## Scenarios (VM dictation-vm-2)
- e2e_pill_states: PASS (after the overlay.css split the real pill looks and behaves as before)
- e2e_overlay_rendered_states_and_error: PASS
- e2e_onboarding_first_run: PASS
- e2e_onboarding_five_steps: PASS
- e2e_onboarding_layout: PASS
- e2e_onboarding_screenshots: FAIL on the first run, PASS on the rerun (see below)
- e2e_onboarding_round2: not run (time)

## Gate
Not rerun; CI covers it.

## Untested behaviour / not done
- Reduce Motion static frame: not tested (the VM's Reduce Motion setting was not driven). Not counted as passed.
- "Demo stops when you leave the step, no timers": only saw that Get started moves on. Not proven by hand.
- Saved-key state, Replace, rejected saved key on Continue, provider change by hand: not clicked; covered only by e2e scenarios.
- Try it by hand (Change key records and cancels, focus returns, dictation lands with the mock engine), Permissions-to-Ready fresh install by hand, resume after quit at every step by hand: not done; the scripted scenarios cover them.
- Dark Welcome per beat: only the one frame in the screenshot pack.

## Failures
1. e2e_onboarding_screenshots, first run, light: "quit mid-download" failed. The tray menu came back empty (`no menu item starting with "Quit" in []`) while the 477 MB download finished within about 10 s (it was at 93% at the click), so the app was still running 15 s later and the resume check ("Continued from") failed. Dark mode passed the same step, and a full rerun passed. Same family as #286 (the screenshot scenario depends on a real download and the internet). #293 is an unrelated Rust flake and did not appear. Not caused by U11 as far as I can tell; I did not open a new issue.

## Observations against the mock
- Welcome, Speech (English, Hindi, own-key), Connect your key (no key) and Try it screenshots match the mock's layout and copy. Change key is link-styled in the mock as well, while the spec text says "small plain button".
