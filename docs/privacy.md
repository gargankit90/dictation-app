# Privacy: usage events and error reports

Vocet can share anonymous usage events and error reports so we can see
what works and what breaks. This page lists everything it sends. The list is
enforced in code: events are a closed enum (`crates/ports/src/telemetry.rs`),
and error reports are built field by field from an allow-list
(`crates/telemetry/src/errors.rs`), so a field not listed here cannot be sent.

- **On by default.** Turn it off in Settings → Privacy → "Share usage and
  error events". One switch covers both. Off takes effect at once: nothing
  more is sent, events and reports still waiting are deleted, requests under
  way are dropped, and the install ID is deleted. Turning it on again makes a
  new ID.
- **Where it goes.** PostHog Cloud US (`https://us.i.posthog.com`), sent by
  the app itself, never by the webview.
- **A build without a PostHog key sends nothing.** Development builds, CI and
  forks have no key (`DICTATION_POSTHOG_KEY` is read at build time).

## Error reports (Sentry)

When the app panics (a crash in our own code) or a dictation fails, it can
send a report to Sentry in the US region (`*.ingest.us.sentry.io`), from the
app itself, never the webview. A build without `DICTATION_SENTRY_DSN` (dev
builds, CI, forks) sends nothing. A report holds only:

| Field | Value |
| --- | --- |
| `release` | the app version, for example `0.3.0` |
| `environment` | `production` or `development` |
| tag `error_code` | the error code (for example `mic_denied`), for a failed dictation |
| tag `error_type` | the kind of failure, as in `dictation_failed` above |
| tag `arch`, `macos_major` | the chip and the macOS major version |
| exception type | `panic`, or the error code |
| for a panic | the source location (for example `crates/app/src/x.rs:10`) and the stack: function names and file names relative to the crate, with line numbers |

