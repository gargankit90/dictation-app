# End-to-end tests

The built app, run inside a Lume VM and driven from the host over ssh. PRs
don't run them (the Tester does), and no GitHub workflow does: the Lead runs
them on the owner's Mac after each merge with `scripts/e2e-main.sh` (see "After
each merge" below). Run them locally:

```sh
tests/e2e/run.sh                                   # build, reset the VM, run all scenarios
E2E_VM=dictation-vm-2 tests/e2e/run.sh             # the same, in the second VM
tests/e2e/run.sh e2e_startup                       # one scenario
tests/e2e/run.sh --exclude e2e_real_groq           # all but that one (repeatable)
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
`cua-driver` daemon in the VM). A scenario that drives the app's UI and fails
also saves the app's accessibility tree and tray state
(`<scenario>.ax.txt`), which shows what the scenario could and couldn't find.

Each scenario runs in a subshell of its own with `set -e`, so one that dies
(a failed command, `die`, a signal) ends only itself: it is a `FAIL` with
its exit status (`ended early (exit status N)`), its log and screenshot are
saved, and the run goes on. A scenario passes only by reaching its end with
no failed check; one that exits half way, even with status 0, fails. See
"Trusting the helpers" below for what else keeps a failure from looking
like a pass.

## Two VMs

Apple allows two macOS VMs at once, so there are two test VMs, both clones of
the golden image `dictation-vm-golden`:

| VM               | Used by    | Selected with                 |
| ---------------- | ---------- | ----------------------------- |
| `dictation-vm`   | Developers | the default                   |
| `dictation-vm-2` | Testers    | `E2E_VM=dictation-vm-2`       |

**Developers use `dictation-vm`, Testers use `dictation-vm-2`.** That way a
Developer's run and a Tester's run (and the `cua-vm` MCP server, which also
points at `dictation-vm`) don't reset each other's VM.

- `E2E_VM` is `dictation-vm` (default) or `dictation-vm-2`. Any other name,
  the golden image and its backups included, is refused before `lume` is
  called. Everything that names the VM follows it: the `lume` calls, the ssh
  `HostKeyAlias`, the IP lookup and the output directory
  (`tests/e2e/out/<timestamp>-<vm>/`). The first run on `dictation-vm-2` copies
  `dictation-vm`'s `known_hosts` entries to the alias `dictation-vm-2` (clones
  keep the golden image's host key).
- **Memory and CPUs:** after cloning, a reset runs `lume set <vm> --memory 6GB
  --cpu 4` before starting the VM. Override with `E2E_VM_MEMORY` (default
  `6GB`) and `E2E_VM_CPU` (default `4`). `lume set` runs only on the fresh
  clone, never on the golden image. 6 GB each leaves the Mac room for two VMs
  and a build.
- **A lock per VM:** a run takes `~/.cache/dictation-e2e/<vm>.lock` after the
  build and holds it until the run ends, however it ends (normal end, `die`,
  Ctrl-C, kill, closed terminal, a scenario's own cleanup trap). The lock is a
  symlink whose target is the run's `<pid> <time>`: `ln -s` creates it with its
  owner in one atomic step, so a lock is never held by "nobody yet", and a lock
  whose pid is alive is never taken over, however young. A second run on the
  same VM stops at once with `VM busy: <vm> is in use by a run (pid N since
  T)`; start it on the other VM or wait. The traps are armed before any lock
  is taken, and a release removes only a lock whose pid is the run's own. A
  short `golden-clone.lock` makes two runs clone the golden image one after
  the other (the second waits, up to 10 minutes, or `E2E_CLONE_LOCK_WAIT`
  seconds). Each scenario runs in a subshell (`run_scenario`), so the cleanup
  traps it arms don't replace the run's.
- **A stale lock:** a run killed with `kill -9` (or a crashed Mac) leaves its
  lock behind with a pid that is gone. Nothing takes such a lock over by
  itself; the next run on that VM fails with

  ```
  stale lock for dictation-vm from pid 12345 (no longer running); remove it with: tests/e2e/run.sh --clear-stale-lock dictation-vm
  ```

  Run that command and start again. `--clear-stale-lock <name>` needs no
  build or VM, and `lume` only for `vm-start` (below). It removes the lock only if its pid is not running (it
  refuses with `pid N is still running` otherwise) and only for
  `dictation-vm`, `dictation-vm-2` and the two host-wide locks (`vm-start`,
  `golden-clone`); any other name, the golden image and its backups
  included, is refused. The same message and command cover `vm-start` and
  `golden-clone`. If the pid was reused by an unrelated process, the command
  refuses too; check the lock with `readlink ~/.cache/dictation-e2e/<name>.lock`
  and remove that file by hand. Run it only when no run is starting on that
  VM. `vm-start` is stricter: its lock stands for a start nobody confirmed, so
  `--clear-stale-lock vm-start` also asks `lume ls` and refuses while the VM
  the lock was taken for is running, while two macOS VMs are running, or when
  `lume ls` fails; it names the VM to `lume stop` first.
- **At most two running VMs:** a reset refuses to start when two other macOS
  VMs are running already (`lume ls`; the golden image counts if it is
  running). The count and `lume run` are one step under `vm-start.lock`, held
  until `lume ls` lists the new VM as running, polled up to
  `E2E_START_VISIBLE_WAIT` seconds (default 120). The lock is never released
  while the start is unconfirmed, so a third start is refused (or waits, up to
  `E2E_START_LOCK_WAIT` seconds, 300) until then. If the deadline passes
  (or `lume ls` keeps failing), the reset runs `lume stop <vm>`, releases the
  lock and fails with `lume ls didn't list <vm> as running within N s`. If
  `lume stop` fails, or the run gets a signal after `lume run`, the lock is
  kept instead (stale once the run exits): the next start fails with the
  stale-lock message until the VM is stopped and `--clear-stale-lock vm-start`
  is run. Two
  resets together can't both start. Lock order: VM lock, then the clone lock
  or the start lock (never both).
- The lock is per VM, not per checkout: two worktrees pointing at the same VM
  also exclude each other. Two parallel runs from one checkout share the
  `target/` build, so give the second `--no-build`.
