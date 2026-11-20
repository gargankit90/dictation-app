# Offline cleanup model comparison (Voice Arena public set, 151 clips)

Experiment, not for `main` (draft PR #327). 2026-11-19. Quality only: end-to-end latency is not
measured here.

**Question:** do OpenAI's paid models clean up Groq Whisper text better than Groq's open gpt-oss
models? **Answer so far:** yes, clearly, but the prompt matters more than the model. The best result
is **gpt-6.2-sol + v4: ZER 82.1 % (Claude judge) / 80.8 % (Codex judge)**, against 60.3 / 57.6 for
the raw Whisper text and 64.9 / 64.2 for the VM run of gpt-oss-20b + v1. The cheapest OpenAI model,
**gpt-6-luna + v4, gets 77.5 / 76.2 at about 1/20 of sol's price**.

**Best pipeline so far:** Nemotron 3.5 transcripts cleaned by GPT-6.2-sol + v4 through the OpenRouter
API, with the app's guard, score **84.8 % (Claude) / 84.1 % (Codex)** on all 151 clips and 81.3 / 78.7
on the held-out check half. AssemblyAI Smart scores 88.1 / 86.8 (check half 86.7 / 84.0); the gap is
within noise ([section below](#nemotron-with-openrouter-gpt-62-sol-and-v4)).

## Method

- **Input:** the raw Whisper text (`whisper-large-v3-turbo` via Groq, Vocet's cloud path) saved by the
  Groq cleanup v1 VM run, one draft per clip (`input/raw_whisper.csv`, the stripped
  `groq_clean_v1_raw/harness_results.csv`). Every model cleans the same 151 texts.
- **Request = the app's** (`exp/groq-cleanup`, PR #324): the prompt file (trimmed) as the system
  prompt, `<transcript>\n…\n</transcript>` as the user message, the reply trimmed and stripped of the
  tags, then the **app's guard**, ported to Python in `guard.py` (empty, echo, longer than 1.25× + 20
  characters, a dropped number, email/URL/file name or capitalised name → paste the raw text). Ported
  check for check; on the VM run's 149 replies the port gives the app's verdict every time (0
  mismatches). Language-level differences (Unicode classes in Rust vs Python) affect no clip here.
- **Providers.** OpenAI models through `codex exec` on the owner's ChatGPT plan (reasoning effort
  `low`, read-only sandbox, tools and web search off, the cleanup prompt as Codex's model
  instructions, transcript on stdin). **Caveat:** Codex still adds its tool definitions and a role note
  to the request, and its timing is a CLI round trip (median ~5 s), not API latency; so these numbers
  measure quality only, and an API call with the same prompt may behave slightly differently. Groq
  models through Groq's API with the app's exact request body (temperature 0, `reasoning_effort` low,
  the app's token budget), run by the owner (`run_groq.sh`; no agent handled the key).
- **Scoring:** the Voice Arena judges, unchanged: Claude Opus 5.5 (`score_claude_cli.py`) and GPT-6
  Astra via Codex (`score_codex_cli.py`), AMEND v1 prompt. ZER = share of clips the judge leaves
  untouched (higher is better); AMEND = character edits / characters (lower is better). Judge caches
  are seeded from all earlier runs, so a byte-identical draft keeps its verdict (no judge noise on
  unchanged text). Tune / check = the fixed speaker split `eval/groq-cleanup/split.csv` (76 / 75 clips).
  With 151 clips one clip is 0.66 ZER points; the 95 % speaker-bootstrap interval is about ±8 points,
  so differences under ~5 points are noise.
- **Prompts** (`prompts/`, see `prompts/SOURCE.md`): v1 = Blurt's Smart rules (the app's built-in);
  v4 = v3 rules + fix clearly misheard technical words and code names (the brief's "v3"); g3 / g4 = the
  Groq worker's pushed v3 / v4 (= my v3 / v4 plus one sentence on dangling commas). **No prompt was
  changed in this spike**: the Codex budget went to model coverage, and the tune-half failures of the
  best prompt point at the guard, not the prompt (below).

## Results

ZER % on tune / check / **all**; AMEND % on all; fixed / broke = clips where the cleanup took the raw
draft from ≥1 edit to 0, or from 0 to ≥1, per judge; changed = clips whose pasted text differs from raw.

| Run | n | Claude ZER tune / check / all (95% CI) | Codex ZER tune / check / all | AMEND all Claude / Codex | Fixed / broke vs raw (Claude; Codex) | Fallbacks (reasons) | Changed | Model ms median |
| --- | ---: | --- | --- | --- | --- | --- | ---: | ---: |
| groq_clean_v1_raw | 151 | 55.3 / 65.3 / **60.3** (51.9–68.3) | 55.3 / 60.0 / **57.6** | 1.34 / 1.32 | 0/0; 0/0 | - | - | - |
| groq_clean_v1 | 151 | 61.8 / 68.0 / **64.9** (55.3–73.7) | 64.5 / 64.0 / **64.2** | 1.45 / 1.37 | 11/4; 12/2 | 10/151 (dropped_name 1, dropped_number 2, empty 5, timeout 2) | 48 | 465 |
| groq_clean_v2 | 151 | 64.5 / 65.3 / **64.9** (55.7–73.3) | 67.1 / 62.7 / **64.9** | 1.26 / 1.15 | 17/10; 19/8 | 2/151 (dropped_name 2) | 57 | 545 |
| luna_g3 | 151 | 77.6 / 76.0 / **76.8** (68.7–84.4) | 77.6 / 72.0 / **74.8** | 0.48 / 0.46 | 27/2; 26/0 | 0/151 | 60 | 4545 |
| luna_g4 | 151 | 75.0 / 76.0 / **75.5** (67.5–82.8) | 76.3 / 72.0 / **74.2** | 0.52 / 0.49 | 25/2; 25/0 | 3/151 (dropped_name 2, dropped_number 1) | 57 | 4392 |
| luna_v1 | 151 | 69.7 / 69.3 / **69.5** (59.9–78.3) | 69.7 / 65.3 / **67.5** | 1.40 / 1.35 | 19/5; 18/3 | 8/151 (dropped_number 8) | 58 | 5670 |
| luna_v4 | 151 | 75.0 / 80.0 / **77.5** (69.2–84.8) | 76.3 / 76.0 / **76.2** | 0.48 / 0.44 | 28/2; 28/0 | 3/151 (dropped_link 1, dropped_name 2) | 61 | 4568 |
| oss120b_g3 | 141 | 71.8 / 65.7 / **68.8** (60.9–76.9) | 70.4 / 64.3 / **67.4** | 0.64 / 0.61 | 16/3; 15/0 | 2/141 (dropped_name 1, dropped_number 1) | 51 | 589 |
| oss120b_g4 | 18 | 70.0 / 71.4 / **70.6** (46.7–88.9) | 72.7 / 71.4 / **72.2** | 1.33 / 0.98 | 2/0; 2/0 | 0/18 | 6 | 564 |
| oss120b_v1 | 5 | 66.7 / 50.0 / **60.0** (20.0–100.0) | 66.7 / 50.0 / **60.0** | 1.54 / 1.54 | 1/0; 1/0 | 0/5 | 4 | 939 |
| oss120b_v4 | 150 | 72.4 / 70.3 / **71.3** (61.5–80.0) | 72.4 / 67.6 / **70.0** | 0.63 / 0.56 | 19/3; 19/1 | 1/150 (dropped_number 1) | 56 | 570 |
| oss20b_v1 | 151 | 65.8 / 66.7 / **66.2** (57.1–74.8) | 67.1 / 61.3 / **64.2** | 1.27 / 1.25 | 13/4; 13/3 | 11/151 (dropped_name 2, dropped_number 2, empty 7) | 46 | 496 |
| reserve_v1 | 151 | 69.7 / 68.0 / **68.9** (59.3–77.6) | 71.1 / 65.3 / **68.2** | 0.86 / 0.82 | 20/7; 19/3 | 11/151 (dropped_name 1, dropped_number 10) | 59 | 4874 |
| sol_v1 | 151 | 73.7 / 69.3 / **71.5** (62.3–79.9) | 73.7 / 65.3 / **69.5** | 0.81 / 0.81 | 18/1; 18/0 | 3/151 (dropped_number 3) | 55 | 5268 |
| sol_v4 | 151 | 78.9 / 85.3 / **82.1** (74.8–88.7) | 80.3 / 81.3 / **80.8** | 0.44 / 0.44 | 33/0; 35/0 | 7/151 (dropped_link 1, dropped_name 4, dropped_number 2) | 66 | 5099 |

Offline gpt-oss-20b + v1 exceeded the VM by 1.3 Claude ZER points and tied on Codex. For 120b, scores exclude missing API replies. The v2 VM used a separate transcription run, so its fixed/broke comparisons against v1 raw include transcription differences. The 120b g4 and v1 runs returned only 18 and 5 usable replies respectively. Claude had 17 matched g4 verdicts. Claude judging hit its session cap. OpenRouter results appear below.

Guard counterfactuals (no new model calls; same replies, a different guard, scored by both judges):

| Run | Claude ZER all | Codex ZER all | What changed |
| --- | ---: | ---: | --- |
| gpt-6-luna + v1, relaxed guard | 69.5 | 67.5 | 7 resolved self-corrections pasted ("9, sorry, 10" → "10") |
| gpt-6-luna + v1, no guard | 69.5 | 67.5 | + dict_0024, where luna dropped a whole sentence (the guard was right) |
| gpt-6.2-sol + v1, relaxed guard | 71.5 | 69.5 | 3 resolved self-corrections pasted |
| gpt-6.2-sol + v4, no guard | 84.8 | 83.4 | 7 replies the guard refused, all good fixes (below) |

Relaxed guard = the app's guard, but a dropped number or name is allowed when the raw text follows
it within 30 characters with a correction cue (sorry, wait, no, actually, make that, …). It moves
neither judge: the Voice Arena intended text keeps self-corrections, or the clips have other edits.

## What the numbers say

1. **Paid OpenAI models beat gpt-oss-20b on the same prompt (v1):** luna 69.5 / 67.5, reserve 68.9 /
   68.2, sol 71.5 / 69.5 vs the VM's 64.9 / 64.2. Most of the gap is fewer
   failures, not better edits: gpt-oss-20b returned nothing on 5 clips (its reasoning used the token
   budget) and timed out on 2 at the app's 3 s limit; the OpenAI models never came back empty. On v1
   the three OpenAI models are within noise of each other on ZER; sol halves AMEND (fewer and smaller
   leftover errors) and has the fewest fallbacks (3).
2. **The prompt is worth more than the model.** v1 → v4 adds 8 ZER points on luna (69.5 → 77.5) and
   10 on sol (71.5 → 82.1), and halves AMEND. The Groq worker's v3 rules (keep meaning-carrying
   sentences, keep self-corrections, symbols in addresses, drop a phantom final "Thank you.") give
   most of it: luna + g3 76.8 / 74.8 with **zero** guard fallbacks. The misheard-words rule adds little
   on luna (g3 vs g4 within noise) but is where sol pulls ahead (sol fixes `billing.tease` →
   `billing.ts`, `Pyndanic` → `Pydantic`, `dateFNS` → `date-fns`, `idipotency` → `idempotency`).
   The dangling-comma sentence (g4 vs my v4) is neutral to slightly worse (−2 points ≈ 3 clips, noise).
3. **sol + v4 is the best on every measure:** highest ZER on both halves and both judges (check half
   85.3 / 81.3, so not tuned to the tune half), lowest AMEND, most clips fixed (33 / 35) and **none
   broken by either judge**. luna + v4 is 4–5 points behind at ~1/20 of the price.
4. **With v4, the strict guard now blocks good fixes.** All 7 sol + v4 fallbacks were correct
   corrections the guard read as dropped content: `billing.tease` → `billing.ts` (link), `Pyndanic` →
   `Pydantic`, `Connect Timeout` → `` `connectTimeout` ``, `El Pastor` → `al pastor`, `Road J` → `Row J`
   (names), `$17.50 a month` rent → `$1,750` and `house 2 session` → `how-to session` (numbers).
   Without the guard sol + v4 scores 84.8 / 83.4. The guard still earns its keep on v1: luna + v1
   dropped a whole sentence ("Oh, and we're on dateFNS version 3, so don't use the old imports.") and
   the dropped-number check caught it.

5. **What is left on the tune half (sol + v4, 16 clips with edits, Claude judge):** 5 are the guard
   refusing a good fix (dict_0024, 0119, 0428, 0614, 0616: the raw text with its errors was pasted);
   6 are sound-alike recognition errors the prompt does not cover and a cleanup can only guess
   (`Austrian` → `Austin`, `boat` → `board`, `tent` → `temp`, `two` → `too`, `megs` → `MB`,
   `Rahman` → `Raman` in a name and an email); 1 is the dict_0399 loss below; the rest are single
   words (a kept filler "like", "refreshes" vs "refetches", a sign-off). So the next gain is in the
   guard (accept a dropped token when the reply has a close spelling of it, e.g. edit distance or
   same sound, or when the prompt asked for that kind of fix), not in more prompt text. Prompt
   changes from these clips would mean guessing homophones, which risks changing meaning.

## Harms (content dropped or meaning changed)

Read by hand from `results/<run>/harms.md` (clips a judge says the cleanup broke) and `word_loss.py`
(clips that lost ≥4 content words, whatever the judges say).

- **v1 prompt, all models: deletes too much.** Its "remove openers aggressively" and "delete false
  starts" rules drop real content, and the guard cannot see dropped words that aren't numbers or
  names:
  - luna + v1, dict_0235: the last sentence vanished ("Write it in plain, friendly language,
    something I can print on one page and hand out at the door."). Pasted.
  - luna + v1, dict_0335: "It started with release 2.14, wait 2.15. Can you find which commit caused
    this?" → "It started with release 2.14, 2.15." (the actual request is gone). Pasted.
  - luna + v1, dict_0350: "update the project model so the default scope hides archive projects.
    Actually no, don't use a default scope." → "Update the project model." Pasted.
  - luna / reserve + v1, dict_0621: "Keys under the blue pot, sorry, the green one" → "Keys under the
    green one" ("pot" lost).
  - reserve + v1, dict_0414: "I think they're in the fire safe" → "They're in the fire safe" (hedge
    turned into a fact).
  - sol + v1 and reserve + v1, dict_0346: "Ok, so in the FastAPI service" → "in the FastAPI service"
    (lower-case start; cosmetic).
- **v4 prompt: far fewer.** luna + v4 / g3 / g4 break 2 clips for the Claude judge only, both small
  (dict_0474 "but it's I think it's" → "but it's": a hedge dropped with the stammer; dict_0350 a
  capitalisation). sol + v4 breaks none for either judge; `word_loss.py` finds one real loss the judges
  did not count (the raw draft already had edits there): **dict_0399** "The oil change is it's due at
  like 42,000 miles" → "It's due at 42,000 miles" (the subject treated as a false start). Its other
  flagged clip, dict_0539, is all good fixes (`paybridge` → `PayBridge`, `idipotency` → `idempotency`,
  "asked didn't stand up" → "asked in stand-up").
- No reply was an answer to the transcript, an echo or much longer than the raw text, in any run.

## Price (API list price per 1M tokens, 2026-11-19)

| Model | Input | Cached input | Output | Source |
| --- | ---: | ---: | ---: | --- |
| gpt-6-luna | $0.10 | $0.01 | $0.50 | developers.openai.com/api/docs/pricing (Standard) |
| gpt-reserve | not listed | not listed | not listed | same page |
| gpt-6.2-sol | $2.00 | $0.10 | $10.00 | same page |
| openai/gpt-oss-20b (Groq) | $0.075 | $0.037 | $0.30 | console.groq.com/docs/model/openai/gpt-oss-20b |
| openai/gpt-oss-120b (Groq) | $0.15 | $0.075 | $0.60 | console.groq.com/docs/model/openai/gpt-oss-120b |

Rough cost per 1,000 dictations with v4 (~800 prompt tokens + ~60 transcript tokens in, ~60 out; the
mean raw text is 235 characters), before prompt caching and **without reasoning tokens** (billed as
output, not measured here): gpt-oss-20b ≈ $0.08, gpt-6-luna ≈ $0.12, gpt-oss-120b ≈ $0.17,
gpt-6.2-sol ≈ $2.30. The system prompt is the same on every call, so cached-input pricing would cut
the input share by 50–90 %.

## Recommendation

1. **Ship prompt v4 (or g3/g4) whatever the model.** It is the biggest single win (+8 to +10 ZER over
   v1 on the same model, AMEND halved) and it stops v1's harmful deletions (whole sentences dropped,
   a cancelled instruction kept as the instruction).
2. **Best quality: gpt-6.2-sol + v4** (82.1 / 80.8, nothing broken). **Best value: gpt-6-luna + v4**
   (77.5 / 76.2) at 1/20 of sol's price and about the price of gpt-oss-120b.
3. **Fix the guard next, before tuning prompts further.** With v4 the guard's name/number/link checks
   refuse correct spelling fixes (7 of 7 sol + v4 fallbacks; +2.7 / +2.6 ZER without it). Keep the
   checks (they caught a dropped sentence on v1) but accept a changed token when the reply has a
   close spelling of it at the same place (edit distance / sound-alike) or when it is a resolved
   self-correction; then re-run this eval (`relax_guard.py` shows the method).
4. **Caveats:** OpenAI models went through Codex (extra agent context, no API latency); check the
   chosen model once through the API with the app's request before shipping. Latency matters in the
   app (the VM run has a 3 s timeout) and is not measured here: sol and luna are reasoning models and
   must be timed on the API at the lowest effort. Self-corrections: the Voice Arena references keep
   them, so resolving them is neither rewarded nor punished here; decide that on product grounds.

## Files

- `offline_cleanup.py` (runner), `guard.py` (the app's guard + the relaxed variant),
  `test_offline_cleanup.py` (`python3 -m unittest discover -s eval/cleanup-offline -p 'test_*.py'`:
  the Rust guard's tests case for case, the relaxed guard, the Groq provider end to end against a fake
  server), `score.sh` (both judges), `summarize.py` (tables, `harms.md`), `word_loss.py`,
  `relax_guard.py` (counterfactuals), `run_groq.sh` (the owner's Groq command).
- `results/<run>/`: `results.csv` (harness format, transcript = pasted text, plus raw, reply, reason,
  model_ms), `cleanup.jsonl` (model replies), `scored*.csv` and judge caches, `harms.md`.
- `input/vm/`: the VM v1 run's scores and replies, copied from the Groq cleanup eval for the table.
- Budget used: Codex 1,252 calls (1,060 cleanup incl. a 3-clip smoke test + 192 judging) (cleanup + judging), Claude judge 179 clips, plus 155 Claude verdicts for the Nemotron + OpenRouter sol pairs.

## Nemotron with OpenRouter GPT-6.2-sol and v4

All 151 saved Nemotron transcripts received an API reply. The standard guard accepted 147 and returned the raw text for four dropped-name flags. Cleanup wall time was 0.972 s median and 2.126 s p90 (nearest rank). OpenRouter reported $0.29247 total.

| Judge | Paired clips | Raw ZER % | Cleaned ZER % | Fixed / regressed | Raw / cleaned AMEND % |
| --- | ---: | ---: | ---: | ---: | ---: |
| GPT-6 Astra, all | 151 | 64.9 | **84.1** | 29 / 0 | 1.35 / 0.41 |
| GPT-6 Astra, tune | 76 | 69.7 | 89.5 | 15 / 0 | 1.05 / 0.37 |
| GPT-6 Astra, check | 75 | 60.0 | **78.7** | 14 / 0 | 1.64 / 0.45 |
| Claude Opus 5.5, all | 151 | 66.2 | **84.8** | 28 / 0 | 1.16 / 0.34 |
| Claude Opus 5.5, tune | 76 | 69.7 | 88.2 | 14 / 0 | 1.03 / 0.37 |
| Claude Opus 5.5, check | 75 | 62.7 | **81.3** | 14 / 0 | 1.29 / 0.30 |

Both judges now cover all 151 pairs (Claude Opus 5.5 max, amend-va 0.2.1 prompt v1, the shared judge caches; 155 fresh Claude verdicts finished the 44 cached pairs). The judges agree: cleanup adds about 19 ZER points under each, fixes 28–29 drafts and makes no perfect draft imperfect. 27 of the fixes are common to both judges, and 126 pasted drafts are perfect under both. Fixes and regressions count transitions to and from zero edits. `paired_summary.py` regenerates this table and `changes.md`.

| Pipeline (151 clips) | Claude ZER tune / check / **all** | Astra ZER tune / check / **all** | Claude / Astra AMEND % |
| --- | --- | --- | --- |
| AssemblyAI Smart (VM run 2, pasted) | 89.5 / 86.7 / **88.1** | 89.5 / 84.0 / **86.8** | 0.26 / – |
| Nemotron 3.5 + OpenRouter GPT-6.2-sol v4, guarded | 88.2 / 81.3 / **84.8** | 89.5 / 78.7 / **84.1** | 0.34 / 0.41 |
| Groq Whisper + GPT-6.2-sol v4 (Codex), guarded | 78.9 / 85.3 / **82.1** | 80.3 / 81.3 / **80.8** | 0.44 / 0.44 |

Nemotron + sol trails AssemblyAI Smart by 3.3 (Claude) and 2.7 (Astra) points overall, and by 5.4 and 5.3 on the held-out check half. One clip is 0.66 points and the speaker-bootstrap interval is about ±8, so neither gap is significant. The check half is where it falls behind. AssemblyAI's row is a single VM run whose ZER moved 86.8 → 88.1 between runs under Claude, and its latency is key-up to text, which this offline pairing does not measure.

Dropped-name fallbacks: dict_0346, dict_0351, dict_0498, dict_0559.

Paired fixes and regressions.

One already imperfect clip, dict_0399, worsened from 11 to 25 edits under both judges (the subject treated as a false start, as with Groq + sol). It falls outside the zero-edit regression count.

## Open-weight cleanup on the same Groq Whisper drafts

All models used prompt v4 and the standard guard. Scores exclude requests without parseable replies. Claude coverage reflects exact cached Opus 5.5 verdicts only and favors familiar wording. GPT-6 Astra used cached verdicts plus fresh judgments. Tune/check/all entries are ZER percentages. Fixed and broken count transitions to and from zero edits relative to raw on the same clips.

| OpenRouter model | API replies / 151 | Codex n; tune / check / all | Claude n; tune / check / all | Fixed / broken (Codex; Claude) | Fallbacks / missing | Median / p90 seconds | Recorded USD | USD/M input / output |
| --- | ---: | --- | --- | --- | --- | ---: | ---: | ---: |
| deepseek/deepseek-v4.1-flash | 149 | 149; 77.3 / 75.7 / 76.5 | 138; 80.6 / 80.3 / 80.4 | 27/0; 22/0 | bad_reply 2, dropped_link 1, dropped_name 1, dropped_number 4 | 1.71 / 5.96 | 0.04320 | 0.0507 / 0.2028 |
| deepseek/deepseek-v4-pro | 147 | 147; 78.4 / 78.1 / 78.2 | 138; 81.2 / 82.6 / 81.9 | 28/0; 24/0 | bad_reply 4, dropped_name 1, dropped_number 3 | 4.18 / 11.19 | 0.20743 | 0.9 / 1.5 |
| google/gemma-4-31b-it | 146 | 146; 78.7 / 78.9 / 78.8 | 143; 79.5 / 82.9 / 81.1 | 28/0; 26/0 | bad_reply 5, dropped_link 1, dropped_name 3, dropped_number 5 | 3.48 / 7.57 | 0.04604 | 0.2 / 0.4 |
| z-ai/glm-5.3-flash | 151 | 151; 76.3 / 76.0 / 76.2 | 135; 80.0 / 86.2 / 83.0 | 28/0; 22/0 | dropped_link 1, dropped_name 1, dropped_number 3 | 0.91 / 2.35 | 0.01833 | 0.15 / 0.5 |
| mistralai/mistral-large-2512 | 151 | 151; 69.7 / 69.3 / 69.5 | 117; 72.6 / 78.2 / 75.2 | 22/4; 13/1 | dropped_link 2, dropped_name 1, dropped_number 12 | 1.38 / 2.30 | 0.03604 | 0.5 / 1.5 |
| qwen/qwen3.5-397b-a17b | 31 | 31; 73.3 / 75.0 / 74.2 | 29; 71.4 / 80.0 / 75.9 | 5/0; 4/0 | bad_reply 24, not_run 96 | 6.59 / 92.17 | 0.25550 | 0.45 / 3 |

Costs and times cover recorded parseable replies. The API may bill malformed replies separately. Each model link lists every matched draft whose edit count increased. Price source: [OpenRouter model catalog](https://openrouter.ai/api/v1/models).

For context, GPT-6.2-sol via Codex with v4 scored 80.8% overall and 81.3% on check under GPT-6 Astra. The separate on-device Gemma 4 E2B + v3 evaluation on Groq Whisper scored 72.2% overall under Astra and 73.5% under Claude; its raw baseline and prompt differed slightly.
**GLM-5.3-Flash + v4 is the strongest open candidate for an app trial on these drafts.** Counting all 151 clips as the app would paste them, including raw fallback for malformed replies, GPT-6 Astra gives GLM, Gemma 31B and DeepSeek Pro 76.2% ZER, DeepSeek Flash 75.5%, and Mistral 69.5%. GLM returned a usable reply for every clip and its guarded output never increased the judge’s edit count. Its $0.01833 recorded cost and 0.91 s median are the lowest of the five finished runs. GPT-6.2-sol + v4 still leads on the same Whisper drafts (80.8%). Its Codex CLI latency is unsuitable for this comparison.

**Long-draft failures change the ranking.** On dict_0255, Gemma ended a game-night food request after “eight adults, and”. GPT-6 Astra’s edits rose from 4 to 267. DeepSeek Flash and Pro also truncated long drafts. Mistral increased the edit count on nine drafts, including four that were already perfect. GLM fixed 28 imperfect drafts to zero edits.

Qwen’s owner batch ended with 31 usable replies, 24 malformed replies and 96 clips without a completed result. Even its check score covers only a small subset; keep Qwen out of the ranking. The completed runs and the partial Qwen scores required no additional owner-paid cleanup.
