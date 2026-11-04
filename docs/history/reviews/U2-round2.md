Reviewer: CHANGES REQUESTED

# U2 review, round 2

PR: #249 at 3445bbc2518c5cba94df75c3bb1938a3d5e09456
Verdict: CHANGES REQUESTED

## Findings

| # | Severity | file:line | Finding | Suggested fix |
| - | -------- | --------- | ------- | ------------- |
| U2-4 | blocker | src/languages.ts:225 | OpenAI uses the same list as Groq, including `yue`, despite the app using OpenAI `whisper-1` rather than large-v3. The cited Groq/upstream-v3 language set does not establish OpenAI API acceptance of this new token. | Exclude `yue` for OpenAI unless provider evidence establishes acceptance; keep it for Groq. Test switching providers with a saved `yue`, preserving and flagging the value where unsupported. Replace the test requiring identical sets. |

[Inline finding](https://github.com/gargankit90/dictation-app/pull/249#discussion_r4464644885).

Evidence and limitation: [OpenAI's API announcement](https://openai.com/index/introducing-chatgpt-and-whisper-apis/) identifies the hosted Whisper API as large-v2. [OpenAI's large-v3 model card](https://huggingface.co/openai/whisper-large-v3) identifies the Cantonese token as new relative to v2. The [current speech guide](https://developers.openai.com/api/docs/guides/speech-to-text) describes the older Whisper language set separately from newer transcription models. The compatibility concern is an inference from these model versions, not a live API rejection reproduced by this reviewer; no secret or billable API was used. The source comment asserting acceptance of every code needs provider-specific evidence.

## Earlier findings

- U2-1: Bengali and Telugu are now offered and named. The original omission is fixed; U2-4 addresses the shared list's new model-version mismatch.
- U2-2: Fixed. An effect scrolls the active element into view on opening, navigation and option changes; tests assert the actual target element for arrows and End.
- U2-3: Fixed. Names and queries are normalized and combining marks removed consistently before word matching. Hindi full-name and vowel-sign prefixes pass; the earlier Hindi-versus-Chinese regression remains covered.

## Changed-scope checklist

1. Scope: rechecked only round 2 changes. Plain English/multilingual model wording matches the requested revision; helper selection and unsupported-value preservation remain intact.
2. Correctness: U2-4. Scrolling and native-name search corrections are sound.
3. Security/privacy: OK. No new request, analytics property or settings migration. The cloud language parameter path is unchanged.
4. Architecture: OK. UI-only changes; no new dependency or Rust changes in this round.
5. Rust/TypeScript practice: OK. The jsdom stub is test setup only. The scrolling assertion targets the active option.
6. Licences/reuse: OK. Language data is documented; no new dependency.
7. Tests: 205 tests passed in 11 files; TypeScript check passed. Provider-set equality is asserted rather than independently validated and needs the regression described in U2-4. No VM computer use or additional Rust test run for this UI-only revision.
8. Git/PR: Only this report is committed by the reviewer. No minors or debt issues. No merge; threads remain open because approval is withheld.

## Publication

Review file: [U2-round2.md](https://github.com/gargankit90/dictation-app/blob/u2-language-keycap/docs/history/reviews/U2-round2.md). Normal commit and push hooks apply; full gate results accompany the coordinator handoff.
