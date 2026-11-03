Reviewer: CHANGES REQUESTED

# T2 review, round 1

PR: #137, reviewed at `77a6e0b1faaa310bf18a325411a5b1593e919a6d`.
Counts: 0 blockers, 1 major, 0 minors. No merge performed.

## Findings

### R1 — major: a press during the hotkey handoff is lost

`crates/app/src/hotkey_setting.rs:219` (also lines 130–131 and 221–227).

The replacement source starts with an inactive generation. The old source is
then stopped **before** the new generation becomes active. Once the old
listener has stopped receiving events, but while its `stop()` is still joining
its threads, neither source can deliver a new press: the old listener is
stopping, and the new forwarder silently drops it. A user who begins holding
the newly selected key during this interval gets no recording, even if the
hold continues beyond the successful save. Its release is forwarded without
the press; only the next complete hold works. The macOS adapter makes this a
real interval: `MacHotkey::stop` sets `done` and joins a listener that polls at
50 ms, then drops/joins the underlying tap.

Reproduced without OS input or a VM, against the production app crate, in
`/tmp/t2-hotkey-probe` (`cargo run --manifest-path
/tmp/t2-hotkey-probe/Cargo.toml`; output in `/tmp/t2-hotkey-probe.log`). Its
injected source installs a normal sink. The old source's `stop()` drops its
sink, then emits `Pressed` through the already-started F6 source before
returning. After `switch(F6)` returns `Now`, the harness emits F6 `Released`.
The observed pipeline-sink events are:

```text
First F6 hold, press during old listener stop: [Released]
Next F6 hold: [Pressed, Released]
```

Suggested fix: linearize the generation change and held-state transition
with event delivery before waiting for the old source to stop. Keep the old
generation active if replacement startup fails, ensure an old hold is
interrupted before accepting the replacement's first press, and prevent an
old callback that already passed the generation check from arriving after
the handoff. Add a deterministic barrier-based test that presses the new
source while the old source's `stop()` is blocked; also retain the existing
failed-start rollback and mid-hold tests. Existing tests exercise events only
before or after the switch and do not cover this interval.

## Product checks

- Startup reads the saved spec; case-insensitive existing settings still
  parse. Canonicalization changes serialization, not the settings schema or
  accepted spellings. No settings migration is required.
- A synchronous replacement `start()` failure leaves the old listener and
  saved hotkey intact, saves the other settings, and returns an error naming
  the requested and working keys. The UI does not show “Settings saved.” for
  that result and retains the typed value for retry. Failed-write rollback
  is tested; restoration failure remains best effort and is logged.
- `launch_without_focus` changes Tao's launch activation flag only. Checked
  the pinned Tauri/Tao/Wry implementation: hidden webviews do not activate
  their app; the launch flag reaches `activateIgnoringOtherApps(false)`.
  Settings still explicitly shows and focuses its window, the tray/menu and
  non-activating overlay paths remain intact, and microphone permission
  requests still call AVFoundation independently of this flag. These are
  source checks, not fresh native first-run/permission-prompt evidence.
- The reported paste miss after tray Quit is **unresolved**, not established
  as a product regression by this review. No VM or host desktop was used.
  Shutdown discards a recording and stops the hotkey; a paste already in
  progress has the existing documented non-cancellable limitation. Nothing
  in this diff establishes that an exited app holds focus or leaves a paste
  pending in the reported recording/Quit sequence. Conversely,
  `textedit_fresh` checks only the frontmost process, not the document's key
  window, and Terminal jobs themselves activate Terminal. Thus the supplied
  “frontmost but not key” observation does not distinguish a test focus
  precondition failure from a native app/OS focus bug. The Tester/Lead should
  capture the key/focused window immediately before paste and compare a
  normal physical tray-Quit/relaunch before attributing the cause. The PR's
  reported full VM run remains 17 pass / 1 required fail / 1 optional skip;
  this review does not convert that into a passing run.

