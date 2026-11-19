# Voice Arena Dictation Bench: Vocet experiment

An experiment, not product code and not for `main`. It runs Voice Arena's public
Dictation Bench set (151 clips) end to end through Vocet, the way Voice Arena
ran 14 macOS dictation tools for their November 2026 whitepaper, and scores it
with their metric. The results are in [REPORT.md](REPORT.md).

Results are **unofficial: single judge (Claude Opus 5.5 via Claude Code),
public set only.**

## What is reused, and from where

| Piece | Source | Licence |
| --- | --- | --- |
| Clips and references | Hugging Face `VoiceArena/Dictation_Dataset`, split `test`, revision `ab671bb2`, 151 clips | CC BY-NC 4.0 |
| Playback harness | [VoiceArenaAI/Dictation-harness](https://github.com/VoiceArenaAI/Dictation-harness) at `3c3329e0` | MIT |
| Metric, judge prompt | [amend-va](https://pypi.org/project/amend-va/) 0.2.1 (the whitepaper's version), prompt `v1` | see the package |

Dataset credit: *Voice Arena Dictation Dataset (public test)*, Voice Arena,
CC BY-NC 4.0. The audio is never committed (`clips/` is git-ignored), and
the committed CSVs leave out the dataset's reference texts; join them on
`clip_id` with `clips/manifest.csv` after `bench.sh fetch`. REPORT.md quotes the
intended text of ten clips to explain the failures.

## Files

| File | What it does |
| --- | --- |
| `fetch_dataset.py` | downloads the dataset parquet (pinned revision and sha256), writes `clips/<id>.flac` and `clips/manifest.csv` |
| `run_vocet_dictation.py` | the Vocet driver for the harness, next to the other `run_<tool>_dictation.py` files |
| `score_claude_cli.py` | sends amend-va's exact v1 prompt to Claude Opus 5.5 (effort max) with `claude -p`, then scores with amend-va's own `char_edits` |
| `report.py` | ZER / AMEND overall, by domain and length, speaker-bootstrap intervals, latency, the 10 worst drafts |
| `strip_results.py` | copies the harness CSV without the reference text column |
| `bench.sh` | one command per step and per engine (below) |
| `results/<engine>/` | `harness_results.csv` (the harness's own row per clip), `harness.log`, `scored.csv`, `scored.judge.jsonl` (each judge answer), `summary.json` |

## How the Vocet driver works

- **Start / stop:** Vocet is hold-to-talk. Right Option down starts a
  recording; Right Option up (after the 4 s tail) stops it. The keys are posted
  as `flagsChanged` events, as the harness's Meta AI driver does.
- **Ready signal:** Vocet's recording pill, a 420 × 72 point panel owned by the
  Vocet process, read through `CGWindowList` (owner, layer and size). Then the
  usual 0.5 s start delay. Vocet's menu-bar icon is also a small raised
  window, so the pill is matched by size. `--no-pill` falls back to the
  fixed delay.
- **Microphone:** Vocet records from the system default input, so the VM's
  default input is BlackHole 2ch. The batch check re-reads it every 5 clips,
  with Vocet's provider, model, language and hotkey from its settings file.
- **Latency:** Vocet keeps the microphone open for 30 s after a recording,
  so "the capture ended" can't mark the moment the stop arrived, which is what
  the harness's precise mode times from. The driver runs in the plain mode and
  the latency is `finalize_s`: from the key-up to the last change of the text in
  TextEdit (engine round trip, plus paste).
- **Timing:** identical to every tool in the whitepaper: 300 ms padding, 0.5 s
  start delay, 4 s tail, 2 s settle, 30 s timeout, 1 s gap. `bench.sh run` passes
  them explicitly.

## How the judge differs from the whitepaper's

The whitepaper's AMEND column is amend-va 0.2.1 with Claude Opus 5.5 at max
effort through the Anthropic API. Here the same prompt (built by amend-va's
own `build_prompt`, so byte for byte the same), model, effort and output
schema go through the `claude -p` CLI on the owner's Claude subscription
instead, because we have no API or OpenRouter key. Differences: the CLI adds a
one-line system prompt; amend-va's server-side model fallback is not used. The
answer is parsed and scored with amend-va's own functions.

The whitepaper's prompt fingerprint `d26616976626` can't be reproduced, since
amend-va doesn't publish how it is computed. `prompts.py` in the 0.2.1 wheel and
in 0.2.2 are byte identical. Every scored row records its own fingerprint
(`8ed04caf4663`, sha256 of the v1 template), the prompt version, the judge model
and effort, the route, and the amend-va version, so two engines' runs can be
checked as comparable.

## Running it

Hard rules: computer use and audio only inside the Lume VM
`dictation-vm-eval`, never the owner's screen, never `dictation-vm-golden`. The
shared test VMs `dictation-vm` and `dictation-vm-2` are not used. Apple allows two
running macOS VMs, so one of them may have to be stopped first (ask the Lead).
Keys live only in the VM's keychain, entered in Vocet's own UI.

One-time VM setup (done on 2026-11-18):

```sh
lume clone dictation-vm-golden dictation-vm-eval
lume set dictation-vm-eval --memory 6GB --cpu 4
eval/voicearena/bench.sh fetch                 # host: 151 clips into eval/voicearena/clips/
eval/voicearena/bench.sh start
eval/voicearena/bench.sh push-harness          # pinned harness + driver + clips into ~/va in the VM
npm ci && npm run tauri build -- --debug --bundles app   # Vocet signed with the dev certificate
eval/voicearena/bench.sh install-app
```

Then, inside the VM:

- BlackHole 2ch is already in the golden image. Make it the default input
  (System Settings → Sound → Input; this run set it with the harness's
  `audio_input.set_default_input`). The output stays on the virtual speakers.
- uv: `curl -LsSf https://astral.sh/uv/install.sh | sh` (user-local, no sudo).
- Mac-wide text substitutions off: `defaults write -g <key> -bool false` for
  each key the harness lists (`NSAutomaticCapitalizationEnabled`,
  `NSAutomaticPeriodSubstitutionEnabled`, `NSAutomaticSpellingCorrectionEnabled`,
  `NSAutomaticQuoteSubstitutionEnabled`, `NSAutomaticDashSubstitutionEnabled`,
  `NSAutomaticTextReplacementEnabled`, `NSAutomaticInlinePredictionEnabled`,
  `NSAutomaticTextCompletionEnabled`).
- Vocet onboarding: English, "Use my own Groq or OpenAI key", provider Groq,
  paste the key and Test & Save (the owner did this step), Continue
  (Microphone and Accessibility were already allowed in the golden image),
  "Skip for now", "Share usage and error events" **off** for the benchmark,
  Start dictating. Settings used: Groq `whisper-large-v3-turbo`, language `en`,
  hotkey Right Option, history 25.
- The first harness runs ask to let Terminal control System Events and
  TextEdit; allow both (the harness README asks for it).

Another engine (the experimental Vocet Cloud (AssemblyAI) provider from PR
#322, run on 2026-11-18/19 from `exp/assemblyai` at `6a652ca`):

- Build that commit the same way (dev certificate) and `bench.sh install-app
  <path to its Vocet.app>`.
- The token is in the VM's login keychain (the owner put it there; agents
  never read it). In Settings → Speech → Change key…, pick the provider
  "Vocet Cloud (AssemblyAI)" and type nothing in the key field. The rebuilt app
  is a new code signature, so macOS asks whether Vocet may use the keychain
  item. For the Groq item we answered Deny (not needed any more). For the
  token we typed the throwaway VM user's password and chose Always Allow,
  as the e2e scenarios do for the same VM user. The provider switches only
  when its key check succeeds, so pick another provider and pick Vocet Cloud
  again after allowing it. Cleanup stays at its default, Smart.
- Run with `--provider assemblyai`, which makes the driver's settings
  check expect that provider instead of Groq.

On-device engines (run on 2026-11-19 from `cfe583a`, the same build as Groq):

- The 0.1.0 build can't read a settings file with `"provider": "assemblyai"`
  (written by the AssemblyAI build). With Vocet quit, the VM's
  `settings.json` was set back to `"provider": "groq"` and
  `"model": "whisper-large-v3-turbo"` (backup: `settings.json.aai-backup`).
- The new build again asked for the old keychain items. All those prompts
  were denied; the on-device engine needs no key.
- Settings → Speech: "English" → Download (Parakeet), wait for "downloaded
  and verified" in Vocet's log, then choose it. Later "30 languages" →
  Download (Qwen3), then choose it.
- Run with `--local-model parakeet-unified-en-0.6b-q4_k_m` (or
  `qwen3-asr-0.6b-q4_k_m`), which makes the driver's settings check expect
  `transcription: local` and that model.

AssemblyAI with the verbatim text (run 2, 2026-11-19, `exp/assemblyai` at
`9911b0a`, which adds an opt-in reply dump):

- With Vocet quit, set the VM's settings back to `provider: assemblyai`,
  `model: assemblyai-dictation`, `transcription: cloud` (backup of the
  on-device settings: `settings.json.local-backup`).
- Start it with the dump on: `open -g --env
  VOCET_EVAL_DUMP_DIR=/Users/lume/va/dump ~/Applications/Vocet.app`, which goes
  through LaunchServices so Vocet keeps its own permissions. The new
  signature asks again for the token's keychain item: VM user's password,
  Always Allow; then relaunch the same way so the engine reads it.
- After the run, copy `~/va/dump/assemblyai-replies.jsonl` to
  `results/<engine>/`, then `uv run aai_split.py join results/<engine>`, score
  both `results/<engine>` and `results/<engine>_verbatim` with
  `score_claude_cli.py`, then `uv run aai_split.py wer` and `uv run
  aai_split.py cleanup`.

Two VMs at once: `BENCH_VM` picks the VM for every command (only
`dictation-vm-eval` and `dictation-vm-eval-*` are accepted). Since
2026-11-19 there is a second eval VM, `dictation-vm-eval-2`, a clone of
`dictation-vm-eval` with the harness, clips, BlackHole and uv set up. Apple
allows two running macOS VMs, so the Lead frees a slot first. Give each run its
own engine label, so its results go to their own `results/<engine>/` folder:

```sh
BENCH_VM=dictation-vm-eval-2 eval/voicearena/bench.sh start
BENCH_VM=dictation-vm-eval-2 eval/voicearena/bench.sh run <engine> ...
```

Each run, per engine (`<engine>` is a label such as `vocet_groq`):

```sh
eval/voicearena/bench.sh check                       # harness preflight in the VM
eval/voicearena/bench.sh run <engine> --limit 5      # a short trial
eval/voicearena/bench.sh run <engine>                # all 151; rerun the same command to resume
                                                     # (add --provider assemblyai for Vocet Cloud)
eval/voicearena/bench.sh score <engine>              # judge + score (needs the claude CLI, signed in)
eval/voicearena/bench.sh report results/<engine> [results/<other engine>]
```

`run` starts the harness in the VM's Terminal (the only place with
Accessibility and the window server), waits, copies the log and results back
and writes the stripped `harness_results.csv`. The harness skips clips already
`ok` in its results file, so a rerun resumes. To rerun particular clips, delete
their rows from `~/va/results_<engine>.csv` in the VM and rerun, or pass
`--clips-file`.
