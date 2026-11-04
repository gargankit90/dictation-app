Reviewer: CHANGES REQUESTED

# U6 review, round 2

PR: #262 at 04dfa12354989a23c71833af7e0b93355fdb02f8
Verdict: CHANGES REQUESTED

## Findings

| # | Severity | file:line | Finding | Suggested fix |
| - | -------- | --------- | ------- | ------------- |
| R1-1 (remaining lifecycle case) | major | src/settings/Speech.tsx:224 | The generation guard fixes edits/removal within one mounted Speech pane, but each mount owns a different keyRun ref and no cleanup invalidates the old one. With an existing key, start Test & Save for A, switch to General while the provider test is pending, then return to Speech and Remove key. That new pane deletes the key and reports success; when the old test succeeds, its captured run still equals its old ref, so it writes A at line 228 and silently restores a key after removal. Saving B in the new pane instead likewise allows A to overwrite B. Settings renders one pane at a time (Settings.tsx:326), so normal sidebar navigation reaches this case. | Invalidate pending tests on unmount and ensure any already-started key write remains serialized with mutations from the next mount. Alternatively hold operation ownership/generations above the pane lifecycle. Add a delayed-test → leave Speech → return → remove/new save regression test. |

## Earlier findings

- Round 1 finding 1: partially fixed. The generation check before setApiKey correctly rejects obsolete tests after edits/removal/new saves within the same mount, and the new tests cover those cases. The lifecycle case above remains, so keep that thread open.
- Round 1 finding 2: fixed in 04dfa12. A failure from write_change now rebuilds from stored settings. The new Unix test makes the settings directory unwritable, switches between downloaded local models, and checks that the stored model and warm/live engine return to the old selection.

## Focused re-review

- Resume: OK. The new action routes to downloadLocalModel, as Retry already did. The existing ModelStore reads the part size from disk, so a fresh process can resume; it uses HTTP Range, verifies the entire completed part against the pinned SHA-256 before renaming, removes corrupt parts, handles servers ignoring Range by truncating/restarting, and checks remaining bytes plus headroom before fetching. Existing tests exercise resume from a pre-existing part, ignored ranges, corrupt/oversized files, and insufficient space. No download-backend change in this round.
- Needs a key: presence reads show Checking while pending, Needs a key when absent, and Active after successful saving. Successful save invalidates older presence reads, and remove starts a fresh presence lookup. No additional blocking chip-flapping issue found in the changed paths; cross-mount key mutation remains the major above.
- Copy: local-audio wording, English/Multilingual titles, and the language count now derive from language_codes. The paused state has Resume. The e2e language assertion checks the stored value.
- Scope/security/architecture/practice/licences/Git: reviewed only changes since round 1; no additional blocker or major found beyond R1-1. Criteria 7 and 7b remain the expressly pending U5 integration. No product files or tests edited by Reviewer.

## Validation and disposition

Targeted Vitest: 63/63 passed (Settings, App, modelRow). Source-traced the cross-mount scenario above; the added tests do not unmount/remount Speech during the pending test. Full quality gate runs through the mandatory pre-push hook; VM and manual evidence remain Tester/Lead responsibilities.

0 blockers, 1 remaining major, 0 minors. Changes requested; no merge performed. After two rounds, Lead should coordinate the remaining lifecycle fix under the repository workflow.
