Reviewer: APPROVED

PR: #157, round 2. Reviewed fix `228505a647433242114560a512356fae83a02270`
at branch head `3364a6efae4d9b6d5e9d704af00e0e9b961f55c9`.
Scope: R1 only; unchanged T4a code was not re-reviewed. No open blocker,
major or minor findings in this fix. R1 is resolved. This review does not merge
or authorize merging without the workflow's other conditions.

## R1 resolution

The shared client now receives the configured base URL and disables all proxies
only for parsed plain HTTP URLs whose host is exactly `localhost`, a loopback
IPv4 address or IPv6 `::1`. It compares parsed scheme/host values rather than
URL prefixes or substrings. Non-loopback hosts and HTTPS retain the existing
proxy configuration. Redirect following remains disabled.

Dictation uses `Engine::from_settings_at` → `OpenAiCompat::new` → `with_wait`.
Settings' Test key preserves the selected base URL through `check_key` →
`check_key_within` → `OpenAiCompat::new` → `with_wait`. Both therefore use the
fixed client. The existing override validator still refuses non-loopback HTTP,
HTTPS and URL credentials; the fix does not widen accepted overrides.

## Verification

- `cargo test -p dictation-transcribe --test loopback_no_proxy`: **2 passed**.
  The parent launches its child with proxy variables set and both NO_PROXY
  forms removed; dictation and Test key reach the local provider, and the
  proxy receives zero connections.
- `cargo test -p dictation-transcribe http::tests`: **3 passed**.
- Rebuilt `/tmp/t4a-proxy-repro` against the new head and reran its original
  `run.py`: no proxy request/header/audio observations; the unavailable local
  provider now returns a network error directly.
- Extended that external harness with a capturing proxy and checked both
  dictation and Test key: **24 routing checks passed**. Plain HTTP to
  `127.0.0.1`, `localhost`, `[::1]`, `127.1.2.3` and numeric IPv4
  `2130706433` produced zero proxy connections. HTTP to `localhost.invalid`,
  `127.0.0.1.invalid`, `localhost@remote.invalid` and `10.0.0.1` used the
  configured proxy. HTTPS to loopback, the Groq preset and the OpenAI preset
  also used the proxy (CONNECT). The proxy rejected forwarded requests;
  no requests went to external providers. All credentials/audio were synthetic.
- Copied the unchanged regression into an isolated archive of pre-fix
  `db31268`, using its own build directory: **failed as expected** at
  `dictation: Err(Network("error sending request"))`. This confirms that
  the regression catches the old proxy routing behavior.
- `git diff --check db31268 HEAD`: passed.

## Checklist for the changed scope

- Scope: OK; three-file R1 implementation/regression change only.
- Correctness: OK; URL parsing, scheme distinction, both construction paths and
  preserved HTTPS proxy behavior checked above.
- Security/privacy: OK; R1 fixed, redirects remain disabled, no new logging or
  secret persistence. Hostname lookalikes do not trigger proxy bypass.
- Architecture: OK; shared transport policy remains in transcribe, no new crate
  dependency, platform conditional or unsafe code.
- Rust/TypeScript practice: OK; fallible parsing fails closed to existing proxy
  behavior, no product panic or unrelated UI change.
- Licences/reuse: OK; no new dependency or copied implementation.
- Tests: OK; subprocess environment isolation, both request paths, and actual
  failure on old code verified. VM scenarios and unchanged behavior remain
  covered by the Tester rather than repeated in this review.
- Git/PR: OK for this fix; conventional commit includes the regression. No new
  minor findings or follow-up issues.

Only this review file is committed; no product, test, spec or guard edits were
made in the checkout. Full quality checks remain enforced by the push hook.
