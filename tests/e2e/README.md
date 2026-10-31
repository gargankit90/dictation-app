# End-to-end tests

The built app, run inside the Lume VM `dictation-vm` and driven from the host
over ssh. These aren't part of CI (CI has no VM). Run them locally:

```sh
tests/e2e/run.sh                                   # build, reset the VM, run all scenarios
tests/e2e/run.sh e2e_startup                       # one scenario
tests/e2e/run.sh --no-build --no-reset e2e_press_release   # reuse the build and the running VM
```

It exits 0 only if every check passed. Each run writes to
`tests/e2e/out/<timestamp>/` (git-ignored): `build.log` and one
`<scenario>.app.log` per scenario, which is the app's own log copied back
from the VM.

## What a run does

1. **Build:** `npm run tauri build -- --debug --bundles app`. The bundle is
   signed with the `dictation-app Dev` certificate (`scripts/make-dev-cert.sh`,
   once per Mac).
2. **Reset the VM** (skipped with `--no-reset`):
   `lume stop dictation-vm`, `lume delete dictation-vm --force`,
   `lume clone dictation-vm-golden dictation-vm` and
   `lume run dictation-vm --no-display --detach`. The run then waits for an
   IP (`lume get dictation-vm -f json`, `.ipAddress`) and for ssh to answer.
   The golden image is only cloned, never started or changed.
3. **Install:** the `.app` is copied with `tar` over ssh into
   `~/Applications` in the VM, and its signature is checked there.
   `tests/e2e/fixtures/` (a 1.5 s 440 Hz tone, 16 kHz mono 16-bit; no voice)
   goes to `~/e2e-fixtures` in the VM.
4. **Scenarios:** each one quits any running copy of the app, clears its log
   (`~/Library/Logs/com.dictationapp.desktop/` in the VM), launches the app
   with `open -n …/dictation-app.app --args <flags>`, and checks the log and
   the process list.

| Scenario | Criterion | Checks |
| -------- | --------- | ------ |
| `e2e_startup` | M1.5 #1 | The app starts (first launch with `--debug-audio-file`, so the press needs no mic permission) (`ready` in the log) and one copy runs. A second launch with `--press` is forwarded (`test hook event=Pressed from="a second launch"`) and the pipeline starts recording. Still only one process; the app was set up once. |
| `e2e_press_release` | M1.5 #2, M2.2 #1 | A first launch with `--debug-audio-file ~/e2e-fixtures/tone-16k.wav --press`, a second with `--release` a second later. The app records the fixture, not the mic; the log shows `Recording Transcribing Inserting Idle` in that order, and the stand-in transcript isn't in it. |
| `e2e_real_mic` | M2.2 | Skipped unless `E2E_REAL_MIC=1`. `--press`, then `--release`, recording the VM's real microphone: no permission error or microphone failure, and the full state sequence. Needs the mic granted in the golden image and an input device in the VM. |

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
- **Accessibility:** these scenarios need no permissions. Without
  Accessibility, the real Right Option hotkey can't start, and the app logs
  (at warn, right after `starting`):

  ```
  WARN dictation_pipeline::test_hooks: PermissionMissing: the hotkey is off; only the --press/--release test hooks work error=Accessibility permission is missing.
  ```

  The synthetic flags still work, so the scenarios pass with or without the
  grant. Granting it to the app in the VM shows a
  password sheet that refuses synthetic keys (see
  `test-reports/M1.4-run2-vm.md`), so tests that need the real hotkey grant
  it through the VM's own VNC display, not the host screen.
- **Microphone:** since M2.2 the app records the real microphone. The
  scenarios pass `--debug-audio-file` on the first launch, so they need no
  microphone permission and no prompt appears. Without that flag the first
  press shows the macOS microphone prompt and, until it's allowed, logs
  `Microphone permission is missing.` (`e2e_real_mic`).
- **Paste:** still a stand-in (`crates/app/src/stand_ins.rs`), so no
  Accessibility prompt.

## Adding a scenario

Put a function in `tests/e2e/scenarios/<name>.sh`, named after the file,
using the helpers in `lib.sh` (`launch`, `wait_for_log`, `logged_phases`,
`app_count`, `expect`, `collect_log`). Add it to `ALL_SCENARIOS` in `run.sh`.
