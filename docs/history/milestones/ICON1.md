# ICON1: App icon uses the Vocet mark

Owner decision, 2026-11-20: replace the placeholder app icon (purple rounded square with a white dot,
`src-tauri/icons/`) with the Vocet brand mark used on the website and on Hugging Face: the blue `#1F4FC9`
rounded square with three white sound bars. Source SVG (from the site favicon):
`/Users/gauri/vocet-screens/u9/brand/vocet-logo.svg`.

## Scope
1. Make a 1024×1024 PNG master **with a transparent background**, following macOS app-icon layout (artwork
   about 824×824 centred, corner radius matching the macOS icon grid, a subtle shadow only if it matches
   Apple's template; no white corners). Commit the SVG master next to the icons (e.g.
   `src-tauri/icons/source/vocet-icon.svg`).
2. Regenerate every icon Tauri uses (`icon.icns`, `icon.ico`, `icon.png`, `32x32.png`, `128x128.png`,
   `128x128@2x.png`, and any others `tauri.conf.json` lists) with `npx tauri icon` from that master.
3. Do not change the menu-bar tray icons (`src-tauri/icons/tray/`, template images) unless they still show the
   old dot; if they do, note it in the PR and leave them for the Design Lead.
4. Use the same mark for anything else that shows the app icon (e.g. the DMG background or the website
   favicon already matches; check the onboarding/About logo).

## Acceptance criteria
1. [auto] Gate passes; bundle builds with the new icons.
2. [manual] Lead: the Dock, Finder, About window and DMG show the blue Vocet mark; it looks right next to
   other macOS app icons (no white corners, correct size).

Developer: Sonnet 5.5. Time box: 1 hour. Small, independent of Q3/Q4.
