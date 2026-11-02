# End-to-end tests

The built app, run inside the Lume VM `dictation-vm` and driven from the host
over ssh. These aren't part of CI (CI has no VM). Run them locally:

```sh
tests/e2e/run.sh                                   # build, reset the VM, run all scenarios
tests/e2e/run.sh e2e_startup                       # one scenario
tests/e2e/run.sh --no-build --no-reset e2e_press_release   # reuse the build and the running VM
```

It needs no human input. It exits 0 only if no check failed and no required
scenario was skipped. The summary (and `results.txt`) gives one line per
scenario, `PASS`, `FAIL` or `SKIP`, and a total such as `8 passed, 0 failed, 1
skipped`. A skipped scenario never passes silently: the optional ones
(`OPTIONAL_SCENARIOS` in `run.sh`: `e2e_real_mic`, which `E2E_REAL_MIC=0` skips,
and `e2e_real_groq`) may skip and the run still exits 0, but it says what was
skipped; any other scenario that skips fails the run. Each run
writes to `tests/e2e/out/<timestamp>/` (git-ignored): `build.log`,
`results.txt` and, per scenario, the
app's own log copied back from the VM (`<scenario>.app.log`) and a screenshot
of the VM's desktop at the end (`<scenario>.png`, taken through the
`cua-driver` daemon in the VM).

## What a run does

1. **Build:** `npm run tauri build -- --debug --bundles app`. The bundle is
   signed with the `dictation-app Dev` certificate (`scripts/make-dev-cert.sh`,
   once per Mac).
2. **Reset the VM** (skipped with `--no-reset`):
   `lume stop dictation-vm`, `lume delete dictation-vm --force`,
   `lume clone dictation-vm-golden dictation-vm` and
   `lume run dictation-vm --no-display --detach`. The run then waits for an
   IP (`lume get dictation-vm -f json`, `.ipAddress`) and for ssh to answer.
   The golden image is only cloned; change it by hand (see "The golden image"
   below), never from a run.
3. **Install:** the `.app` is copied with `tar` over ssh into
   `~/Applications` in the VM, and its signature is checked there.
   `tests/e2e/fixtures/` goes to `~/e2e-fixtures` in the VM: `tone-16k.wav` (a
   1.5 s 440 Hz tone, 16 kHz mono 16-bit; no voice), `silence-16k.wav` (1.5 s
   of zeros), `right-option.js` (posts a real Right Option key event, or
   says with `state` whether the system thinks it is held), `type-keys.js`
   (types a-z and spaces with real key events, US key positions, no modifiers),
   `input-source.js` (reads or switches the keyboard input source),
   `clipboard-image.js` (puts a PNG on the clipboard or reads it back) and
   `image-24.png` (a 24x24 test image).
4. **Scenarios:** each one quits any running copy of the app, clears its log
   (`~/Library/Logs/com.dictationapp.desktop/` in the VM), launches the app
   with `open -n …/dictation-app.app --args <flags>`, and checks the log and
   the process list.

