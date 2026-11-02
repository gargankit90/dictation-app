Reviewer: APPROVED

PR: #160, round 2, reviewed merge `0adb75d9ba73f2cc8cd0cff1a34e39c0e0bdc0f8` (parents `7ec7f09`, `86ee0f8`).

## Findings

No blocker, major or new minor findings. This review covers only integration of M5.2 and M5.6 into the previously approved T4b; unchanged code retains the round-one assessment. No merge performed.

## Integration behaviour

- **Late overlay listener and #155:** `HotkeyRestored` passes through `show_ui_events` to the overlay queue. `OverlayModel` hides only a currently displayed hotkey error; `run` cancels its expiry and sends Hidden through `Surface::show` to `Relay::send`. The relay replaces the remembered error before emission. A page subscribing after recovery therefore receives Hidden, including through the new cross-feature test `a_page_listening_after_the_hotkey_is_restored_gets_hidden`. If subscription happens before recovery, the relay's shared lock orders replay and the subsequent Hidden update. Recovery does not hide a replacement recording or unrelated error.
- **Queue saturation and #154:** both main UI and overlay use the same bounded queue policy. A newest hotkey outage survives a full error queue, supersedes older outage/recovery transitions, and a subsequent recovery remains queued. Draining either queue therefore ends on the latest hotkey state; the relay remembers the resulting view rather than a historical error. The handshake reads that view without changing visibility or restarting an error timer.
- **Merge preservation:** inspected the remerge diff and both parent comparisons. The only text conflict was the scenario list; resolution preserves both microphone-denial recovery and paste-modifier cleanup. The only extra resolutions are the onboarding README row and the cross-feature overlay test. M5.2 onboarding, M5.6's model/queue and paste changes, and T4b's handshake and journeys remain present. No integration loss found.
- **Registration:** extracted `ALL_SCENARIOS` from both parents and the merge: 31, 31 and 33 runnable scenarios respectively. The merged set equals the exact union of both parents and all `e2e_*.sh` scenario files, without duplicates, missing files or missing README rows. Helper scripts are not runnable scenarios.

## Checklist

| Section | Assessment within this re-review |
| --- | --- |
| Scope | OK: merge integration and its two small follow-ups only. |
| Correctness | OK: recovery, late subscription, error replacement, timer cancellation and bounded queue ordering traced above. |
| Security and privacy | OK: integration adds no permissions, network destination or sensitive logging; handshake remains overlay-local. |
| Architecture | OK: model and bounded queues remain in app; relay remains shell wiring. |
| Rust and TypeScript | OK: no new production change in the manual merge resolution; added test pins observable recovery behaviour. |
| Licences and reuse | OK: no new dependency or copied code in the resolution. |
| Tests | OK: focused verification below and Developer's reported native regression runs. |
| Git and PR | OK: merge commit preserves both histories; review-only commit, no product/test/spec/guard edits and no merge. |

## Validation and limits

- `cargo nextest run -p dictation-pipeline -p dictation-app -E 'test(overlay::) | test(ui_events::)'`: 36 passed, including the new recovery/late-listener test and #154 saturation tests.
- `npx vitest run src/overlay`: 26 passed.
- `bash tests/e2e/test-runner.sh`: 67 passed.
- Scenario-union, scenario-file and README-row comparison: passed; 33 runnable scenarios.
- `bash -n tests/e2e/run.sh` and whitespace check of the manual resolution files: passed. The broader first-parent whitespace check flags generated `src/bindings.ts` formatting inherited byte-for-byte from the default-branch parent; it is not a merge-resolution regression.
- Developer's [merge verification comment](https://github.com/gargankit90/dictation-app/pull/160#issuecomment-6573706879) reports native PASS on `dictation-vm` for rendered overlay states/error, paste modifier cleanup and hotkey listener recovery, plus its successful full push gate. I did not repeat native VM journeys or operate the owner's screen.
