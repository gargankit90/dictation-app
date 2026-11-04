Reviewer: CHANGES REQUESTED

# U2 review, round 1

PR: #249 at 046fd1bd6294253b3bae1334f930941f9d0a5272
Verdict: CHANGES REQUESTED

## Findings

Severity follows the checklist: incorrect user-visible behaviour is a blocker.

| # | Severity | file:line | Finding | Suggested fix |
| - | -------- | --------- | ------- | ------------- |
| U2-1 | blocker | src/languages.ts:74 | The shared 57-code cloud list omits supported Groq languages such as Bengali (`bn`) and Telugu (`te`). These cannot be chosen, and existing values are incorrectly flagged unsupported. | Use verified provider-specific sets and names; test omitted Groq languages and provider switching. |
| U2-2 | blocker | src/LanguagePicker.tsx:89 | Arrow keys and End change the active option without scrolling the 220px list. Keyboard targets beyond the visible area, including a saved selection near the end on opening, remain invisible. | Keep the active option visible on opening and navigation; add regression coverage. |
| U2-3 | blocker | src/languages.ts:159 | Native-name search splits combining marks out of words. Searching the displayed Hindi name `हिन्दी` produces no match: its split words are `ह`, `न`, `द`. Other Indic native names have the same problem. | Preserve Unicode marks within words and test native names and prefixes containing marks. |

Inline findings: [U2-1](https://github.com/gargankit90/dictation-app/pull/249#discussion_r4464328608), [U2-2](https://github.com/gargankit90/dictation-app/pull/249#discussion_r4464328800), [U2-3](https://github.com/gargankit90/dictation-app/pull/249#discussion_r4464328982).

Provider evidence: [Groq model documentation](https://console.groq.com/docs/model/whisper-large-v3-turbo) documents 99+ languages; [upstream Whisper language table](https://github.com/openai/whisper/blob/main/whisper/tokenizer.py) includes Bengali and Telugu. The OpenAI published quality subset should not automatically restrict Groq.

## Checklist

1. Scope: U2-1 through U2-3 prevent criteria 3/4 from being fully met. Keycap CSS follows the specified dimensions. Manual light-mode evidence remains for the Lead; the PR supplies dark-mode screenshots only.
2. Correctness: U2-1 through U2-3. Settings still loads the saved string, preserves unsupported values through Save, and saves null for automatic. Model selection derives choices from the current catalogue; the helper selects the multilingual model. No settings migration. Cloud settings-to-config and multipart `language` paths are unchanged.
3. Security and privacy: OK. No telemetry enum, property, network destination, capability, secret, audio or clipboard changes. `language_codes` is catalogue IPC data, not an analytics property.
4. Architecture: OK. Rust copies `LocalModel::languages` into the thin IPC view; no new crate dependency, unsafe code or platform conditional.
5. Rust and TypeScript practice: OK apart from the search correctness finding. Typed generated bindings include the new field; no new suppression or panic path.
6. Licences and reuse: OK. No new dependency or declared adapted code.
7. Tests: Existing UI suite passes (201 tests in 11 files), including recorder and Settings tests covering pause, single writer and Save while listening. TypeScript check passes. New tests cover common search, arrows, Escape, automatic/null, stored unsupported values and model filtering, but miss the three findings. Rust catalogue test compares every returned language-code list with the core catalogue. VM e2e is delegated to the Tester; no computer use performed in this review.
8. Git and PR: PR notes describe implementation, tests, risks and screenshots. Rust language codes are included in the style commit, as disclosed; no functional defect in that mapping. No guard modifications. No minor findings requiring debt issues. No merge performed.

## Validation

- `npm ci`: succeeded.
- `npx vitest run`: 201 passed, 11 files.
- `npx tsc -b --noEmit`: passed.
- Direct Node reproduction of the native-name split: confirmed the Hindi-name failure.
- `DICTATION_NO_LAUNCH=1 cargo test -p dictation-app criterion_4_lists_the_catalogue_with_statuses`: passed (1 test).
- The normal push hooks run when publishing this documentation commit; CI remains authoritative for the full gate.
