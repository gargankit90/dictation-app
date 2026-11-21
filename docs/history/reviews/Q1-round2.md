# MQ1 review, round 2

PR: #335 at db5988da6c4c61dd21af13d25c235616e9bf0867
Verdict: CHANGES REQUESTED

Rechecked only the fixes since round 1. No merge performed.

## Findings

| # | Severity | file:line | Finding | Suggested fix |
| - | -------- | --------- | ------- | ------------- |
| 1 (round 1) | blocker | eval/reports/asr-screen.md:172 | Dataset wording remains: the named substitution for clip 0218 and the named substitutions in the table at lines 180–181 are still per-clip dataset text. voicearena.md:424–429 and 517–520 also retain dataset-derived mishearings and self-correction quotations. Removing the longer passages did not complete the owner's exclusion of per-clip dataset text. | Remove the remaining dataset wording throughout the reports; keep clip IDs, scores, counts and error categories. |

## Earlier findings

- Round-1 finding 1: **open**, with examples above; [thread reply](https://github.com/gargankit90/dictation-app/pull/335#discussion_r4649207231).
- Round-1 finding 2: **fixed**. Regression uses eval/offline_cleanup.py, checks script paths before paid subprocesses, and tests the real-run subprocess order and selected prompt path offline; [thread reply](https://github.com/gargankit90/dictation-app/pull/335#discussion_r4649206898).
- Round-1 finding 3: **fixed**. ASR test subprocesses use sys.executable, not uv, and skip if numpy, soundfile or soxr is unavailable. Requests use the local fake server; [thread reply](https://github.com/gargankit90/dictation-app/pull/335#discussion_r4649207021).

## Checklist delta

1. Scope: finding 1 remains.
2. Correctness: regression invocation fix verified.
3. Security and privacy: finding 1 remains; fixes do not change key handling.
4. Architecture: unchanged from round 1.
5. Rust and TypeScript practice: unchanged from round 1.
6. Licences and reuse: finding 1 remains under the explicit owner exclusion.
7. Tests: command orchestration coverage added; gate dependency issue fixed. Latency scenario unchanged; existing Tester report records 20 mock dictations with p50 12 ms and p95 35 ms on the previous head.
8. Git and PR: changes remain limited to the requested fixes, tests and review/test evidence. Lead manual paid regression remains pending.

## Validation

- eval/test.sh: 58 cases, one optional-dependency skip, 12.779 s; all 10 latency parser cases passed in 0.082 s.
- UV_OFFLINE=1 uv run --offline pytest eval -q: 63 passed.
- eval/regress.sh --dry-run: passed without a key. This worktree has no Q2 app prompt file yet, so it correctly selects the documented eval fallback; the existing app-path preference remains in evalcommon.py.
- No paid API calls, real judge calls or new VM run performed.
