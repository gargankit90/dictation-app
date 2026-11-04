Reviewer: APPROVED

# CF1 review, round 1

PR: #264 at 86f6369b50be2f10a630ccb432f43ae4d6b0b7c6
Verdict: APPROVED

## Findings

No blocker, major or minor code findings.

## Checklist

1. Scope: OK. Only the static-site deployment config, exclusions and deployment documentation change; acceptance criteria are in the PR notes (no separate CF1 milestone file).
2. Correctness: OK. The `vocetlabs` Worker explicitly retains custom domains `vocetlabs.com` and `www.vocetlabs.com`, and `workers_dev: true`. Static assets resolve from the config's directory. The root `.assetsignore` excludes `README.md`, `wrangler.jsonc`, itself and the entire `.wrangler` directory through gitignore semantics. Public HTML, fonts and link-preview image remain included.
3. Security and privacy: OK. No token, secret, application telemetry or workflow change. Production branch selection and disabled non-production builds are dashboard controls, as stated in the PR; `preview_urls: false` alone does not disable branch builds. With the stated dashboard setup and protected `main`, fork PR code cannot trigger a production deploy. Dashboard settings were not independently inspected.
4. Architecture: OK; static-site configuration only, no crate or Tauri changes.
5. Rust and TypeScript practice: OK; no Rust or TypeScript changes, no suppressions.
6. Licences and reuse: OK; no dependencies or adapted application code.
7. Tests: OK for this configuration change. Wrangler 4.157.0 `deploy --dry-run` succeeds with no bindings and without login or deployment. Prettier accepts JSONC trailing commas; `typos site/` and `scripts/check-doc-links.sh` pass. Wrangler's installed asset-ignore implementation reads this file using gitignore-style matching and always excludes `.assetsignore` itself. No product test harness added.
8. Git and PR: OK. Small Conventional Commit, complete PR notes, no guarded-file changes. This review changes only this report.

## Deployment evidence and limits

The README matches Cloudflare's documented static-asset exclusions, production branch builds, watch-path settings and domain configuration. Branch and watch-path controls live in the dashboard, not this config. The Lead should retain the PR's post-merge checks of both domains, excluded-file 404s and retained domain bindings; dry-run does not test the live routing or upload assets.

At review time, GitHub reports `Workers Builds: vocetlabs` as failed, linking build `c00306cb-9cf7-4e8b-9c67-d9ba063eb79f`; its check output supplies no failure reason. The Lead subsequently explained that this was a preview-branch build with a stale `npm run build` command; the owner has now disabled preview-branch builds, cleared that build command and set the watch path to `site/*`. Production settings are attested as no build command, `npx wrangler deploy`, root `site/`, watch path `site/*`; the failed check is explained and is not required. The first production build still needs the planned post-merge confirmation. The Linux and macOS required checks were still pending when inspected. This is code approval, not authorization to merge without the normal Tester and Lead checks.

References: [asset exclusions](https://developers.cloudflare.com/workers/static-assets/binding/), [branch controls](https://developers.cloudflare.com/workers/ci-cd/builds/build-branches/), [watch paths](https://developers.cloudflare.com/workers/ci-cd/builds/build-watch-paths/), [Wrangler configuration](https://developers.cloudflare.com/workers/wrangler/configuration/).