| Scenario | Criterion | Checks |
| -------- | --------- | ------ |
| `e2e_startup` | M1.5 #1 | The app starts (first launch with `--debug-audio-file`, so the press needs no mic permission) (`ready` in the log) and one copy runs. A second launch with `--press` is forwarded (`test hook event=Pressed from="a second launch"`) and the pipeline starts recording. Still only one process; the app was set up once. |
| `e2e_press_release` | M1.5 #2, M2.2 #1 | A first launch with `--debug-audio-file ~/e2e-fixtures/tone-16k.wav --press`, a second with `--release` a second later. The app records the fixture, not the mic; the log shows `Recording Transcribing Inserting Idle` in that order, and the stand-in transcript isn't in it. |
| `e2e_real_mic` | M2.2 | `--press`, then `--release`, recording the VM's real microphone (granted in the golden image): no permission error or microphone failure, and either the full state sequence or, when the VM's microphone heard only silence (it varies; Lume feeds it), `Recording Transcribing Idle` with the log saying `not transcribing … verdict=Silent`. `E2E_REAL_MIC=0` skips it. |
| `e2e_mock_paste` | M4.3 #1 | Full app: the tone fixture, `--mock-text "hello world"`, the real paste. TextEdit (a fresh document in front) holds exactly `hello world`, read back with `osascript`; the log has the full state sequence and not the transcript. |
| `e2e_clipboard_restore` | M4.3 #2 | A `pbcopy` sentinel is still on the clipboard after a dictation, and after a second one; the transcript is not left there. |
| `e2e_silence` | M4.3 #3 | `silence-16k.wav`: `Recording Transcribing Idle` (never Inserting), the log says `not transcribing … verdict=Silent` and no `dictation failed` (the dropped dictation shows no error), TextEdit stays empty and the clipboard sentinel is untouched. |
| `e2e_rapid` | M4.3 #4 | Five dictations in a row (0.4 s holds, each started once the last is back in Idle): five complete `Recording Transcribing Inserting Idle` sequences, ending in Idle, and `--mock-text-sequence one,two,three,four,five` gives each dictation its own word, and TextEdit must hold exactly `one two three four five` (the spaces are the smart leading space, M5.5a): a paste out of order, doubled, dropped or replayed from an earlier dictation changes it. |
| `e2e_hold_release` | #90, M1.6 #4 | A real Right Option hold (down, 1.5 s, up) posted from Terminal.app: one `Recording … Idle` sequence and the transcript in TextEdit, with no synthetic `--press` flag in the log. However it ends, the key is released (see "Key cleanup"). |
| `e2e_held_at_launch` | #90, M1.6 B1 | Right Option down, then the app starts and stays up 5 s with the key still held: no Recording. The key goes up: still nothing. Then a normal hold: exactly one `Recording … Idle` sequence. |
| `e2e_overlay_focus` | M5.1 #3, #102 M1 | The full app with the tone and the mock transcriber, TextEdit in front. Once started, the log's `overlay native window` line must say the overlay is still the non-activating panel (`non_activating_panel=true`: an `NSPanel` subclass, `canBecomeKeyWindow` and `canBecomeMainWindow` false, the non-activating style mask, and backing the overlay's webview window), read back by the app on the main thread. `--press`: the log shows `overlay shown state=recording` and the app has a 420x72 window at the status level (25) on screen, read with `CGWindowListCopyWindowInfo` through JXA over ssh (`cua-driver list_windows` leaves such windows out); a screenshot is saved as `e2e_overlay_focus.recording.png`. `--release`: `overlay shown state=transcribing`, back to Idle, `overlay hidden`, and the window is off screen again. The frontmost app, sampled with `lsappinfo` every 0.2 s from before the press until after Idle, is always TextEdit, and TextEdit holds exactly `hello world`. |
| `e2e_error_bad_key` | M4.4 #2 | The VM's login keychain gets the invalid key `gsk_invalid_test_key`; the app starts on the real engine (`--transcriber settings`) and dictates the tone fixture. Groq's 401 shows as `The provider rejected your API key…` in the log (`shown to the user`), the phases are `Recording Transcribing Idle` (nothing inserted), TextEdit stays empty, and a second dictation fails the same way (the app is still usable). Then the app restarts on the mock and a dictation pastes `This is a test dictation.`. Real request to Groq, so the VM needs internet; needs no key from you. |
| `e2e_real_groq` | M4.4 #1 | Skipped unless `GROQ_API_KEY` is set in the environment of the run. The VM makes a WAV of a sentence with `say`, the key goes into the VM's keychain for the dictation only, and the text pasted into TextEdit must contain at least 80% of the sentence's words. See "Real Groq" below. |
| `e2e_dmg_install` | M5.4 #2 | The tester README's install, scripted: the debug build's DMG passes `scripts/check-dmg.sh`; it goes into the VM with the quarantine flag a browser sets, is mounted, the app is copied out and quarantined, and the README's `xattr -dr com.apple.quarantine` clears it; the app has its licence notice and a valid signature; then a first dictation (tone + mock) pastes `hello world` into TextEdit. The Gatekeeper dialog and the permission prompts are not scripted (manual, in the PR). |
| `e2e_smart_space` | M5.5a #3 | The mock answers `world`. `hello` typed into TextEdit with real key events (`type-keys.js` in Terminal), then a dictation: TextEdit holds exactly `hello world` (the app read the `o` before the cursor over Accessibility). In a fresh empty document: exactly `world`. After `hello ` (typed space): `hello world`, no second space. The log never holds `hello`. |
| `e2e_layout_paste` | M5.5a #4 | For AZERTY (`com.apple.keylayout.French`), Dvorak and Russian: the input source is selected in the GUI session (`input-source.js` in Terminal; enabled first if the clone lacks it), TextEdit comes to the front, a dictation pastes exactly `hello world`, and the layout is still selected afterwards (checked after the dictation: the check runs in Terminal, which takes the front). On Dvorak the chord must use key 47 (where V is); QWERTY's key 9 is ⌘K there and pastes nothing. The starting layout is selected again, and the sources it enabled are disabled again, however it ends (an EXIT trap). |
| `e2e_image_clipboard_restore` | M5.5a #5 | `image-24.png` put on the clipboard as `public.png` (`clipboard-image.js`); after a dictation into TextEdit the clipboard's PNG has the same SHA-256 and the clipboard holds no text. |
| `e2e_launch_at_login` | M5.3 #3 | The login item is off, then turned on through the app (`--debug-launch-at-login-on`, the code behind Settings' "Launch at login" toggle, run through `open` so it is in the GUI session). macOS in the VM is restarted cleanly (System Events `restart`, boot time checked): the app is running by itself once the GUI session is back, and logged `starting` and `ready` once. Turned off again, the flag reports `disabled`. Last in the run, since it reboots the VM (about two minutes). |

