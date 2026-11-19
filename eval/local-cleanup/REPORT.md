# On-device cleanup: which small model, which prompt, and what it buys

**Unofficial: two single judges (Claude Opus 5.5 via Claude Code, GPT-6 Astra
via Codex), Voice Arena public set only (151 clips).** Run on 2026-11-19 on the
owner's Mac (Apple M4 Pro, 14 cores, 24 GB), llama.cpp 1.0.0 (build 12359,
Metal), every model at Q4_K_M, temperature 0, thinking off. Issue #295. How it
was run: [README.md](README.md).

## The short answer

**Use Gemma 4 E2B (instruct, Apache-2.0) with prompt v3.** On Parakeet's
drafts (the on-device default) it lifts the share of dictations you could send
with no edit from **53.0% to 65.6%** (Claude) and from 50.3% to 62.3% (Codex).
On the check half, which no prompt was tuned on: **54.7% → 64.0%** (Claude,
+9.3 points) and 52.0% → 60.0% (Codex, +8.0).

| | Parakeet raw | **Parakeet + Gemma 4 E2B** | Groq raw | **Groq + Gemma 4 E2B** | Qwen3 ASR raw | **Qwen3 ASR + Gemma 4 E2B** | AssemblyAI (cloud) |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| ZER, Claude | 53.0 | **65.6** (57.8–72.8) | 60.3 | **73.5** (66.0–80.9) | 49.0 | **60.9** (53.5–68.3) | 86.8 |
| ZER, Codex | 50.3 | **62.3** | 57.0 | **72.2** | 47.0 | **58.9** | – |
| ZER, both judges agree | 49.7 | **61.6** | 57.0 | **70.9** | 46.4 | **58.3** | – |
| AMEND, Claude | 1.38% | **0.76%** | 1.43% | **0.81%** | 1.70% | **0.99%** | 0.26% |
| Check half ZER, Claude | 54.7 | **64.0** | 65.3 | **74.7** | 50.7 | **60.0** | – |

What it costs a user, on this Mac:

| | Gemma 4 E2B Q4_K_M |
| --- | --- |
| Download | **3.11 GB** (3,106,738,272 bytes) |
| Memory while loaded | **about 3.5 GB** resident (most of it the memory-mapped weights; 0.4 GB of other allocations) |
| Added delay per dictation | **0.39 s median, 0.97 s p90**, 2.9 s worst (151 clips, 2 to 140 words), measured with nothing else on the GPU |
| Load time | about 2 s (file already in the OS cache; a first load after a reboot reads 3 GB from disk) |
| Licence | Apache-2.0: fine to download and run from a free MIT app; ship its licence and notice |

Where that would put Vocet on the public set (Voice Arena's table in
[../voicearena/REPORT.md](../voicearena/REPORT.md), same caveats): Parakeet +
cleanup at 65.6 is about **10th of 15** (between Claude 65.6 and Meta AI 69.5),
up from about 14th. Groq + cleanup at 73.5 is about 7th (level with Granola).
The cloud AssemblyAI engine (86.8) is still far ahead: what's left after
on-device cleanup is mostly what the speech engine misheard, which a text
cleanup can't recover.

## What was tested

Three rounds, the tune / check speaker split shared with the Groq cleanup eval
(`split.csv`: 76 tune clips from 37 speakers, 75 check clips from 34). Prompts
changed only in response to failures on the tune half. Every version is
reported on both halves.

1. **Round 1:** 17 models, prompt v1 (Blurt's cleanup instruction as a chat
   prompt), on Parakeet's raw drafts. A cheap screen against the intended text,
   then both judges on the best 5.
2. **Round 2:** prompts v2 and v3 for the leaders, plus models the Lead added
   during the run (two dictation-cleanup fine-tunes, MiniCPM5-2B, LFM3-1.7B,
   and two that were screened then dropped). Guard variants.
3. **Round 3:** the winner, Gemma 4 E2B v3, on Groq's and Qwen3 ASR's raw
   drafts.

