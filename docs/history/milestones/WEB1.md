# WEB1: Website for 0.2.0 (students and engineers)

Owner decisions, 2026-11-20. Inputs: `/Users/gauri/vocet-screens/u9/website-brief.md`, `website-codex.md`,
`website-gemini.md`. One static page, `site/index.html` (Cloudflare, vocetlabs.com). Ship with 0.2.0; build
now on a branch, merge when 0.2.0 is released (the Download link must point at the 0.2.0 release).

**Audience:** students and engineers. Minimum complexity: fewer sections, plain words.

## Hero
- Headline: **"Speak rough. Paste clean, in any Mac app."**
- Subhead: "Hold one key and talk. Vocet cleans up fillers, false starts and punctuation, then pastes the text
  where your cursor is: Claude Code, Cursor, Google Docs, Notion, Slack or Mail."
- One **Download for Mac** button (macOS 13+, Apple silicon) linking directly to the 0.2.0 release asset or
  release page — not `releases/latest` (it skips pre-releases). Secondary link: source on GitHub.
- Under the hero: a static before/after (no rotating hero): spoken "um so in the auth function, no wait,
  the login handler, can you add a retry, like three times" → pasted "In the login handler, add a retry
  (three attempts)." Our own example text, not Voice Arena data.

## Sections (in order)
1. Hero + before/after.
2. **Made for how you work** (two cards):
   - Engineers: long prompts to Claude Code / Cursor / Codex, PR and code-review comments, Slack replies, docs.
   - Students: lecture and reading notes, first drafts of essays and reports, emails, questions to an AI tutor.
3. **Two ways to run**:
   - Default — Connect OpenRouter: "Vocet is free. Click Connect OpenRouter and authorize your account. If
     you need credits, OpenRouter's minimum top-up is $5. About $0.70 per 1,000 dictations."
   - On this Mac only: "Free and private. A one-time 4.4 GB download, works offline, English."
   - One line: "We publish our cleanup prompts, model choices and eval harness on GitHub." (link to `eval/`).
     **No percentages or leaderboard claims.**
4. **Where your data goes**, per mode (replace the blanket "never sent" list): Connect OpenRouter — audio goes
   through OpenRouter to OpenAI for transcription; the text goes through OpenRouter to a zero-data-retention
   cleanup provider; no Vocet server. On this Mac only — audio and text stay on your Mac. Both — History stays
   on your Mac; anonymous usage and error reports are on by default and can be turned off in Settings → Privacy.
   **Don't publish a retention period** until the Lead confirms it for the route used (Q5 privacy policy).
5. **Questions**: What does it cost? / How is this different from macOS Dictation? (cleanup) / Why does it
   need Accessibility? / What if the paste misses? (Copy last) / Is it open source? (source available, FSL).
6. Closing: free beta, source available (FSL-1.1-ALv2, Apache 2.0 after two years), Download, GitHub.

## Cut
The separate setup section; Parakeet / Qwen3-ASR / "30 languages" / "about 100 with OpenRouter" claims;
paste-a-key instructions; "Alpha" labels and old mockup text ("Build 0.1.0 is ready"); the rotating hero
and the four use-case tabs; the built-in Dictation comparison and Windows teaser; "Other apps are still being
tested".

## Meta
Title "Vocet: Mac dictation that cleans up what you say"; description about cleanup for students and
engineers; canonical `https://vocetlabs.com/`; og/twitter title, description, image alt; a new og.png with the
before/after; small `SoftwareApplication` JSON-LD (free, macOS).

## Acceptance criteria
1. [auto] Site checks in the gate pass (links, fonts self-hosted, no external trackers added).
2. [auto] No "open source", no Groq/OpenAI-direct, no percentages, no Parakeet/Qwen3 on the page (grep).
3. [manual] Lead + owner read the page on desktop and phone width; Download points at 0.2.0.

Developer: Sonnet 5.5. Time box: half a day.
