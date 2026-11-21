# MTOS1 review, round 1

PR: #352 at 47a31551d09ecb0f4ee360c7e750f4e94fdf756e
Verdict: CHANGES REQUESTED

## Findings

| # | Severity | file:line | Finding | Suggested fix |
| - | --------- | --------- | ------- | ------------- |
| 1 | major | src-tauri/src/onboarding.rs:338 | A failed `accepted_terms` write is swallowed after setting the in-memory acceptance flag. The command returns success, starts reports and advances to Speech without storing the required terms date. This is not reliably repaired by asking again next launch: saved settings skip Welcome, and finishing setup supplies the legacy acceptance fallback. | Return the write error through the command; publish acceptance and enable reports only after successful persistence. Keep Welcome open with the existing error UI and test a failed write followed by a successful retry. |

[Inline finding](https://github.com/gargankit90/dictation-app/pull/352#discussion_r4654398911).
A concrete failure case is creating a directory at `accepted_terms` while a fresh install is waiting on Welcome, then pressing Get started; a full disk or a permission error on that file also fails the write.

## Checklist

1. Scope: finding 1 (required terms-date storage on failure). Four steps, calmer connection, catalogue sum and Terms links match the milestone. Omitting an unverifiable hourly price is reasonable and disclosed. The requested e2e updates are authorized by criterion 3 and the review task.
2. Correctness: finding 1. Normal setup progression, Done/login-checkbox behavior and permission-grant consumption reviewed. The grant is consumed only in the onboarding view; first reads establish a baseline and unchanged permissions do not repeatedly activate the window.
3. Security and privacy: OK for normal consent and switch paths. Both PostHog and Sentry start with `share && terms_accepted`; Sentry's disabled adapter rejects error reports and panic persistence and removes pending envelopes. AppOpened goes through the consent gate. Turning the Privacy switch off closes the gate and disables both adapters before storage work. No new event fields, destinations, secrets or content logging.
4. Architecture: OK. No new crate dependencies, platform cfg or unsafe code; typed command and main-window capability updated together.
5. Rust and TypeScript practice: finding 1 (hidden persistence failure). Otherwise no new panic shortcuts or suppressions; Welcome already has a usable command-error path.
6. Licences and reuse: OK. No dependency or adapted-code changes. Terms describe the beta, OpenRouter billing, local model licences and separate source licence without promising unsupported product behavior; governing-law TODO is explicitly expected. This is a product/code consistency review, not legal advice.
7. Tests: finding 1 needs a persistence-failure/retry regression. Six focused Rust TOS1 tests and 85 onboarding/model-row Vitest tests pass. Developer reports VM e2e results in the PR; I did not repeat VM runs. Real permission grant/foreground behavior still needs the stated VM/manual check.
8. Git and PR: OK. Conventional commits, detailed PR notes and no guard changes. Milestone is Lead-owned input. No new minors raised; no tech-debt issues needed. Do not merge in this task.

## Validation

- `cargo test -p dictation-app tos1 --lib`: 6 passed.
- `cargo test -p dictation-app sharing_switch --lib`: 8 passed, including Sentry, cancellation and failed opt-out saves.
- `npx vitest run src/Onboarding.test.tsx src/settings/modelRow.test.ts`: 85 passed.
- Review traced the removed Ready/sharing-caption symbols: no remaining product references.
- Lead/owner fresh-install light/dark walkthrough and reading the Terms remain manual requirements, separate from this verdict.
