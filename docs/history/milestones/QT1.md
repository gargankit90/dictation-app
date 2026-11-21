# QT1: Test coverage for the 0.2.0 cleanup pipeline

Three independent test audits (2026-11-20: `/Users/gauri/vocet-screens/u9/test-audit.md` (Claude),
`test-audit-codex.md`, `test-audit-gemini.md`) agree on the same top gaps. Q2 is merged (2924ad5, mQ2).
Connect OpenRouter tests belong to Q3 (#338) and local-mode tests to Q4 (#337); this milestone covers the
cloud cleanup path on main. Lead approval: this milestone may edit `tests/e2e/` (fixtures and scenarios).

## Scope
1. **Pipeline integration** (`crates/app/tests/`): cleaned reply is pasted; guard rejection, provider error,
   truncated reply (finish_reason != "stop") and the 1.5 s deadline each paste the raw transcript; a
   cancelled cleanup doesn't delay the next dictation (cleaned → raw at deadline → cleaned again).
2. **Guard matrix** (`crates/core`): dropped or invented words, numbers, links, names; self-corrections keep
   both values (prompt v4 rule); token-limited replies rejected. Table-driven, small.
3. **OpenRouter wire test** (`crates/transcribe/tests/`): privacy filters (speech data_collection deny;
   cleanup zdr true + data_collection deny) and attribution headers on EVERY request including retries and
   the key check; 401/402/404(no route)/429/5xx mapping; the key never appears in logs on the speech,
   cleanup or key-check paths (one shared helper instead of five copies).
4. **e2e cleanup scenarios** (`tests/e2e/`): extend the fake provider (`fixtures/local-provider.pl`) so
   `/chat/completions` can (a) return a rewritten reply, (b) hold past the deadline. Scenario A: the pasted
   text in TextEdit is the cleaned reply, not the transcript. Scenario B: on hold, the raw transcript is
   pasted within ~2 s and the next dictation is cleaned. Assert via TextEdit content, not logs.
5. Make cleanup visible to tests without logging text: one info-level log line per dictation with the
   cleanup outcome (applied / fallback_timeout / fallback_guard / fallback_error) and its duration in ms,
   no text. `e2e_real_openrouter` then asserts the outcome line exists.

Out of scope: test removals (SIMP1), Connect (Q3), local mode (Q4).

## Acceptance criteria
1. [auto] New unit/integration tests pass; gate green.
2. [auto] Both new e2e scenarios pass in a VM; the existing suite still passes.
3. [manual] Lead re-runs `e2e_real_openrouter` with the real key and sees the cleanup outcome line.

Developer: Sonnet 5.5. Time box: 1 day.
