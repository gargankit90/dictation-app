# R1 test report, run 1

PR: #190 at c16930a
Result: PASS (criteria 1-4 and hosted CI; 5 is the owner's manual step)

| # | Criterion | Test(s) | Result | Notes |
| - | --------- | ------- | ------ | ----- |
| 1 | Updater endpoint and allowlist | `cargo nextest run -p dictation-app updater` (25/25); 5 local mutations of `package_url_problem`; 24 own look-alike URLs | PASS | Endpoint in `tauri.conf.json` is asserted by the shipped-config test (`assert_eq!(endpoints, [url(ENDPOINT)])`). |
| 2 | `release.sh` publishes here, at the built commit | `scripts/test-release.sh` (fake tools, local bare "origin"); 7 local mutations of `release.sh` | PASS | Two minors below. No real publish, no tag, no Keychain read. |
| 3 | Docs | `git grep -n dictation-app-releases` | PASS | See below. |
| 4 | No key-handling change; public key unchanged | `git diff origin/main...HEAD` on `tauri.conf.json`; decoded the key | PASS | Only the endpoint line changed. Key id `F6C80F173A96D78F`. |
| 5 | Hosted CI | `gh pr checks 190` | PASS | Quality gate (Linux) pass 7m16s, DMG (macOS) pass, Platform crate (macOS) pass. |

## Criterion 1

Mutations of `src-tauri/src/updater.rs` (each restored with `git checkout`; tree clean after):

| Mutation | `a_package_must_be_https_on_this_repos_release_downloads` |
| -------- | ------ |
| also accept `/gargankit90/dictation-app-releases/` | fails |
| drop the https scheme check | fails |
| drop the `%2f` / `%5c` / `..` check | fails |
| prefix without `download/` | fails |
| drop the host comparison | fails |

My own look-alike URLs (a temporary test, not committed). Rejected: `dictation-app-releases`; `dictation-app.evil.com` host; `github.com@evil.com`; `evil.com/?https://github.com/...`; `http://`; `/download/../x`; `%2e%2e` (resolved by `Url`, ends outside `download/`); `%2E%2E%2F`; `%2f`; `%5C`; other owner (`GargAnkit90`, `someoneelse`); repo case (`Dictation-App`); `/releases/download` without a trailing path; `/releases/downloadx/`; `releases?/download`; `releases#/download`; `foo?/releases/download`; `//` double slash; `./releases/../archive/x`; trailing-dot host; `github.com:443@evil.com`. Accepted (correct): the real asset URL, uppercase host `GITHUB.com` (the parser lowercases it), and a query/fragment on a good path.
One note, not a defect: a literal `\` in the path (`download/v1\..\a`) is turned into `/` by `Url` and resolves to `/releases/download/a`, still inside this repository's download path, so it is accepted harmlessly.

## Criterion 2

`scripts/test-release.sh` passes (all 6 cases). It refuses, before the Keychain, `npm` or `gh`: a dirty tree, a commit not on `origin/main`, a local tag `v<version>` and (with the tag deleted locally) a tag only on origin; each with the "tags are immutable ... needs a new version" message. The success case asserts `gh release create v<version> --repo gargankit90/dictation-app --target <HEAD sha>`, no `dictation-app-releases` in the gh call, and `latest.json` URLs under this repository.

Mutations of `scripts/release.sh` (restored; clean):

| Mutation | Result |
| -------- | ------ |
| drop the `merge-base --is-ancestor` check | caught ("passed with an unmerged commit") |
| drop the local-tag check | caught |
| drop `--target` | caught |
| `RELEASES_REPO` back to the old repository | caught |
| drop the clean-tree check | caught |
| drop the remote-tag (`ls-remote`) check | **not caught** (minor 1) |
| drop the "tree or HEAD changed during the build" guard | **not caught** (minor 2) |

Minors (not blockers):
1. The remote-tag check is shadowed: `git fetch origin` earlier in the script pulls an origin-only tag in as a local tag, so the local check refuses first, and the test can't tell the two apart. Harmless today (either way the publish is refused); the `ls-remote` line is effectively redundant, or the test should fetch without tags to prove it.
2. The mid-build "HEAD changed" guard has no test.

## Criterion 3

`git grep -n dictation-app-releases` outside `test-reports/`, `reviews/`, `docs/milestones/`:
- `docs/ENGINEERING.md:71` and `docs/releasing.md:25`: "archived and was never used for a release".
- `scripts/test-release.sh:274`: negative assertion that `gh` is not pointed at it.
- `src-tauri/src/updater.rs:805`: the rejected-URL test case.
No other mention. `docs/TESTERS.md:14` links `https://github.com/gargankit90/dictation-app/releases/latest`; README has no release link. `docs/releasing.md` shows the new endpoint. (Two other files say "never the releases repository" in a test-harness note, `tests/e2e/README.md:219` and `e2e_update_keeps_permissions.sh:2`; those don't name the old repo and are still accurate.)

## Not done

Did not run the full gate (CI and the push hook do). Did not run `release.sh --publish` for real, create a tag or release, or touch the Keychain items.