Judge calls: **625 new Claude calls** (out of ~1,500 allowed; drafts already
judged in PR #323 or in an earlier run are cached by prompt hash) and **1,045
Codex calls** in total, 403 of them for the three raw baselines (out of
~1,500). Neither judge reported a usage limit.

### The prompts

All in `prompts.py`.

- **v1**: Blurt's "Smart" cleanup instruction (MIT, © 2026 Alex Kroman, via
  Vocet's `crates/transcribe/src/assemblyai_prompt.rs` on `exp/assemblyai`),
  with a frame for a chat model: the transcript goes in the user turn between
  `<transcript>` tags as data, never to be answered; reply with only the
  cleaned text. Removes fillers, openers and false starts; keeps every other
  word.
- **v2** (from round 1's tune failures: models kept "like", "Okay, so", "Oh,
  and", "I was going to", and wrote "ops at northwind.io", "slash register"):
  v1 plus a rule to write spoken emails, paths and web addresses as typed, and
  four invented examples (none from the dataset), one of them a dictated
  request that must not be answered.
- **v3** (from round 2's tune failures for Gemma 4 E4B v2: what was left was
  mostly words misheard as a similar-sounding word, "get" for git, "red" for
  read, "boat" for board): v2 plus one narrow exception, fix a word when it is
  clearly a misrecognition and the sentence makes the intended word certain,
  with two invented examples; never change names, numbers or meaning.
- **s1** / **bv**: the exact prompts the two fine-tunes were trained with
  (their model cards), not ours.

### The guard

`guard.py`, the same rules as the other cleanup work: the raw text is pasted
when the reply is empty, an echo of the instruction or tags, assistant-style
("Sure, here is…"), more than 1.2× longer, less than half as long, or loses any
number, email/URL/dotted name, or capitalised mid-sentence name that is in the
raw text. Fallbacks are counted per run.

## Results

### All models, round 1 (Parakeet drafts, prompt v1)

Size is the Q4_K_M file. Latency is the cleanup alone, per clip, one request at
a time. Memory is the server's peak resident size (including the mapped
weights). CER is the screen: character error rate against the intended text
(raw Parakeet: 4.79). ZER is judged (raw: Claude 53.0 / Codex 50.3).

| Model | Licence | Size | Load s | Memory | Median / p90 s | Fallbacks | Screen CER | ZER Claude / Codex |
| --- | --- | ---: | ---: | ---: | --- | ---: | ---: | --- |
| Qwen3 0.6B | Apache-2.0 | 0.40 GB | 0.6 | 1.8 GB | 0.15 / 0.38 | 6 | 5.24 | – |
| Qwen3 1.7B | Apache-2.0 | 1.11 GB | 0.8 | 1.9 GB | 0.28 / 0.72 | 2 | 4.72 | – |
| Qwen3 4B | Apache-2.0 | 2.50 GB | 1.5 | 3.9 GB | 0.59 / 1.53 | 4 | 4.53 | – |
| Qwen3 4B Instruct 2507 *(added: the non-thinking 4B)* | Apache-2.0 | 2.50 GB | 1.5 | 3.8 GB | 0.62 / 1.60 | 9 | 4.60 | – |
| Qwen3.5 0.8B *(added: newer Qwen)* | Apache-2.0 | 0.53 GB | 1.0 | 1.5 GB | 0.23 / 0.58 | 1 | 4.80 | – |
| Qwen3.5 2B *(added)* | Apache-2.0 | 1.28 GB | 1.2 | 2.2 GB | 0.36 / 0.88 | 2 | 4.56 | 55.6 / 54.3 |
| Qwen3.5 4B *(added)* | Apache-2.0 | 2.74 GB | 1.9 | 3.7 GB | 0.74 / 1.82 | 5 | 4.42 | 55.6 / 54.3 |
| Gemma 3 1B | Gemma Terms of Use | 0.81 GB | 0.8 | 1.4 GB | 0.32 / 0.78 | 7 | 5.01 | – |
| Gemma 3 4B | Gemma Terms of Use | 2.49 GB | 1.5 | 3.7 GB | 0.90 / 1.77 | 12 | 4.27 | 57.6 / 57.0 |
| **Gemma 4 E2B** *(added: newer Gemma)* | **Apache-2.0** | 3.11 GB | 2.1 | 3.7 GB | 0.39 / 0.95 | 9 | 4.49 | **62.3 / 60.9** |
| **Gemma 4 E4B** *(added)* | **Apache-2.0** | 4.98 GB | 2.9 | 5.3 GB | 0.70 / 1.72 | 3 | 4.52 | **61.6 / 60.3** |
| Llama 3.2 3B | Llama 3.2 Community | 2.02 GB | 1.5 | 2.9 GB | 0.43 / 1.08 | 7 | 4.78 | – |
| Phi-4-mini | MIT | 2.49 GB | 1.6 | 3.3 GB | 0.53 / 1.28 | 5 | 4.56 | 57.0 / 55.6 |
| SmolLM3 3B | Apache-2.0 | 1.92 GB | 1.4 | 2.5 GB | 0.43 / 1.05 | 5 | 4.92 | – |
| Granite 4.2 3B *(added)* | Apache-2.0 | 2.24 GB | 1.5 | 2.8 GB | 0.49 / 1.18 | 9 | 4.43 | – |

Gemma 4 and Qwen3.5 were added because they are the current small releases of
two of the requested families (Gemma 4 E2B/E4B, March 2026; Qwen3.5, February
2026), and Gemma 4 moved from the Gemma Terms to Apache-2.0. Granite 4.2 3B
(August 2026) is a recent Apache-2.0 3B. The newer models did clearly better
than the requested older ones on the judged runs.

**Licences, for a free MIT app** (the weights are downloaded at run time, never
bundled in the repo): Apache-2.0 and MIT models are fine (ship the licence and
any NOTICE). Gemma 3's Gemma Terms of Use allow redistribution but pass on a use
policy and aren't OSI-approved. Llama 3.2's licence needs "Built with Llama"
attribution and its acceptable-use policy. Both are usable but heavier than
Apache-2.0, and neither model won.

### Round 2: prompts, the leaders and the late additions (Parakeet drafts)

Fixed / worse = clips the cleanup turned from failing to passing / clips where
the judge needed more edits than for the raw draft (in brackets: of those, how
many had passed as raw), Claude judge.

| Model + prompt | Size | Median / p90 s | Fallbacks | Claude ZER tune / check / all | Codex ZER tune / check / all | Both | AMEND (Claude) | Fixed / worse (broke) |
| --- | ---: | --- | ---: | --- | --- | ---: | ---: | --- |
| Parakeet raw | – | – | – | 51.3 / 54.7 / **53.0** | 48.7 / 52.0 / **50.3** | 49.7 | 1.38% | – |
| Gemma 4 E2B v1 | 3.11 GB | 0.39 / 0.95 | 9 | 64.5 / 60.0 / **62.3** | 64.5 / 57.3 / **60.9** | 59.6 | 0.90% | 17 / 4 (3) |
| Gemma 4 E2B v2 | 3.11 GB | 0.41 / 1.05 | 5 | 65.8 / 64.0 / **64.9** | 64.5 / 60.0 / **62.3** | 61.6 | 0.79% | 22 / 8 (4) |
| **Gemma 4 E2B v3** | 3.11 GB | **0.39 / 0.97**\* | 5 | **67.1 / 64.0 / 65.6** | 64.5 / 60.0 / **62.3** | 61.6 | 0.76% | 21 / 4 (2) |
| Gemma 4 E4B v1 | 4.98 GB | 0.70 / 1.72 | 3 | 63.2 / 60.0 / **61.6** | 61.8 / 58.7 / **60.3** | 59.6 | 0.94% | 15 / 4 (2) |
| Gemma 4 E4B v2 | 4.98 GB | 0.80 / 2.06 | 3 | 67.1 / 61.3 / **64.2** | 65.8 / 58.7 / **62.3** | 62.3 | 0.67% | 19 / 2 (2) |
| Gemma 4 E4B v3 | 4.98 GB | 1.18 / 3.04† | 5 | 67.1 / 64.0 / **65.6** | 65.8 / 61.3 / **63.6** | 63.6 | 0.64% | 21 / 2 (2) |
| Phi-4-mini v2 | 2.49 GB | 0.63 / 1.53 | 5 | 56.6 / 60.0 / **58.3** | 53.9 / 60.0 / **57.0** | 56.3 | 0.96% | 11 / 5 (3) |
| Qwen3.5 2B v3 | 1.28 GB | 0.51 / 1.73 | 4 | 52.6 / 58.7 / **55.6** | 51.3 / 56.0 / **53.6** | 53.0 | 1.25% | 5 / 2 (1) |
| S1-mini by Superwhisper (own prompt) | 0.48 GB | 0.22 / 0.59\* | 19 | 55.3 / 58.7 / **57.0** | 53.9 / 56.0 / **55.0** | 55.0 | 1.01% | 10 / 4 (4) |
| BitVoice Qwen3-0.6B FT (own prompt) | 0.40 GB | 0.20 / 0.57 | 18 | 42.1 / 46.7 / **44.4** | 44.7 / 49.3 / **47.0** | 43.0 | 5.24% | 11 / 36 (24) |
| MiniCPM5-2B v2 | 1.56 GB | 0.56 / 1.60 | 2 | 51.3 / 56.0 / **53.6** | 48.7 / 53.3 / **51.0** | 50.3 | 1.47% | 1 / 1 (0) |
| MiniCPM5-2B v3 | 1.56 GB | 0.36 / 0.90 | 2 | 51.3 / 56.0 / **53.6** | 48.7 / 53.3 / **51.0** | 50.3 | 1.47% | 1 / 1 (0) |
| LFM3-1.7B v2 | 1.10 GB | 1.89 / 3.27 | **109** | 51.3 / 54.7 / **53.0** | 48.7 / 52.0 / **50.3** | 49.7 | 1.39% | 0 / 1 (0) |
| LFM3-1.7B v3 | 1.10 GB | 1.32 / 2.17 | **104** | 51.3 / 54.7 / **53.0** | 48.7 / 52.0 / **50.3** | 49.7 | 1.39% | 0 / 1 (0) |

\* Re-measured with nothing else on the GPU (`runs/latency_check/`, same
outputs, 151 of 151 identical). † Another worker was running a model on the
same GPU during part of this run, so this is pessimistic; E4B's clean figure is
nearer its v2 run (0.80 / 2.06 s).

95% speaker-bootstrap intervals for all of these are in
`runs/parakeet/summary.md`; Gemma 4 E2B v3's is 57.8–72.8 (Claude) against raw
Parakeet's 44.6–62.0.

**Why Gemma 4 E2B v3:**

- **On the tune half** (where choices are allowed) E2B v3 ties E4B v2/v3 on
  Claude (67.1) and is one clip behind on Codex (64.5 vs 65.8). On the check
  half E2B v3 and E4B v3 are tied on Claude (64.0); E4B v3 is ahead on Codex
  by one clip (61.3 vs 60.0). That's noise: one clip is 1.3 points.
- **E2B is the cheaper one by a lot:** 3.1 GB instead of 5.0 GB, about half
  the delay (0.39 s vs 0.8–1.2 s median), 3.5 GB instead of 4.3–5.3 GB of
  memory. The Lead's review reached the same reading (E2B the incumbent, E4B
  within noise).