## Key cleanup

A scenario that posts a real key-down (`e2e_hold_release`, `e2e_held_at_launch`)
must not leave Right Option held in the VM, where it would spoil later hotkey
tests and `--no-reset` runs. Three layers:

- `arm_key_cleanup <scenario>` (first thing in the scenario) sets an EXIT
  trap, with INT, TERM and HUP routed to `exit`. However the scenario dies
  (a failed command under `set -e`, `die`, Ctrl-C), the trap saves the app's
  log and a screenshot, quits any Terminal job still running, quits the app,
  posts Right Option up (an up with no down does nothing), closes TextEdit's
  documents and Terminal, and exits with the scenario's own status. A normal
  end does the same through `key_scenario_end`.
- `real_key_hold`'s Terminal job has its own EXIT/INT/TERM/HUP trap that
  posts the up if the down was posted, so a job cut short releases the key
  with no help from the host.
- `vm_terminal` kills a job that doesn't finish within
  `VM_TERMINAL_TIMEOUT` seconds (default 30), so it can't go on posting events;
  the kill reaches the job's trap.

To see it work, force a failure and then ask the VM what it thinks:

```sh
E2E_INJECT_FAILURE=after_key_down tests/e2e/run.sh --no-build --no-reset e2e_held_at_launch
E2E_INJECT_FAILURE=mid_hold       tests/e2e/run.sh --no-build --no-reset e2e_hold_release
ssh -i ~/.ssh/dictation_vm_ed25519 -o HostKeyAlias=dictation-vm lume@$(lume get dictation-vm -f json | jq -r '.[0].ipAddress') \
  osascript -l JavaScript '~/e2e-fixtures/right-option.js' state    # right option up
```

`after_key_down` dies right after the down is posted (a failed launch);
`mid_hold` dies inside the hold job between its down and up.

## Real Groq (opt-in)