## Tests and acceptance checks

- Local full quality gate: **PASS**, 5m52s (13:01:28–13:07:20 PST), log
  `/tmp/t2-review-gate.log`. Workspace nextest: 840 pass / 2 skip; explicit
  platform run: 260 pass. Coverage: core 98.93%, transcribe 98.61%, app
  97.62%. Vitest: 93 pass; production frontend build passes; guard checks
  63 pass, key checks 39 pass, runner checks 62 pass. CI is unavailable
  because of the reported billing restriction. This local timing fits the
  approximate ten-minute budget; it does not measure hosted-CI cache/setup
  time.
- Temporary source archive: `/tmp/t2-review-copy`; product/test source in
  the review worktree was not edited. All 13 hotkey-setting integration
  tests pass in that archive. Four independent hotkey mutants were killed
  by the corresponding new tests: default instead of saved startup spec,
  leaving the old source listening, stopping the old source after a failed
  replacement, and omitting canonical serialization. Logs:
  `/tmp/t2-mutant-hotkey-*.log`.
- The five T2 Settings UI tests pass. Independently restored four bugs in
  the temporary archive: missing `testing` submit guard, marking the login
  choice unedited after a failed re-read, accepting a late provider-test
  reply, and omitting saved-key status after partial save. Each matching
  test fails with a behavioral assertion; logs `/tmp/t2-mutant-*.log` and
  script `/tmp/t2-ui-mutants.py`.
- ACL test uses the real Tauri resolver and includes its own tray/menu,
  resource/window, remote-origin and missing-command mutants for both
  windows. It runs in the full gate.
- Reviewed the process tests: independent child launches and receipt-before-
  acknowledgment checks, cold-start barrier, socket inode ownership after
  loser exit/SIGKILL, idle/oversized/malformed peers, and missing/invalid
  delivery acknowledgments. Local gate executes these tests; the claimed
  50-run CPU-load stress is developer evidence, not a separate reviewer run.
- Read all six new VM scenarios. Their oracles include saved form/file
  values and actual native hotkey outcomes, tray title/error clearing,
  process/tray counts and delivered launches, a positive paste control
  before Secure Input suppression, transcript and user-copy/change-count
  outcomes, and tray Quit/process exit/no paste/relaunch. They are real
  outcome checks, not screenshot-only or log-only substitutes. Fresh VM
  execution and criterion 8 remain with the Tester/Lead.
- Reviewed `vm`/`vm_terminal` status propagation, unique job names/tokens,
  atomic done-file publication, stale-result rejection, timeout cancellation,
  isolated scenario failure accounting and cleanup status preservation.
  A separate local zsh-job deadline probe returned 124 and produced no late
  marker after cancellation (`/tmp/t2-deadline-probe.sh` and `.log`). The
  polling deadline is a polling budget plus bounded SSH calls, not a strict
  wall-clock upper bound when SSH itself stalls; no silent-pass defect was
  reproduced. Existing 39 key checks and new 62 runner checks are in the gate.
- `npm run build` is now in the canonical gate called by the every-PR Linux
  workflow. No VM dependency or broad mutation suite was added to the gate.

## Checklist

| Section | Result |
| --- | --- |
| Scope and criteria | Mapped above; R1; native/manual evidence remains with Tester/Lead |
| Correctness, races, cancellation | R1; paste/Quit attribution remains unresolved as described |
| Security and privacy | OK: no new secret getter, logging exposure, network destination, analytics or capability grant |
| Architecture | OK: policy in app, OS cfg in platform, no new unsafe or dependency direction |
| Rust and TypeScript | OK apart from R1's synchronization; blocking registration stays off the UI thread |
| Licences and reuse | OK: no dependency additions or outside-source adaptation |
| Tests | Existing/new tests and eight reviewer mutants checked; R1 needs a transition test |
| Git and PR | No guard changes; report-only reviewer commit; no merge; no new minors requiring issues |
