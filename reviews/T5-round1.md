Reviewer: CHANGES REQUESTED

# T5 review, round 1

PR: #163, `t5-test-reliability` into `main`.
Reviewed implementation: `84f205b71a7b445283b138b8dccada9228ee9748` (base `86ee0f8c5e761b42233c3771bdbf5834c94c36fc`).
Review file: [reviews/T5-round1.md](https://github.com/gargankit90/dictation-app/blob/t5-test-reliability/reviews/T5-round1.md).

## Findings by severity

- Blocker: 0.
- Major: 1 (R1).
- Minor: 0. No new tech-debt issues required.

| ID | Severity | Location | Finding | Suggested fix |
| --- | --- | --- | --- | --- |
| R1 | Major | `tests/e2e/test-runner.sh:288` (job at line 279, assertion at line 294) | The new cancellation assertion can pass without cancellation: it waits about 30 seconds for a job that naturally ends after 30 seconds, starting after the 2-second deadline. Removing the deadline's `pkill` from `vm_terminal` leaves all 73 runner checks green. | Keep the exit wait, but make the job unable to finish naturally within its failure bound (for example, give the job a lifetime well beyond that bound), or explicitly assert receipt of TERM through the stub. Add the missing-cancellation mutant to the validation evidence. |

### R1 reproduction and impact

In an isolated copy of the submitted implementation, replace only the deadline cancellation line in `tests/e2e/lib_full.sh:204`, `vm "pkill -f $(q "$VM_RUN_DIR/[j]ob-$id.command")" || true`, with `:`. Leave the stale-token cancellation path and every test assertion unchanged. Run `bash tests/e2e/test-runner.sh`: it exits 0 with `runner tests: 73 passed, 0 failed`.

`vm_terminal` still returns 124 at its deadline and prints the expected cancellation diagnostic, so those checks pass even though the job keeps running. Then `job_gone` waits long enough for `/bin/sleep 30` to end naturally and reports success. A regression that leaves timed-out jobs running, potentially posting input after the scenario has moved on, is therefore hidden by a green test run. The old immediate exit assertion would detect this surviving process; waiting for asynchronous exit is appropriate, but the new wait and the job's natural lifetime must be separated.

This is a test-sensitivity gap in #149 and the milestone's explicit requirement to preserve what each case proves. No product-behavior defect is asserted by this finding. No merge was performed.

## What I checked

### Races and preserved assertions (#147, #149)

The Perl exec wrapper restores SIGINT before Bash starts; it does not suppress the signal or retry the case. Each signal case now proves the exact exit status, removal of the stored marker, and exactly one cleanup. Both ordinary and inherited-SIGINT-ignored starts pass all 42 checks. In a temporary copy, removing the Perl reset reproduces the INT failure (41 pass, 1 fail).

The synchronous default Terminal stub removes the unrelated scheduling-versus-poll-budget race from scenario/status tests. Async coverage remains separate: `term_late` checks an actual later completion and its output; the deadline cases use real sleeps far beyond their deadlines and retain status, cancellation diagnostic, elapsed upper-bound, and process-exit checks. The ordinary runner passes all 73 checks. Reducing `vm_terminal` to one poll in a temporary copy fails both `term_late` assertions (71 pass, 2 fail), so the new synchronous default has not made the wait contract untestable. The elapsed-return upper bounds still distinguish deadline return from natural completion (60-second guest job versus 40-second bound; 30-second Terminal job versus 20-second bound).

### Blind checks and mutation sensitivity

All mutations below were made in an isolated `/tmp` source copy, not the review checkout, and restored after running. Expected failing mutant commands are positive sensitivity evidence, not failures of the submitted implementation; the surviving cancellation mutant is R1.

| Issue | Independent check | Result |
| --- | --- | --- |
| #149 | Remove only the deadline cancellation `pkill` in `vm_terminal` | **Survives: all 73 runner checks pass; R1** |
| #153 | Restore the old `textedit_is_empty` implementation, which ignores read failure | The unreadable-document check and its diagnostic check fail; baseline runner is green |
| #159 | Remove `state === "granted" &&` from onboarding refresh | Both new refusal tests fail; baseline is 20/20 green |
| #156 | Replace the pseudo-event's session-flags seam with a direct `CGEventSource::flags_state` read | `tap_disabled_pseudo_event_takes_the_session_flags` fails; baseline pseudo-event tests pass |
| #138 | Flip the builder's activation argument to `true` | New launch-focus source pin fails; baseline passes |
| #142 | Halve the stop budget | New test fails with 2.5 seconds versus the required 10 seconds |
| #142 | Replace the accept loop's stopping guard with `false` | New exit-fence test fails after its 10-second failure bound |
| #143 | Omit `Scratch::drop` directory removal | Both new fixture-cleanup tests fail; baseline is 2/2 green |
| #150 | Disable queue coalescing | Fails in 0.036 seconds with the explicit 3017-events-queued diagnostic |
| #150 | Block overlay `run` for 60 seconds without yielding | CI-profile nextest terminates it at 5.003 seconds, proving inheritance of the default-profile override |

For #136 I reviewed the added Dvorak Right-Handed/Turkish inputs, exact-transcript assertion, post-dictation layout check, and existing restoration trap. The documentation correctly distinguishes layouts whose Command-V key remains key 9 from layouts that detect a hard-coded key 9. No VM was used, so I did not independently repeat the developer's VM layout mutants. Likewise #138 is deliberately a source pin, not a runtime focus proof; the README states this limitation and retains the VM behavior guard.

### Product behavior and CI

Reviewed every change under `src`, `src-tauri`, `crates`, and `third_party/handy-keys`. Onboarding changes are test-only; singleton instrumentation and ACL cleanup are test-only. The hotkey production seam invokes the same combined-session OS flags function as before. WAV encoders preserve validation, header bytes, sample conversion, and capacity for actual samples, while the header-only boundary test no longer reserves gigabytes. `Audio::duration` now uses integer nanoseconds rounded down instead of floating-point rounding; its production callers only log the duration, so this does not alter recording/transcription decisions. Remaining production edits are documentation.

The added standalone platform clippy invocation selects only the library without `testing`; the separate doctest invocation checks the hidden-fakes compile-fail contract. They are separate Cargo invocations, so previous all-features builds do not unify features into them. The self-hosted platform CI job passed, including both new no-feature checks. The quality-gate CI step also passed in 3 minutes 38 seconds (09:37:02-09:40:40 UTC), comfortably under the roughly ten-minute target; the whole job took 3 minutes 54 seconds ([CI run](https://github.com/gargankit90/dictation-app/actions/runs/41421478755)). The five-second timeout matches the one stalled-overlay test and works under the CI profile; normal execution took 0.034 seconds in the focused baseline.

## Validation

- Read the milestone, PR notes, complete diff, reviewer rules, engineering rules, and all eight checklist sections.
- Focused nextest baseline: 61/61 pass, including WAV round trips/boundaries, audio duration, stalled overlay, launch source pin, and both singleton additions.
- Shell baselines: key handling 42/42 normally and 42/42 with inherited ignored SIGINT; runner 73/73.
- UI baseline: onboarding 20/20.
- Additional isolated baselines: platform additions 3/3; ACL cleanup 2/2; vendored pseudo-event tests 3/3.
- Complete local `scripts/gate.sh`: PASS in 206 seconds (3 minutes 26 seconds), including both platform feature modes, coverage, all UI checks, vendor tests, guards, VM-lock helpers, and runner helpers.
- Coverage: core 98.99%, transcribe 98.67%, app 97.17%, all above the required thresholds.
- An initial local gate attempt reused an isolated mutant's vendored test binary through a shared target directory; I identified that validation-cache contamination, cleared the affected vendor target, disconnected the temporary copy from the checkout's target, and reran the complete gate. No tracked source was changed for the mutants.
- The parallel Tester owns the required 30-run load loops; I did not duplicate those loops or use a VM/the owner's screen.

## Checklist

| Section | Assessment |
| --- | --- |
| 1. Scope | R1: all 15 issues represented, but the #149 cancellation check lost sensitivity; VM layout execution is evidenced by developer notes and left to the Tester |
| 2. Correctness | R1: race causes addressed and signal/error statuses retained, but a missed cancellation remains green |
| 3. Security and privacy | OK: no new destinations, secrets, transcript logging, capabilities, clipboard behavior, or persistence |
| 4. Architecture | OK: dependency direction unchanged; OS code stays in platform/vendor; test hooks excluded from production |
| 5. Rust and TypeScript practice | OK: no new production panic path, unsafe block, suppression, or untyped IPC |
| 6. Licences and reuse | OK: no dependency or external-code addition; existing MIT vendor patch documented in PATCHES.md |
| 7. Tests | R1: the missing-cancellation mutant survives; other independent sensitivity checks and full-gate results are recorded above |
| 8. Git and PR | OK: focused Conventional Commits and detailed race/mutation notes; no guard changes; no unresolved minor findings |
