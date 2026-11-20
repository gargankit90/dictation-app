# LIC1: Relicense Vocet from MIT to FSL-1.1-ALv2 (Fair Source)

Owner decision, 2026-11-20. Use the licence Sentry chose: the Functional Source License, version 1.1,
with the Apache 2.0 future licence (**FSL-1.1-ALv2**, SPDX id `FSL-1.1-ALv2`). Every commit on `main`
is the owner's (one `init` commit by a placeholder identity, also the owner's); no outside PR was
merged, so the owner can relicense the whole repository.

## Why

MIT lets anyone repackage and sell Vocet. FSL keeps the source readable, usable, modifiable and
self-hostable, but forbids a **Competing Use** (a commercial product or service that substitutes for
Vocet) for two years per release, after which each release becomes Apache-2.0. Versions up to and
including 0.1.0 (`cfe583a`) were published under MIT and stay MIT; say so plainly.

## Scope

1. `LICENSE`: replace with the verbatim FSL-1.1-ALv2 text from https://fsl.software (the template's
   placeholders filled: Licensor "Ankit Garg (Vocet Labs)", Software "Vocet", copyright year 2026).
   Do not edit the licence wording.
2. Metadata: `Cargo.toml` `license = "FSL-1.1-ALv2"` (crates inherit via `license.workspace`);
   `package.json` `"license"`; `src-tauri` bundle copyright/licence fields if present; the generated
   `src/bindings.ts` comment comes from Rust, so change the Rust source and regenerate.
3. Words: "open source" → "source available" / "Fair Source" wherever it describes **Vocet's own
   licence**: `README.md` (licence section; add "Versions up to 0.1.0 were released under MIT"),
   `CONTRIBUTING.md` (contributions are licensed under FSL-1.1-ALv2), `site/index.html` (meta
   description and the "Free, open source, and yours to inspect" section: e.g. "Free, source
   available, and yours to inspect"; "FSL licence, Apache 2.0 after two years"),
   `src/settings/HelpAbout.tsx` (Licence row). Keep "open-source" where it describes **third-party**
   software (credits, `docs/TESTERS.md`).
4. Keep `NOTICE`, `THIRD-PARTY-LICENSES.txt` and the third-party licence checks unchanged in substance:
   the MIT/Apache code we adapted keeps its notices. Update `about.toml` / `deny.toml` comments that say
   "Our own crates are MIT"; make sure `cargo deny` and `scripts/check-licences.sh` still pass with our
   private crates under FSL (they are `publish = false`; add `FSL-1.1-ALv2` to an allow list only if a
   check needs it for our own crates, never for dependencies).
5. `AGENTS.md` licence rule and `docs/ENGINEERING.md` licence notes: our licence is FSL-1.1-ALv2; the
   rule about adapting code only from MIT/Apache repos stays.

Out of scope: rewriting git history, changing release 0.1.0, any change to guard files.

## Acceptance criteria

1. [auto] `LICENSE` is the unmodified FSL-1.1-ALv2 template with the three placeholders filled
   (diff against the fsl.software text shows only those).
2. [auto] `git grep -n -i "MIT"` on non-third-party files finds no claim that Vocet is MIT, except the
   sentence that versions up to 0.1.0 were MIT. Cargo and npm metadata say `FSL-1.1-ALv2`.
3. [auto] `scripts/gate.sh` passes, including `cargo deny` and the licence-notice check.
4. [auto] Settings → Help & About shows the FSL licence line (unit/component test updated).
5. [manual] The Lead reads the site's licence section and the README licence section.

Developer: Sonnet 5.5. Small change; time box 1.5 hours.
