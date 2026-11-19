# Two judges on the Voice Arena public set: Claude and Codex

An experiment on branch `eval/codex-judge` (from `eval/voicearena`, PR #323),
not for `main`. It adds a second AMEND judge next to Claude Opus 5.5 and
rescores Vocet's four finished runs of the 151-clip public set with it, plus
AssemblyAI run 2 and its verbatim text.

**Unofficial: two judges (Claude Opus 5.5 via Claude Code, GPT-6 Astra via
Codex), public set only.** Run on 2026-11-19.

## The short answer

- **The rankings and conclusions don't change.** Both judges put the four
  engines in the same order on ZER and on AMEND. Vocet Cloud (AssemblyAI) is
  still first on the public set, Groq still about 13th, and the on-device
  engines still about 14th, under either judge or both together.
- **Codex is a little stricter.** Its ZER is 1.3 to 3.3 points lower per
  engine (2.6 pooled; 95% speaker-bootstrap interval −5.0 to −0.8). AMEND
  barely moves (±0.05 points).
- **The two judges agree about as well as the paper's Opus/Astra pair.** Same
  verdict on 96.7% of drafts (κ 0.931), against the paper's 97.6% (κ 0.946).
  Same edit count on 92.7%, against 94.2%.
- **Where they disagree, Codex adds the edit 18 times out of 20.** Mostly
  word forms Claude accepts ("archive" → "archived", "Watch" → "Watched"), a
  missing unit ("350" → "350°F") and leftover false starts. Asked again, Codex
  gives the same verdict on 18 of those 20 drafts: a steady difference in how
  the two judges read the rules, not random noise.
- **Repeat-run noise and the cleanup gain look the same under both judges.**
  AssemblyAI run 1 → run 2: +1.3 ZER points under each judge, with 10 (Claude)
  and 8 (Codex) drafts flipping verdict. Verbatim → cleaned (run 2): +17.2
  (Claude) and +18.5 (Codex). Neither judge finds a draft that cleanup broke.

## Second judge (Codex)

### The judge

| | Claude judge (`score_claude_cli.py`) | Codex judge (`score_codex_cli.py`) |
| --- | --- | --- |
| Model | Claude Opus 5.5 (`claude-opus-5-5`) | **GPT-6 Astra** (`gpt-6-astra`) |
| Effort | max | max |
| Route | `claude -p` (Claude Code CLI), Claude subscription | `codex exec` (Codex CLI 0.166.0), ChatGPT plan |
| Prompt | amend-va 0.2.1 `v1`, `build_prompt`, fingerprint `8ed04caf4663` | the same bytes |
| Output | `{"fixed": string}` schema, `json.loads(...)["fixed"].strip()` | the same (`--output-schema`) |
| Scoring | amend-va `char_edits`, pooled AMEND, ZER = 0 edits | the same |

**Why GPT-6 Astra:** it is the OpenAI judge on Voice Arena's own five-judge
panel (whitepaper section 8.1, effort max), so its numbers can be compared with
the paper's per-judge tables. Codex offers it on the owner's plan. Codex
lists newer models (GPT-6.2 Astra and Sol), but using a newer model would
break that comparison. Effort `max` is the panel's setting and the highest
plain level Codex offers. `ultra` also lets the model hand work to helper
agents, which a judge must not do.

**What differs from the paper's API call.** Codex isn't a bare API client.
The script replaces Codex's agent instructions with the Claude judge's
one-line system prompt and turns off its environment, permissions,
collaboration, apps and skills blocks, web search, shell tools and hooks. The
sandbox is read-only. Codex still sends its tool definitions and a short
multi-agent role note (about 5.7k input tokens), and the CLI can't turn those
off. The judge called no tools: each answer is a single JSON message. Before an
answer is cached, the script checks Codex's run header for `gpt-6-astra` /
`max` / `read-only`. The header shows what the client asked for. Codex doesn't
report which model the server actually used.

**Calls:** 599 judge calls for the four runs (5 smoke test, then 145 + 151 +
149 + 149, one per unique prompt), 296 for AssemblyAI run 2 and its verbatim
text (148 + 148), 40 for the stability check below, and 3 setup probes: about
940 in all, under the ~2,000 budget (first ~1,000). No failures, retries or usage-limit messages. 151 drafts take about
4 minutes with 8 workers.

**Smoke test:** on the first 5 Groq clips the answers parsed with amend-va's
own parsing, and every edit count matched Claude's (0, 6, 1, 10, 17).

### Per judge

| Engine | ZER Claude | ZER Codex | Codex − Claude (95% CI) | AMEND Claude | AMEND Codex |
| --- | ---: | ---: | --- | ---: | ---: |
| Vocet Cloud (AssemblyAI) | 86.8% | 85.4% | −1.3 (−4.0 to +1.2) | 0.26% | 0.31% |
| Groq `whisper-large-v3-turbo` | 60.3% | 57.0% | −3.3 (−6.3 to −0.7) | 1.43% | 1.46% |
| Parakeet (on-device) | 53.0% | 49.7% | −3.3 (−6.2 to −0.7) | 1.38% | 1.36% |
| Qwen3 (on-device) | 49.0% | 46.4% | −2.6 (−5.9 to +0.0) | 1.70% | 1.72% |
| All 604 drafts | 62.3% | 59.6% | −2.6 (−5.0 to −0.8) | 1.20% | 1.21% |
| AssemblyAI run 2 | 88.1% | 86.8% | −1.3 (−3.4 to +0.0) | 0.26% | 0.28% |
| AssemblyAI run 2, verbatim text | 70.9% | 68.2% | −2.6 (−5.5 to −0.6) | 0.99% | 1.01% |

"All 604 drafts" pools the four engines' first full runs. Run 2 and the
verbatim text are listed separately.

The intervals resample speakers (`compare_judges.py`). Mean edits per draft:
Claude 2.71, Codex 2.75.

### Both judges together

"Strict" counts a draft as ready only if **both** judges make no edit;
"lenient" counts it if **either** does. That gives the range a two-judge vote
could land in.

| Engine | ZER strict (both) | ZER lenient (either) |
| --- | ---: | ---: |
| Vocet Cloud (AssemblyAI) | 84.8% | 87.4% |
| Groq | 57.0% | 60.3% |
| Parakeet | 49.7% | 53.0% |
| Qwen3 | 45.7% | 49.7% |

### Agreement, against the paper's Opus/Astra pair

| | Same verdict | Cohen's κ | Identical edit count | Edit-count ρ |
| --- | ---: | ---: | ---: | ---: |
| This run, Claude · Codex, 604 drafts | **96.7%** | **0.931** | **92.7%** | **0.955** |
| Paper, Opus 5.5 · GPT-6 Astra, 10,626 drafts (9.6) | 97.6% | 0.946 | 94.2% | 0.949 |
| Paper, mean of its 10 judge pairs | 97.0% | 0.930 | 93.2% | 0.933 |
| AssemblyAI only | 97.4% | 0.889 | 96.7% | 0.891 |
| Groq only | 96.7% | 0.932 | 93.4% | 0.966 |
| Parakeet only | 96.7% | 0.934 | 90.1% | 0.962 |
| Qwen3 only | 96.0% | 0.920 | 90.7% | 0.962 |
| AssemblyAI run 2 | 98.7% | 0.940 | 96.0% | 0.949 |
| AssemblyAI run 2, verbatim | 97.4% | 0.938 | 95.4% | 0.959 |

The two judges also return the same corrected text, character for character,
on 88.7% of drafts. AssemblyAI's κ is lower only because 87% of its drafts
need no edit, which leaves little room above chance. Its raw agreement is the
highest.

The agreement matches the paper's pair, slightly lower and well inside the
paper's range of pairs (κ 0.910–0.960). **The strictness gap doesn't match.**
In the paper, Astra's ZER is only 0.3 points below Opus's (67.2% against
67.5%, all drafts). Here Codex is 2.6 points below Claude. Possible causes,
which this run can't separate: the Codex route (its tool definitions and
role note), a different served model version, and the draft mix. The paper's
drafts average a ZER near 67%, and Vocet's weaker engines produce more
borderline drafts, which is where judges differ. The gap is
smallest for the strongest engine (AssemblyAI, −1.3), which fits the
draft-mix explanation.