The scenarios run on the mock engine: `launch` passes `--transcriber mock` to
a first launch (the engine is chosen when the app starts; a forwarded second
launch can't change it). `e2e_error_bad_key` and `e2e_real_groq` set
`E2E_TRANSCRIBER=settings` for their first launch instead, which is the
app's default: the provider, model and language from the settings and the
provider's key from the keychain.

```sh
GROQ_API_KEY=gsk_... tests/e2e/run.sh e2e_real_groq        # a real key, a real request
tests/e2e/run.sh e2e_error_bad_key                         # a wrong key; no key needed
```

How the key reaches the VM: the app reads it from the login keychain, the
way a user's Settings window leaves it, so the test has the app put it
there. The debug build takes two flags that run in a process of their own,
then exit: `--debug-set-api-key-stdin` (the key is read from stdin, never an
argument) and `--debug-delete-api-key`; both go through the same code as the
Settings window (`groq_key_store` and `groq_key_remove` in
`scenarios/_groq_helpers.sh`). The app has to create the item itself: an item
made by the `security` tool doesn't list the app in its access list, so
macOS prompts when the app reads it and nobody is there to click. A release
build ignores both flags. The key is sent on the stdin of ssh, so it is never
in a process list (host or VM), a file, the app's log or a report; the
scenario also checks that the app's log doesn't hold it. The helpers turn
`bash -x` tracing off while they handle the key (and put it back), so a
trace of a run doesn't hold it either; a scenario never expands
`$GROQ_API_KEY` itself. The key is removed again however the
run ends (`groq_guard_start`: an EXIT/INT/TERM/HUP trap that keeps the run's
exit status), and a removal that fails is reported as a failure with a
warning that the key may still be in the VM's keychain; reset the VM then.
`tests/e2e/test-key-handling.sh` (part of the gate, no VM needed) checks all
of this with a made-up key and a stubbed `vm`.

The golden image's login keychain is locked in an ssh session, so every
keychain command unlocks it first with the VM user's password: `lume`, the
throwaway VM user's, or `VM_PASSWORD` if you set it. It travels on stdin too
and is never logged. The app reads no environment variable for a key, so a
release and a debug build have the same one way to get a key. Without
`GROQ_API_KEY` the scenario is skipped, not failed.

The paste is read back from TextEdit with `osascript`, so these two need the
golden image to have Accessibility granted to the app and Automation allowed
for `osascript` to control TextEdit (M4.3).

## Requirements on the host

- `lume` (at `~/.local/bin`), `jq`, `ssh`, `tar`, plus the toolchain for the
  build (`npm ci` done).
- The VM ssh key at `~/.ssh/dictation_vm_ed25519` (override with `SSH_KEY`).
  `known_hosts` needs the `dictation-vm` host-key alias, since every clone
  gets a new DHCP address but keeps the host key.
- VMs `dictation-vm-golden` (clean, with the `lume` user logged in to the
  GUI) and, after the first reset, `dictation-vm`.
- Only one run at a time: a reset deletes `dictation-vm`, so check that no
  other agent is using the VM (cua-vm MCP, a Tester run) first.

## macOS notes (Gatekeeper, TCC)

- **Gatekeeper:** files copied over ssh carry no `com.apple.quarantine`
  attribute, so the self-signed debug build opens without the "unidentified
  developer" dialog. If you copy it any other way (AirDrop, a browser
  download), clear the attribute first: `xattr -dr com.apple.quarantine
  ~/Applications/dictation-app.app`.
- **GUI session:** apps started directly from an ssh shell don't get a
  window server connection. `open` asks LaunchServices to launch the app in
  the logged-in user's GUI session, which is why the scenarios use it. `-n`
  starts a new process even while one runs, the way a second command-line
  launch would. Without `-n`, `open` just activates the running app and
  drops the arguments.
- **Accessibility:** granted in the golden image to the app, to Terminal and
  to `cua-driver`, so the real hotkey, the real paste and the key events work
  with no prompt. Without it the real Right Option hotkey can't start, and
  the app logs (at warn, right after `starting`):

  ```
  WARN dictation_pipeline::test_hooks: PermissionMissing: the hotkey is off; only the --press/--release test hooks work error=Accessibility permission is missing.
  ```

  The synthetic flags still work without it, but the paste then fails. Granting
  Accessibility in a VM shows a password sheet that refuses synthetic keys
  (see `test-reports/M1.4-run2-vm.md`), so it was granted through the VM's
  own display, not the host screen.
