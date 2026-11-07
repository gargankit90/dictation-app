# MN1 review, round 1

PR: #309 at 32e04a2
Verdict: CHANGES REQUESTED

## Findings

| # | Severity | file:line | Finding | Suggested fix |
| - | -------- | --------- | ------- | ------------- |
| 1 | major | docs/releasing.md:278 | The first-time owner's renewal guidance promises five years, but Apple's October 1, 2026 notice says certificates issued by the current G2 authority expire annually. This leaves N1 criterion 6's current expiry guidance incorrect. | Document the current annual renewal policy, tell the owner to check the certificate's actual expiration, and link Apple's current notice. |

Source: [Apple's current Sub-CA notice](https://developer.apple.com/news/?id=w4atic4c). It explicitly distinguishes the authority's 2031 expiry from annual leaf-certificate renewal. It also confirms G2 selection and continued operation of previously notarized, securely timestamped Mac apps.

## Checklist

1. Scope: criterion 6 needs finding 1; other changes map to N1. The milestone file is supplied as the assignment's specification.
2. Correctness: release.sh read line by line. App signing precedes app notarization/stapling; archive re-creation and re-signing and DMG replacement/signing follow it; DMG notarization/stapling follows that. Extracted copies must retain the checked CDHash and valid staples. Both archive and manifest signatures are checked against the shipped archive before publication. A failed build/notarization can leave intermediate artifacts under target/release/bundle, but they are not copied to the release folder or published. Explicit failures protect checks used in conditional contexts; minisign cryptographic checks explicitly return failure. notarytool submit deliberately ignores process status but requires an Accepted JSON status, followed by staple and Gatekeeper validation. No unsafe publication path found.
3. Security/privacy: OK. Configuration is parsed with sed, never sourced. Configured missing/invalid Developer ID fails without fallback. --no-notarize cannot accompany --publish. No new analytics event or identifying property. No owner secret, Keychain item or credential was read; no real release invocation occurred.
4. Architecture: OK. Core adds only serializable progress state, no I/O or OS conditionals. Existing shell onboarding adapter reads permissions and persists state. No new unsafe or platform cfg.
5. Rust/TypeScript: OK. Defaulted progress permits older files; asked_again persists until finish/start-over. Notice explicitly covers voluntary permission removal rather than falsely asserting a certificate change. The UI permits Done only with both grants.
6. Licences/reuse: OK; no added dependency or copied implementation.
7. Tests: bash scripts/test-release.sh passed using stubs and throwaway keys; selected Onboarding/App Vitest tests passed (138). Relaunch fallback does not reset failures or reinstall: it presses the real UI control and waits for permission controls to disappear; the scenario subsequently requires Done, preserved data and real dictation. It allows OS-dependent recovery via relaunch, so does not prove every grant reaches a running process. Full VM suite and real Developer ID notarization were not run by this reviewer; real tickets, service responses, mount capacity and Gatekeeper remain owner/tester evidence requirements.
8. Git/PR: no guard changes. Required CI jobs were pending when inspected. No merge performed. Review report committed locally; no push during this dispatch.

## Entitlements and owner documentation verification

The installed pinned tauri-utils source defaults hardened_runtime to true; this PR explicitly pins the config and tests it, and the release command overrides it to true and checks the resulting signature. Existing builds using that default were already hardened; this is not proof about arbitrary past configuration overrides. The pinned transcribe-cpp-sys 0.4.1 build script defaults to static linking, embeds the Metal library, and uses framework linkage for Apple services; no shared/dynamic-backend feature is enabled here. This supports retaining library validation; no reason to add a broad runtime exception was found. Runtime confirmation still belongs to the VM evidence; no built app was available here for independent otool inspection.

`xcrun notarytool store-credentials --help` was run without any credential operation: it confirms an omitted password receives a secure prompt when Apple ID and team ID are supplied. Apple's [Developer ID certificate guide](https://developer.apple.com/help/account/certificates/create-developer-id-certificates/) confirms the Account Holder role, Application versus Installer choice, CSR/download/import sequence, continued operation after certificate expiry, and revocation consequences. Apple's current notice confirms G2 and the timestamp claim, but contradicts the five-year promise (finding 1). The guide's categorical statement that membership lapse immediately prevents signing should also be qualified: Apple's account guide says a valid existing certificate remains usable, while a new certificate requires membership after expiry; notarization eligibility should be described separately.
