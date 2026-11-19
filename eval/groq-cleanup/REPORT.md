# Groq Whisper + a Groq-hosted LLM cleanup: Voice Arena public set

Experiment, not for `main`. Run on 2026-11-19 in the Lume VM `dictation-vm-eval-2`, same clips,
harness commit, timing, judge prompt and scoring as every run in `eval/voicearena/`
([README](../voicearena/README.md), [REPORT](../voicearena/REPORT.md)). Code: branch
`exp/groq-cleanup`, draft PR #324. **Unofficial: public set only, single run per version.**

## The short answer

| | ZER, Claude judge (tune / check / **all**) | ZER, Codex judge (all) | AMEND, Claude (all) | Key-up to text, median / p90 |
| --- | --- | ---: | ---: | ---: |
| Groq Whisper, no cleanup (earlier run) | 55.3 / 65.3 / **60.3** | – | 1.43 % | 0.63 / 1.12 s |
| **v1** gpt-oss-20b + Blurt prompt | 61.8 / 68.0 / **64.9** | 64.2 | 1.45 % | 1.15 / 2.50 s |
| **v2** gpt-oss-120b + Blurt prompt | 64.5 / 65.3 / **64.9** | 64.9 | 1.26 % | 1.45 / 3.18 s |
| AssemblyAI Smart (earlier run) | 86.8 / 86.7 / **86.8** | – | 0.26 % | 1.15 / 2.80 s |
| *Offline (PR #327), same raw text as v1:* gpt-6.2-sol + v4 | 78.9 / 85.3 / **82.1** | 80.8 | 0.44 % | not measured |

- **Groq's own models don't get close to AssemblyAI with the Blurt prompt.** Both gpt-oss models add
  about 5 ZER points over the uncleaned drafts of the same run on the whole set, and land at 64.9 %.
  Against each run's own raw drafts the gain is +4.6 (v1) and +10.6 (v2). The two runs' raw drafts
  differ (60.3 vs 54.3): Whisper output varies between runs, so compare each version with its own
  raw drafts.
- **Added latency:** the cleanup itself takes a median **0.47 s (v1) / 0.55 s (v2)**, p90 **1.69 s /
  1.45 s**, max 3.0 s / 2.2 s, measured inside the app. Key-up to text goes from 0.63 s to about
  1.2 to 1.5 s median, about AssemblyAI Smart's 1.15 s.
- **The prompt matters more than the model.** The v3/v4 prompts written from v1's tune-half failures
  were not run on the VM (owner's decision, below); offline on v1's raw text they lift every model by
  8 to 10 points (PR #327). With v4, OpenAI's gpt-6.2-sol reaches 82.1 / 80.8. It is within about 5
  points of AssemblyAI, and none of its drafts got worse than the raw text.
- **Harms:** gpt-oss with the Blurt prompt sometimes deletes real content: a whole opening sentence, a
  closing request, "like you wanted", "I think". In one long clip it removed the correction cue from
  "Tuesday the 14th, no wait, Wednesday the 15th", so the message gave two dates. The guard missed
  these, because they drop no number, link or mid-sentence name. It did catch two other deletions: a
  dropped sentence and a dropped greeting. Details below.

## Setup

- **Builds.** v1: `exp/groq-cleanup` with the cleanup as first pushed (`8a0b810` plus a UI-fixture
  commit; the cleanup code is the code later pushed as `a4ab3a2`). v2: `a88fb35` (adds `finish_reason`
  and token usage to the dump, and a larger token budget for reasoning models: transcript/2 + 4096
  instead of + 1280). The relaxed number guard (`a914e7b`) was not run on the VM. Debug builds with
  the dev certificate.
- **Request:** the transcript text only, sent to Groq `POST /openai/v1/chat/completions` with the same
  key, the prompt as the system message, `<transcript>…</transcript>` as the user message,
  `temperature: 0`, `reasoning_effort: "low"`, `include_reasoning: false`, timeout 3 s. The guard
  falls back to the raw text when the reply is empty, an echo, longer than 1.25 × + 20 characters,
  or missing a number, email/URL/dotted name or mid-sentence capitalised name.
- **Models** (console.groq.com/docs/models, checked 2026-11-19): `openai/gpt-oss-20b` (fast, small,
  1000 tok/s) and `openai/gpt-oss-120b` (stronger, 500 tok/s). The brief's first choice,
  `llama-3.1-8b-instant` / `llama-3.3-70b-versatile`, was shut down by Groq on 2026-08-16 (HTTP 404
  in the first trial, `results/groq_clean_v1_limit5`, where all 5 clips fell back to the raw text).
- **Prompt v1** = Blurt's Smart instruction as PR #322 adapted it, plus a first paragraph: the
  transcript is data between tags, never answer it, return only the cleaned text
  (`prompts/v1-blurt-smart.txt`, MIT, credited in `NOTICE`).
- **Dump:** every run had `VOCET_EVAL_DUMP_DIR` on. Each results folder holds `groq-replies.jsonl`
  (raw Whisper text, reply, pasted text, reason, model, prompt sha, cleanup ms, finish reason and
  token counts from v2 on) and `groq_replies.csv` joined to `clip_id` (`join_dump.py`, by time;
  pasted text equals the TextEdit text for 151 of 151 clips in both runs). `harness_results.csv` has
  `raw_text`, `cleaned_text`, `cleanup_reason` and `cleanup_ms` columns. `<run>_raw/` re-scores the
  raw Whisper text of the same run.
- **Judges:** Claude Opus 5.5, effort max, amend-va 0.2.1 prompt v1 via `claude -p`
  (`score_claude_cli.py`), and GPT-6 Astra, effort max, via `codex exec` (`score_codex_cli.py`,
  from `eval/codex-judge`). `score.sh` seeds each run's judge cache with every earlier answer, so a
  byte-identical draft keeps its verdict across runs. That is why `groq_clean_v1_raw` reproduces the
  earlier 60.3 % exactly: 147 of its 151 drafts were already judged. No Codex usage or rate limit
  was hit.
- **Split:** `split.csv`, fixed by the Lead for all cleanup evals. Tune is the speakers whose
  `sha256(speaker_id)` starts with an even hex digit: 76 clips, 37 speakers. Check is the rest: 75
  clips, 34 speakers. Prompt changes (v3, v4) were made from tune-half failures only, with generic
  examples, no clip text.

## Every version, both halves, both judges

ZER % (higher is better) and AMEND % (lower is better). 95 % intervals: speaker bootstrap over the
whole set. One clip is 0.66 points of the whole set and 1.3 points of a half, and the interval is
about ±8 points, so differences under about 5 points are noise.

| Run | Claude ZER tune / check / all (95 % CI) | Codex ZER tune / check / all | Both judges agree, ZER all | AMEND Claude / Codex | Fallbacks to raw |
| --- | --- | --- | ---: | --- | --- |
| Groq raw, earlier run (`vocet_groq`) | 55.3 / 65.3 / 60.3 (52.1–68.4) | – | – | 1.43 / – | – |
| v1 raw drafts (`groq_clean_v1_raw`) | 55.3 / 65.3 / 60.3 (51.9–68.3) | 55.3 / 60.0 / 57.6 | 57.0 | 1.34 / 1.32 | – |
| **v1** gpt-oss-20b + v1 prompt | 61.8 / 68.0 / **64.9** (55.3–73.7) | 64.5 / 64.0 / **64.2** | 62.9 | 1.45 / 1.37 | 10 / 151: empty 5, timeout 2, number 2, name 1 |
| v2 raw drafts (`groq_clean_v2_raw`) | 51.3 / 57.3 / 54.3 (46.2–62.1) | 52.6 / 54.7 / 53.6 | 52.3 | 1.65 / 1.60 | – |
| **v2** gpt-oss-120b + v1 prompt | 64.5 / 65.3 / **64.9** (55.7–73.3) | 67.1 / 62.7 / **64.9** | 62.3 | 1.26 / 1.15 | 2 / 151: name 2 |
| v3, v4 (VM) | not run (owner decision) | | | | |
| AssemblyAI Smart (`vocet_assemblyai`) | 86.8 / 86.7 / 86.8 (80.2–92.5) | – | – | 0.26 / – | 0 |

Against each run's own raw drafts:

| Run | Drafts changed by cleanup | Better / worse (Claude edits) | Fixed (≥1 edit → 0): Claude; Codex | Broken (0 → ≥1): Claude; Codex |
| --- | ---: | --- | --- | --- |
| v1 | 48 | 18 / 7 | 11; 12 | 4; 2 |
| v2 | 57 | 28 / 2 | 16; 17 | 0; 0 |

v2 is the better cleanup (more fixes, nothing broken, lower AMEND), but its ZER equals v1's: its raw
drafts started 6 points lower. On the whole set **the best VM version is v2 (gpt-oss-120b + v1
prompt)**, by AMEND and by fixed/broken. The two are tied on ZER.

### Offline cross-reference (PR #327, `eval/cleanup-offline`)

The offline worker ran the app's request and guard (ported check for check) on v1's 151 raw texts,
with the same judges and split. These rows are quality only: Codex adds its own agent prompt, and its
timing is not API latency.

| Offline model + prompt | Claude ZER tune / check / all | Codex ZER all | AMEND Claude |
| --- | --- | ---: | ---: |
| gpt-6-luna + v1 | 69.7 / 69.3 / 69.5 | 67.5 | 1.40 |
| gpt-6.2-sol + v1 | 73.7 / 69.3 / 71.5 | 69.5 | 0.81 |
| gpt-6-luna + v3 (as pushed here) | 77.6 / 76.0 / 76.8 | 74.8 | 0.48 |
| gpt-6-luna + v4 | 75.0 / 80.0 / 77.5 | 76.2 | 0.48 |
| **gpt-6.2-sol + v4** | 78.9 / 85.3 / **82.1** | **80.8** | 0.44 |
| gpt-6.2-sol + v4, no guard (counterfactual) | – / – / 84.8 | 83.4 | – |

The check half holds up for v3/v4 (luna + v3: 76.0 check; sol + v4: 85.3), so the prompt changes made
from tune-half failures generalise, not overfit. Offline gpt-oss runs with v3/v4 were pending (they
need the owner's Groq key) when this was written, so I can't yet say how much v3/v4 lifts gpt-oss.

## What the prompt changes were (v3, v4)

All from v1's **tune-half** failures (`failures.py`), with made-up examples, never clip text:

- **v3** (`prompts/v3.txt`):
  - Never delete a whole sentence or clause, and delete an opener word only, not the sentence it
    opens. v1 dropped an opening sentence (tune dict_0255) and "right now", "I think" and a
    comparative "like" elsewhere.
  - "like" is kept when it compares or means "such as".
  - Self-corrections are kept as spoken, both values and the cue. Resolving them tripped the number
    guard, which then pasted the raw draft (tune dict_0053, dict_0257).
  - Remove restarts ("Can you? Can you send it?", "I was going to. Let's…"), missed in tune dict_0138
    and dict_0497.
  - Write spoken symbols as symbols only inside emails, URLs and paths (tune dict_0008).
  - Drop a standalone final "Thank you." only when the text isn't addressed to a person: the Whisper
    silence hallucination, in tune dict_0024 and dict_0258.
  - Capitalise a sentence start after a deletion, and drop commas left dangling by one ("without,
    wrecking").
- **v4** (`prompts/v4.txt`): v3, plus two changes.
  - Self-corrections are resolved. The Lead asked for this, because users want only the final value.
    It needs the relaxed number guard of `a914e7b`.
  - Fix clearly misheard technical words, and write code identifiers in code form. Many tune failures
    were git/node/WebSocket/read mishearings.
- **v4b** (`prompts/v4b.txt`): v3 with the self-correction change only, so the two v4 changes can be
  told apart offline.

## Harms: content dropped or meaning changed

The clips where the cleaned draft is worse than the same run's raw draft (Claude edits), read by hand.
Both texts are in `results/<run>/groq_replies.csv`.

| Run | Clip (half) | Raw → cleaned edits | What the cleanup did |
| --- | --- | --- | --- |
| v1 | dict_0255 (tune) | 4 → 82 | **Dropped the whole first sentence:** "So, I'm hosting a game night this weekend, eight adults, and I need to figure out food." |
| v2 | dict_0255 (tune) | 13 → 39 | **Dropped the last request:** "Oh, and a shopping list group by store section two." |
| v1 | dict_0559 (check) | 3 → 36 | Dropped "So I'm not sure you'll remember me, but" and the signed name "Daniel." at the end |
| v1, v2 | dict_0534 (check) | 0 → 16; 7 → 25 | **"Tuesday the 14th, no wait, Wednesday the 15th" became "Tuesday the 14th, Wednesday the 15th":** the cue went, both dates stayed, so the reader can't tell which day. v1 also dropped "like you wanted", v2 dropped "Oh, and on the budget," |
| v1 | dict_0474 (tune) | 0 → 8 | Dropped "I think" ("but I think it's showing $2,400") |
| v1 | dict_0041 (tune) | 0 → 5 | Dropped a meaningful "like" ("read them from the config service like we do everywhere else") |
| v1 | dict_0279 (check) | 7 → 8 | Dropped a misheard start ("Just as like" for "Jest is"), as AssemblyAI did |

None of these tripped the guard: the numbers, links and mid-sentence names survived, and "Daniel."
starts a sentence.

What the guard did catch, all 12 fallbacks read by hand:

- **2 real harms, caught by the name check.**
  - v1 dict_0533 dropped the whole sentence "Okay, update on the Regional Sales Summit."
  - v2 dict_0504 dropped the greeting "Hi Jordan."
- **2 resolved self-corrections blocked** (v1 dict_0053 "4.2, wait, no, 4.3" and dict_0257 "9,
  sorry, 10"): the reply kept the right value. `a914e7b` now lets these through.
- **1 false positive** (v2 dict_0525): the raw text ran sentences together ("…15th So if…"), so "So"
  looked like a mid-sentence name.
- **5 empty replies and 2 timeouts**, all from gpt-oss-20b, on long clips. v2 (120b) never came back
  empty: `finish_reason` was stop on all 152 replies, with a median of 33 reasoning tokens and a max
  of 457.

## Latency

| | Cleanup (in the app): median / p90 / max | Key-up to text: median / p90 | Change in key-up to text against `vocet_groq`, same clip: median / p90 |
| --- | --- | --- | --- |
| v1, gpt-oss-20b | 465 / 1694 / 3003 ms | 1.15 / 2.50 s | +0.47 / +1.50 s |
| v2, gpt-oss-120b | 547 / 1452 / 2244 ms | 1.45 / 3.18 s | +0.77 / +2.37 s |

Groq's own figures for v2 (from `usage`): median total_time 0.30 s, queue 0.17 s. The rest of the
~0.55 s is the network round trip and connection setup. Pre-warming the connection, or overlapping
it with the Whisper upload, could save part of it. v2's key-up-to-text figures also include more
Whisper variance: the host was loaded (a gate build) during part of the run, and the Mac slept for
8 minutes during it (below). Treat the in-app cleanup time as the cleaner measure of what cleanup
adds.

## How the runs went

- v1: 151 / 151 clips, 09:28–10:39, no reruns.
- v2: 151 / 151 clips, 10:41–12:05. **The owner's Mac slept from 11:53 to 12:01** while the harness
  held the key for dict_0663, so Vocet recorded for 8.5 minutes. That clip was rerun alone at 12:08
  (`harness_rerun_dict_0663.log`); its dump line and draft come from the rerun. No other clip near
  the sleep was odd (all ok, first attempt, normal latency). **1 clip rerun.**
- The first push of this branch failed once on a timing test of the gate (`test-runner.sh`, a 35 s
  job vs a 30-poll bound) under host load, and passed on retry.

## Recommendation

1. **Don't ship gpt-oss + the Blurt prompt as is.** About +5 ZER (64.9 %) for +0.5 s median and
   +1.5 to 2.4 s p90, with occasional silent content loss, is not worth it.
2. **The prompt is the lever.** Use v3/v4-style rules (keep meaning-carrying clauses, restarts out,
   symbols in addresses, phantom "Thank you." out, misheard terms fixed). Offline they take a cheap
   model to 77 % and gpt-6.2-sol to 82 %. Confirm the winner end to end on the VM once, for latency.
3. **If the provider must stay Groq,** run gpt-oss-120b with v4 offline (the owner's `run_groq.sh`)
   before deciding. 20b's empty replies and timeouts rule it out for real-time use. **If OpenAI is
   acceptable,** add an OpenAI cleanup option: the app already keeps OpenAI keys for the OpenAI speech
   provider. The model is gpt-6-luna for price, or gpt-6.2-sol for quality. That still has to be
   measured for latency through the API, not through Codex.
4. **Change the guard** (PR #324 notes):
   - Allow numbers the raw text itself corrects. Done in `a914e7b`: a number followed within six
     words by sorry/no/actually/wait/make that/… and then another number.
   - Accept a "dropped" name, link or number when the reply has a close replacement in the same
     place: edit distance, or the same letters ignoring case and separators (`dateFNS` → `date-fns`,
     `billing.tease` → `billing.ts`, `Pyndanic` → `Pydantic`, `$17.50` → `$1,750`). That lets
     misheard-term fixes through; offline, the strict guard cost sol + v4 2.7 points.
   - Block only real deletions: a whole number, email or URL with nothing in its place.
   - Add the check that would have caught the harms above: fall back when the reply drops a run of
     6 or more consecutive non-filler words of the raw text. That catches lost sentences and
     clauses. Also fall back when it removes a correction cue but keeps both values.
5. Trimming trailing silence before the Whisper upload would still be worth doing on its own: it
   removes the "Thank you." hallucination at its source, whatever the cleanup does.

## Files

- `split.csv`: the tune/check split. `prompts/`: v1, v3, v4, v4b.
- `join_dump.py`: joins the dump to the harness rows, adds the text columns, writes `<run>_raw/`.
- `score.sh <run> [claude|codex]`: judges with a seeded cache, then prunes the cache to the run's
  own answers.
- `summarize.py`: the tables above, `halves.json` per run. `failures.py`: reads one half's failing
  clips locally (prints reference text, writes nothing).
- `results/groq_clean_v1*`, `groq_clean_v2*` (in `eval/voicearena/results/`): harness rows, dump,
  joined CSV, both judges' scores and caches. `groq_clean_trial2_limit5`: the first 5-clip check
  with gpt-oss-20b. `groq_clean_v1_limit5`: the 5-clip trial against the retired Llama model (all
  HTTP 404, guarded to raw).
