# ICON1 test report, run 1

PR: #342 at 9ac8f80
Result: PASS

| # | Criterion | Test(s) | Result | Notes |
| - | --------- | ------- | ------ | ----- |
| 1a | Every icon in `tauri.conf.json` exists | file/ls | PASS | 32x32, 128x128, 128x128@2x, icon.icns, icon.ico |
| 1b | Regenerated from the committed SVG master | inspect | PASS | `source/vocet-icon.svg`: 824px artwork on 1024 canvas; icons show blue `#1F4FC9` (31,79,201) |
| 1c | Transparent backgrounds | PIL alpha check | PASS | All four corners alpha 0 in the PNG files and every icns size; 128/256/512 centre pixels white (bars) |
| 1d | Sizes | `file`, iconutil | PASS | 32, 128, 256 (@2x), 512 PNG files; icns has 16–1024; ico has 6 sizes |
| 1 | Debug bundle builds with new icons | `npm run tauri build -- --debug --bundles app` | PASS | `Vocet.app` built; Info.plist `CFBundleIconFile=icon.icns`; bundled icns byte-identical to committed one |
| 1 | Gate passes | `scripts/gate.sh` | PASS | all checks passed; CI also green (Linux, macOS, DMG) |
| 2 | [manual] Dock/Finder/About/DMG | n/a | Lead | not Tester's check |

## Gate
All checks passed locally; CI green on the PR.

## Untested behaviour found
Artwork bbox is 47–465 of 512 (~81.6%) vs spec ~80.5%: within antialiasing/tolerance. Not checked: tray icons, onboarding logo (outside the stated Tester scope).

## Failures
None.
