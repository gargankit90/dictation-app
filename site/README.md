# vocetlabs.com

The public website for Vocet (issue #211): one static page, no build step.

- `index.html`: the landing page, with its styles and the small demo script inline.
- `fonts/`: Inter and Inter Tight (WOFF2, SIL OFL, `fonts/LICENSE.txt`), served from here so the page makes no request to Google.
- `og.png`: the 1200×630 link-preview image.

It is a snapshot of what was live on vocetlabs.com on 2026-11-03, formatted with Prettier; nothing else was changed. The design comes from the Design Lead's round-2 "A" landing page.

To preview it, open `index.html` in a browser. Hosting is on Cloudflare and deploys are manual for now: upload these two files. Before changing copy about data or analytics, check it against `docs/privacy.md`, which must match the code.
