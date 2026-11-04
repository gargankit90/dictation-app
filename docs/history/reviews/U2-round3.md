Reviewer: APPROVED

# U2 review, round 3

PR: #249 at 3d20385a1879fa35483df56d123b693431109cd1
Verdict: APPROVED

## Findings

No open blocker, major or minor findings in the requested changed scope.

U2-4 is fixed: the OpenAI set excludes `yue`; Groq retains it. Tests assert the set difference and switch from Groq to OpenAI with saved Cantonese, checking both the unsupported label and unchanged `language: "yue"` after Save. Earlier scrolling and native-name fixes remain unchanged.

The e2e update replaces the old language-field assumptions with the displayed automatic/German labels. `combo-pick` uses the existing focused-field keyboard helper, types the search and presses Return, then checks the exact displayed label. The scenario still independently checks stored `de`, reopening Settings, and hotkey behavior. The existing AX field matcher accepts `AXComboBox`. No direct settings write substitutes for the picker interaction.

## Changed-scope checklist

1. Scope: OK; only U2-4 and the picker-related e2e update rechecked.
2. Correctness: OK; provider difference and retained unsupported value have regression coverage; the e2e assertions match the picker behavior.
3. Security/privacy: OK; no new analytics, secret access, request fields or transcript logging.
4. Architecture: OK; no dependency or Rust changes.
5. Rust/TypeScript practice: OK; provider mapping remains typed; e2e helper follows existing AX conventions.
6. Licences/reuse: OK; no new dependency or adapted code.
7. Tests: 28 targeted UI tests pass; TypeScript and JS/shell syntax checks pass. Real VM execution of the updated scenario remains the Tester's responsibility; no computer use was performed in this review.
8. Git/PR: Only this report is committed by the reviewer. No debt issues needed. Approval does not replace the current Tester PASS or Lead manual checks; no merge performed.

## Validation

- `npx vitest run src/LanguagePicker.test.tsx src/LocalModels.test.tsx`: 28 passed in 2 files.
- `npx tsc -b --noEmit`: passed.
- `bash -n tests/e2e/scenarios/e2e_settings_save_reopen.sh`: passed.
- `node --check tests/e2e/fixtures/ax.js`: passed.
- Normal push hooks apply; full gate outcome is included in the coordinator handoff.

Review file: [U2-round3.md](https://github.com/gargankit90/dictation-app/blob/u2-language-keycap/docs/history/reviews/U2-round3.md).