- **Microphone:** granted to the app in the golden image (M4.3, issue #92),
  so a launch without `--debug-audio-file` records the real microphone with
  no prompt. The other scenarios pass `--debug-audio-file` anyway: the same
  audio every time, no input device needed.
- **Paste:** the real paste (M4.1) into TextEdit. The app is a menu-bar app
  and doesn't take focus, so the paste lands in whatever is in front: the
  scenarios call `textedit_fresh` after the app is up.
- **Real key events:** an ssh session has no Accessibility grant, so a
  script run from it can't post key events (`AXIsProcessTrusted()` is false).
  `vm_terminal` (in `lib_full.sh`) runs a command in Terminal.app instead,
  with `open -a Terminal <script>.command` from ssh; Terminal has the grant.
  `fixtures/right-option.js` is a JXA script that posts a `flagsChanged`
  event for key 61 (Right Option) with the alternate mask and the right-alt
  device bit (0x40), down or up; the system's modifier state then shows the
  key held, until the up event. Running a script in Terminal puts Terminal in
  front, so `real_key_hold` puts TextEdit back with `open -a TextEdit` before
  it presses. Terminal is quit at the end of the scenario.
- **Screenshots and clicks:** `cua-driver` runs as a daemon in the VM (with
  Screen Recording and Accessibility), so `ssh … '~/.local/bin/cua-driver
  call get_desktop_state "{}"'` returns a screenshot (`screenshot_png_b64`).
  To click a system prompt (the owner's `cua-vm` MCP does the same from the
  host), use `run_steps` in one call:
  `cua-driver call run_steps '{"pid":<UserNotificationCenter pid>,"window_id":<id from list_windows>,"steps":[{"click":{"role":"button","name":"Allow"}}]}'`.
  Element tokens don't survive from one `cua-driver call` to the next.

## The golden image

`dictation-vm-golden` is changed by hand, never by a run, and with a backup
first (`lume clone dictation-vm-golden dictation-vm-golden-backup-<date>`;
neither is ever deleted). Start it with `lume run dictation-vm-golden
--no-display --detach`, reach it over ssh like `dictation-vm`, and shut it down
from inside (`osascript -e 'tell app "System Events" to shut down'`), never by
killing it. A password sheet means stop: the owner types the VM password
through the VM's display.

What M4.3 changed (backup `dictation-vm-golden-backup-20261101`):

- The M4.3 debug build at `~/Applications/dictation-app.app`, signed
  `dictation-app Dev` (runs replace it).
- **Microphone** granted to `dictation-app`: launched, a `--press` raised the
  macOS prompt, and "Allow" was pressed with `cua-driver` over ssh. Shown in
  System Settings > Privacy & Security > Microphone; a real-microphone
  dictation then ran without the permission error. This closes #92.
- **Automation** for ssh sessions (`sshd-keygen-wrapper`) to control TextEdit
  and System Events, so the scenarios can read TextEdit back with `osascript`
  over ssh and the golden image can be shut down from inside. Both prompts
  were allowed once.

The grants belong to the app's code signature: its designated requirement
names the bundle id and the hash of the `dictation-app Dev` certificate that
`scripts/make-dev-cert.sh` made on this Mac. A build signed with another
certificate (another Mac, or a new certificate) would not match them; if a run
shows `Microphone permission is missing.` or `PermissionMissing`, redo the
grants above. (Not tried: every run so far used this Mac's certificate.)

## Adding a scenario

Put a function in `tests/e2e/scenarios/<name>.sh`, named after the file,
using the helpers in `lib.sh` (`launch`, `wait_for_log`, `logged_phases`,
`app_count`, `expect`, `collect_log`) and, for full-app scenarios,
`lib_full.sh` (`launch_full`, `textedit_fresh`, `textedit_trimmed`,
`set_clipboard`, `clipboard`, `dictate_once`, `real_key_hold`). A scenario that
posts a real key-down calls `arm_key_cleanup` first and `key_scenario_end` last.
Add it to `ALL_SCENARIOS` in `run.sh`, and to `OPTIONAL_SCENARIOS` if it may
skip without failing the run.
