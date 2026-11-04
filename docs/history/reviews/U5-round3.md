Reviewer: APPROVED

# U5 review, round 3

PR: #248 at 4c216c2f019fa0c8d945f2fa5874b06b538b6e0e

## Findings

No open blocker, major or minor findings in this re-review.

R2-1 is fixed: a generic `HotkeyFailed` no longer sets the Accessibility-history latch. `HotkeyRestored` clears both the down flag and that latch, so a recovered non-permission outage returns to Ready with the normal icon immediately. A real Accessibility re-grant offers Relaunch only while the listener remains down; restoration clears that state. The three regression tests cover generic outage recovery, re-grant with the tap still down, and re-grant with the tap restarted.

All four round-1 findings remain fixed as recorded in round 2: marked clipboard writes on the real pasteboard path, serialized native application without hub locks across main-thread calls, template status on every icon swap, and invalidation of old engine/key/model errors on reconfiguration.

## Checklist re-check

- Scope and correctness: OK for the round-3 delta. State transitions and menu/fix/icon selection match the intended recovery behavior; no new problem found.
- Security and privacy: unchanged from the approved parts of round 2; no new transcript, clipboard, logging, report or analytics path.
- Architecture and Rust/TypeScript: OK; only pure tray state and regression tests changed in product code. No applier lifecycle or dependency change.
- Licences and reuse: unchanged.
- Tests: the Accessibility e2e again requires Ready and an actual successful hold in the running process before quit/relaunch, restoring the recovery check rather than accepting only a relaunch workaround. The coordinator reports 3/3 successful VM runs at this head; I did not rerun the VM.
- Git and PR: restored the previously unpushed round-2 report by rebasing it onto the PR branch; this round adds only this report. No merge performed. Manual crowded-menu-bar checks remain the Lead's responsibility.

## Validation

Reviewed the complete delta from ee0a67f through 4c216c2, including the e2e assertions and three new state-transition cases. The U1 wording adapter and earlier clipboard/applier/template/engine fixes are unchanged.

- `DICTATION_NO_LAUNCH=1 cargo nextest run -p dictation-pipeline tray::`: all 26 tests passed.
- Re-ran the round-2 scratch reproduction against the compiled production crates with the corrected expectation: Ready → HotkeyFailed → HotkeyRestored now prints `before: Vocet: Ready` / `after: Vocet: Ready`, and asserts no relaunch is needed. No repository code or tests changed.
