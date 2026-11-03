# Privacy: anonymous usage events

dictation-app can share anonymous usage events so we can see what works and
what breaks. This page lists everything it sends. The list is enforced in code:
events are a closed enum (`crates/ports/src/telemetry.rs`), so a field not
listed here cannot be sent.

- **On by default.** Turn it off in Settings → Privacy → "Share anonymous
  usage events". Off takes effect at once: nothing more is sent, events still
  waiting are deleted, and the install ID is deleted. Turning it on again
  makes a new ID.
- **Where it goes.** PostHog Cloud US (`https://us.i.posthog.com`), sent by
  the app itself, never by the webview.
- **A build without a PostHog key sends nothing.** Development builds, CI and
  forks have no key (`DICTATION_POSTHOG_KEY` is read at build time).

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

## For the owner: PostHog set-up

1. Create a PostHog **Cloud US** project.
2. In Project settings, turn on "Discard client IP data"; leave autocapture,
   session replay and surveys off.
3. Give the release build the project API key (`phc_…`) as the environment
   variable `DICTATION_POSTHOG_KEY`. It is not secret in the sense of an API
   key (it can only write events) but keep it out of the repo.
