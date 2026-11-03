Reviewer: CHANGES REQUESTED

PR: #107, reviewed at `64527e14958bf7f9f7e91a9e5dd702d62fb4bbf3`.
Findings: 0 blockers, 1 major, 1 minor. No failure in the current product
implementation was found; the major is a demonstrated gap in the principal
microphone regression oracle required by T1 criterion 2.

## Findings

### Major M1: shutdown masks a microphone left open by overflow cancellation

**Location:** `crates/app/tests/overflow_outcome.rs:224` (also
`crates/app/tests/m2_3_edges.rs:315`).

The new outcome test asserts that the microphone is closed only **after**
`pipeline.shutdown()`. Shutdown unconditionally discards audio, independently
of whether overflow cancellation discarded it. The Idle event is a fence for
the coordinator, not for the audio worker. Eight `yield_now()` calls neither
prove that the audio worker finished nor check its recording flag before
shutdown. The rewritten Tester criterion 7 similarly checks the flag after
shutdown and starts a fresh dictation before checking cancellation's audio
outcome.

**Concrete regression accepted by the suite:** the UI becomes Idle following
an overflow, but the microphone keeps recording until the next dictation or
app shutdown. In `/tmp/t1-review-overflow-mic`, I changed only
`Runner::keys_overflowed` to execute the coordinator's cancellation commands
except `DiscardRecording`, preserving its state transition, queue draining,
and UI events:

```rust
let (state, commands) = step(self.state, Event::CancelPressed);
self.state = state;
for command in commands {
    if !matches!(command, Command::DiscardRecording) {
        self.execute(command);
    }
}
```

`cargo nextest run -p dictation-pipeline` still passed **all 142 tests**,
including both tests above. I then added a completion notification to the
temporary fake microphone's `discard()` and awaited it, with a bounded timeout,
after releasing the blocked start and **before** shutdown. The strengthened
outcome test failed on this mutant after 10 seconds with
`overflow left the microphone open until shutdown`; restoring the original
product code made the same strengthened test pass in 0.011 seconds.

**Suggested fix:** fence completion of the fake audio source's discard after
releasing the blocked start, then assert `!recording` and no transcription
before shutdown or another key/dictation. Apply this to the overflow outcome
test and preserve the Tester test's recovery check. No sleep is needed. This
closes the missing criterion-2 oracle; the current product code already sends
the discard correctly.

Reproduction and evidence on this machine:
`/tmp/t1-review-overflow-mic.py`,
`/tmp/t1-review-overflow-mic-existing.log`,
`/tmp/t1-review-overflow-mic-probe.log`, and
`/tmp/t1-review-overflow-mic-control.log`.
All product/test mutations and the extra probe were confined to `/tmp`.

### Minor N1: redirect recovery advice names an unavailable setting

**Location:** `src-tauri/src/commands.rs:207` and
`crates/ports/src/transcribe.rs:75`.

The new redirect messages say to check the provider's address, but Settings
offers only Groq/OpenAI selection, and Test key gets its endpoint from the
compiled preset. Users cannot follow that instruction. The dedicated redirect
classification and explanation that redirects are refused are correct.

**Suggested fix:** omit the address advice or replace it with guidance users
can act on with the shipped settings. Nonblocking follow-up:
[tech-debt #109](https://github.com/gargankit90/dictation-app/issues/109).

## What I checked

- Read the complete PR diff, developer notes, `docs/milestones/T1.md`,
  `docs/ENGINEERING.md`, reviewer role, and all eight checklist sections.
- Ran `cargo nextest run -p dictation-pipeline -p dictation-transcribe
  -p dictation-core -p dictation-platform`: **654 passed**, 2 excluded/ignored;
  test execution took 9.366 seconds after compilation.
- Ran `scripts/vendor-tests.sh` on macOS: **14 hotkey patch tests and 2
  CoreAudio patch tests passed**, with dependency-version reconciliation.
- Independently injected ten individual mutants in an archived `/tmp` copy.
  Every listed mutant compiled and its targeted test failed:

| Mutant | Oracle that caught it |
| --- | --- |
| Test key sample multiplication becomes addition | Exact decoded WAV upload |
| Remove Test key timeout override | Check policy fields |
| Accept a stale `NotTranscribed` ID | All job-result variants table |
| Remove overflow queue draining | Overflow outcome permutations |
| Remove UI phase coalescing | Stalled consumer bound/state/error test |
| Remove `AudioWorker::drop` cleanup | Abandoned runner closes worker |
| Remove real `TokioWait` sleep | Paused-time delay test |
| Body classification `||` becomes `&&` | Truncated-body safe error |
| Settings migration `< 2` becomes `<= 2` | v2 does not apply v1 aliases |
| Debug collision counter `+= 1` becomes `*= 1` | Occupied suffixes test |

- Separately removed the session-flags fallback in the vendored hotkey
  callback in `/tmp`: the synthetic-event regression test failed, reporting
  the unwanted resync. Its baseline passed above. Ordinary modifier release
  still follows the unchanged `FlagsChanged` decoding path; Esc still emits
  its ordinary key event. Real remote-desktop/hardware interaction remains
  unverified here, as requested; no VM or screen interaction was performed.
- Reviewed the bounded UI queue: at most 64 events, at most 16 retained oldest
  errors, excess errors counted/logged, latest phase preserved by coalescing,
  and recovery/closure notifications covered. No lost-final-state defect found.
- Checked stored-key reads share `KeyReads`, retain bounded waits and allow
  retry; typed keys still bypass keychain reads. Exact clip, no language,
  one-attempt policy, redirect refusal, and safe error mapping are covered.
- Reviewed HTTP abort/backoff/partial-body tests and their bounded fences.
  The modified engine request-arrival loop retains its original assertion
  while adding a timeout. The redirect matrix adds 308 without dropping
  previous statuses. The Tester criterion-7 rewrite improves queue/recovery
  assertions but has M1's missing audio-completion oracle.
- CI at the reviewed head was green: Linux Quality gate and macOS Platform
  crate. The Linux gate step took **6m26s**; the whole Linux job took 9m59s,
  including setup. No demonstrated new timing flake or excessive gate cost.

## Checklist and acceptance mapping

| Checklist section | Result |
| --- | --- |
| 1. Scope | T1 criteria 1–9 map to the changed code/tests; criterion 2 has M1. Criterion 10's VM result is the developer's report, not independently rerun here. |
| 2. Correctness | No current product defect found in overflow handling, UI queue, stale results, key checks, redirects, flags, or save collision handling. M1 limits the regression guarantee. |
| 3. Security and privacy | No new secret/transcript exposure, audio persistence outside debug mode, analytics, destinations, capability expansion, or clipboard changes found. |
| 4. Architecture | Dependency direction retained; ports error variant is appropriate; no new Tauri coupling, OS conditional, or unsafe production block introduced. |
| 5. Rust and TypeScript practice | Test-only panic/unwrap allowances have reasons; no new production panic or blocking work on the async path found. N1 is wording only. |
| 6. Licences and reuse | Vendored changes are to the existing MIT dependency and documented in `PATCHES.md`; no new copied source/dependency licence concern found. |
| 7. Tests | M1. Criteria 1, 3–9 otherwise have meaningful oracles; independent mutation checks above support the developer's survivor accounting. Coverage is green in CI. |
| 8. Git and PR | Detailed criterion/mutation/risk notes and Conventional Commits; no guard edits. T1 spec addition is the target spec supplied for this review. Minor tracked as #109. |

Do not merge on this review. Re-review can focus on the pre-shutdown audio
fence/assertion and its failure against the overflow-only discard mutant.