- `tests/e2e/test-vm-lock.sh` (part of the gate, no VM needed) checks all of
  this with a stubbed `lume`.

## After each merge

`scripts/e2e-main.sh [<sha>]` runs the suite on a commit of `origin/main`; the
Lead runs it on the owner's Mac after each merge. It replaces the old E2E
workflow: the repository is public, hosted CI is open to pull requests from
forks, and the VMs and the signing certificate live on the owner's Mac, so no
workflow may reach it.

- **Which commit:** the `origin/main` tip, or the SHA you pass. A SHA that is
  not reachable from `origin/main` (a branch, a PR head, a made-up object) is
  refused with exit 2, before anything is built.
- **How:** it checks the commit out into a fresh temporary worktree (never
  your working tree), runs `npm ci` and then `tests/e2e/ci-run.sh` there, and
  removes the worktree at the end, also after a failure or Ctrl-C.
- **What runs:** every scenario except `e2e_real_groq` and `e2e_real_mic`
  (no API key and no real microphone); the rest use the local provider.
- **The pool:** `ci-run.sh` builds once, then starts `run.sh --no-build` on
  `dictation-vm-2` and, if its lock is taken (or stale), on `dictation-vm`.
  The lock is `run.sh`'s own, so a Developer's or Tester's run just makes it
  wait or take the other VM, and it makes them wait the same way. If both stay
  taken it polls every 30 s (`CI_VM_POLL`) for 45 minutes (`CI_VM_WAIT`,
  seconds) and ends as `skipped: no VM free` with exit 0: a busy VM is not a
  broken `main`. `CI_VM_POOL="dictation-vm"` narrows the pool; it can never
  name anything else, so never the golden image, and `run.sh` still refuses a
  third running VM.