- **v3 over v2 for E2B:** one more clip on the tune half (Claude), equal on
  Codex, half as many drafts made worse (4 vs 8, 2 vs 4 broken), same check
  half. The gain is small; v2 is a fine fallback if v3's "fix obvious
  mishearings" rule worries anyone.
- **The others:** Phi-4-mini (MIT) and Gemma 3 4B gain 4–5 points; Qwen3.5
  barely edits; the two dictation fine-tunes are discussed below; MiniCPM5-2B
  and LFM3-1.7B don't move the score at all.

### Round 3: the winner on Groq and Qwen3 ASR drafts

| Engine | Raw ZER Claude / Codex | + Gemma 4 E2B v3, Claude tune / check / all | Codex all | Both | Fallbacks | Fixed / worse (broke) |
| --- | --- | --- | ---: | ---: | ---: | --- |
| Parakeet | 53.0 / 50.3 | 67.1 / 64.0 / **65.6** | 62.3 | 61.6 | 5 | 21 / 4 (2) |
| Groq Whisper | 60.3 / 57.0 | 72.4 / 74.7 / **73.5** | 72.2 | 70.9 | 7 | 21 / 3 (1) |
| Qwen3 ASR | 49.0 / 47.0 | 61.8 / 60.0 / **60.9** | 58.9 | 58.3 | 6 | 20 / 7 (2) |

