Reviewer: APPROVED

# U5 review, round 4

PR: #248 at becc70d8f1d8cd4301ffa577ec47292768de8696

## Scope and findings

Focused review of the main merge (12cb3fa, parents 4c216c2 and 0f28f4a), the follow-up e2e wording correction c8f33ce, and the report-only merge becc70d. No open blocker, major or minor findings.

Inspected the remerge diff and compared the merged tree against both the approved U5 tree and the main parent. The resolutions retain both sets of behavior rather than reinstating the old tray implementation.

## Merge checks

| Area | Result |
| - | - |
| U1 pill and Try it | Preserved: overlay files and Onboarding match the main parent, including sentence-only errors, no pill codes, and success only after a completed dictation beyond the step's baseline. `dictation_progress`, progress recording in the event loop, its binding and capability remain wired. |
| U1 Troubleshoot | Preserved: local readiness still replaces cloud connection testing in local mode; Relaunch Vocet and its failure message remain present. U5 adds Show log file and automatic report preview alongside them. |
| U4 meter and Paste sent | Preserved: overlay model/UI and event forwarding remain intact; level command, capability and shell state are retained alongside U5 commands. |
| P1 clipboard | Native pasteboard implementation matches main. The insert module differs only by U5's marked-copy adapter and test; P1's insertion/restore and shutdown behavior/tests are retained. |
| S1, U2, U3 | Launch logic, language picker, tokens, and their underlying files match main. Settings retains those controls, adding only U5 focus/setup behavior. |
| U5 menu and recovery | The approved menu ordering, fix mapping, honest Accessibility relaunch predicate, restored-listener Ready transition and recording icon precedence are retained. Normal problem sentences now use U1's `ErrorCode::info().message` directly. E2e expectations use the new sentences. |
| U5 privacy and icons | LastDictation remains memory-only; Copy still reaches `MacPasteboard::write_dictation` through `copy_marked`, with all privacy markers. Applier serialization and template status on every swap are unchanged. |
| No tray title | Old `show_in_tray` and its title setter were removed during resolution. No production tray title setter/builder remains. The new shell regression scans both tray_menu.rs and lib.rs for title calls, and U1's name integration test scans the new tray module too. |
| Commands and bindings | Merged lists/capabilities keep U1's progress/launch notice and U4 level command, replace the old Troubleshoot request with U5's window request, and add setup/log commands. No relevant main command was dropped. |

## Validation and limits

- `npx vitest run src/Onboarding.test.tsx src/Troubleshoot.test.tsx src/overlay/Overlay.test.tsx src/LanguagePicker.test.tsx src/styles/tokens.test.ts`: 115 passed across five files.
- `DICTATION_NO_LAUNCH=1 cargo nextest run -p dictation-pipeline -p dictation-app tray:: tray_menu:: vocet`: 35 passed, including the new no-tray-title regression.
- `DICTATION_NO_LAUNCH=1 cargo nextest run -p dictation-app --test vocet_name`: all three passed, including the user-visible-string scan and saved-settings/key rename test.
- Reviewed all conflict resolutions via `git show --remerge-diff 12cb3fa`, plus main-parent and approved-U5 comparisons.
- No VM/manual checks repeated in this focused merge review. No merge performed. Privacy, architecture, licences and Git checklist: no new concern in the reviewed delta; no dependency or guard change introduced by the resolutions.
