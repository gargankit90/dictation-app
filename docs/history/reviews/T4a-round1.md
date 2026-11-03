Reviewer: CHANGES REQUESTED

PR: #157 at `6bea39fb00c165fec4ed1d27303d59977ed8fb6d` (round 1).
Findings: **1 blocker, 0 majors, 0 minors**. Do not merge until R1 is fixed and rechecked.

## Findings by severity

### R1 — Blocker: the loopback override sends audio and the API key through an HTTP proxy

**Location:** `src-tauri/src/lib.rs:430` (enabling the override), through
`src-tauri/src/engine.rs:185` and `crates/transcribe/src/http.rs:30–38`.
Settings' Test key follows the same client path (`src-tauri/src/commands.rs:463`).

The URL validator limits the destination URL to loopback HTTP, but the HTTP
client still uses environment proxies. With `HTTP_PROXY` set to a proxy and
`NO_PROXY`/`no_proxy` absent, the client sends the absolute loopback URL,
Authorization header and multipart WAV body to that proxy. A tester running
a debug build with a saved real provider key can therefore disclose both that
key and their recording to a remote proxy, despite choosing a local provider.
This is specific to the newly enabled plain HTTP override; normal provider
requests use HTTPS. Redirects are already disabled and do not cause this.

**Reproduction:** an external Cargo project in `/tmp/t4a-proxy-repro` depends on
the checked-out `dictation-transcribe` and `dictation-ports`, using a copy of the
workspace lockfile. Its `OpenAiCompat` config uses the accepted URL
`http://127.0.0.1:18731/v1`; it transcribes synthetic samples using a synthetic
credential. `run.py` starts a capturing HTTP proxy, removes all proxy environment
variables, sets only `HTTP_PROXY` to that listener, and starts the client.
Nothing listens at the provider port. Running
`python3 /tmp/t4a-proxy-repro/run.py` reports:

```text
proxy_received_absolute_loopback_request= True
proxy_received_authorization= True
proxy_received_audio= True
result=Err(InvalidKey)
```

The local proxy substitutes for a remote one without transmitting anything
off-machine. The harness prints only presence checks, no credential or audio.
No real key, keychain access, microphone, VM or screen was used.

**Suggested fix:** bypass proxies whenever the test override is used, for both
dictation and Settings' Test key. Keep normal provider proxy support. Add a
regression using a proxy-configured subprocess without a loopback exclusion:
the loopback provider must receive the request and the proxy must receive none.

[Inline finding](https://github.com/gargankit90/dictation-app/pull/157#discussion_r4438860287).

## What I checked

- Read the whole PR diff, T4a spec, PR notes, engineering rules, reviewer role
  and all review-checklist sections. No product, test, spec or guard edits made.
- Release boundary: `TEST_FLAGS` follows `debug_assertions`; parsing with test
  flags disabled ignores both override forms; release `with_provider_url` is a
  no-op. `LiveEngine::at` is available only for tests/debug. No current profile
  enables release debug assertions. I did not rebuild/run the release binary;
  the release-parser test and source inspection cover this review's check.
- Validation rejects non-loopback destinations, HTTPS and URL credentials;
  redirects are disabled in the shared client. R1 is the remaining transport gap.
- Outside the override, `provider_base_url` returns each provider's existing
  preset. Engine rebuild, keychain reads, retry/cancellation and paste logic
  otherwise remain unchanged. No dependency, capability, CSP or unsafe changes.
- #151: every error gets the short title; its full message remains in the
  tooltip and the same event is forwarded to the overlay. Recording still clears
  both tray fields. The updated VM scenario checks title and tooltip before,
  during and after the next recording, without weakening the prior transitions.
- Secret handling: app keys still use `Secret`/keychain and existing log
  scrubbing; the local fixture logs verdicts and request metadata, not headers,
  keys or transcript text. The scenarios use explicitly synthetic credentials;
  `secret-set` passes that synthetic value in JXA argv and its Terminal job
  script, so this helper must not be mistaken for a real-key entry path.
  The no-file search checks app/provider files, not those Terminal job scripts.
- Read all five new scenarios and provider/helpers. TextEdit reads encode the
  full UTF-8 document before shell capture, retaining trailing newlines. Expected
  documents include surrounding text and trailing newlines. Only the intended
  outer transcript whitespace is trimmed. Request totals come from provider
  events and unreadable events fail. The gate captures a request before release.
- Mutation spot-check by reading the logic, without running VM mutations: no
  rebuild leaves post-save request 3 unauthorized and its exact paste fails;
  recording cancellation removed changes phases/request count; a busy press
  starting recording changes the phase sequence and/or request total; focus
  stolen at insertion leaves B wrong; altered trimming/combining marks changes
  the base64 document; shutdown waiting for the held reply exceeds the 10-second
  Quit bound (gate holds 120 seconds, request timeout 30 seconds).
- Quit also requires spontaneous process exit, stopped log, connection closure,
  exactly one request, no Inserting, empty exact TextEdit read and unchanged
  clipboard after releasing the reply; relaunch must then dictate successfully.
- Existing `textedit_is_empty` debt is already #153; no duplicate finding/issue.
  Actual full VM runs and mutation execution remain the Tester's responsibility.

## Validation and checklist

- Focused Rust nextest selection: **9 passed**, covering provider URL parsing,
  build gating, key testing at the override, tray text and both HTTP shutdown
  cases.
- `bash tests/e2e/test-runner.sh`: **67 passed**.
- `bash tests/e2e/test-key-handling.sh`: **39 passed**.
- `git diff origin/main...HEAD --check`: passed.
- External real-engine proxy reproduction: confirmed R1 using workspace-locked
  dependencies; synthetic data only.
- Scope: OK, including the Lead-authorized tray fix, test edits and policy docs.
- Correctness: OK apart from R1's endpoint routing guarantee.
- Security/privacy: **R1**; other changed paths checked as described above.
- Architecture: OK. Rust/TypeScript practice: OK. Licences/reuse: OK (no new
  dependency or copied implementation).
- Tests: assertions and mutation sensitivity checked; missing proxy regression
  belongs with R1. Coverage/full gate are enforced by the normal push/CI hooks.
- Git/PR: notes map acceptance criteria to checks; spec provenance and explicit
  Lead-authorized scope noted. No new minors, so no tech-debt issues opened.

Review publication does not approve or merge this PR.
