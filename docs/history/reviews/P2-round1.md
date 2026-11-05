Reviewer: APPROVED

# P2 review, round 1

PR: #268 at d2126b42fe272626efa7564eedc830eb384e43a5
Verdict: APPROVED

## Findings

No blocker, major, or minor findings.

## Focus review

- Tauri's `WebviewAttributes::from(&WindowConfig)` forwards `focus` to the
  webview. In wry 0.57.1, application activation and initial first-responder
  selection require `attributes.visible && attributes.focused`. A hidden
  native window alone does not suppress the webview's activation request.
  The explicit config flag removes that request.
- `window::show` retains title, unminimize, show, and native-window focus,
  then focuses the webview. `WebviewWindow::set_focus` targets the native
  window; its `AsRef<Webview>` implementation makes the added call target
  the webview instead. On macOS, wry's webview focus makes it first
  responder. Calls remain ordered through the existing error chain, with
  failures logged. This restores keyboard delivery without an initial
  mouse click to establish the webview as first responder. It does not
  promise automatic selection of a particular Settings input.
- Settings, onboarding (both first-run and tray/relaunch entry points),
  and troubleshooting use the same `main` native window. `App.tsx` switches
  their React views; there are no separate hidden onboarding or
  troubleshooting windows to activate. U6's sidebar DOM focus is unchanged,
  as is onboarding's explicit focus on its Try text area.
- The only other production native window is the overlay. Both platform
  builders already use `focused(false)` and `visible(false)`; macOS also
  uses `no_activate(true)`, `focusable(false)`, and a nonactivating panel.
  Test-only window builders are not launch paths.
- `menu_bar.rs` changes documentation only. The note is useful and correct:
  tao's launch activation policy and wry's webview activation are separate
  paths, so future maintainers must preserve both protections. No runtime
  menu-bar change is needed.

## Validation

- `cargo nextest run -p dictation-app window::`: all four window tests passed.

- Settings, onboarding, and troubleshooting Vitest suites: 99 tests passed
  across three files, including sidebar keyboard navigation.
- Evaluated the regression predicates against the parent revision: the
  config lacks `focus: false`, and `show` lacks the webview focus call, so
  each new check rejects the corresponding old input. The config test
  directly protects the effective flag; the source-text test narrowly
  protects the two focus calls and their order. Neither simulates AppKit
  activation or proves keyboard delivery in a real window.
- Native no-click typing, delayed launch activation after tray Quit, and
  first-run onboarding remain VM/Lead validation. This review did not run
  a VM or use the owner's screen. The PR describes before/after VM evidence
  and explicitly discloses its stale onboarding scenario and untested login
  launch/reboot cases; no independent VM result is claimed here.

## Follow-ups

The four follow-ups in the PR are reasonable separate work for the Lead:
repair the already-failing onboarding scenario, investigate Copy last
Dictation's marker/snapshot interaction, verify whether TextEdit's second
activation workaround is now redundant, and add a permanent post-tray-Quit
launch-focus VM scenario. The clipboard issue deserves tracking because it
may affect user clipboard recovery, but this diff neither introduces nor
changes that path. Do not remove the TextEdit workaround without measuring
it. The missing VM scenario is explicitly disclosed; the config assertion
protects this root cause while behavioral coverage would catch future
activation paths. These are existing or broader follow-ups, not new minor
findings against this patch.

## Checklist

1. Scope: OK. Issue #266 is the brief; no milestone spec. The three PR
   criteria map to the root-cause evidence, fix/regression checks, and
   reported secure-input VM runs respectively.
2. Correctness: OK. Explicit show restores both native and webview focus;
   hidden creation stays unfocused. No pipeline/state/cancellation changes.
3. Security and privacy: OK. No capabilities, CSP, logging payload, network,
   keychain, audio, telemetry, or clipboard changes.
4. Architecture: OK. Config and thin Tauri shell only; platform edit is a
   doc note. No new dependencies, unsafe, or conditional compilation.
5. Rust and TypeScript practice: OK. Existing error propagation retained;
   no new blocking operation, production unwrap, or suppression.
6. Licences and reuse: OK. No dependency or copied-code changes.
7. Tests: OK, with the native-behavior limitations described above.
8. Git and PR: OK. One Conventional Commit includes the tests; notes cover
   cause, risks, validation, and follow-ups. No protected files changed.

Approval is a code-review verdict, not merge authorization. Current-head
Tester PASS and the Lead's manual-check record are still required.
