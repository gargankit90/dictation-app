# MWEB1 review, round 1

PR: #339 at b8d80f3f7f0c44b871c9648937199cfdecee635a
Verdict: CHANGES REQUESTED

## Findings

| # | Severity | file:line | Finding | Suggested fix |
| - | -------- | --------- | ------- | ------------- |
| 1 | major | site/index.html:2118 | Closing Download still points at the unreleased DMG (HTTP 404), while the hero uses the releases page. It lacks `data-download`, so the documented single constant does not control both buttons. | Add `data-download` and make its fallback href the releases page, matching the hero. Verify both destinations before and after changing `DOWNLOAD_URL`. |
| 2 | major | site/index.html:2257 | Every nine seconds the script changes the selected radio and visible example, even while a visitor is reading or using that group. Reproduced with focus remaining on `t-ai` while `t-chat` becomes checked. There is no pause/stop control for the indefinitely cycling hero or examples; reduced-motion preference alone does not supply one. | Provide a visible, keyboard-accessible pause/resume control for the animations and auto-advance; stop advancing after manual tab selection and while the group has keyboard focus. Keep a stable selected example during interaction. |

## Checklist

1. Scope: findings 1–2. Reviewed all changed paths. The restored hero and temporary releases-page destination follow the later Lead decisions, overriding the initial static-hero spec. The milestone file was added by the Developer; treat it as the supplied Lead spec, with later decisions taking precedence.
2. Correctness: findings 1–2. Script executed through 100 deterministic timer callbacks without errors, with normal and reduced motion; reduced motion retains finished states. No-JS markup retains finished examples and radio selection via CSS.
3. Security and privacy: OK. Self-hosted fonts; no external scripts, tracking pixels, fetch/XHR/beacon calls. Per-mode routing, local History, and default-on usage/error reports with the Privacy switch remain accurate against the supplied brief.
4. Architecture: OK; static website only, no crate or IPC changes.
5. Rust and TypeScript practice: OK/not applicable; inline JavaScript checked with jsdom. Native named radios have accessible names, native keyboard selection, and visible focus styling; missing tab roles alone is not a defect for a radio-based selector. Finding 2 covers interference from auto-selection.
6. Licences and reuse: OK. FSL-1.1-ALv2 and Apache after two years wording preserved; no new dependency or copied third-party implementation identified.
7. Tests: limited. Used a temporary jsdom harness outside the repo to reproduce both findings. Attempted the existing token test, but this review worktree lacks npm dependencies and Vite could not resolve. CI checks were pending; no browser/VM visual or screen-reader test was performed.
8. Git and PR: PR notes describe the intended releases-page fallback, contradicted by finding 1. No guard changes. No minors raised.

The Accessibility FAQ still mentions the one-character spacing read; Copy last dictation and History recovery, per-mode privacy, licence wording, and Apple silicon/macOS 13 requirements remain present. Pricing and model names match the supplied owner brief; no retention periods, anonymity claims or leaderboard claims were introduced. No merge performed.
