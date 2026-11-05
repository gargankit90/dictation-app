# U9 test report, run 2

PR: #285 at 4db13b2
Result: PASS

VM: dictation-vm-2, reset before each run. The real app was driven with real
pointer clicks and real key events (cua `desktop` scope) in the VM's window.
The hotkey press and release were the app's own `--press` and `--release` test
hooks, not a held physical key (the tool can't hold a key). The mock engine
isn't used for the Try it checks: the real engine ran against the local
stand-in provider, which answers "hello world", so the words are real
pipeline output. Screenshots are in `/Users/gauri/vocet-screens/u9/tester-run1/`.

## The owner's problems, in the real app

| # | Check | Result | Notes |
| - | ----- | ------ | ----- |
| 1a | Try it: record a two-key shortcut (Ctrl+F), then dictate without clicking the box | PASS | The box had the caret and ring after the recorder closed. Words landed. |
| 1b | Switch to TextEdit and back, dictate again | PASS | Words landed in the box; nothing went to TextEdit. |
| 1c | Start and cancel a recording, dictate again | PASS | The box was empty and unfocused during the recording, focused again after Cancel. Words landed. |
| 2 | Failed-paste panel, forced miss (`--debug-fail-paste`), shortcut Ctrl+F, panel open | PASS | At the default size and at 680x520 the whole panel and its Copy button are in view. After a recorder cancel and after TextEdit and back at 680x520 the panel stayed in view, focus returned. While a recording is open at 680x520 the panel's last line sits partly under the footer; the button is still in view. |
| 3 | Real click and hover on English, Hindi, More, light and dark | PASS | The label stayed readable after a click, with the pointer still over it, and with the window inactive (switched to Terminal or TextEdit). Light: filled blue, white label. Dark: light fill, dark label. I did not photograph every state in dark after switching away and back; I did see the inactive window in dark. |
| 4 | Language search box: real click, typing, arrows, Enter | PASS | Setup (More...) and Settings → Dictation: ring, typing filters, Down moves the active row, Enter picks it ("Finnish", "German", "Saved"). No row clipped under the field, also with the notice shown. Closed state shows a chevron. |
| 5 | Settings → Dictation notice, live | PASS | With Accessibility really off, the page shows the orange notice with `Right ⌥`, in full, and the Shortcut row. When Accessibility was switched on the notice was gone when I looked next (about half a minute later). |
| 6 | Report preview | PASS | `hotkey: ⌃F`. With the default key the notice and Ready show `Right ⌥`. No window showed `RightAlt`. `RightAlt` is still in the app's log lines (warn and ready), as before. |
| 7 | Scenarios | PASS | `e2e_onboarding_layout` (13:56), `e2e_setup_no_permissions`, `e2e_setup_own_key_no_permissions`, `e2e_onboarding_five_steps`. Run 1's five_steps failure is gone (it now uses the local stand-in model server). |

Step 2 with no Accessibility, window only, no red message:
`step2-no-accessibility-light.png`. Settings notice:
`settings-dictation-notice-no-accessibility-light.png`.

## Not tested
- The own-key route was proved by the scenario, not by hand with the mock engine.
  There is no mock provider for Test & Save, so I used the stand-in provider.
- Step 2 with no Accessibility in dark: not photographed. The dark chips were checked on step 2 with Accessibility on.
- Hover on the unselected chips in dark, and hover while focused in dark, were not each shot on their own.
- A held physical hotkey (press and hold a real key): the tool can't hold a key; the app's press and release hooks stood in.
- `e2e_onboarding_first_run`, `e2e_settings_save_reopen` and `e2e_settings_key_provider_recovery` were not re-run (they passed in run 1 on an earlier head).
- Real microphone speech: the audio is the fixture tone.

## Found
- Minor: with the setup window open at a step, choosing Settings… or Report a problem from the tray gives the window the title "Vocet Settings" but it keeps showing setup until setup is closed or finished; after closing setup, Settings shows normally. It looks like the same window being reused. Not a U9 change; I didn't check whether it exists on main.
- The failed-paste debug flag also makes the tray offer "Open Accessibility settings…". That is the flag's doing.

## Gate
Not re-run by me; the Developer's push ran it. My push ran the pre-push gate and it passed.
