# TOS1: Terms on Welcome, a four-step setup (after SIMP1 PR B merges)

Owner feedback and decisions, 2026-11-21 (owner tried the 0.2.0 dev build from 9812e2b): agree to the Terms and
Privacy Policy on the first screen; explain OpenRouter to people who have never heard of it; show the true local
download size; make the Connect screen calmer; fix the app falling behind System Settings after permissions; drop
the Ready page. Lead's calls, accepted with the milestone: a line under Get started (not a checkbox); "Open Vocet
when I log in" becomes a checkbox on Try it, checked by default.

Setup becomes **Welcome → Speech → Permissions → Try it** (four steps).

## Scope
1. **Terms page** `site/terms.html`, same design as `site/privacy.html`. Plain and short: who (Ankit Garg, Vocet
   Labs, vocetlabs@gmail.com); free beta; with Connect OpenRouter your OpenRouter account and the model providers'
   terms apply and you pay them directly; on-device models come from Hugging Face under their licences (OrukQwen
   Apache-2.0, Gemma terms); acceptable use (lawful use; don't record others without consent where the law
   requires it); provided "as is", no warranty, liability limited as far as the law allows; age as in the privacy
   policy (13+, 16+ in the EU); the source licence (FSL-1.1-ALv2) is separate; dated changes announced in release
   notes; governing law left as a TODO for the lawyer (say so in the PR). Terms link in the site footer
   (index.html, privacy.html) and wherever docs link the privacy policy.
2. **Welcome**: under Get started, "By continuing, you agree to the Terms and Privacy Policy." (links open the
   site pages). Get started stores `accepted_terms` (the terms date). **Nothing goes to analytics before Get
   started** (including AppOpened on a fresh install); after it, reports are on by default with the off switch in
   Settings → Privacy (keep it; EU law needs it). Remove the pre-switch special-casing for setup events.
3. **Speech step**
   - OpenRouter card: one plain sentence for people who don't know it, e.g. "OpenRouter is one account for many AI
     models. You add a few dollars of credit and pay only for what you use: about 20 cents per hour of speaking."
     Verify the per-hour figure from OpenRouter's current prices for `openai/gpt-4o-mini-transcribe` plus cleanup
     (`z-ai/glm-5.3-flash`) and OpenRouter's credit fee; round honestly; cite the source in the PR. Keep "Needs
     internet. Your speech goes through your own OpenRouter account."
   - On this Mac card: the size is the **whole** download, speech + cleanup (OrukQwen 1,319,830,656 B + Gemma 4
     E2B 3,106,738,272 B ≈ **4.4 GB**), e.g. "English · 4.4 GB download (speech + cleanup)". Compute it from the
     catalog, not a literal.
4. **Connect OpenRouter screen**, calmer:
   - Not connected: the one-sentence explainer, one primary **Connect OpenRouter** button, a small "Paste a key
     instead" link under it. Remove "I'll use Vocet on this Mac only" (Back does that).
   - Connected: "✓ Connected to OpenRouter" with the key hint in small text, one secondary **Add credits** link,
     and Continue. Remove "Connect again" and "Paste a key instead" here (Settings → Speech keeps reconnect,
     paste and disconnect).
5. **Permissions bug**: after the person allows Microphone and Accessibility, Vocet stays behind System Settings.
   When a permission flips to granted while setup is open, bring the setup window to the front (activate the app).
   Unit-test the trigger; check it in the VM.
6. **Remove the Ready step** (`ReadyStep.tsx`, its summary, sharing switch and caption, `sharing_on_throughout`,
   StepFrame's "nothing may leave this step yet" logic and the Rust state behind them). Try it's **Done** finishes
   setup: closes the window and shows the existing "Find Vocet in the menu bar" hint (or the tray pointer). Try it
   gets one checkbox under the practice box: "Open Vocet when I log in", checked by default, applied on Done.
7. **Privacy policy and app text**: "Usage and error reports start after you press Get started on the first
   screen; turn them off any time in Settings → Privacy." Keep `docs/privacy.md` in sync. Settings → Privacy links
   to both pages.

Out of scope: lawyer review (before Vocet Cloud or any training-data feature), a re-consent flow.

## Acceptance criteria
1. [auto] Gate passes; unit tests: Get started stores the terms date; no analytics event before it; the size is
   the sum of both models; a permission flipping to granted activates the window.
2. [auto] Setup has four steps; Done on Try it finishes setup and applies the launch-at-login checkbox; Settings →
   Privacy still turns reports off and that stops sending.
3. [auto] e2e onboarding scenarios updated and green; site checks pass for terms.html and footer links.
4. [manual] Lead and owner read the Terms and walk through setup on a fresh install (light and dark).

Developer: Sonnet 5.5. Time box: 1 day. Start from main after SIMP1 PR B merges (it rewrites errors and telemetry).
