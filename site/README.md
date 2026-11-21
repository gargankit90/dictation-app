# vocetlabs.com

The public website for Vocet (issue #211): one static page, no build step.

- `index.html`: the landing page, with its styles and the small demo script inline.
- `fonts/`: Inter and Inter Tight (WOFF2, SIL OFL, `fonts/LICENSE.txt`), served from here so the page makes no request to Google.
- `og.png`: the 1200×630 link-preview image.

The page is the WEB1 rewrite for 0.2.0 (students and engineers). Before it merges, set `DOWNLOAD_URL` at the bottom of `index.html` to the 0.2.0 release asset or page (not `releases/latest`, which skips pre-releases); until then the Download buttons link to the releases list.

To preview it, open `index.html` in a browser. Hosting is on Cloudflare (the `vocetlabs` Worker, static assets only).

## Deploys

Deploys are automatic: Cloudflare Workers Builds watches `site/*` on the default branch and runs `npx wrangler deploy` in this folder after each merge that changes it. Nothing to upload by hand.

- `wrangler.jsonc` names the Worker and keeps its domains: `vocetlabs.com`, `www.vocetlabs.com` and the `workers.dev` address. A domain added in the dashboard must also be added here, or the next deploy drops it.
- `.assetsignore` keeps this README and the config off the site.
- No API token lives in the repo: the Cloudflare GitHub app does the build. Preview builds for other branches are off, so only merged changes go live.
- Dashboard settings (vocetlabs → Settings → Builds): root directory `site/`, no build command, deploy command `npx wrangler deploy`, watch path `site/*`.
- To roll back: vocetlabs → Deployments, pick an earlier version, Rollback. Before changing copy about data or analytics, check it against `docs/privacy.md`, which must match the code.