The same prompt, tuned only on Parakeet, lifts all three engines by 12–13
points (Claude) and the check halves by 9–10. Prompt v2 on the same drafts
(Codex only): Groq 70.2, Qwen3 ASR 58.9.

### Does the guard matter?

Same replies, three guards (`apply_guard.py`: none; the standard guard; the
standard guard plus a check that no more than 3 content words are dropped or
added):

| Model | No guard: ZER Claude / Codex, worse (broke) | Standard guard | Stricter guard |
| --- | --- | --- | --- |
| Gemma 4 E2B v3 | 65.6 / 62.9, 4 (2) | 65.6 / 62.3, 4 (2) | 65.6 / 62.3, 4 (2) |
| Gemma 4 E2B v1 | 63.6 / 62.3, 4 (3) | 62.3 / 60.9, 4 (3) | 62.3 / 60.9, 4 (3) |
| S1-mini | 55.6 / 53.6, 8 (6) | 57.0 / 55.0, 4 (4) | 57.6 / 55.6, 3 (3) |
| BitVoice | 41.7 / 45.0, 52 (28) | 44.4 / 47.0, 36 (24) | 47.7 / 50.3, 25 (19) |

For Gemma 4 E2B the guard almost never fires (5 of 151) and changes neither
the Claude score nor the harms. Its 5 fallbacks: 2 correctly resolved
self-corrections ("starting at 9, sorry, starting at 10" → "starting at 10"
drops the 9), which the benchmark doesn't penalise either way, and 3 good
fixes the name check rejected ("Fast API" → "FastAPI", "Pyndanic" →
"Pydantic", "Earl Gray" → "Earl Grey"), which is why Codex scores the
unguarded replies 0.6 points higher. It earns its place on the weaker models
(S1-mini, BitVoice) and as the non-negotiable safety net from #295. It does
not catch every harm (below).