### Where they disagree

20 of 604 drafts get different verdicts, on only 11 clips. The same clip
splits the judges across engines (dict_0350 for all four engines, dict_0069
and dict_0302 for three), so a disagreement follows the clip's wording, not
the engine. Codex makes the edit in 18; Claude in 2. All 20 are listed in
[disagreements.csv](disagreements.csv), without the dataset's references.

| Pattern | Drafts | Example (draft → Codex's fix; Claude left it) | Which reading fits the prompt |
| --- | ---: | --- | --- |
| **Word form of an ordinary word**: tense or participle | 10 | "nullable archive timestamp" → "archived", "add a name scope" → "named" (Rails, 4 engines); "add a confirmed password field" → "confirm" (3); "Watch the sun rise…" → "Watched" (2, a diary note); "the bank closes early" → "closed" | The prompt exempts "tense … changes that keep the meaning". Claude applies that. Codex treats these as wrong words, which is fair where the meaning shifts ("Watch" reads as an order; "named scope" is a Rails term) |
| **Exact text, unit or format** | 5 | "Bake at 350" → "350°F" (3); "user's" → "users" and "ilike" → "ILIKE" in a SQL request; "United1208" → "United 1208" | A missing unit and SQL code count as exact text, so Codex fits. The flight number is formatting, which the prompt exempts, so Claude fits |
| **Leftover false start or discourse word** | 3 | "but it's I think it's showing" → "but I think it's showing" (2); "Also…", "Last thing, a…" in a long recap | The prompt says to delete repeated text and leading discourse words, so Codex fits |
| Claude only: grammar slip | 1 | "Anywhere near you work." → "works" | The prompt exempts small grammar slips, so Codex fits |
| Claude only: stray word | 1 | "sort them in together into" → drop "in" | A garbled extra word, so Claude fits |

Neither judge is plainly more correct. Codex applies the "fix these" list
more literally (exact text, fillers, wrong words). Claude applies the "do not
fix" list more widely (tense, formatting). On the Vocet-specific questions in
the main report, Codex's extra edits add a little weight to "fillers and
false starts kept" and "code names" as failure causes. They don't add a new
category.

**Is the gap stable?** Codex was asked the 20 disputed drafts again
([rejudge_disagreements.py](rejudge_disagreements.py),
[rejudge.json](rejudge.json)), with 20 random drafts the judges agree on as a
control. It gave the same verdict on 18 of 20 disputed drafts and on 20 of 20
controls. The two that flipped were "Watch → Watched" for Groq and the flight
number for Parakeet. So Codex is consistent with itself, and the two judges
differ steadily in strictness: the disagreements aren't sampling noise.

### AssemblyAI run 2 and the verbatim text, under both judges

The verbatim folder has no harness CSV: its drafts come from the reply dump
(`aai_split.py join`). So both run-2 folders were Codex-scored from a
harness-style CSV built from their Claude `scored.csv` (`clip_id`, `status`,
`finalize_s`, `transcript` = `draft`). That makes the drafts, and so the
prompts, identical for both judges, and `compare_judges.py` checks it.

**Run 1 against run 2** (the same engine on the same 151 clips, run twice;
122 drafts came out identical):

| Judge | ZER run 1 | ZER run 2 | Change | Drafts that flip verdict (ok → edit / edit → ok) |
| --- | ---: | ---: | ---: | --- |
| Claude | 86.8% | 88.1% | +1.3 | 10 (4 / 6) |
| Codex | 85.4% | 86.8% | +1.3 | 8 (3 / 5) |

Both judges see the same run-to-run change. Run 2 was judged with its own
cache, so each judge answered the 122 unchanged drafts a second time, which
doubles as a self-consistency check. Each judge gave the same verdict and the
same edit count on 121 of the 122. The one flip is the same clip, dict_0726,
for both judges: a borderline draft rather than a flaky judge. The other
flips (9 under Claude, 7 under Codex) are among the 29 drafts whose text
changed. For AssemblyAI, the run-to-run noise (+1.3 points, 8 to 10 drafts
flipping) is as large as the gap between the judges (1.3 points, 2 to 4 drafts
per run). So a second run and a second judge each add about the same
uncertainty to a single-run, single-judge ZER.

**Verbatim against cleaned** (run 2: the transcript before and after
AssemblyAI's cleanup, judged as if each had been pasted; 87 drafts are the same
text in both):

| Judge | ZER verbatim | ZER cleaned | Gain | Fixed by cleanup / broken by cleanup |
| --- | ---: | ---: | ---: | --- |
| Claude | 70.9% | 88.1% | +17.2 | 26 / 0 |
| Codex | 68.2% | 86.8% | +18.5 | 28 / 0 |

The main report's finding holds under the second judge: cleanup lifts ZER by
about 17 to 18 points and breaks no draft. Codex credits cleanup with two more
fixes, because it is stricter about the fillers and false starts that cleanup
removes.

### Rankings

Placed in Voice Arena's public-set column (whitepaper 10.6; 14 tools, the
paper's ZER is a five-judge vote, its AMEND is Opus alone):

| Engine | ZER rank, Claude | ZER rank, Codex | ZER rank, both judges | AMEND rank, Claude | AMEND rank, Codex |
| --- | --- | --- | --- | --- | --- |
| Vocet Cloud (AssemblyAI) | 1st (86.8, Aqua 83.4) | 1st (85.4) | 1st (84.8) | 1st (0.26, Wispr 0.34) | 1st (0.31) |
| AssemblyAI run 2 | 1st (88.1) | 1st (86.8) | 1st (86.8) | 1st (0.26) | 1st (0.28) |
| Groq | ≈13th (Superwhisper 61.6 > 60.3 > Perplexity 53.0) | ≈13th (57.0) | ≈13th (57.0) | 12th (1.43) | 12th (1.46) |
| Parakeet | ≈13–14th (53.0, ties Perplexity) | ≈14th (49.7) | ≈14th (49.7) | ≈11–12th (1.38, ties Kivi) | ≈11th (1.36) |
| Qwen3 | ≈14th (49.0) | ≈14th (46.4) | ≈14th (45.7) | 12th (1.70, Perplexity 1.80) | 12th (1.72) |

Every engine stays in the same place, and the order of the four Vocet engines
is the same under both judges (ZER: AssemblyAI > Groq > Parakeet > Qwen3;
AMEND: AssemblyAI < Parakeet < Groq < Qwen3). The one change is Parakeet: under
Codex it drops clearly below Perplexity on ZER instead of tying it.
Comparisons stay apples to oranges for ZER: a single judge against the paper's
five-judge vote, and Codex runs stricter than the paper's own Astra did. The
Claude column remains the closest to like for like (see the main
[REPORT.md](../voicearena/REPORT.md)).

**Conclusions from the main report that still hold:** AssemblyAI is first on
the public set by both measures; the gap between AssemblyAI and Groq (26.5
ZER points under Claude, 28.4 under Codex) is ten times the judges' gap; the
on-device engines trail Groq; most remaining errors are fillers, misheard
words and code names; AssemblyAI's cleanup adds about 17 to 18 ZER points and
breaks nothing.

### Not done

- The `_check` (1 clip) and `_limit5` (5 clips) trial folders weren't
  rescored: their drafts are already in the full runs.
- The other three panel judges (Gemini 4 Argon, Muse Wonder, Kimi K3.1 Pro)
  have no CLI route here, so a 3-of-5 vote still can't be reproduced.

## Files

| File | What it is |
| --- | --- |
| `../voicearena/score_codex_cli.py` | the Codex judge (README: "Second judge (Codex)") |
| `../voicearena/results/<engine>/scored_codex.csv` | Codex-judged rows, same columns as `scored.csv` |
| `../voicearena/results/<engine>/scored_codex.judge.jsonl` | each Codex answer, keyed by prompt sha256 (corrected text, model, effort, tokens, route) |
| `compare_judges.py` | the tables above; writes `disagreements.csv` and, with `--json`, `summary.json` |
| `summary.json` | the numbers, per engine and pooled |
| `disagreements.csv` | the 20 drafts with different verdicts in the four first runs, both corrected texts |
| `rejudge_disagreements.py`, `rejudge.json` | the stability check |

```sh
python3 eval/codex-judge/compare_judges.py --json eval/codex-judge/summary.json
```
