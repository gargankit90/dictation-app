# MQ1 review, round 1

PR: #335 at be7e5028af81057ec8fd269df2bc30b6f9cf9d3b
Verdict: CHANGES REQUESTED

## Findings

| # | Severity | file:line | Finding | Suggested fix |
| - | -------- | --------- | ------- | ------------- |
| 1 | blocker | eval/reports/cleanup.md:130 | Reports commit per-clip Voice Arena source text and derived drafts despite the owner's explicit exclusion. The surrounding section contains multiple identified clips and sentence quotations; hosted-asr.md also contains per-clip textual examples. | Remove per-clip dataset quotations throughout the reports; retain IDs, error categories and aggregate results. Store textual examples only in ignored local output. |
| 2 | major | eval/regress/regress.py:163 | Every real regression run invokes nonexistent eval/cleanup/offline_cleanup.py. The consolidated script is eval/offline_cleanup.py. A first run can spend money transcribing before failing here. Dry-run tests do not exercise this command. | Correct the path and add an offline orchestration test covering the actual subprocess commands. |
| 3 | major | eval/test_transcribe_openrouter.py:155 | When host Python has numpy and soundfile, eval/test.sh includes tests that invoke uv run on a script with dependency metadata. A cold cache can resolve/download dependencies; uv is also an undeclared requirement of this nominally stdlib gate step. | Invoke the current interpreter with installed optional dependencies, skipping when any are missing, or keep dependency-bearing tests explicitly outside the gate. Verify with optional dependencies present and an empty cache/network disabled. |

Inline discussions: [1](https://github.com/gargankit90/dictation-app/pull/335#discussion_r4648884943), [2](https://github.com/gargankit90/dictation-app/pull/335#discussion_r4648884847), [3](https://github.com/gargankit90/dictation-app/pull/335#discussion_r4648885051).

## Checklist

1. Scope: findings 1 and 2. Consolidation otherwise follows the trimmed PR scope; Q2 prompt-file integration is explicitly deferred in PR notes.
2. Correctness: finding 2. Judge adapters share one interface and cache by full prompt SHA-256, separated by judge/model. Cleanup cache also includes prompt, provider, model and effort. Existing budget checks, bounded retries and rate-limit stops remain; cleanup spend is checked after reported costs, so it is a stop threshold rather than a strict prepaid reservation.
3. Security and privacy: finding 1. API runners obtain keys from environment and avoid printing response bodies; hygiene and fake-server sentinel checks pass. No tracked audio or key-shaped values found. App capabilities, clipboard behavior and telemetry are unchanged.
4. Architecture: OK. Python tooling and e2e helpers; no application dependency changes.
5. Rust and TypeScript practice: OK, no Rust/TypeScript changes.
6. Licences and reuse: finding 1. Dataset fetch is pinned by revision and SHA-256, and local clip/output/cache paths are ignored. No evidence of prohibited GPL code adaptation found.
7. Tests: findings 2 and 3. Tests pass but do not cover the broken real regression invocation. Latency uses existing mock dictation helpers, has 20 bounded cycles and a 5 s sanity threshold; real VM duration/reliability needs Tester evidence.
8. Git and PR: OK for reviewed implementation. Conventional commits and detailed PR notes; no guard changes. Manual real regression check remains the Lead's responsibility; no merge performed.

## Validation

- eval/test.sh: 57 unittest cases, one optional-dependency skip, 13.453 s; latency parser: 10 cases passed, 0.094 s.
- uv run --offline pytest eval: 62 passed in 22.05 s with locally cached dependencies. The outer uv offline flag does not establish that nested uv commands are offline (finding 3).
- eval/regress.sh --dry-run: exit 0, printed 40-clip plan, prompt hash and speech/cleanup estimate without a key; implementation returns before subprocess/network operations.
- Full application gate, paid regression, real judge CLIs and VM scenario were not run by this reviewer. CI checks were pending when inspected.
- Minimum complexity: the consolidated runners are reasonably small; fixes above require no additional framework. No minor findings requiring tech-debt issues.
