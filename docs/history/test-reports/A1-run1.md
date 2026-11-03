# A1 test report, run 1

PR: #220 at f1470be
Result: PASS

| # | Criterion | Test(s) | Result | Notes |
| - | --------- | ------- | ------ | ----- |
| 1 | Port, fake, PostHog US adapter, batched, bounded, drops on failure | `cargo nextest run -p dictation-telemetry -p dictation-ports` (all pass); code read | PASS | `POSTHOG_HOST = https://us.i.posthog.com` (asserted by a test); queue cap 200, batch 20, tokio task, one attempt, queue dropped on failure, unreachable host tested; `record` only takes a mutex and pushes, so it never blocks dictation. |
| 2 | Closed event enum | `crates/ports/src/telemetry.rs` read | PASS | Every property is an enum, a bucket, `u16` or the validated `Version` (digits/letters/`.-+`, 32 max). No free `String` reaches the payload. Events match the spec list. |
| 3 | Nothing sensitive sent | `usage_privacy.rs` read and run | PASS | Plants transcript, key, app/bundle name, path, host through the real pipeline (transcriber reply, insert errors, secure input) and scans serialised events plus a 24+ char key-shape check. **Mutation check:** added `detail: String` carrying the error's Debug text to `DictationFailed`; the test failed (`hunter2 leaked into [...]`). Restored with `git checkout`. Limit: it covers pipeline-derived events; shell events (`app_opened`, `setting_changed`, ...) are safe by the closed types, not by this test. |
| 4 | Identity | `install_id::tests`, `events_go_out_as_one_anonymous_batch`, `turning_it_off_clears...` | PASS | Random v4 UUID from getrandom, local file; payload has `$process_person_profile=false`, `$geoip_disable=true`, no `$ip`; off deletes the ID and the queue, on makes a new one. |
| 5 | Settings → Privacy toggle | `npx vitest run` (122 pass incl. "share anonymous usage events"), Rust `turning_it_off_clears_the_queue_and_the_id_and_sends_nothing`, settings-change tests | PASS | Default `true` (missing in old files = true). "What's sent" has 7 bullets covering the 8 events (the two onboarding events share one); matches `docs/privacy.md`. |
| 6 | Keyless build sends nothing | `without_a_key_nothing_is_queued_sent_or_written`, `a_build_without_a_key_sends_nothing`; built debug app without the key | PASS | `BUILD_KEY = option_env!(...)`; `Config::new` drops blank keys; `set_enabled` is a no-op without a key; scripted (e2e) launches never get a key. The built binary contains no `phc_` string. No host override exists in the shipped path (`host` is test-only), so I checked the code path rather than capturing traffic. |
| 7 | Docs | AGENTS.md, ENGINEERING.md, privacy.md read against code | PASS | Text matches the code (US host, 200/20/60 s/10 s, ID deletion, no `$ip`). The old "No analytics" and "analytics stay off" lines are gone. |
| 8 | Gate, deny | `gh pr checks 220`; `cargo deny check` locally | PASS | Quality gate (Linux), Platform crate (macOS), DMG (macOS) all green; deny: advisories, bans, licenses, sources ok. |

## review-checklist.md edit

The Developer changed the "no analytics" line in `docs/review-checklist.md`
(outside its edit list). The edit is **correct and needed**: the old line
("no new network destinations ...; no analytics") would make every reviewer
flag the milestone's own work. The new line allows PostHog and requires new
events to be in `docs/privacy.md` and hold only enums, buckets or versions.
The Lead should accept it.

## Gate

Local: 484 Rust tests pass (telemetry, ports, pipeline, core), 1 skipped;
vitest 122/122; cargo deny ok. CI green on all three jobs. Coverage was not
re-measured here (the CI gate covers it).

## E2E (VM)

`dictation-vm-2` showed a busy lock held by a pid that no longer exists; I did
not clear it and used `dictation-vm` (no Developer run was active). Built from
the PR head, no PostHog key.

- e2e_settings_save_reopen: PASS
- e2e_onboarding_first_run: PASS

2 passed, 0 failed, 0 skipped (output `tests/e2e/out/20261103-080124-dictation-vm`).

## Untested behaviour found

Minors, none blocking:

- The privacy test drives only the pipeline-derived events; shell-side events
  (`app_opened`, `setting_changed`, `engine_selected`, onboarding) are safe by
  their closed types, with no end-to-end plant test.
- The real HTTPS path to PostHog (rustls platform verifier) is only tested
  against wiremock over HTTP. The first real send needs the owner's key and a
  look at the PostHog project.

## Failures

None.
