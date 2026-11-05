Reviewer: CHANGES REQUESTED

# U9 review, round 1

PR: #285 at 4a19a2eb9650bc8a24360a1db3adadb69e0e0b82

## Findings

| # | Severity | file:line | Finding | Suggested fix |
| - | -------- | --------- | ------- | ------------- |
| 1 | major | crates/core/src/report.rs:142; src/Troubleshoot.tsx:339 | Criterion 2 explicitly forbids raw key specs in user-facing text anywhere. The diagnostics report deliberately renders `hotkey: RightAlt`, and Troubleshoot previews that exact report before copying, saving or submitting it. A support field is still user-facing here; its purpose does not create an exception to the criterion. | Render a validated keycap label in the visible report, preserving the report's closed-field/privacy guarantees, and test the preview for modifier and combination keys. Alternatively the Lead must explicitly change the acceptance criterion to exempt the report before approval. |

## Focused review

- **Save semantics:** `preferences::write` still propagates task, validation, config-read, model-load and file-write failures. Its four callers change language/cloud/local engine/model, never request a new shortcut. Dropping the incidental hotkey outcome therefore removes the setup blocker without masking failure to save that preference. `set_hotkey` and `save_settings` still inspect `Saved::problem`; `Kept` remains an error and failed file writes still attempt rollback and report rollback failures. `saveHotkey` rereads actual settings even on error. Settings' existing save error UI remains connected. No caller needs the former incidental `NotListening` error for control flow.
- **Notice:** the backend reads state and delivery with the established lock order and clears the notice when the listener recovers. Settings fetches on mount, after shortcut saves and every two seconds. Overlapping requests can transiently display an older snapshot, but subsequent polls converge; no persistent stale value or save failure is introduced. Initial absence until the first failed attempt and polling latency are acceptable.
- **Focus:** the recording ref guards callbacks, including async save completion and window focus. The recorder ends on blur and its cleanup can call DOM element focus while the window is inactive; this code uses no native window activation or app-focus API. Real cross-app activation and first dictation remain VM/manual evidence, not proven by jsdom. The failed-paste panel is revealed after focus and resize; long text scrolls within its capped area.
- **Chip/CSS:** the original hover selector outranks the old selected selector. The new selected hover and active rules outrank the generic button states and use light/dark tokens. Changes are scoped to selected onboarding chips, picker inputs, the notice and failed-paste words; unrelated buttons retain their rules. The browser harness exercises the actual cascade with equivalent-specificity classes, including combined hover/active/focus states.
- **Picker:** combobox/listbox roles, labels, active descendant, arrow/Home/End navigation, Enter selection, Escape and Tab dismissal remain. Scrolling only the list for the active row avoids hiding the input; the chevron is decorative and does not intercept input.
- **No-permissions e2e:** the scenario resets settings/models, revokes Accessibility, resets Microphone and checks that the real listener is down before choosing Hindi and the download route. On the old code language/save reports the hotkey error and download never starts, so its assertions exercise the regression. No download fixture exists; cancelling a real download is disclosed. The runner restricts mutable VM targets to clones, with the golden image only a clone source. Its EXIT cleanup first quits the app, then restores Accessibility and Microphone through existing helpers, resets settings and deletes model files; cleanup failure fails the scenario. Each scenario runs in a subshell, so permission flags cannot leak from another scenario. I inspected this path but did not independently run the VM scenario or reproduce its red run.
- **Diagnostics verdict:** **not acceptable under criterion 2 as written**, regardless of its intentional support purpose. This is a scope/acceptance failure, not a privacy leak.

## Checklist

1. Scope: finding 1; other criteria map to code and tests in the PR notes.
2. Correctness: OK for inspected paths, with native focus verification left to Tester/Lead.
3. Security and privacy: OK; new capability is read-only and confined to the existing settings window. No new secrets, logs, telemetry, networking or audio persistence.
4. Architecture: OK; no dependency, unsafe or target-specific changes.
5. Rust and TypeScript practice: OK; typed IPC updated consistently, blocking notice lock moved off the event loop.
6. Licences and reuse: OK; no new dependency or adapted external code.
7. Tests: focused UI suite passed (150 tests in Onboarding, LanguagePicker and Settings). Rust hotkey integration suite passed (24 tests); preference suite passed (15 tests). TypeScript check and scenario shell syntax check passed. Browser/VM evidence in PR notes was inspected, not independently reproduced.
8. Git and PR: OK; scoped commits and detailed notes, no guard changes. Milestone-file permission was explicitly delegated by the Lead per PR notes. No merge authorized by this verdict.
