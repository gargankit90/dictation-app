Reviewer: APPROVED

# U3 review, round 1

PR: #245 at 92ab90bb9dc15e9ed8c19690ac62f951e4b12881.
Verdict: APPROVED — 0 blockers, 0 majors, 2 minors tracked below. Do not merge on this review alone; Tester and Design Lead evidence remain separate requirements.

## Findings

| # | Severity | file:line | Finding | Suggested fix / tracking |
| - | -------- | --------- | ------- | ------------------------ |
| 1 | minor | src/overlay/overlay.css:28 | Horizontal pill padding grows from 14 to 16 px per side, so a short recording pill grows 4 px wider; constrained messages lose 4 px of content width. Radius also changes from 18 px to pill radius. This departs from criterion 4's visual preservation but does not change native bounds or focus behavior. | Preserve geometry with approved overlay tokens or explicitly accept it in design review. [Issue #246](https://github.com/gargankit90/dictation-app/issues/246). |
| 2 | minor | src/settings.css:177 | Destructive, group/group-row and chip selectors are defined but no TSX element uses those classes. Delete remains secondary; the PR notes claim a destructive class was added. No action or state behavior changes. | Connect appropriate elements during UI work and correct the PR notes. [Issue #247](https://github.com/gargankit90/dictation-app/issues/247). |

Both inline threads were resolved after recording the follow-ups, as requested.

## Evidence and acceptance criteria

- Scope comparison: the branch and main have multiple merge bases because previously reviewed milestones and site work were merged concurrently. Inspected the U3 commits and the direct main-to-head diff to avoid treating already-landed K1/F1/R2 changes as U3 changes. The review document on main missing from this older branch is not a U3 deletion commit.
- AC1: the app token file was byte-identical to the approved local DS1 source at initial inspection after removing its required first-line source header. The Design Lead subsequently added three overlay-only tokens locally (22:10:59) and requested them in [Design Lead: CHANGES](https://github.com/gargankit90/dictation-app/pull/245#issuecomment-6623992614); they are absent at the reviewed head. That newer design requirement and the appearance-dependent warning badge remain part of the Lead approval gate, not a claimed match to the revised file. Both entry points import it once. The new lint covers CSS and quoted TS/TSX hex literals, with one documented exception list. It passes.
- AC2: settings and overlay reference shared colors, type, and spacing; app font remains the system stack. The dark media query overrides the same tokens and no settings rule overrides those colors. No Rust changes, Tauri capability changes, command changes, event changes, or application TSX logic/copy changes in the effective U3 diff; the only application TSX changes are CSS imports. Some control geometry retains dimensional literals (for example switch knob size), not color literals.
- AC3: button variants, focus rings, switch and keycap styling exist. The switch remains a native checkbox with unchanged semantics. Component integration gaps are minor finding 2.
- Contrast: calculated sRGB relative luminance and WCAG contrast for the actual token pairs. Light/dark respectively: ink/bg 17.21/14.76, secondary/bg 11.39/11.49, muted/bg 6.89/6.87, muted/grouped 6.26/6.25, primary text/accent 6.95/7.35, primary hover 9.78/9.75, ready/ready-soft 4.72/6.24, attention/attention-soft 5.22/7.98, danger/danger-soft 4.92/5.62, accent/accent-soft 5.98/5.94. All exceed 4.5:1 for enabled main text styles; disabled control opacity is intentional.
- AC4: native overlay dimensions and non-activating panel implementation are unchanged. Overlay markup, lifecycle and events are unchanged. Pill geometry caveat is minor finding 1. Background remains transparent outside the pill.
- AC5: inlined token definitions match the original DS1 values; the later overlay-only additions above are not present. The site intentionally pins its used neutral/accent values to light mode. The complete site body and scripts are byte-identical to current main, so copy, links and demo behavior are preserved. Existing site illustration colors remain outside the app's hex lint scope.
- Mobile CSS: both 300 px grid minima now shrink to 100% of available content width at 320 px; nav links receive minimum width 44 px alongside their existing minimum height 44 px; brand, footer and data-card links receive 44 px height; narrow layouts hide the nav CTA and use two-column tabs, and chat bottom padding increases to separate the Pasted badge. These are real layout rules, not overflow hiding. Runtime viewport evidence belongs to the parallel Tester.
- AC5b/licences: all 14 WOFF2 sources use relative fonts/ URLs and exist; all faces use font-display: swap. Inter includes 400/500/600/700 and Tight includes 600/700/800 in Latin and Latin-ext (600 preserves an existing used weight). Both full SIL OFL 1.1 texts and copyright notices are in site/fonts/LICENSE.txt, with NOTICE, THIRD-PARTY-LICENSES.txt and site/README credits. Removed Google font links; no remote font/import/script/image request was introduced. The existing GitHub releases API lookup remains the only third-party automatic request. Outbound links are navigation, not page-load requests.
- Reuse: styling follows the supplied local DS1 source; no suspicious new implementation or copied GPL/company-page content found. Site prose and scripts are unchanged.
- AC6: app transition durations become zero under reduced motion; both overlay animations are explicitly disabled. Site retains its existing reduced-motion handling.
- AC7/screenshots: PR notes reference local screenshots rather than embedded before/after images. The Design Lead has explicitly requested branch synchronization/conflict resolution, stable overlay warning colors, and screenshots before approval. VM comparison and Design Lead approval must be supplied/confirmed by the Lead before merge; this static review does not claim to perform them.

## Checklist

1. Scope: OK, with two tracked minor presentation deviations above; manual evidence remains the Lead's gate.
2. Correctness: OK; no pipeline, state-machine, cancellation, persistence or input-handler changes in U3.
3. Security and privacy: OK; no new keys, logs, analytics, capabilities or remote destinations. Font requests become local.
4. Architecture: OK; no dependency, platform, unsafe or coordinator changes in U3.
5. Rust and TypeScript practice: OK; styling imports and tests only, no new suppression or untyped IPC.
6. Licences and reuse: OK; font OFL notices and credits included, no new executable dependency.
7. Tests: all 11 frontend test files / 193 tests pass; production TypeScript/Vite build passes. New token checks pass. Full repository gate attempted separately; see validation note below. VM checks delegated to the parallel Tester, not repeated here.
8. Git and PR: Conventional U3 commits; no guard changes. Spec was added in its own documentation commit before implementation. Both minors have tech-debt issues and inline comments. No merge performed.

## Validation note

scripts/gate.sh passed all checks through documentation links, including Rust lint/dependencies/licences, workspace and platform tests, doctests, coverage, vendor tests, TypeScript/ESLint/Vitest/build, spelling, 496 guard tests and hosted-workflow checks. Its process then exited with signal 143 while starting scripts/test-e2e-main.sh; this is an interrupted run, not a claimed full-gate pass. The remaining four shell checks were run separately before pushing; pre-push hooks were left enabled.
