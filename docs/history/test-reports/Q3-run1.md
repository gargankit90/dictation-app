# Q3 test report, run 1

PR: #338 at 1a6fcb2 (plus the Tester's e2e commit)
Result: PASS

| # | Criterion | Test(s) | Result | Notes |
| - | --------- | ------- | ------ | ----- |
| 1 | PKCE vectors; listener accepts one valid callback, rejects others, times out, loopback only | `connect::tests::*` in `dictation-transcribe` (RFC 7636 vector, wrong state/path/no code, timeout, cancel, IPv4 + `::1`); gate | PASS | Gate ran the Rust suites. |
| 2 | Exchange against a local fake; key stored; failures clear; key never logged | `dictation-app` `connect::tests::*`, `the_connect_path_never_logs_the_key_the_code_or_the_verifier`; new e2e `e2e_connect_openrouter` | PASS | The e2e adds the real keychain and a real PKCE check by the fake. |
| 3 | 402 gives the Add credits fix | ports/core/tray mapping tests; vitest (439 passed); new e2e: a 402 shows "! Your OpenRouter credits ran out." with "Add credits…" in the tray | PASS | The tray row was read, not clicked: clicking opens a browser in the VM. |
| 4 | Gate passes; e2e onboarding scenarios updated | `scripts/gate.sh` (all checks passed); the 7 scenarios below | PASS | The PR notes said the scenarios were not updated; this run does that. |
| 5 | [manual] real account | Lead | n/a | Not checked here. |

## E2E (dictation-vm, run `20261120-193849`, round 2 re-run on the same build)

- Fixed (8 of 8 pass): `e2e_setup_own_key_no_permissions`, `e2e_onboarding_first_run`,
  `e2e_onboarding_round2`, `e2e_setup_redesign_screens`, `e2e_onboarding_screenshots`,
  `e2e_settings_key_provider_recovery`, `e2e_local_model` (needed no change), and the new one.
- The card is now "Use OpenRouter" (the default), the screen "Connect OpenRouter", and a typed key
  sits behind "Paste a key instead". Waits for "Key saved" became "Connected to OpenRouter".
- `e2e_onboarding_round2`: OpenRouter is the default and pressing a selected radio changes nothing,
  so the scenario picks the on-device model first, then OpenRouter.
- `e2e_setup_redesign_screens`: dropped the "Replace" step and its screenshot (the old saved-key
  card has no counterpart now). Settings recovery: "Change key…" is now "Manage…", then "Paste a key instead".
- New `e2e_connect_openrouter` (added to `ALL_SCENARIOS`): `local-provider.pl` now serves
  `GET /auth` (calls the app's loopback callback back with the same `state`),
  `POST /auth/keys` (checks the code and that S256(verifier) is the challenge) and a `force-status`
  file. The scenario: fresh onboarding defaults to OpenRouter, one press of Connect OpenRouter
  connects, the fake logs `exchange pkce=ok code=ok`, the engine is rebuilt with the key, a relaunch
  finds it in the keychain, a dictation pastes with `auth=good`, a 402 shows the Add credits tray fix,
  Settings → Manage… → Disconnect leaves the engine with no key, and neither the log nor any file holds the key.
  The key is made up and never printed; the control file with it is deleted after the exchange.

## Gate
`scripts/gate.sh`: all checks passed (including the e2e helper tests). vitest: 17 files, 439 tests passed.

## Untested behaviour found
- Clicking "Add credits" in the VM was not tested (it opens the real browser). The link mapping has unit tests.
- The real OpenRouter service accepting `localhost` callbacks is the [manual] check.

## Failures
None. One first-run failure (`e2e_onboarding_round2`) was a scenario assumption, fixed above.