- **Result:** exit 0 for passed or skipped, 1 for a failed build or scenario.
- **Evidence:** `target/e2e-main-evidence/<sha12>-<time>/` in your checkout
  (untracked; `E2E_MAIN_EVIDENCE_ROOT` moves it). A passing run keeps only
  `about.txt` (SHA, VM, status) and `results.txt` (PASS/FAIL/SKIP per
  scenario): a few KB (#174). A failed or skipped run keeps everything:
  the run's output, the app logs, `.ax.txt` files and screenshots, with
  key-shaped strings hidden. Delete old directories by hand when you like.
- **A stale lock** (a killed run) makes it skip that VM, and the run ends as
  skipped if both are stale: clear it with `tests/e2e/run.sh
  --clear-stale-lock dictation-vm-2` (or `dictation-vm`), as above.
- `scripts/test-e2e-main.sh` (part of the gate, no VM) checks the refusal, the
  worktree and the evidence size with a stub `ci-run.sh`;
  `tests/e2e/test-ci-run.sh` (through `test-runner.sh`) checks the pool with a
  fake `run.sh` and real lock files.

## What a run does

1. **Build:** `npm run tauri build -- --debug --bundles app`. The bundle is
   signed with the `dictation-app Dev` certificate (`scripts/make-dev-cert.sh`,
   once per Mac).
2. **Reset the VM** (skipped with `--no-reset`; the VM is `dictation-vm`, or
   the one `E2E_VM` names): the VM lock is taken first, then
   `lume stop dictation-vm`, `lume delete dictation-vm --force`,
   `lume clone dictation-vm-golden dictation-vm`,
   `lume set dictation-vm --memory 6GB --cpu 4` and
   `lume run dictation-vm --no-display --detach`. The run then waits for an
   IP (`lume get dictation-vm -f json`, `.ipAddress`) and for ssh to answer.
   The golden image is only cloned; change it by hand (see "The golden image"
   below), never from a run.
3. **Install:** the `.app` is copied with `tar` over ssh into
   `~/Applications` in the VM, and its signature is checked there.
   `tests/e2e/fixtures/` goes to `~/e2e-fixtures` in the VM: `tone-16k.wav` (a
   1.5 s 440 Hz tone, 16 kHz mono 16-bit; no voice), `silence-16k.wav` (1.5 s
   of zeros), `right-option.js` (posts a real Right Option key event, or
   says with `state` whether the system thinks it is held), `post-key.js`
   (the same for any modifier key: `post-key.js left-option down|up|state`),
   `type-keys.js` (types a-z and spaces with real key events, US key
   positions, no modifiers), `input-source.js` (reads or switches the keyboard
   input source), `clipboard-image.js` (puts a PNG on the clipboard or reads
   it back), `image-24.png` (a 24x24 test image), `ax.js` (drives the tray,
   menus and windows through the Accessibility API; see "The app's UI" below)
   `clipboard-race.js` (copies like a user while a dictation's paste is
   pending, for `e2e_user_copy_wins`), `keys.js` (presses Escape and the
   arrows, alone or with Shift, Command or Right Option, with real key
   events), `local-provider.pl` (the local provider, below),
   `combo.js` (holds Ctrl+Shift+A with real key events, for a combination
   hotkey) and `update-server.pl` (serves a throwaway update's `latest.json` and bundle on
   loopback, for `e2e_update_keeps_permissions`).
4. **Scenarios:** each one quits any running copy of the app, clears its log
   (`~/Library/Logs/com.dictationapp.desktop/` in the VM), launches the app
   with `open -n …/Vocet.app --args <flags>`, and checks the log and
   the process list.

| Scenario | Criterion | Checks |
| -------- | --------- | ------ |
| `e2e_startup` | M1.5 #1 | The app starts (first launch with `--debug-audio-file`, so the press needs no mic permission) (`ready` in the log) and one copy runs. A second launch with `--press` is forwarded (`test hook event=Pressed from="a second launch"`) and the pipeline starts recording. Still only one process; the app was set up once. |
| `e2e_press_release` | M1.5 #2, M2.2 #1 | A first launch with `--debug-audio-file ~/e2e-fixtures/tone-16k.wav --press`, a second with `--release` a second later. The app records the fixture, not the mic; the log shows `Recording Transcribing Inserting Idle` in that order, and the stand-in transcript isn't in it. |
| `e2e_microphone_denied_then_recover` | T4b (#126), M5.7 (#161) | In the VM's clone only, `tccutil reset Microphone com.dictationapp.desktop`; the app starts with no `--debug-audio-file` (which skips the permission check) on the mock. Before any press the tray menu has no Relaunch. A press: the app logs `waiting for the microphone prompt` and macOS asks; while the prompt is up nothing shows an error (none logged, the overlay hidden, the tray plain, no Relaunch offered) and the app stays Idle. `Don’t Allow` is pressed (`ax.js` in UserNotificationCenter), the app logs `microphone prompt answered granted=false` (the scenario fails if the denial isn't reached) and System Settings shows its switch off. Another press: no new prompt, and `Microphone is off. Allow it in Privacy & Security, Microphone, then relaunch from the menu bar.` in the log, rendered in the overlay and in the tray's tooltip, and the tray menu offers `Relaunch Vocet`; no phase logged (Idle), nothing pasted, the clipboard untouched; Settings opens and closes. The switch is turned on in System Settings › Privacy & Security › Microphone; a press in the running app is only recorded (macOS doesn't tell it; see `docs/ENGINEERING.md`); Relaunch from the tray menu restarts the app, and a dictation records the microphone and pastes. Evidence (macOS, the signature, the switch, the answers) in `<scenario>.evidence.txt`. If the scenario didn't see the grant work, the cleanup grants it again through the prompt (`E2E_INJECT_FAILURE=after_mic_denied` shows it), so `e2e_real_mic` after it passes. |
| `e2e_real_mic` | M2.2 | `--press`, then `--release`, recording the VM's real microphone (granted in the golden image): no permission error or microphone failure, and either the full state sequence or, when the VM's microphone heard only silence (it varies; Lume feeds it), `Recording Transcribing Idle` with the log saying `not transcribing … verdict=Silent`. `E2E_REAL_MIC=0` skips it. |
| `e2e_mock_paste` | M4.3 #1 | Full app: the tone fixture, `--mock-text "hello world"`, the real paste. TextEdit (a fresh document in front) holds exactly `hello world`, read back with `osascript`; the log has the full state sequence and not the transcript. |
| `e2e_paste_leaves_no_modifier` | M5.6 #1 (#140) | Reads `CGEventSourceFlagsState` (combined and HID) and the flags of a source-less key event in Terminal before any paste, after a synthetic dictation and after a real Right Option hold. Each paste lands in TextEdit, and none of the three words carries the Command mask or a Command key's device bit afterwards. |
| `e2e_clipboard_restore` | M4.3 #2 | A `pbcopy` sentinel is still on the clipboard after a dictation, and after a second one; the transcript is not left there. |
| `e2e_silence` | M4.3 #3 | `silence-16k.wav`: `Recording Transcribing Idle` (never Inserting), the log says `not transcribing … verdict=Silent` and no `dictation failed` (the dropped dictation shows no error), TextEdit stays empty and the clipboard sentinel is untouched. |
| `e2e_rapid` | M4.3 #4 | Five dictations in a row (0.4 s holds, each started once the last is back in Idle): five complete `Recording Transcribing Inserting Idle` sequences, ending in Idle, and `--mock-text-sequence one,two,three,four,five` gives each dictation its own word, and TextEdit must hold exactly `one two three four five` (the spaces are the smart leading space, M5.5a): a paste out of order, doubled, dropped or replayed from an earlier dictation changes it. |
| `e2e_hold_release` | #90, M1.6 #4 | A real Right Option hold (down, 1.5 s, up) posted from Terminal.app: one `Recording … Idle` sequence and the transcript in TextEdit, with no synthetic `--press` flag in the log. However it ends, the key is released (see "Key cleanup"). |
| `e2e_hotkey_listener_recovers` | #88, M5.5b #5 | A real Right Option hold dictates. `--debug-hotkey-outage` kills the hotkey's listener as revoking Accessibility would (re-granting it in the VM needs a person at the password sheet): the tray says `Hotkey unavailable`, the log shows the supervisor retrying, and a hold starts nothing. `--debug-hotkey-recover`: the app starts a new event tap by itself, the tray clears, and the next real hold pastes the transcript, with no relaunch. |
| `e2e_accessibility_revoked` | M5.9 #6 (#173) | The app runs with its real hotkey; typing (`type-keys.js`, the job also prints the frontmost app) reaches TextEdit and a mouse move (`mouse.js`) lands. The app's switch in System Settings › Privacy & Security › Accessibility is turned off as a person would: the switch pressed through AX, then the password typed into macOS's sheet over the VM's VNC server (`vnc_keys.py`; the sheet refuses software key events). Typing still reaches TextEdit and the mouse still moves, right after and 5 s later (before the fix, typed keys stopped arriving until the app quit). Quit and started again, the app logs `PermissionMissing`, shows the hotkey message in the log and tray, and offers Relaunch. The switch on again (password over VNC): the hotkey comes back by itself, Relaunch goes, and a real hold dictates; after a quit and relaunch the hotkey listens at once and a hold dictates. The cleanup turns the switch back on if the scenario left it off. Evidence in `<scenario>.evidence.txt`. |
| `e2e_hotkey_shortcuts` | M5.9 #2 (#172) | Hotkey `RightCmd` (settings file). In TextEdit with `alpha` selected, right ⌘+C (`chord.js`, held about 0.7 s) copies it and nothing gets past Recording; right ⌘+V at the end pastes the user's clipboard and only that; the clipboard is the user's. A right ⌘ hold alone still dictates. |
| `e2e_hotkey_recorder` | K1 (#170, #232) | Settings → Dictation key records the pressed key. The keycap shows `Right ⌥`; **Record a different key** starts recording; a plain `a` (`type-keys.js`) is refused with a reason and recording goes on; Esc (`keys.js`) cancels and saves nothing (each check runs in the same Terminal job as its key, because a job that ends brings Terminal to the front and the recorder cancels on blur). Holding the current key (Right Option) while recording starts no dictation: no phase is logged and nothing is pasted. A real Right ⌘ press records `RightCmd`: the keycap shows `Right ⌘`, the app logs `hotkey applied … hotkey=RightCmd`, `settings.json` has it, and a Right ⌘ hold dictates at once. After Quit and relaunch (`ready … hotkey=RightCmd`) it dictates again and Settings still shows `Right ⌘`. The key cleanup releases the keys and removes `settings.json`. |
| `e2e_no_words_transcript` | M5.9 #5 (#172) | The mock answers `.`; a dictation with `alpha` selected in TextEdit is transcribed but never inserted: TextEdit still holds `alpha`, the clipboard is unchanged, no error shown. |
| `e2e_first_paste_after_launch` | P1 #1, #5 (#239) | For Right Option and for `Ctrl+Shift+A` (settings file), `E2E_FIRST_PASTE_RUNS` times (default 3): relaunch the app, put an empty TextEdit in front and hold the key once (`combo.js` for the combination). Each first paste lands; an OBS line per try gives the clipboard afterwards (the user's must be back after the app's 1.5 s `RESTORE_DELAY`). |
| `e2e_quit_right_after_paste` | P1 round 4 (#241) | A clipboard sentinel, then a dictation into TextEdit while a Terminal job armed beforehand clicks Quit in the tray as soon as the log shows Inserting (inside the app's 1.5 s `RESTORE_DELAY`). TextEdit got the transcript; after the app is gone the clipboard holds the sentinel again, and the log says `restored the clipboard before quitting`. |
| `e2e_held_at_launch` | #90, M1.6 B1 | Right Option down, then the app starts and stays up 5 s with the key still held: no Recording. The key goes up: still nothing. Then a normal hold: exactly one `Recording … Idle` sequence. |
| `e2e_overlay_focus` | M5.1 #3, #102 M1 | The full app with the tone and the mock transcriber, TextEdit in front. Once started, the log's `overlay native window` line must say the overlay is still the non-activating panel (`non_activating_panel=true`: an `NSPanel` subclass, `canBecomeKeyWindow` and `canBecomeMainWindow` false, the non-activating style mask, and backing the overlay's webview window), read back by the app on the main thread. `--press`: the log shows `overlay shown state=recording` and the app has a 420x72 window at the status level (25) on screen, read with `CGWindowListCopyWindowInfo` through JXA over ssh (`cua-driver list_windows` leaves such windows out); a screenshot is saved as `e2e_overlay_focus.recording.png`. `--release`: `overlay shown state=transcribing`, back to Idle, `overlay hidden`, and the window is off screen again. The frontmost app, sampled with `lsappinfo` every 0.2 s from before the press until after Idle, is always TextEdit, and TextEdit holds exactly `hello world`. This is also the only guard of the launch itself (`menu_bar::builder` not activating the app, #138): Tauri's builder can't be inspected outside a real launch, so the unit test in `menu_bar.rs` only pins that the call is in the source. |
| `e2e_error_bad_key` | M4.4 #2 | The VM's login keychain gets the invalid key `gsk_invalid_test_key`; the app starts on the real engine (`--transcriber settings`) and dictates the tone fixture. Groq's 401 shows as `The provider rejected your API key…` in the log (`shown to the user`), the phases are `Recording Transcribing Idle` (nothing inserted), TextEdit stays empty, and a second dictation fails the same way (the app is still usable). Then the app restarts on the mock and a dictation pastes `This is a test dictation.`. Real request to Groq, so the VM needs internet; needs no key from you. |
| `e2e_local_model` | L1 #8 | Settings → **Speech**, **Download** the default model (the real ~477 MB Hugging Face file; progress, then _Downloaded_), **Use this** (applied once the model has loaded; U6 has no Save); three dictations of `fixtures/speech-16k.wav` (a `say` clip, `--debug-audio-file`) with the real on-device engine each paste text containing "dictation test", with the time from press to Idle logged as `LATENCY`. Then the same offline, if the VM allows passwordless sudo to switch its network off (#222); otherwise a NOTE. Uses `ax.js radio-press`. |
| `e2e_real_groq` | M4.4 #1 | Skipped unless `GROQ_API_KEY` is set in the environment of the run. The VM makes a WAV of a sentence with `say`, the key goes into the VM's keychain for the dictation only, and the text pasted into TextEdit must contain at least 80% of the sentence's words. See "Real Groq" below. |
| `e2e_overlay_rendered_states_and_error` | T4b (#119) | The overlay's rendered text, read from its WebView's AX tree (`ax.js overlay`: `hidden`, or `shown: ` and the text), on the local provider with replies held. A `--press` on the first launch (delivered at `ready`) renders Listening (`Recording` before U4; an event sent before the page listens was lost; the page now says when it listens), the reply held renders `Transcribing…`, and after the paste (and U4's `Done`) it is `hidden` and the panel off screen; a second dictation the same. The provider then takes another key: the 401's `Transcription failed: The provider rejected your API key. Check it in Settings.` is rendered, watched every 0.1 s (`ax.js overlay-watch`, started while recording): it stays up 6 s (±0.6 s, also by the log) with nothing in between, then hides and leaves the screen. Another failed dictation, and a press right after its error shows: Recording replaces it and stays up past the error's 6 s; with the key accepted again that dictation pastes. |
| `e2e_pill_states` | U4 | The recording pill v2, read from the overlay's AX tree and the log, with a screenshot of each state. The file source plays its level in real time, looping: with the tone the pill renders `Listening Release to paste · Esc to cancel` and stays Listening after 2.5 s; the mock's paste renders `Done` for 800 ms (±0.3 s, by the log) and then hides. With the silence fixture it renders Listening, then `No sound Check your microphone` about 2.5 s (±0.3 s) after the press; nothing is pasted, so it hides without `Done`. |
| `e2e_dmg_install` | M5.4 #3 | `docs/TESTERS.md`, scripted: the debug build's DMG passes `scripts/check-dmg.sh`; it goes into the VM with the quarantine flag a browser sets, is mounted, the app is copied out and quarantined, and the guide's own `xattr -dr com.apple.quarantine /Applications/Vocet.app` line (read from `docs/TESTERS.md`, path swapped to `~/Applications`) clears it; the app has its licence notice and a valid signature; a first dictation (tone + mock) pastes `hello world` into TextEdit. Then the guide's sections 3–5: Microphone and Accessibility hold (`permissions_hold`: hotkey on, a paste, a recording with no WAV and no permission error or prompt), the made-up key typed into Settings, **Test & Save** (Speech section) reaches the local provider and says **✓ Connected · saved in Keychain**, and a dictation pastes the provider's transcript (2 requests in all). The key is removed again however it ends. The Gatekeeper dialog and the permission prompts themselves are not scripted (the golden image has both granted). |
| `e2e_update_keeps_permissions` | M5.4 #4, #5 | The updater on a **throwaway** key (never the owner's, never the releases repository): `scripts/updater-test.sh` builds 0.1.0 and 0.1.1 (signed `dictation-app Dev`, trusting only that key, fetching `http://127.0.0.1:18741/latest.json`) into `target/e2e-updater/`; a second throwaway key signs the same bundle for a bad manifest. `fixtures/update-server.pl` serves one manifest at a time on the VM's loopback. 0.1.0 has both permissions; up to date: the launch check (`--debug-update-check`) shows nothing, the tray's **Check for updates…** says _You're up to date_. Bad signature: **Install and restart** ends in _Couldn't install the update_ with the signature message, the same process keeps running, 0.1.0 still installed. Server down: _Couldn't check for updates_. Good 0.1.1: the launch check offers it by itself (**Later** keeps 0.1.0), the tray's check installs it, the app restarts as 0.1.1 by itself (one copy), the bundle is 0.1.1, signed `dictation-app Dev`, not quarantined, and both permissions still hold with no prompt. `permissions_hold` waits for TextEdit's window to be on screen and TextEdit frontmost before it dictates, and keeps each launch's app log (`permissions-<version>-launch<n>.app.log`) and, on a failed paste, a screenshot. The server is stopped by its own pid (checked to be this fixture), its folder removed and the run's build reinstalled, however it ends. |
| `e2e_smart_space` | M5.5a #3 | The mock answers `world`. `hello` typed into TextEdit with real key events (`type-keys.js` in Terminal), then a dictation: TextEdit holds exactly `hello world` (the app read the `o` before the cursor over Accessibility). In a fresh empty document: exactly `world`. After `hello ` (typed space): `hello world`, no second space. The log never holds `hello`. |
| `e2e_password_field_not_read` | M5.8 #2 (#135) | The app logs at debug (`open --env DICTATION_LOG=debug`); the mock answers `world`. The target is `password-field.js`, run in Terminal: a window whose one field holds `hello` with the cursor after it, and an Edit menu for ⌘V. Control, a plain field (no subrole): a dictation gives exactly `hello world` (the app read it). Then the same field answering `AXSecureTextField` to `AXSubrole`, with Secure Input checked off (so the subrole, not Secure Input, stops the read): exactly `helloworld` (no space), and the debug log has `no leading space error=a password field has focus` once. Both dictations paste, and the log never holds `hello`. The fixture stops when asked, or the scenario kills the one pid it wrote. |
| `e2e_layout_paste` | M5.5a #4 | For AZERTY (`com.apple.keylayout.French`), Dvorak, Russian, Dvorak Right-Handed and Turkish (`Turkish-Standard`): the input source is selected in the GUI session (`input-source.js` in Terminal; enabled first if the clone lacks it), TextEdit comes to the front, a dictation pastes exactly `hello world`, and the layout is still selected afterwards (checked after the dictation: the check runs in Terminal, which takes the front). Only the layouts whose V is not on QWERTY's key 9 under ⌘ prove the lookup (#136): Dvorak (key 47; key 9 is ⌘K there and pastes nothing), Dvorak Right-Handed (43) and Turkish (8); AZERTY and Russian use key 9 and prove no regression. The starting layout is selected again, and the sources it enabled are disabled again, however it ends (an EXIT trap). |
| `e2e_image_clipboard_restore` | M5.5a #5 | `image-24.png` put on the clipboard as `public.png` (`clipboard-image.js`); after a dictation into TextEdit the clipboard's PNG has the same SHA-256 and the clipboard holds no text. |
| `e2e_launch_at_login` | M5.3 #3, T4b (#123) | Off at the start (`--debug-launch-at-login-status`, the code behind Settings). Settings → General's "Open Vocet at login" switch (`ax.js check-set`) is turned on and applies at once (the window answers within 5 s while the OS works; U6 has no Save); closed and reopened it shows the OS's state, checked, and the OS says enabled. Quit from the tray, macOS restarts (System Events `restart`, boot time checked): exactly one copy starts by itself, logs `starting` and `ready` once, no test hook, and a real Right Option hold with Esc while recording gives `Recording Idle` (no request), nothing pasted, the clipboard untouched. The switch is turned off, reopened off, the OS says disabled; Quit, a second restart: the GUI session is back (Dock, GUI domain) and for 60 s (at least three times the first auto-start's delay; `LOGIN_QUIET_SECS`) no copy runs and nothing is logged. A manual launch then starts one ready copy. The cleanup turns the item off and resets the settings. Last in the run, since it reboots the VM twice (about four minutes). |
| `e2e_single_instance_cold_race` | T2 #2 (P1-01) | Four copies launched at once in one ssh command (`open -g -n`, each with a first launch's flags), none waiting for another. Exactly one process is left, and stays; the log has one `starting`, one `ready` and one `engine` line; the other three launches were each handed to it (three `ignoring the debug audio and mock text flags` warnings, no `not delivered`); the tray has one status item (`ax.js tray-info`). A later `--press`/`--release` is forwarded (`delivered=true`) and the dictation pastes `hello world` into TextEdit. |
| `e2e_user_copy_wins` | T2 #7 (P2-05) | A clipboard sentinel, then a dictation while `clipboard-race.js` (over ssh) watches the clipboard: on the app's dictation write (its `com.dictationapp.session` item) it waits 0.5 s (`E2E_USER_COPY_DELAY`, inside the app's 1.5 s `RESTORE_DELAY`), checks that item is still there (`pending=yes`: the restore hasn't run) and copies a new text. TextEdit got the transcript; 2 s after Idle the clipboard holds the user's copy, not the sentinel or the transcript, and its change count is the one right after the copy (nothing wrote after it). |
| `e2e_secure_input_blocks_paste` | T2 #7 (P2-05), M4.2 #1 | The target is a Terminal window running `cat` in non-canonical mode, saving what is pasted into it to a file. Control: with Secure Keyboard Entry off, a dictation's word lands in the file. Then Terminal restarts with `defaults write com.apple.Terminal SecureKeyboardEntry -bool true` (Terminal's menu item if that doesn't do it), Secure Input must be on (`ioreg` `kCGSSessionSecureInputPID`), and a second dictation leaves the file and a clipboard sentinel unchanged and logs `shown to the user error=A password field is focused; dictation wasn't pasted.`. Secure Keyboard Entry goes off again however it ends (an EXIT trap), and Secure Input must then be off. |
| `e2e_quit_during_recording_then_relaunch` | T2 #7 (P2-05) | `--press`, then Quit Vocet in the tray menu (`ax.js tray-click`) while recording: the app exits by itself within 15 s (`QUIT_TIMEOUT`; no kill), logs `quit from the tray` and `stopped`, never reached Inserting, TextEdit is empty and Right Option is up. A relaunch logs one `starting`, and a dictation pastes. |
| `e2e_tray_error_clears_on_next_recording` | T2 #7 (P2-01) | No key in the keychain and the real engine (`--transcriber settings`): a dictation fails offline with `Transcription failed: Add your API key in Settings.`: the tray's title becomes `⚠ Dictation failed` (the same short text for every error, so the status item always fits in the menu bar, #151) and its tooltip `Vocet: ` and the message (`ax.js tray-info`, the status item's AXTitle and AXHelp). The next `--press` clears both while it records; its release shows the error again. Then Quit from the tray, a relaunch on the mock: the dictation pastes and the tray stays clear. |
| `e2e_settings_save_reopen` | T2 #1, #7 (P1-03, P2-01) | Settings… in the tray opens the window (on screen, by the window server's list); in its Dictation section (U6: changes apply at once, no Save button) it shows `Right ⌥` and "Detect automatically". German is picked in the Language popup (`ax.js combo-pick`: focus, type `german`, Return) and is saved at once (the log and `settings.json`); Left Option is recorded as the shortcut (Record a different key, a real key from `post-key.js`), the app logs `hotkey applied … hotkey=LeftAlt`, the keycap shows `Left ⌥`, and `settings.json` has both (`de`). The close button hides the window; Settings… shows it again on Dictation with the saved values ("German (Deutsch)"). A real Left Option hold (`post-key.js`) then dictates into TextEdit with no restart, and a Right Option hold starts nothing. Quit from the tray and relaunch: `ready … hotkey=LeftAlt`, Left Option dictates, Right Option doesn't. The key cleanup releases both keys and removes `settings.json` (the defaults). |
| `e2e_update_from_dictation_app` | U1 round 2 | The build from before the rename (`E2E_TRANSITION_OLD_REF`, default `b7ae67e`; `Vocet.app`) is installed with a saved key, non-default settings and launch at login on; the new build is Vocet. The tray's update installs it (`Vocet.app` stays, Vocet inside), then Vocet.app replaces it as the DMG guide says. Each time: the settings file and engine settings, the key in the keychain, the login item enabled and one entry in Background Task Management at the right path, and Microphone and Accessibility holding. The updater still offers a newer version. Evidence file lists the BTM dump; screenshots of the keychain prompt macOS shows after an update and of the first-run window. `E2E_TRANSITION_KEEP_OLD=1` reuses the old build. |
| `e2e_onboarding_first_run` | U8 #8 (M5.2 #3) | A first run with no saved key on the local provider, launched with `--debug-onboarding`, down the own-key path: onboarding shows at launch at Welcome. In Speech, Test & Save refuses a key the provider rejects (`ax.js secret-set`), saving nothing with Continue still locked, and accepts one it takes. Permissions shows both allowed; the practice dictation lands in the box; Ready shows the key, Groq connected and launch at login off; Start dictating hides the window and takes Resume setup off the tray. Run setup again in Settings, Help reopens it at Welcome, with Resume setup still off the tray. Neither key nor the transcript is in the log. |
| `e2e_onboarding_layout` | U8 #1 | The real onboarding at its minimum content size with every text made pseudo-long (`src/onboarding/layoutHarness.tsx`, on a fake backend, in the VM's Safari), for each step in light and dark. Also Ready at the usual size with its real text and the download strip, where the sharing switch's row must be in view without scrolling (U10). Reads the page's verdict: the primary action and its reason unclipped and uncovered, no overlapping footer parts, nothing sticking out sideways, and the content scrolls to its end. |
| `e2e_onboarding_five_steps` | U8 #7, #8 | All five steps with the real local model and a `say` clip. Download and continue starts the download and moves on; the strip shows it on later steps; a quit mid-download comes back at Permissions with the download picked up. Try it waits for the model; a dictation that lands in TextEdit shows the words with Copy and doesn't count; one that lands in the practice box does. Ready says the engine is ready, and Start dictating ends setup. Downloads ~477 MB. |

| `e2e_settings_key_provider_recovery` | T4a #3 (#114) | On the local provider, with a wrong key in the keychain: a dictation is rejected (`auth=bad`, a 401), the error is shown, nothing is pasted and the clipboard is untouched. In Settings → Speech (no Save button), the wrong key typed and **Test & Save** is rejected by the provider and not saved; the right key typed into the password field (`ax.js secret-set`) and **Test & Save** reaches the provider with the typed key, says `✓ Connected · saved in Keychain`, and the engine is rebuilt. With the same pid, the next dictation is sent with the new key (default model, no language) and pastes exactly `hello world`, and the tray's error is gone; after Quit and a relaunch it still does. Neither key is in any file of the app's (settings, logs, caches, web storage, preferences) or the provider's. |
| `e2e_cancel_and_busy_presses` | T4a #3 (#116) | Real keys: Right Option down, Esc (with Right Option held) once the log shows Recording, Right Option up: `Recording Idle`, no request, nothing pasted, the clipboard untouched; a fresh hold pastes. Then a hold whose reply is held: another hold and an Esc while it is pending start nothing (no Recording, no second request); the release pastes the original transcript once and nothing follows; the next hold pastes. |
| `e2e_focus_switch_during_transcription` | T4a #3 (#117) | Two TextEdit documents, A with text and B empty. A dictation starts with A in front; while its reply is held B comes to the front; B gets exactly one transcript and A is byte for byte unchanged, TextEdit stays frontmost. Again with Settings opened and closed from the tray while the reply is held, B put back in front: B gets the second transcript after the first, A unchanged, Settings closed. The policy is in `docs/ENGINEERING.md` ("Paste target"). |
| `e2e_unicode_multiline_selection_paste` | T4a #3 (#118) | The provider answers texts with non-BMP emoji (as JSON surrogate pairs), a flag, a ZWJ sequence, combining marks, Hebrew and Arabic, a tab and inner newlines, wrapped in blanks and newlines. The seeded document is read back first and must be exactly the intended text, else the scenario ends with "setup: document not as expected" (#185). Pasted at a caret inside existing text, over a selected range (placed with real arrow keys), and at a caret again in a fresh document: each time the whole document, read back as base64 of its UTF-8, is exactly the text around it plus the transcript trimmed of its surrounding whitespace only; the clipboard is the user's after each. |
| `e2e_quit_during_transcription_then_relaunch` | T4a #3 (#115), M5.8 #4 | While the provider holds the reply to a dictation's request, Quit in the tray: the app exits by itself within 10 s, the provider sees the connection close, and the reply it then sends pastes nothing (no Inserting, TextEdit empty, the clipboard untouched) and is never retried (one request). A relaunch dictates and pastes. |
## Key cleanup

A scenario that posts a real key-down (`e2e_hold_release`, `e2e_held_at_launch`,
`e2e_settings_save_reopen`)
must not leave a key held in the VM, where it would spoil later hotkey
tests and `--no-reset` runs. Three layers:

- `arm_key_cleanup <scenario> [key...]` (first thing in the scenario) sets an
  EXIT trap, with INT, TERM and HUP routed to `exit`. However the scenario
  dies (a failed command under `set -e`, `die`, Ctrl-C), the trap saves the
  app's log and a screenshot, quits any Terminal job still running, quits the
  app, runs `KEY_CLEANUP_HOOK` if the scenario set one (the settings reset),
  posts Right Option up (and each key named, such as `left-option`; an up
  with no down does nothing) and checks with `state` that the system sees it
  up, closes TextEdit's documents and Terminal, and exits with the
  scenario's own status. A normal end does the same through
  `key_scenario_end`. A cleanup step that fails (the app won't quit, a key
  still held, the hook) is reported and fails the scenario, but never
  replaces the status a scenario died with.
- `real_key_hold`'s (and `post_key_hold`'s) Terminal job has its own
  EXIT/INT/TERM/HUP trap that posts the up if the down was posted, so a job
  cut short releases the key with no help from the host.
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
`mid_hold` dies inside the hold job between its down and up;
`after_mic_denied` (`e2e_microphone_denied_then_recover`) dies right after
the microphone was refused, and the cleanup must grant it again.

## Trusting the helpers

A helper that reports success when the guest failed would turn a broken app
green (P2-05 in the test audit, #103). So:

- **`vm` has a deadline** for the whole command, not just the connection:
  `VM_CMD_TIMEOUT` seconds (default 120; the app's copy into the VM gets
  600). A command that hangs (a dialog nobody answers, a stuck `osascript`)
  is cancelled and `vm` returns 124 with a `vm: no answer within …` line.
  Otherwise it returns the guest command's own status.
- **`vm_terminal` returns the guest's exit status.** The job writes
  `<token> <status>` to its `.done` file (through a temporary file and a
  rename), and `vm_terminal` returns that status after printing the job's
  output. Each job's name is unique (the run's id, a counter shared by
  subshells, and the calling shell's pid) and so is its token; any old
  `.done` file under that name is removed before the job starts, and a
  `.done` file without this job's token is refused (status 125). A job past
  `VM_TERMINAL_TIMEOUT` is killed and `vm_terminal` returns 124.
- **Checks look at the status and the output.** `job_says <output> <cmd…>`
  is true only if the Terminal job exited 0 and printed exactly that, so a
  key script that printed `posted down` and then failed is a failure.
- **Cleanup doesn't hide a failed Quit.** `fresh_app_state` (TERM, then
  KILL) is setup and fails if a copy is still running after the KILL. A
  scenario whose journey is quitting uses `quit_from_tray`: Quit in the tray
  menu, then the app must be gone within `QUIT_TIMEOUT` seconds (default 15)
  by itself. If it isn't, it is killed so the next step starts clean, but
  the check has already failed.

`tests/e2e/test-runner.sh` (part of the gate, no VM) proves these with a
stubbed `vm` that runs the runner's own bookkeeping in a temporary
directory, a fake Terminal that runs the job with bash, and fake
`osascript`, `open`, `sleep` and `ssh`: a guest exit status of 23 comes back
as 23, a deadline fails, a stale or planted `.done` file is refused or
removed, a key script that prints the right text but exits 1 fails
`e2e_hold_release`, a key the cleanup can't release fails a scenario that
had passed, a failed launch keeps its status (23) through the cleanup (also
when the cleanup fails too), a failed setup fails before anything is
launched, an `exit 0` half way fails, a required skip fails the run and an
optional one is listed, and a Quit that doesn't end the app (or can't be
clicked) fails.

## The app's UI

The app is a menu-bar app: its tray (the status item) and the Settings
window are reached through the Accessibility API by `fixtures/ax.js`, run
in Terminal (`ax <command> …` in `lib_full.sh`), since Accessibility is
Terminal's grant; the ssh session has none, and System Events' UI scripting
would need it too. `ax.js` calls the AX functions directly (through JXA's
`ObjC.bindFunction`), so it needs no Automation grant either. It finds the
status item through the app's `AXExtrasMenuBar`, opens its menu (a press
with a short messaging timeout, since the press returns only when the menu
closes) and presses an item by the start of its title (`Settings`, `Quit`);
it reads a WebView's fields by their DOM id (`AXDOMIdentifier`: `hotkey`,
`language`), presses buttons by title (`Save`) and waits for text
(`Settings saved.`). It reads and sets checkboxes (`check-get`, `check-set`:
Settings' `launch-at-login`, or System Settings' switch for the app,
`Vocet_Toggle`), and reads what the overlay renders (`overlay`,
`overlay-wait`, `overlay-watch`): the window titled `overlay` is in the
app's `AXWindows` only while on screen, and its WebView's `AXStaticText`
is what WebKit laid out. Whether a window is on screen comes from the window
server's list (`CGWindowListCopyWindowInfo`), not AX. Typing focuses the
field, makes the app frontmost (AX), empties the field and posts the keys at
the HID tap; it types lowercase letters and digits (the US layout's key codes). When a UI scenario fails, `dump_ui` saves the tree to
`<scenario>.ax.txt`; `ax dump Vocet` shows it by hand (run it in
Terminal in the VM).

## The local provider (T4a)

The scenarios that need the real engine without Groq (a wrong key, a reply
that is late, a transcript with every kind of character) use
`fixtures/local-provider.pl`, a small OpenAI-compatible transcription server
on the VM's loopback, in plain Perl (the VM has no Python). The debug build
is pointed at it with `--debug-provider-url http://127.0.0.1:18731/v1`: the
real engine, the key from the keychain, the real HTTP client, retries and
JSON parsing, and Settings' Test key, all go there instead of the provider.
The flag exists only in debug (test) builds and takes only a loopback `http`
URL; a release build treats it as an unknown argument
(`flags::tests::release_builds_ignore_the_provider_url`).

`scenarios/_provider_helpers.sh` starts it fresh for each scenario
(`provider_start <key>`) and drives it through files in `~/e2e-provider` in
the VM:

- **Keys:** it accepts one made-up key (lowercase letters and digits, so
  `ax.js` can type it into Settings) and answers any other with a 401, as
  Groq does. Only the key's SHA-256 is on disk, so a scenario can check that
  no file holds a key. The key reaches the app's keychain through the app
  itself (`groq_key_store`, as for Groq).
- **The gate:** `provider_gate_close` makes it hold each reply once the
  request is read whole; `provider_release` lets them go. A held reply keeps
  the text it came with. Holds stay well under the app's 30 s request
  timeout, past which the app would retry.
- **What it saw:** one line per event in `events`: `request n=… auth=good|bad
  model=… language=…`, `held`, `closed` (the client hung up), `released`,
  `replied … status=…`; never a key or a transcript. `provider_requests_are`,
  `provider_wait_event` and `provider_request_has` read it.
- **Replies:** `provider_reply <base64>` sets the transcript, byte for byte.
  Texts in the scenarios are written as ASCII with `\n`, `\t` and `\u{hex}`
  escapes (`esc_b64`), and TextEdit is read back as base64 of its UTF-8
  (`textedit_b64`, `textedit_is`), so no whitespace is lost on the way.

Each provider scenario arms the key cleanup with `provider_cleanup` as its
hook: however it ends, the reply is released, the provider stopped, the key
removed from the keychain and the settings reset.

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
  GUI) and, after the first reset, `dictation-vm` and `dictation-vm-2`.
- One run per VM at a time (the lock above). A reset deletes the VM, so check
  that no other agent is using it without a run (the `cua-vm` MCP server on
  `dictation-vm`, a hand-started session) first.

## macOS notes (Gatekeeper, TCC)

- **Gatekeeper:** files copied over ssh carry no `com.apple.quarantine`
  attribute, so the self-signed debug build opens without the "unidentified
  developer" dialog. If you copy it any other way (AirDrop, a browser
  download), clear the attribute first: `xattr -dr com.apple.quarantine
  ~/Applications/Vocet.app`.
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
  (see `docs/history/test-reports/M1.4-run2-vm.md`), so it was granted through the VM's
  own display, not the host screen.
  A scenario that must change it (`e2e_accessibility_revoked`) types the
  password into that sheet through the VM's VNC server with
  `tests/e2e/vnc_keys.py` (host side, `python3` with `cryptography`): VNC
  keys arrive through the VM's virtual keyboard, which the sheet accepts.
  System Settings must be in front first, or the keys go to Terminal.
