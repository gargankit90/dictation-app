Reviewer: APPROVED

# T2 review, round 2

PR: #137 at `44b545158319f5ce7dc1af312259dd3d41698a12`.
Scope: fixes since `e211cc2`, principally `b04cc30` and `44b5451`.
Counts: 0 blockers, 0 majors, 0 new minors. R1 is resolved. No merge performed.

## Findings and R1 resolution

No open findings in the reviewed fixes.

The original round-1 reproduction was rerun against this worktree's production
app crate (`/tmp/t2-hotkey-probe`). Its old-source stop drops the old sink and
emits the replacement's press before returning; the replacement is released
after `switch(F6)` returns. It now receives:

```text
First F6 hold, press during old listener stop: [Pressed, Released]
Next F6 hold: [Pressed, Released]
```

The original assertion expecting the broken `[Released]` result failed with
that corrected output. After updating only the temporary probe's expectation,
the probe passes. Log: `/tmp/t2-round2-probe.log`. No repository product or test
files were changed for the reproduction.

At `crates/app/src/hotkey_setting.rs:297`, the handoff precedes the old source's
stop/join. Every generation check, held-state change and sink delivery uses the
same mutex (`:104`, `:145`, `:211`). An old callback either finishes before the
handoff or sees the new active generation and is dropped; it cannot pass a
separate check and arrive later. A replacement press before handoff is only
remembered, then sent once at handoff; a press after handoff is delivered
directly. The mutex serializes those alternatives. An old hold is interrupted
before a remembered new press. Failed replacement startup abandons only the
pending generation (`:291`), leaving the old listener active.

## Deadlocks and delivery latency

- The control path takes state then delivery; callbacks take delivery only.
  Replacement construction/start and old stop/join run outside delivery, as
  does stopping the current source. Thus a listener can finish an in-flight
  callback while its owner waits for its thread. Synchronous callbacks during
  replacement start can also acquire delivery.
- The actual downstream sink (`crates/app/src/pipeline.rs:140`) timestamps and
  uses `try_send`; it never waits for the pipeline to consume the event and
  never calls the hotkey control. The synthetic wrapper takes its own sink
  mutex downstream, but shutdown stops the real source before taking that
  mutex (`crates/app/src/test_hooks.rs:302`). There is no reverse lock path or
  callback-to-stop wait cycle in these production paths.
- Normal delivery adds no start, stop, I/O or wait for the control/state mutex.
  The release-mode temporary probe sent 100,000 press/release pairs and checked
  the event count: synchronous press delivery measured 42 ns median, 84 ns
  p99, and 247,750 ns maximum on this run. This measures the wrapper with an
  in-memory sink, not native OS or microphone latency. Source inspection and
  this measurement give no indication of noticeable added normal-press latency.

## Post-Quit paste and the scenario changes

The [Developer's controlled probes](https://github.com/gargankit90/dictation-app/pull/137#issuecomment-6564703756)
support treating the observed miss as a VM/synthetic-activation artifact:
misses also occurred when TextEdit was activated with no app running; a fresh
VM landed 3/3; a second activation landed 12/12 with the relaunched app still
running. These observations support a failed target-window precondition over
an exited app retaining focus or a pending paste. They do not independently
establish physical-click behavior on the owner's Mac.

`tests/e2e/lib_full.sh:50` now activates TextEdit again after it is frontmost,
before dictation. It does not retry dictation or paste, or relax the transcript
assertions. The Quit/relaunch scenario still checks exit without a kill, no
paste from the abandoned recording, then exactly one full dictation and the
actual transcript in TextEdit after relaunch. The Settings scenario still uses
the real saved hotkey after Quit/relaunch and checks the document text.
`e2e_error_bad_key` now establishes the target after its cold launch and still
requires the mock transcript in TextEdit and the full state sequence. A user
who activates a document after relaunch and dictates remains covered; a missed
paste in that sequence still fails.

These helpers establish the target before the press, so they do not prove
that a cold launch preserves a previously key window. Existing #138 tracks the absence of gate
coverage for launch focus, and the physical Mac checks remain with the
Lead/owner. No new app focus regression was established
by the reviewed fixes. The Developer reports the fresh-VM full run at this
head as 18 passed, 0 failed, 1 optional real-API skip; this reviewer did not use
the VM or the owner's desktop.

## Verification

- `cargo nextest run -p dictation-pipeline --test hotkey_settings`: 17/17 pass,
  including all four fenced handoff tests, failed-start rollback, mid-hold
  interruption and failed-file-write restoration. The new tests observe
  recording/transcription/insertion and check exactly one inserted transcript;
  the stop fence also proves the replacement can deliver before stop returns.
- Original R1 probe and normal-delivery measurement: pass as described above.
- Full local `scripts/gate.sh`: PASS (`gate: all checks passed`), including
  844 workspace tests (2 skipped), the explicit platform run, doc tests,
  coverage, frontend checks/build and all guard/key/runner checks. Log:
  `/tmp/t2-round2-gate.log`.
- CI remains unavailable because of the reported GitHub billing restriction.

## Checklist (changed code only)

| Section | Result |
| --- | --- |
| Scope and criteria | OK: R1 handoff fix and VM target setup; no expansion of product scope |
| Correctness, races, cancellation | OK: R1 resolved; serialized generation/held-state delivery; rollback retained |
| Security and privacy | OK: no new secret access, transcript logging, audio persistence, network destination or capability |
| Architecture | OK: app policy remains in app; no OS cfg, unsafe or dependency changes |
| Rust and TypeScript | OK: short delivery critical section; start/stop outside it; no new suppressions or TS changes |
| Licences and reuse | OK: no dependencies or external adaptations added |
| Tests | OK: four fenced regressions plus existing rollback tests; paste outcome assertions retained |
| Git and PR | OK: report-only reviewer change; no guards/spec changes; existing #138 remains tracked; no merge |
