# U3 test report, run 1

PR: #245, tested at 92ab90b (the branch has since moved to 68b7f6e: a merge of main plus 9 lines in `site/index.html`, `src/LocalModels.tsx`, `src/overlay/overlay.css`, `src/styles/tokens.css`; not re-tested)
Result: FAIL (see "Failures": one e2e scenario failed and was not re-run; the VM screenshots could not be taken)

| # | Criterion | Test(s) | Result | Notes |
| - | --------- | ------- | ------ | ----- |
| 1 | tokens.css is the DS1 copy, imported by both entry points, hex lint | `npx vitest run src/styles` (5 passed); `diff design-system/tokens.css src/styles/tokens.css` | PASS | Only difference: the one-line "Source: ... DS1" header. Mutation: appending `.x{color:#ff00ff}` to `settings.css`, to `overlay.css`, and `"#ff00ff"` in a new `src/zz.ts` each made the test fail (`offenders` lists the file); `rgb(1,2,3)` in `settings.css` also failed. All restored. |
| 2 | App sheets use only `--vc-*` | test "styles app sheets..." + grep | PASS (minor) | No legacy names, no rgba in the two sheets. Raw px literals remain for component dimensions (switch 36x20, dots 10px, outline-offset 2px); the lint does not cover radius/spacing literals: #252. |
| 3 | Components match DS1 | code read of `settings.css` vs `design-system/index.html` | PARTIAL | Not seen on screen (see 7). Code differences from DS1: buttons are `font-weight` inherit, DS1 `.btn` is 500; destructive button has a danger border, DS1 `.danger` has `--vc-line-strong` border with danger text; DS1 primary has no border, the app's has an accent border (visually the same). Chips: same colours, radius, 11 px/600. Focus ring: 3 px accent, 2 px offset (`:focus-visible`, settings.css:12-14), matches. |
| 4 | Overlay literals replaced | `overlay.css` has no rgba/hex (test) | PASS (code); not seen | Overlay screenshots not obtained. |
| 5 | Site on tokens | test "keeps the website on the same tokens" | PASS | Site screenshots: `U3-screens/site-*.png`. |
| 5b | Fonts self-hosted | Playwright network log, see Site | PASS (minor) | Only api.github.com is third-party. Unused Tight 600 and Inter 500 files shipped: #253. |
| 6 | Reduced motion | tokens.css has the `prefers-reduced-motion: reduce` block (test) | PASS (code) | Not exercised in the VM. |
| 7 | [manual] VM screenshots light/dark, minimum sizes | `tests/e2e` custom scenario (not committed) | NOT DONE | See "Failures". |

## Gate
- Hosted CI on the current head 68b7f6e: DMG (macOS) pass, Platform crate (macOS) pass, Quality gate (Linux) pass. (At 92ab90b the PR was CONFLICTING and no checks had run; the branch was updated while I tested.)
- `npx vitest run src/styles`: 5/5.

## Behaviour unchanged (VM, dictation-vm-2, build of 92ab90b, `tests/e2e/run.sh --no-build`)
| Scenario | Result |
| --- | --- |
| e2e_settings_save_reopen | PASS |
| e2e_onboarding_first_run | PASS |
| e2e_overlay_focus | PASS |
| e2e_tray_error_clears_on_next_recording | PASS |
| e2e_hotkey_recorder | **FAIL** (first half passed: Esc cancel, refusal, current key, Right Option; the Right Command half failed) |

## Site (headless Playwright, local `python3 -m http.server` on `site/`)
WebKit and Chromium, 320x568, 360x800, 375x667, 390x844, 430x932, 844x390, 768x1024, 1024x768, 1440x900 (phones with touch, Chromium isMobile).
- `documentElement.scrollWidth == innerWidth` in all 18 runs (320 included; the review had 353).
- Elements under 44 px (a, button, input, select, summary, [role=tab], visible): none in any of the 18 runs.
- Network requests, all runs combined (deduplicated): `index.html`, `fonts/inter-latin-400-normal.woff2`, `inter-latin-600-normal.woff2`, `inter-latin-700-normal.woff2`, `inter-tight-latin-700-normal.woff2`, `inter-tight-latin-800-normal.woff2`, and `https://api.github.com/repos/gargankit90/dictation-app/releases/latest` (404 as before, the page falls back quietly). Nothing to Google. `grep -c "googleapis\|gstatic" site/index.html` is 0. Loaded fonts per `document.fonts`: Inter 400/600/700, Inter Tight 700/800.
- Mobile review items: #1 must-fix (320 sideways scroll) fixed; #2 and #3 tap targets fixed (nav, brand, footer, inline links all >= 44); #4 "Pasted" badge at 320 is clear of the last message line (`U3-screens/site-wk-320-top.png`); #5 nav CTA hidden on phones, header is one row of links; #6 tabs are a 2x2 grid (`site-cr-390-tabs-slack.png`); #7 `text-wrap: balance` present (no orphan in the 320 h1); #8 console shows only the one 404; #9 only used weights are requested. The site stays light-only by design.
- Screens: `U3-screens/site-wk-320-top.png`, `site-cr-390-tabs-slack.png`, `site-wk-1440-top.png`.

## Untested behaviour found
- The lint test does not cover radius/spacing/size literals (#252).
- The 404 from api.github.com is expected until the first release.

## Failures
1. **e2e_hotkey_recorder failed in the Right Command half; not re-run, so unconfirmed.** Steps: `E2E_VM=dictation-vm-2 tests/e2e/run.sh --no-build e2e_u3_screens e2e_settings_save_reopen e2e_onboarding_first_run e2e_hotkey_recorder ...`. After `ax press ... Cancel` ("no button Cancel"), pressing "Record a different key" succeeded but `wait-text "Press a key"` timed out at 5 s, so the following checks (keycap shows Right ⌘, hotkey applied, settings.json, dictation, relaunch) failed. Possible causes: a real regression from the restyle (for example the recorder's text or button now not found by the AX scenario), or the scenario's state: it ran in the same VM session after my screenshot scenario had left about 25 Terminal windows and a pending Automation dialog open (the failure came later in the run). I could not tell which: my re-run (clean VM reset) lost the VM-lock race to other runs three times, and the 60-minute box ran out. Needs a re-run on a clean VM: `E2E_VM=dictation-vm-2 tests/e2e/run.sh e2e_hotkey_recorder`. The first half of the same scenario (recorder listens, Esc cancels, letter refused) passed, which argues for the environment, but it is not proof.
2. **Criterion 7 evidence missing.** No usable light/dark screenshots of Settings, Troubleshoot, onboarding steps, overlay states or minimum sizes. Two causes: (a) in the VM, a Terminal job that runs osascript against System Events raises "Terminal wants access to control System Events" (`U3-screens/vm-blocked-onboarding-min.png`), so dark mode could not be switched and windows could not be resized; (b) full-desktop screenshots are covered by the accumulating Terminal job windows (`vm-blocked-troubleshoot.png` shows Troubleshoot behind them). Filed as #254. The Developer's and Lead's DS1 comparison remains the only visual evidence. No app window minimum size is set in `tauri.conf.json` (480x640 default, no `minWidth`), which I could not exercise.
3. `dictation-vm-2` was held by another worktree's run for about 50 minutes at the start, which cost most of the time box.
My screenshot scenario (`tests/e2e/scenarios/e2e_u3_screens.sh`) stayed uncommitted in my worktree; it needs the Automation grant above before it is useful.

Minors filed: #252 (lint scope), #253 (unused font files), #254 (VM Automation grant / screenshots).