Never in a report: the panic's or the error's message, breadcrumbs, a user, an
IP address (the app sets none; the Sentry project should also have "Prevent
storing of IP addresses" on), the computer's host name or name, environment
variables, a file path beyond the crate-relative one, local variables, or
memory. The report is built from the fields above only; nothing else is
copied in. A panic report is written to a small file in the app's data folder
and sent at the next launch (so a crash that ends the app is still reported);
turning sharing off deletes it. At most 30 reports are sent per run.

**Native crashes (a segfault in the on-device engine) are not reported.** A
crash dump holds raw memory, which could include what you dictated, and cannot
be cleaned. If that changes it will be listed here first.

## Never sent

Audio. Transcripts. Anything you type or paste. API keys. The names, bundle
IDs or window titles of the apps you use. File paths. Host names. Your user
name. Error messages (only the kind of failure). Setting values (only which
setting changed). Your IP address: the app sends no `$ip` and sets
`$geoip_disable`; the PostHog project must also be set to discard client IP
data (Project settings → "Discard client IP data").

## Identity

A random install ID (a version 4 UUID from the operating system's random
source), kept in the app's data folder in `install-id`. It is not derived from
your hardware, account or name. It is sent as PostHog's `distinct_id`. Person
profiles are off (`$process_person_profile: false`), so PostHog keeps events,
not profiles. The ID is pseudonymous, not anonymous in the legal sense: it
links one install's events together. Turning sharing off deletes it.

## Properties on every event

| Property | Value |
| --- | --- |
| `distinct_id` | the install ID |
| `$process_person_profile` | `false` |
| `$geoip_disable` | `true` |
| `$lib` | `dictation-app` |

## Events

| Event | When | Properties |
| --- | --- | --- |
| `app_opened` | The app starts | `version` (for example `0.1.0`), `macos_major` (for example `15`), `arch` (`apple_silicon`, `intel`, `other`) |
| `onboarding_step_reached` | You get to an onboarding step | `step` (`microphone`, `accessibility`, `key`, `try`) |
| `onboarding_completed` | You finish onboarding | `step` (`try`) |
| `engine_selected` | You pick a provider or save its key, or switch between On this Mac and Your own key | `kind` (`cloud`, `local`), `provider` (`groq`, `openai`; cloud only; which local model is never sent) |
| `dictation_finished` | A dictation was pasted | `engine_kind` (`cloud`, `local`), `duration_bucket` (`under2s`, `from2_to5s`, `from5_to15s`, `from15_to60s`, `over60s`), `latency_bucket` (`under500ms`, `from500_to1000ms`, `from1_to2s`, `from2_to5s`, `over5s`) |
| `dictation_failed` | A dictation failed | `error_type` (`transcription`, `empty_transcript`, `insert`, `secure_input`, `microphone_permission`, `accessibility_permission`, `hotkey`) |
| `setting_changed` | You save a changed setting | `setting` (`provider`, `model`, `language`, `hotkey`, `launch_at_login`, `share_usage_events`). Turning sharing off sends nothing, not even this. |
| `update_installed` | The first launch after an update | `from_version`, `to_version` |

Versions hold only digits, letters, dots, dashes and plus signs, at most 32
characters.

## Delivery

Events are queued in memory (at most 200), sent in batches of up to 20 at least
once a minute and when you quit, with one attempt and a 10 second timeout. If
that fails, the queued events are dropped. Nothing is retried and dictation
never waits for it.

## Troubleshoot and "Report a problem" (separate from analytics)

Tray menu → Troubleshoot… (also Settings → Help → Troubleshoot…) checks, on
this Mac, the microphone and Accessibility permissions, whether the hotkey
works, whether the engine is ready (a saved key can be read, or the chosen
model is downloaded and the engine has loaded it, or says it is still loading
or couldn't load), and the last error. These checks record no
audio and contact nobody. "Test connection" is the one button that does: it
sends a short silent clip to your cloud provider with your saved key, only
when you press it.

"Report a problem" builds a short text report from a fixed list of fields
(`crates/core/src/report.rs`; there is no slot for anything else):

- the app version, macOS version and chip
- whether the microphone and Accessibility permissions are granted
- your hotkey, as the setting writes it
- the engine: cloud with the provider's name, or on this Mac with the model id
- whether the engine is ready
- whether anonymous usage events are on
- the last ten error codes (for example `mic_denied`) with the time of each

It never holds audio, transcripts, anything you typed or copied, API keys, app
names or window titles, file paths, your user name, log lines, or what a
provider answered. A text field that isn't plain (a path, a key, odd
characters) shows `unknown` instead. Error codes are kept in memory only
(the last ten since the app started); the error text itself isn't kept.

The app shows you the exact text first. Then you choose: **Save report…** (a
file you pick), **Copy**, or **Open a GitHub issue** (opens your browser at a
new issue on this project with the report as its body, which you read and
submit yourself). The app uploads nothing and never sends a report by itself.

This is separate from the analytics switch above. Turning analytics off
doesn't stop you making a report, and turning it on never sends one. The
errors that analytics sends (`dictation_failed`, only the kind of failure)
stay governed by that switch alone.

## For the owner: PostHog set-up

1. Create a PostHog **Cloud US** project.
2. In Project settings, turn on "Discard client IP data"; leave autocapture,
   session replay and surveys off.
3. Give the release build the project API key (`phc_…`) as the environment
   variable `DICTATION_POSTHOG_KEY`. It is not secret in the sense of an API
   key (it can only write events) but keep it out of the repo.
   `scripts/release.sh` reads it from the Keychain (`docs/releasing.md`).

## For the owner: Sentry set-up

1. Create a Sentry project (platform "Rust", US region, free Developer plan).
2. In Project settings → Security & Privacy, turn on "Prevent Storing of IP
   Addresses" and "Data Scrubber".
3. Give the release build the project's DSN as `DICTATION_SENTRY_DSN`;
   `scripts/release.sh` reads it from the Keychain (`docs/releasing.md`).
