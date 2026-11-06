# MH1 review, round 1

PR: #305 at e20b0034b6b378ffb545db9f2a1511fba1adb0cc
Verdict: CHANGES REQUESTED

## Findings

| # | Severity | file:line | Finding | Suggested fix |
| - | -------- | --------- | ------- | ------------- |
| 1 | major | crates/transcribe/tests/long_upload.rs:70 | Criterion 4's slow-upload regression is not exercised: wiremock receives the complete small request before delaying its response by 1 s, and every test finishes before the old production 30 s limit. This proves a scaled response deadline, but would miss a remaining 30 s upload cap. | Add a local server that controls request-body consumption, with a body large enough to exceed socket buffering. Demonstrate upload still incomplete after 30 s, then completes and succeeds under the computed deadline; retain a fixed-limit failure control. Synchronize progress explicitly and allow generous scheduling margins. |

## Checklist

1. Scope: finding 1 (criterion 4); criteria 1–3 and 6 otherwise OK. Criterion 5 assessed as "Esc behaves as before" per the Lead's PR comment.
2. Correctness: OK by inspection. Each attempt applies the same computed request timeout; reqwest carries its total deadline through response-body consumption. Timeout failures alone increment the new counter, with two timeout attempts for long clips and the existing total-attempt cap still enforced. Other failure policies unchanged.
3. Security and privacy: OK; no new destinations, secrets, transcript logging, capabilities or persistence.
4. Architecture: OK; timeout and retry policy are pure core functions, dependencies unchanged, no new unsafe or OS-specific code.
5. Rust and TypeScript practice: OK. Integer duration conversion avoids floating-point boundary errors; remainder is less than a u32 sample rate so multiplication by one billion fits u64. WAV validation rejects zero/overflowing sample rates before duration conversion; empty audio has the base timeout. Saturating duration addition avoids overflow.
6. Licences and reuse: OK; no new dependencies or adapted code.
7. Tests: finding 1. Existing scaled tests pass; the silent-server deadline has a generous 2.4–10 s assertion window. The success test has 1.4 s scheduling slack; an actual slow-upload test should use explicit progress rather than narrow wall-clock assumptions.
8. Git and PR: OK; Conventional Commit, detailed PR notes and explicit limitations. The added milestone is the Lead's spec; no guards changed.

## Validation and limits

- `cargo nextest run -p dictation-core -p dictation-transcribe`: 388 passed, 1 skipped, including all three long-upload tests, key-check deadlines, body/header timeouts and transport cancellation.
- Inspected the installed reqwest 0.13.5 implementation: request timeout covers connect/send and is transferred into the response body; connect timeout wraps direct and proxied connection futures, including DNS and TLS. Proxy configuration remains unchanged; 10 s is a reasonable bounded setup budget, though any DNS/proxy setup exceeding it will time out by design.
- Esc/coordinator and e2e assertions remain unchanged, consistent with the Lead's correction. VM tests and a real poor hotspot were not rerun by this reviewer.
- Full gate/coverage not independently rerun; CI is authoritative for those checks.
- No merge performed in this dispatch. Review report committed locally for the Lead; not pushed to the developer branch.