- **Microphone:** granted to the app in the golden image (M4.3, issue #92),
  so a launch without `--debug-audio-file` records the real microphone with
  no prompt. `e2e_microphone_denied_then_recover` resets it in the clone
  (never the golden image) and grants it again before it ends. The other scenarios pass `--debug-audio-file` anyway: the same
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

- The M4.3 debug build at `~/Applications/Vocet.app`, signed
  `dictation-app Dev` (runs replace it).
- **Microphone** granted to `Vocet`: launched, a `--press` raised the
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
`set_clipboard`, `clipboard`, `dictate_once`, `real_key_hold`,
`post_key_hold`, `job_says`, `ax`, `ax_ok`, `tray_info`, `quit_from_tray`,
`dump_ui`) and, on the local provider, `scenarios/_provider_helpers.sh`. A scenario that
posts a real key-down calls `arm_key_cleanup` first and `key_scenario_end` last.
Wait on a predicate with a deadline (`wait_for_log`, `wait_for_idle_count`,
`app_count_is`, `wait_for_no_app`) rather than a fixed sleep; keep a fixed
wait only for what is being tested (a hold's length) or to show that
something does *not* happen. Leave the VM as you found it: the app quit,
TextEdit's documents closed, Terminal quit, no key held, default settings.
Add it to `ALL_SCENARIOS` in `run.sh`, and to `OPTIONAL_SCENARIOS` if it may
skip without failing the run.
