# How is Vocet doing? Voice Arena Dictation Bench, public set

> **Update, 2026-11-19:** the same 151 clips through the experimental
> **Vocet Cloud (AssemblyAI)** engine score **ZER 86.8%, AMEND 0.26%**, against
> 60.3% / 1.43% for Groq: first on the public set by both measures, within
> noise of Aqua Voice and Wispr Flow. See
> [Groq vs Vocet Cloud (AssemblyAI)](#groq-vs-vocet-cloud-assemblyai) at the end.
>
> **Update, 2026-11-19, on-device:** Vocet's own on-device engines score
> **Parakeet (the default) ZER 53.0%, AMEND 1.38%** and **Qwen3 ZER 49.0%,
> AMEND 1.70%**: below Groq, around 14th of 15 on the public set. See
> [On-device (Parakeet / Qwen3)](#on-device-parakeet--qwen3).

**Unofficial: single judge (Claude Opus 5.5 via Claude Code), public set
only.** Run on 2026-11-18 with Vocet built from `cfe583a` (the 0.1.0 release
commit, debug build, dev certificate), engine **Groq `whisper-large-v3-turbo`**,
language English, in the Lume VM `dictation-vm-eval`. How it was run:
[README.md](README.md).

## The short answer

| | Vocet + Groq | 95% interval (speaker bootstrap) |
| --- | ---: | --- |
| **ZER**: drafts you could send with no edit | **60.3%** (91 of 151) | 52.2 – 68.5% |
| **AMEND**: share of characters that need fixing | **1.43%** | 1.11 – 1.76% |
| Empty drafts | 0 of 151 | |
| Latency, key-up to final text: median / p90 | **0.63 s / 1.12 s** | min 0.37 s, max 1.87 s |

In plain words: about 6 in 10 dictations are ready to send. The rest usually
need one small fix, and most of those fixes are things an app-side cleanup
step would catch, not mistakes in what Groq heard. Vocet is fast: the text is
in place well under a second after you let go of the key.

## Where that puts Vocet among the 14 tools

Voice Arena's public-set column (whitepaper section 10.6), with Vocet + Groq
added:

| Rank by ZER | Tool | ZER % public | AMEND % public |
| ---: | --- | ---: | ---: |
| 1 | Aqua Voice | 83.4 | 0.37 |
| 2 | Wispr Flow | 80.8 | 0.34 |
| 3 | Blurt | 79.5 | 0.41 |
| 4 | Willow Voice | 77.5 | 0.46 |
| 5 | Gemini | 76.2 | 1.30 |
| 6 | Spokenly | 75.5 | 0.56 |
| 7 | Granola | 73.5 | 0.79 |
| 8 | Kivi | 70.2 | 1.38 |
| 9 | Meta AI | 69.5 | 2.26 |
| 10 | Claude | 65.6 | 0.82 |
| 11 | ChatGPT | 64.9 | 1.14 |
| 12 | Superwhisper | 61.6 | 1.37 |
| **≈13** | **Vocet + Groq (this run)** | **60.3** | **1.43** |
| 14 | Perplexity | 53.0 | 1.80 |
| 15 | Apple Dictation | 35.1 | 2.65 |

By ZER, Vocet + Groq sits between Superwhisper and Perplexity: 13th of 15.
By AMEND it is 12th, just behind Superwhisper (1.37) and Kivi (1.38). Its
interval (52–68%) overlaps everything from Perplexity up to Claude, so
"somewhere from 10th to 14th" is the honest range.

Why this placement isn't exact, and which comparisons are like for like:

- **AMEND is close to like for like.** The paper's public-set AMEND column is
  Claude Opus 5.5 alone, the same judge model, effort, prompt (amend-va 0.2.1,
  v1) and scoring code used here. Two differences remain: the request went
  through the `claude -p` CLI instead of the Anthropic API (a one-line system
  prompt added, no server-side model fallback), and every tool, Vocet
  included, was run only once.
- **ZER is not like for like.** The paper's ZER is a 3-of-5 vote of five judges
  from five providers. Ours is Opus alone. The paper's own per-judge table
  (section 10.3) shows Opus's ZER within 1.3 points of the five-judge vote
  for every tool, usually a little *below* it, so the single-judge figure
  is a fair, slightly conservative stand-in. The paper doesn't publish per-judge
  ZER for the public set alone, so the nearest exact comparison is Opus's
  full-set ZER: there Vocet's 60.3 would sit just *above* Superwhisper
  (59.9), 12th instead of 13th. Superwhisper and Vocet are tied either way.
- **Public set only.** The paper notes the public set is a little easier than
  the private one: every tool scored 1.5 points higher on it on average.
  The ranking on it is almost the same as on the full benchmark (Spearman 0.995).
- **Different machine.** The paper ran on MacBook Air M5s with macOS 27.2.
  This ran in a 4-CPU, 6 GB macOS 15.6.1 VM on the owner's Mac, over the
  host's network. That matters for latency, not for what Groq transcribes.
- **Same harness, same timing.** Voice Arena's own harness at a pinned commit,
  same padding (300 ms), start delay (0.5 s), tail (4 s), settle (2 s) and
  timeout (30 s), same BlackHole audio path, text landing in TextEdit. The
  Vocet driver is new (`run_vocet_dictation.py`); the latency is measured from
  the key-up, because Vocet keeps its microphone open for 30 s after a
  recording (README, "Latency").
- **The prompt fingerprint.** The paper names fingerprint `d26616976626`.
  amend-va doesn't publish how that is computed, so it can't be checked. The
  prompt here is built by amend-va 0.2.1's own `build_prompt`, and its
  `prompts.py` is byte identical in 0.2.1 and 0.2.2. Each scored row records the
  model, effort, route, prompt version, our fingerprint (`8ed04caf4663`) and the
  amend-va version.

## Results by category

**By writing context** (the dataset's `domain`):

| Context | Clips | ZER % | AMEND % |
| --- | ---: | ---: | ---: |
| Personal messages | 25 | 72.0 | 1.33 |
| Work chat | 24 | 66.7 | 1.33 |
| Email | 22 | 68.2 | 0.74 |
| Docs and notes | 22 | 68.2 | 1.88 |
| Code | 26 | 69.2 | 1.57 |
| AI prompts | 22 | **36.4** | **2.79** |
| Long-form (60 s and more) | 10 | **10.0** | 0.98 |

**By length** (from the spoken word count; the dataset has no length-band
column, so the whitepaper's bands are approximated):

| Length | Clips | ZER % | AMEND % |
| --- | ---: | ---: | ---: |
| Short (15 words or fewer) | 20 | 80.0 | 3.78 |
| Standard (16–80 words) | 121 | 61.2 | 1.54 |
| Long (more than 80 words) | 10 | 10.0 | 0.98 |

**By difficulty:** not possible. The public dataset doesn't include the
whitepaper's easy/medium/hard labels.

Reading these: short messages are usually right (80%), but when one is wrong
the error is a big share of a short text (AMEND 3.8%). Long-form is almost
never perfect (1 in 10), but the errors are a small share of a long text
(AMEND 1.0%). Over a minute of natural speech, one kept "you know" or one
misspelt name is enough to need an edit. AI prompts score worst because
they're the most conversational ("Okay, so…", "like", "you know") and the
most technical (code names). Long clips also cross Whisper's 30-second
window, which causes the hallucination below.

## What goes wrong

60 of the 151 drafts needed at least one edit. Each one was put into
categories by hand from the judge's corrections (`categorise.py` records the
mapping). Many drafts have more than one problem.

| Problem | Drafts with it | Drafts where it is the only problem | Who can fix it |
| --- | ---: | ---: | --- |
| **Fillers and hedges kept**: "like", "you know", "Okay, so", "Oh, and", "I kind of", a stutter ("Did you? Did you…") | **28** | 19 | app cleanup |
| **Misheard words or numbers**: "get" for git, "independent" for idempotent, "user's crashed" for user scrolls, "$850" for $150 | **24** | 15 | speech engine (partly app cleanup with context) |
| **An invented "Thank you." at the end** | **11** | 3 | app (trim silence) |
| **Code names and casing**: "calculate prorated amount", "dateFNS", "docker file", "React query" | 8 | 2 | app cleanup with context, or engine vocabulary hints |
| **Spoken symbols left as words** in emails, URLs and paths: "ops at northwind.io", "budget slash 2026" | 7 | 1 | app cleanup |
| **Names spelt wrong**: "Rahman" for Raman, "Thomas" for Tomas, "Kiera" for Kiara | 5 | 1 | engine vocabulary hints, or the user's own dictionary |
| Other: slang units ("89 bucks", "512 megs"), one dropped URL path | 3 | 0 | app cleanup / engine |

The questions from the brief, answered:

- **Fillers kept?** Yes, the biggest single cause: 28 drafts, the only
  problem in 19. Whisper writes down what was said, and Vocet pastes it as is.
- **Self-corrections kept?** Always: all 28 clips with a spoken correction
  ("January 23rd. Sorry, January 29th.") were pasted with both versions and
  the cue words. The benchmark doesn't count a kept self-correction as an
  error, as long as the final value is there, so it costs no ZER (11 of those 28
  drafts were still zero-edit). A user would still have to delete it.
- **Numbers as words?** Rarely. Groq writes most numbers as digits, and the
  judge accepts either. The number errors are real mishearings: "$17.50"
  for $1,750 ("seventeen fifty") and "$850" for $150.
- **Cut-off endings?** None. No draft lost its end, and no draft was much
  shorter than the speech.
- **Empty drafts?** None. Every one of the 151 clips produced text on the first
  try.
- **Hallucination: "Thank you."** 11 drafts end with a "Thank you." nobody
  said. This is a known Whisper habit on stretches of silence. It hit 10 of
  the 11 clips between 25 and 29 s long, plus one 52 s clip, and no others.
  The harness keeps recording 4 s after the speech ends, so these recordings
  run just over 30 s, and Whisper's last 30-second window holds only silence.
  A real user lets go sooner, but any dictation that ends just past a 30 s
  mark, with a pause before the key-up, can get the same "Thank you." Vocet
  sends the recording with its silence; it doesn't trim it.
- One draft (dict_0430) came back all lower case with no punctuation. The
  judge doesn't count punctuation, but a user would notice.

## The 10 worst drafts

Edits are characters the judge had to change, out of the length of the
corrected text. The intended text is the dataset's reference (CC BY-NC 4.0,
Voice Arena), shortened where long.

1. **dict_0430, notes, 20 edits / 309.**
   Intended: "…Owen Park's doing the release notes, and the landing page is
   fieldbook.app/new."
   Draft: "launch sync fieldbook app version 2.4 goes to app store review
   november 2nd sorry november 3rd … owens park doing the release notes and
   the landing page is fieldbook.app slash new Thank you."
   *Wrong name, "slash" not written as /, invented "Thank you.", no
   punctuation at all.*
2. **dict_0533, long-form, 20 / 935.**
   Intended: "Update on the regional sales summit… Harbor View Hotel… Clearpath
   Consulting… summit.northwind.io/register…"
   Draft: "Okay, update on the Regional Sales Summit… Harborview Hotel…
   ClearPath Consulting… Registration link is summit.northwind.io. So please…"
   *Leading "Okay", two name spellings, a dropped word, and "/register"
   lost from the URL.*
3. **dict_0024, AI prompt, 17 / 332.**
   Intended: "Ok so in billing.ts the calculateProratedAmount function returns
   NaN … plan.startDate … rewrite it with date-fns…"
   Draft: "Okay, so in billing.tease the calculate prorated amount function
   returns NAN … plan.startdate … rewrite it with dateFNS … Thank you."
   *Code names, a mishearing ("tease"), invented "Thank you."*
4. **dict_0077, notes, 17 / 359.**
   Intended: "…Tracking sheet's in the shared drive under budget/2026."
   Draft: "…Tracking sheets in the share drive under budget slash 2026. Thank you."
   *"slash" left as a word, invented "Thank you."*
5. **dict_0551, long-form, 17 / 1078.**
   Intended: "…the rest toward the playground shade structure… because they're
   stretched really thin…"
   Draft: "…and you know, the rest towards the playground-shaped structure…
   because are stretched really thin…"
   *A kept "you know", a mishearing ("shaped" for shade), a dropped word.*
6. **dict_0351, code, 16 / 350.**
   Intended: "…moving the FastAPI app's settings over to Pydantic version 2…
   Redis URL… Pool size defaults to 10…"
   Draft: "…moving the FastAPI's app settings over the Pyndanic version 2…
   redis URL… Pull size defaults to 10… Thank you."
   *Mishearings ("Pyndanic", "Pull"), casing, invented "Thank you."*
7. **dict_0544, long-form, 16 / 958.**
   Intended: "…Lorraine Whitfield, Tomas Herrera, Kiara Bell… fooddrive@eastsidehelps.org"
   Draft: "Okay, meeting recap… Thomas Herrera, Kiera Bell… So we confirmed…
   fooddrive at eastsidehelps.org."
   *Names spelt the common way, "Okay"/"So" kept, email not written out.*
8. **dict_0108, long-form, 15 / 1020.**
   Intended: "…Please don't…" (a long team message)
   Draft: "…Like, please don't… Thank you."
   *A kept "Like", invented "Thank you." on a 52 s clip.*
9. **dict_0356, code, 15 / 366.**
   Intended: "…write a test that sends the webhook twice so we know it's idempotent…"
   Draft: "…so we know it's independent… Thank you."
   *A technical word misheard, invented "Thank you." (The kept "15 seconds.
   Actually, make it 20." is allowed.)*
10. **dict_0428, notes, 15 / 402.**
    Intended: "…We'll have a short how-to session on a Saturday morning…"
    Draft: "…We'll have a short house 2 session on a Saturday morning… Thank you."
    *"how-to" misheard as "house 2", invented "Thank you."*

**The pattern:** the worst drafts aren't badly heard. Each has two to four
small problems stacked up, and the invented "Thank you." is in 7 of the 10.
None of the worst drafts is a garbled or missing transcript.

## What an app-side cleanup step would fix, and what needs the engine

**App side, no change of engine:**

- **Trim the silence before sending, or drop an invented closing "Thank you."**
  Vocet already measures the input level (the pill's meter and its "No sound"
  check). Cutting trailing silence before upload would remove the hallucination
  at its source. 11 drafts, the only problem in 3.
- **Remove fillers and hedges** ("like", "you know", "Okay, so", "Oh, and", a
  repeated start). This needs care: "like" is often a real word, so it needs an
  LLM cleanup pass or a careful rule set. 28 drafts, the only problem in 19.
- **Write out spoken symbols** in emails, URLs and paths ("at" → @,
  "slash" → /, "dot" → .). 7 drafts.
- **Resolve self-corrections** (keep only "January 29th"). Free in this
  benchmark, but it's the cleanup users notice most.

If all three of the first changes worked perfectly, 26 of the 60 failing drafts
would need no edit: **ZER about 77.5%**, level with Willow Voice and just behind
Blurt on the public set.

**App side, with more context** (an LLM cleanup that knows the target app, or
a personal dictionary): code identifiers and casing ("calculateProratedAmount",
"date-fns", "Dockerfile") and names the user has used before. That covers 8 more
drafts, about **82.8%** in the best case. Groq's Whisper API also takes a
`prompt` that can carry such names and terms to bias spelling, which is an
app-controlled hint to the engine.

**Needs a better speech engine** (or a different model): genuine mishearings
like "get"/git, "independent"/idempotent, "user's crashed"/user scrolls,
"Pyndanic", "house 2", "$850"/$150. That's 24 drafts, the only problem in 15. An
LLM cleanup can rescue some of them from context (git, idempotent, Jest), but
it can also invent; a number like $850 vs $150 can't be recovered from the text.

These "what if" figures are upper bounds. They assume the cleanup adds no new
errors, and a rewriting step can add some (the paper shows tools that tidy more
drift further from what was said).

## How this run went

- **151 of 151 clips captured**, all on the first attempt, from 22:13 to 23:22
  local time, about 28 s per clip. No clip produced an empty draft.
- **Network outage:** the owner reported about 30 s without internet while
  the 5-clip trial was running, before the 151-clip run started (22:13). The
  full run shows no failed or empty clip, and Vocet's log has no network error, timeout
  or retry. **No clips were rerun.**
- **One harness hiccup in the 5-clip trial:** on the very first clip, the
  harness stalled for about 11 minutes while holding the key, so Vocet heard
  silence and stopped at its 5-minute cap. The retry worked, and the problem
  didn't come back in the full run. Probably first-run setup in the VM (macOS
  had just shown two Automation prompts); not a Vocet issue.
- **Usage sharing** was switched off in the eval VM, so the 151 benchmark
  dictations sent no analytics.
- **Judge:** 151 drafts, each judged once by Claude Opus 5.5 at max effort
  through `claude -p`. Every answer is in `results/vocet_groq/scored.judge.jsonl`.

## Groq vs Vocet Cloud (AssemblyAI)

Run on 2026-11-18/19, 23:47 to 00:58 local time. Vocet was built from
`exp/assemblyai` at `6a652ca` (PR #322, round 2), debug build, dev
certificate. The engine is AssemblyAI's Dictation API with **Smart** rewrite
mode, the cleanup instruction adapted from Blurt, and the rewrite guard (falls
back to the verbatim text if the rewrite is empty or echoes the instruction).
The token sat in the VM's keychain and was never read by us (setup: README).
Everything else matched the Groq run: same clips, VM, harness commit and
timing, judge, prompt and scoring.

### Overall

| | Vocet + Groq | Vocet Cloud (AssemblyAI) |
| --- | ---: | ---: |
| **ZER** | 60.3% (52.2–68.5) | **86.8%** (80.3–92.7) |
| **AMEND** | 1.43% (1.11–1.76) | **0.26%** (0.13–0.43) |
| Drafts needing an edit | 60 | 20 |
| Empty drafts | 0 | 0 |
| Clips captured, first try | 151 / 151 | 151 / 151 |
| Latency median / p90 / max | **0.63 / 1.12 / 1.87 s** | 1.15 / 2.80 / 5.22 s |
| Rewrite used / fell back to verbatim | (no rewrite) | **151 / 0** (0% fallback) |

On the public set, Vocet Cloud would rank **1st by ZER** (Aqua Voice 83.4,
Wispr Flow 80.8, Blurt 79.5) and **1st by AMEND** (Wispr Flow 0.34, Aqua Voice
0.37). Its interval (80.3–92.7%) takes in Aqua Voice and Wispr Flow (Blurt is
just below it), so "tied with the best tools" is the fair reading, not "the
best". The same caveats as above apply:
single judge, one run, public set only.

**Judge noise:** 33 clips got exactly the same draft from both engines, and
the judge gave a different verdict on one of them (dict_0726: zero edits for
Groq, 19 for AssemblyAI). One judge flips a few percent of borderline
verdicts, which is why the paper uses five. It doesn't change the picture:
43 drafts that failed with Groq passed with AssemblyAI, and only 3 went the
other way.

### By category

| Context | Clips | Groq ZER % | AssemblyAI ZER % | Groq AMEND % | AssemblyAI AMEND % |
| --- | ---: | ---: | ---: | ---: | ---: |
| Personal messages | 25 | 72.0 | 92.0 | 1.33 | 0.17 |
| Work chat | 24 | 66.7 | 87.5 | 1.33 | 0.78 |
| Email | 22 | 68.2 | 95.5 | 0.74 | 0.04 |
| Docs and notes | 22 | 68.2 | 90.9 | 1.88 | 0.11 |
| Code | 26 | 69.2 | 80.8 | 1.57 | 0.53 |
| AI prompts | 22 | 36.4 | **90.9** | 2.79 | 0.17 |
| Long-form | 10 | 10.0 | **50.0** | 0.98 | 0.25 |

| Length | Clips | Groq ZER % | AssemblyAI ZER % |
| --- | ---: | ---: | ---: |
| Short (15 words or fewer) | 20 | 80.0 | 90.0 |
| Standard (16–80 words) | 121 | 61.2 | 89.3 |
| Long (more than 80 words) | 10 | 10.0 | 50.0 |

AI prompts gain the most (36% → 91%): they are full of the fillers and "Okay,
so…" openers the rewrite removes. Apart from long-form, code is now the
weakest context (81%), because what's left are technical words misheard ("Pindanic", "independent"
for idempotent, "math" for map).

### What changed in the failures

| Problem | Groq drafts | AssemblyAI drafts |
| --- | ---: | ---: |
| Fillers and hedges kept | 28 | 3 |
| Misheard words or numbers | 24 | 9 |
| Invented "Thank you." | 11 | **0** |
| Code names and casing | 8 | 1 |
| Spoken symbols left as words | 7 | **0** |
| Names spelt wrong | 5 | 4 |
| **Rewrite dropped something that was said** | – | **2** |
| Other (slang units, dropped URL path) | 3 | 1 |

- The rewrite fixed what the app-side cleanup section above predicted:
  fillers, spoken symbols ("priya.raman@northwind.io", "budget/2026") and the
  "Thank you." hallucination (none at all). Most self-corrections were resolved
  too: by the same check, the cue words survive in 14 of 25 clips, against 24
  for Groq.
- **What's left is mostly the speech itself:** 9 misheard words and 4 name
  spellings (Thomas/Tomas, "Owens Park", "d.romarez@").
- **New risk, the rewrite drops content.** In dict_0119 the rewrite removed a
  spoken phone number: "…talk on the phone. My number's Thanks for considering
  it." Groq had "My number is 312-555-0164." In dict_0621, "the green one"
  lost the word "pot". The guard didn't catch either, because it only checks
  for an empty or echoed rewrite. Silent loss of a phone number is the kind of
  error a user may not notice before sending. A cheap guard: compare the
  digits (and maybe the names) of the rewrite with the verbatim text, and fall
  back when the rewrite loses some.
- In dict_0279 the rewrite dropped a misheard start ("Just as like" for "Jest
  is") instead of keeping it, so the draft starts "hanging after the auth
  tests". It's tidier than Groq's, but the subject of the sentence is gone.

### The worst AssemblyAI drafts, next to Groq's

1. **dict_0726, work chat, 19 edits** (Groq: judged 0, same text): "We should
   probably. Let's set up a quick retro…" A restart kept as a sentence; judge
   noise on the Groq side.
2. **dict_0119, long-form, 16 edits** (Groq: 2): the phone number dropped by
   the rewrite (above).
3. **dict_0279, code, 8 edits** (Groq: 7): "hanging after the auth tests. Find
   the open handle." ("Jest is" lost; Groq: "Just as like hanging…").
4. **dict_0229, AI prompt, 5 edits** (Groq: 14): "running like 15 miles a
   week… without, wrecking my knees" (one "like" kept; Groq kept "like"
   and "you know").
5. **dict_0285, code, 5 edits** (Groq: 10): "Just make the button like full
   width on the mobile."

The other 15 drafts need 5 characters or fewer each.

### Latency

Smart mode costs time: median 1.15 s from key-up to text, against 0.63 s for
Groq, and a long tail (p90 2.80 s, max 5.2 s on a 25 s clip). The rewrite runs
after the transcription, on AssemblyAI's side. PR #322 offers a Fast mode
(not measured here), and AssemblyAI's `/v1/transcribe/live` path could overlap
the upload with the recording.

### Is it worth it?

On this benchmark the AssemblyAI engine moves Vocet from the bottom third to
the top. Almost all of the gain comes from the cleanup that the Groq report
said an app-side step would bring (fillers, symbols, the "Thank you."), plus
fewer mishearings. The costs: about 0.5 s more median latency (2 s more at
p90), audio sent to AssemblyAI, and the risk that a rewrite silently drops a
number. That risk needs a guard before this ships.

### Files for this run

- `results/vocet_assemblyai/harness_results.csv`, `harness.log`, `scored.csv`,
  `scored.judge.jsonl`, `summary.json`: as for Groq.
- `results/vocet_assemblyai/vocet_cloud_sources.csv`: per clip, whether Vocet
  pasted the rewrite or the verbatim text, the reason and the rewrite error
  code, from Vocet's log (no text in it). `vocet_cloud_log_lines.txt` is the raw
  log lines.
- `results/vocet_assemblyai_check/`: the one-clip proof run.

## On-device (Parakeet / Qwen3)

Run on 2026-11-19 (Parakeet 05:25–06:41, Qwen3 06:47–08:01 local time) with
Vocet built from `cfe583a`, the 0.1.0 release, same debug build as the Groq
run. Engine: Settings → Speech, on this Mac, nothing else changed. No key and
no network were used for transcription. Everything else matched the Groq and
AssemblyAI runs: same clips, VM, harness commit and timing (30 s timeout),
judge, prompt and scoring. The driver's settings check expected
`transcription: local` and the model (`--local-model`).

| Model | Settings name | Download | Download time (this network) | Load time |
| --- | --- | ---: | ---: | ---: |
| `parakeet-unified-en-0.6b-q4_k_m` (the default) | "English" | 477,274,496 bytes | ~15 s | 10.2 s first load |
| `qwen3-asr-0.6b-q4_k_m` | "30 languages" | 589,560,480 bytes | ~22 s | 0.2 s |

Both downloaded the normal way, from the Download button in Settings, with
the hash checked ("downloaded and verified" in Vocet's log).

### Results

| | Groq | AssemblyAI (Smart) | **Parakeet** (on device) | **Qwen3** (on device) |
| --- | ---: | ---: | ---: | ---: |
| **ZER** | 60.3% (52.2–68.5) | 86.8% (80.3–92.7) | **53.0%** (44.6–62.0) | **49.0%** (41.1–57.1) |
| **AMEND** | 1.43% | 0.26% | **1.38%** (1.05–1.75) | **1.70%** (1.33–2.08) |
| Drafts needing an edit | 60 | 20 | 71 | 77 |
| Empty drafts | 0 | 0 | 0 | 0 |
| Clips captured, first try | 151 | 151 | 151 | 151 |
| Clips over the 30 s timeout | 0 | 0 | **0** | **0** |
| Latency median / p90 / max | 0.63 / 1.12 / 1.87 s | 1.15 / 2.80 / 5.22 s | 1.17 / 2.13 / 7.49 s\* | 1.87 / 3.88 / 14.73 s\* |
| Rank by ZER, public set (of 15) | ≈13 | ≈1 | ≈13–14 (tied with Perplexity, 53.0) | ≈14 (above Apple Dictation only) |
| Rank by AMEND, public set (of 15) | ≈12 | ≈1 | ≈12 (tied with Kivi, 1.38) | ≈12 (between Kivi and Perplexity) |

\* **On-device latency is measured in the VM, which is slower than real
hardware:** 4 virtual CPUs and 6 GB of memory on the owner's Mac (the
whitepaper used MacBook Air M5s with 16 GB). It's the time from key-up to the
final text, so it includes all of the transcription. Expect a real Mac to
be faster; these numbers are only for comparing the two models with each
other. Even here no clip came close to the 30 s timeout (slowest 7.5 s for
Parakeet and 14.7 s for Qwen3, both on long clips), so no timeout rerun was
needed.

The ranks place each engine alone among the paper's 14 tools on the public
set, with the same caveats as above (one judge, one run, public set only).
Parakeet's and Groq's intervals overlap widely; Parakeet is not clearly worse
than Groq on ZER, and its AMEND is the same (1.38 vs 1.43%). Qwen3 is a little
behind both.

### By category

| Context | Clips | Groq | Parakeet | Qwen3 | AssemblyAI |
| --- | ---: | ---: | ---: | ---: | ---: |
| Personal messages | 25 | 72.0 | 64.0 | 52.0 | 92.0 |
| Work chat | 24 | 66.7 | 62.5 | 54.2 | 87.5 |
| Email | 22 | 68.2 | 63.6 | 50.0 | 95.5 |
| Docs and notes | 22 | 68.2 | 40.9 | 54.5 | 90.9 |
| Code | 26 | 69.2 | 53.8 | 53.8 | 80.8 |
| AI prompts | 22 | 36.4 | 54.5 | 50.0 | 90.9 |
| Long-form | 10 | 10.0 | 0.0 | 0.0 | 50.0 |

ZER %. Parakeet beats Groq only on AI prompts (12 of 22 against 8), partly
because it never invents a "Thank you." (5 of Groq's 11 were AI prompts). No
long-form dictation came out perfect on either on-device model.

| Length | Clips | Groq | Parakeet | Qwen3 |
| --- | ---: | ---: | ---: | ---: |
| Short (15 words or fewer) | 20 | 80.0 | 75.0 | 65.0 |
| Standard (16–80 words) | 121 | 61.2 | 53.7 | 50.4 |
| Long (more than 80 words) | 10 | 10.0 | 0.0 | 0.0 |

### What goes wrong on device

Categories by hand, as for Groq (`categorise.py`):

| Problem | Groq | Parakeet | Qwen3 | AssemblyAI |
| --- | ---: | ---: | ---: | ---: |
| Misheard words or numbers | 24 | **36** | **46** | 9 |
| Fillers and hedges kept | 28 | 28 | 30 | 3 |
| Names spelt wrong | 5 | **12** | **15** | 4 |
| Code names and casing | 8 | 10 | 10 | 1 |
| Spoken symbols left as words | 7 | 9 | 8 | 0 |
| Invented "Thank you." | 11 | **0** | **0** | 0 |
| Other (slang units, £ for pounds) | 3 | 3 | 3 | 1 |

**Top three on device:** misheard words (Parakeet 36 drafts, Qwen3 46), kept
fillers (28 / 30), and wrong names (12 / 15).

**Shared with Groq:**

- **Fillers** are the same problem in all three uncleaned engines: 27 drafts
  keep the same "like", "you know", "Okay, so" with Groq, Parakeet and Qwen3
  alike. None of them cleans up, and Vocet pastes what it gets.
- **Self-corrections** are kept by all three: the cue words survive in 24
  of 25 clips for each (AssemblyAI's rewrite: 14). Free in this benchmark but
  visible to a user.
- **Spoken symbols** ("at", "slash", "dot") and **code casing** are about the
  same. Qwen3 is the most literal: "summit dot northwind dot io slash
  register", "benefits.dot.acme.dot.co".
- 16 drafts have the same mishearing in all three ("get" for git, "Austrian"
  for Austin, "independent" for idempotent, "ideals" for ideas): hard audio,
  not one engine's weakness.

**New on device:**

- **No invented endings.** Neither Parakeet nor Qwen3 added a "Thank you." or
  anything else on silence. The 30-second hallucination is a Whisper problem.
- **More mishearings and names**, the main reason they score below Groq:
  "Prius" for Priya's, "O'Caffer" and "Okaforce" for Okafor, "Grendel" for
  Grandma, "salon" for flan, "Olympic" for Alembic,
  "U901208" for United 1208. Qwen3 has 17 mishearings no other engine
  makes; Parakeet has 4.
- **Wrong unit symbols:** Parakeet wrote a food drive's "3,900 pounds" as
  "£3,900" and "100 pounds" as "$100".
- Numbers: Parakeet wrote "seventeen fifty" as "$17.50" (Groq too) and
  "sixteen seventy-five" as "$16.75" for $1,675. Qwen3 wrote "four one five,
  five five five, zero one three eight" as "for 155550138".

**What an app-side cleanup would do here:** fixing fillers and spoken symbols
perfectly would take Parakeet to about 67.5% ZER and Qwen3 to about 63.6%
(upper bounds, as for Groq's 77.5%). Adding code and name formatting, about
74.8% and 68.9%. The bigger mishearing count means cleanup lifts on-device
less than it lifts Groq.

### What this means

- **Out of the box, on-device Vocet is a little below Vocet with Groq**
  (Parakeet 53.0% vs 60.3% ZER, overlapping intervals; same AMEND), and both
  are near the bottom of the 14 tools. The gap to the top is mostly cleanup,
  not the engine.
- **Parakeet is the better default of the two local models:** higher ZER,
  lower AMEND, fewer mishearings, and faster in the VM (median 1.17 s vs
  1.87 s). Qwen3 is the choice only for its other 29 languages, not for
  English accuracy.
- **On-device has one real advantage over Groq:** no hallucinated text on
  silence.
- A cleanup step (local LLM, or the cloud rewrite) would help on-device
  as much as it helps Groq on fillers and symbols, but on-device needs better
  name and term recognition too (a user dictionary or vocabulary hints) to
  reach the top tools.

### Files for these runs

- `results/vocet_parakeet/`, `results/vocet_qwen3/`: `harness_results.csv`,
  `harness.log`, `scored.csv`, `scored.judge.jsonl`, `summary.json`, as for
  Groq.
- `results/vocet_parakeet_check/`, `results/vocet_parakeet_limit5/`,
  `results/vocet_qwen3_check/`: the one-clip and five-clip trials.

## Files

- `results/vocet_groq/harness_results.csv`: the harness's row per clip
  (transcript, timings, Vocet settings), without the reference text.
- `results/vocet_groq/harness.log`: the harness's log.
- `results/vocet_groq/scored.csv`: the draft, the judge's corrected text,
  edits, characters, zero-edit flag, judge model, effort, route, prompt version
  and fingerprint, per clip.
- `results/vocet_groq/summary.json`: the numbers above.
- `results/vocet_groq_limit5/`: the 5-clip trial.