## What the cleanup fixes, and what it breaks

**Fixed** (21 Parakeet drafts went from failing to passing), for example:

- "Did you? Did you take the trash out already? It's like totally overflowing…"
  → "Did you take the trash out already? It's totally overflowing…"
- "Can you like send me the login… The username should be ops at northwind.io"
  → "Can you send me the login… The username should be ops@northwind.io"
- "Gift ideals for dad…" → "Gift ideas for dad…" (v3's misheard-word rule)
- "…the portal is benefits.acme. co" → "…the portal is benefits.acme.co"

**Made worse.** Every clip where the judge needed more edits after cleanup
than before, all three engines, Claude judge (`harms.csv` in each run folder):

| Clip | Engine | What the cleanup did | Real harm? |
| --- | --- | --- | --- |
| dict_0239 | Parakeet | "…make it sound like an actual person wrote, **not some corporation**." → "…wrote." | **Yes: a clause with a negation dropped.** The guard missed it (no number or name in it) |
| dict_0467 | Parakeet | "So could you could **someone** come take a look" → "Could you come take a look" | **Yes, small:** kept the abandoned start instead of the restart |
| dict_0498 | Parakeet, Qwen3 ASR | "usually **way** past midnight" → "usually past midnight" | **Yes, small:** "way" taken for a filler |
| dict_0257 | Qwen3 ASR | "starting at nine. **Sorry, starting at ten.**" → "starting at nine." | **Yes: kept the wrong time.** The guard missed it because the numbers are words, not digits |
| dict_0346 | Qwen3 ASR | "…using **I like** on…" (a misheard ILIKE) → "…using it on…" | Yes: a misheard identifier made unrecoverable |
| dict_0279 | Qwen3 ASR | "Just is like hanging…" (Jest is hanging) → "Hanging…" | Yes, small: the misheard subject dropped instead of kept |
| dict_0024 | Qwen3 ASR | "billing.tes" → "billing.test" (meant billing.ts) | Yes, small: an identifier changed the wrong way |
| dict_0177 | Qwen3 ASR | "one," dropped from a list | Small |
| dict_0559 | Parakeet, Groq | removed "like" in "something like, …" where it carried meaning | Small |
| dict_0461 | Groq, Qwen3 ASR | removed "like"; judge asked for more edits than before | Judge variation |
| dict_0346 | Groq | only "Ok, so" removed; the judge then flagged words it had passed in the raw draft | Judge noise |

**Audit** (`audit.py`, no judge): across the three engines Gemma 4 E2B v3
invented **no** new text (no run of 3+ new words anywhere). It removed a
negation in 15 drafts; all but one were the cue of a resolved self-correction
("Actually, no, don't use…" → "Don't use…", "No, wait, Saturday" → "Wait,
Saturday"); the one real loss is dict_0239 above. By the audit's
check it changed one identifier on Parakeet and one on Groq ("Priya.rahman at
northwind.io" → "priya.rahman@northwind.io", "7.30" → "7:30"), both correct;
the identifier harms in the table above ("I like", "billing.tes") are words
the audit doesn't recognise as code.

**What a better guard could add**, from these: compare negations ("not",
"n't", "never", "no") outside a self-correction cue; treat number words like
digits ("nine" / "ten"); and fall back when a run of 3+ content words
disappears. The stricter guard tried here (4+ content words) caught none of
Gemma's cases. The other way round, the name check could ignore spaces and case
("Fast API" → "FastAPI"), but it should keep blocking changed spellings
("Pyndanic" → "Pydantic" is right here, "Priya" → "Prius" would not be).

## What still fails

52 Parakeet drafts still need an edit after Gemma 4 E2B v3 (judge
corrections, sorted by hand):

| Problem | Drafts | Who can fix it |
| --- | ---: | --- |
| Misheard words ("get"/git, "independent"/idempotent, "Austrian"/Austin, "could"/couldn't, "near"/nil, "boat"/board…) | ~28 | the speech engine; v3 fixes some, not these |
| Names spelt wrong (Rahman/Raman, "O'Caffer"/Okafor, Thomas/Tomas, "Prius"/Priya's) | ~10 | a personal dictionary or vocabulary hints |
| Code identifiers and casing ("calculate prorated amount", "null 20 alpine", "Docker file", "React query") | ~7 | an app-aware prompt, or a dictionary |
| Made worse by the cleanup (above) | 3–4 | a better guard |
| Fillers or false starts left ("I was going to", "down to down to") | 2 | the prompt |
| Units ("89 bucks", "£3,900" for 3,900 pounds) | 2 | engine ITN, or a rule |

Kept fillers, the biggest problem in the raw drafts (28 Parakeet drafts), are
almost gone. The rest is mostly what the engine heard, which is why the
on-device stack stays well below the cloud AssemblyAI engine (86.8), whose
speech model mishears far less (9 drafts vs Parakeet's 36).

## The two dictation fine-tunes

Both are Qwen3-0.6B fine-tuned for exactly this job; each was run with its own
trained prompt, greedy, thinking off.

- **S1-mini by Superwhisper** (`superwhisper/s1-mini-GGUF`, Q4_K_M 484 MB):
  Apache-2.0 **plus an additional term**: any use or product must keep
  identifying the model as "S1-mini" by "Superwhisper", with that exact
  capitalisation. Fast (0.22 s median) and small (1.6 GB memory), and the best
  screen score of all (CER 3.98), but it judges at only 57.0 / 55.0. It
  rewrites more than the benchmark wants (resolves every self-correction,
  reformats times and numbers), so 19 replies hit the guard and some changes
  cost edits. A good fit for a "light" option on 8 GB Macs, but 8 points behind
  Gemma 4 E2B.
- **BitVoice** (`dhanr4j/bitvoice-dictation`, `gguf/qwen3-0.6b-ft.gguf`, 397 MB,
  Apache-2.0 per its card; the collection's Llama and Falcon files are not):
  **worse than no cleanup** (44.4 / 47.0, 36 drafts made worse). It cuts long
  dictations short (12 of the 45 drafts over 300 characters lose more than a
  fifth of their length without the guard) and drops words ("…contact Priya
  Rahman at Priya."). Not
  usable at greedy decoding on this set.

## How well the cheap screen predicted the judges

Over the 19 judged Parakeet runs (Spearman rank correlation with judged ZER;
`screen_vs_judge.py`, `runs/parakeet/screen_vs_judge.json`):

| Screen measure | vs Claude ZER | vs Codex ZER |
| --- | ---: | ---: |
| CER against the intended text | **+0.82** | **+0.83** |
| WER against the intended text | **+0.82** | **+0.83** |
| Exact match with the intended text | +0.73 | +0.73 |
| "match_fix" (intended text or the judge's fix of the raw draft) | −0.30 | −0.31 |
| "broke" (fewer is better) | −0.63 | −0.63 |
| Claude ZER vs Codex ZER | +0.99 | |

- **Trust CER/WER**, not the match columns. CER ranks runs almost as the judges
  do. "match_fix" (the "match 47–52" figures that made Gemma 4 look weak in the
  round-1 screen) doesn't: a cleaned draft rarely equals either reference
  character for character, while the judge is lenient about kept
  self-corrections, digits and punctuation.
- **"broke" is not a harm count.** It counts drafts the judge had passed as
  raw whose words the cleanup changed at all. Most of those changes are
  harmless or good: dropping "Okay, so", "Oh, and" or a repeated word from a
  draft that was already acceptable. That's why it goes up with real cleanup
  and correlates *negatively* with judged harm. Gemma 4 E2B v3 "broke" 21
  drafts by the screen; the judges failed only 2 of them. MiniCPM5-2B and
  LFM3-1.7B had "broke" counts of 0–1 because they hardly change anything
  (MiniCPM5 left 146 of 151 drafts as they were, and 109 of LFM3's replies
  fell back to the raw text), which is also why their judged ZER equals raw.
  The judged "worse (broke)" column in the tables is the real harm count.
- **The two judges agree on the ranking** (0.99) but Codex is 1–3 points
  stricter in level. Report both, or the both-agree ZER.

## Models considered and not used

- **NVIDIA Nemotron 3.5 Bolt-E4B** (a Gemma 4 E4B fine-tune, IFBench 63.8 vs
  42.8 on NVIDIA's card; OpenMDW-1.1 licence): screened with v1 and v2 (CER
  4.40 / 4.11, about Gemma 4 E4B's), then dropped by the Lead: the official
  Q4_K_M is 6.18 GB (bartowski's 5.16 GB build was tested), over our size limit.
- **LFM2.5-1.2B** (LFM Open License v1.0): screened, the worst CER of the
  late additions (5.8–6.0); replaced by LFM3.
- **LFM3-1.7B** (LFM Open License v1.0: commercial use only below $10 M annual
  revenue; the owner accepted that, and weights would be downloaded at run
  time under Liquid's licence, never bundled in the MIT repo): even with
  thinking off (its template's empty `<think></think>`), it writes its
  reasoning as plain text ("Okay, I need to clean up this transcript…") on 37–58
  of 151 clips, or answers the dictation (writes the docstring it was asked
  for). The guard rejects those, so its judged ZER equals raw. Possibly usable
  with thinking on and only the final answer kept, at a higher delay; not
  tried.
- **MiniCPM5-2B** (Apache-2.0, IFBench 66.3 on OpenBMB's card): follows "keep
  every word" too literally; it edits 2–5 of 151 drafts. Judged equal to raw.
- **KRAFTON Raon-1 nano** (Apache-2.0, the best IFBench and grounding scores
  in the Lead's scan): skipped, its GGUF needs KRAFTON's llama.cpp fork, an
  integration risk for the app; worth a look if it lands in mainline.
- **Nemotron-Labs Dory 1.0 4B**: llama.cpp support only in an open PR.
- **Microsoft Aion-1.0-Instruct 2.3B** (MIT, out 2026-11-18): not run, for time;
  its IFBench (32) is below the models tested.
- **Qwen3.6** comes only as large MoE models; **Gemma 3n** is superseded by
  Gemma 4; **Llama 4** has no small model.

## How it would go in the app

What exists: Vocet's on-device speech already runs on **transcribe.cpp**
(`transcribe-cpp` 0.4.1, MIT, Metal), which builds its own copy of ggml. Its
source has speech architectures (conformer, Granite conformer, SANM) and a
`causal_lm` decoder for Qwen3 ASR, but no general chat-model runner (no Gemma 4,
no chat templates), so it can't run this model as it stands.

Two ways in, both MIT-compatible:

1. **llama.cpp's `llama-server` as a bundled helper process** (what this eval
   used). Vocet starts it on 127.0.0.1 with a random port when cleanup is on,
   sends one chat request per dictation, and stops it after a few idle minutes
   to give back the 3.5 GB. Pros: llama.cpp's own ggml stays out of Vocet's
   process, so no clash with transcribe.cpp's ggml; a crash can't take the app
   down; chat templates and prompt caching come for free. Cons: one more signed
   binary (about 20 MB) and a localhost port. **Recommended for a first
   version.**
2. **llama.cpp linked in through a Rust binding** (for example the
   `llama-cpp-2` crate, MIT/Apache). Neater, but llama.cpp and transcribe.cpp
   each build ggml, and two static copies in one binary will clash at link time
   unless both are built against one shared ggml. Needs a spike first.

Either way: download the model on demand from Hugging Face with a pinned
sha256, as Parakeet is now (3.1 GB, so after a clear prompt); keep cleanup off
by default (#295); run the guard from `guard.py` in Rust before pasting, plus
the three checks above; and paste the raw text on any error or a timeout
(the worst clip here took 2.9 s on this Mac; slower Macs need a longer one). The prompt's fixed part (about 870
tokens) is cached by llama.cpp after the first dictation, so only the
transcript is processed each time. Start loading the model when recording
starts, so the 2 s load overlaps the speech.

**Open questions before building:** delay and memory on the slowest supported
Macs (an 8 GB M1 has much less headroom than this 24 GB M4 Pro; S1-mini or
Qwen3.5 2B would be the light fallback there, at a smaller gain); whether to
pass the target app (code editor, chat) to the prompt for identifiers; and a
user dictionary for names, the biggest remaining category after mishearings.

## Files for this run

- `runs/parakeet/<model>__<prompt>/`: round 1 and 2. `__noguard` and
  `__strict` folders are the same replies under the other guards.
- `runs/groq/`, `runs/qwen3/`: round 3, and each engine's `raw/` baseline
  (Claude scores from PR #323, Codex scores new).
- `runs/latency_check/`: the clean latency re-runs.
- `runs/parakeet/summary.md`, `summary_guard.md`, `runs/summary_round3.md`:
  the full tables with intervals; `screen_v1.json`, `screen_r2.json`,
  `screen_vs_judge.json`: the screens.
- `judge_cache/claude.jsonl`, `judge_cache/codex.jsonl`: every judge answer
  (the corrected text only), keyed by prompt hash.
