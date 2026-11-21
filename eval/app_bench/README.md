# Mac dictation app bench

Runs the same 151 public speech clips through several Mac dictation apps, as a user would: hold the app's
key, speak into the microphone, release, and read what the app pastes. It reports the share of
dictations that need no edit (ZER) and the time from key release to text (latency). Every app gets the
same audio, timing, target window and scorer, inside a macOS virtual machine.

MIT licensed ([LICENSE](LICENSE)). It ships code and aggregate results only. It doesn't ship the clips, their
reference texts or any app's transcripts of them: those are CC BY-NC 4.0 and are downloaded at run time from
their original source. Not affiliated with the dataset's authors, nor with any of the apps measured.

## What it measures

For each clip, in the VM's logged-in GUI session:

1. A plain-text TextEdit document is emptied and brought to the front.
2. The app's dictation key is pressed with real key events (`CGEventPost` at the HID tap; a modifier key
   is sent as `flagsChanged`, as a keyboard sends it). Hold-to-talk apps get key-down; toggle apps get a
   tap.
3. The harness waits until some process opens the virtual microphone (CoreAudio's "device is running
   somewhere" on BlackHole 2ch), up to 5 s, then 0.5 s more. An app that keeps its microphone open is ready
   at once.
4. The clip, padded with 300 ms of silence at each end, plays into **BlackHole 2ch**, the VM's default
   input. Nothing goes through a speaker.
5. After a fixed 4 s tail, the key is released (or tapped again).
6. The document is read through Accessibility every 20 ms until its text has not changed for 2 s (timeout
   30 s). **Latency** = key release to the last change of the text (`finalize_s`); `first_text_s` = key
   release to the first text.

Each clip gets up to 2 attempts. One unrecorded dictation runs first so a local model is loaded. The
timing values match the MIT-licensed [Dictation-harness](https://github.com/VoiceArenaAI/Dictation-harness),
so numbers are comparable with runs made with it.

**Score.** `judge.py` asks an LLM judge for the minimal correction of each draft against the clip's
intended text, with the [amend-va](https://pypi.org/project/amend-va/) 0.2.1 prompt `v1`, and counts
character edits with amend-va's `char_edits`. **ZER** = share of the 151 clips with zero edits (a clip with
no text counts as wrong); **AMEND** = pooled edits / pooled characters. Three judges are supported, all through
their own CLIs: `claude` (Claude Opus 5.5, `claude -p`), `codex` and `gemini`. Compare runs only under the
same judge. Single-judge numbers on the public set are unofficial.

## Files

| File | What it does |
| --- | --- |
| `fetch_clips.py` | downloads the clips and references from Hugging Face (pinned revision, sha256 checked) into `clips/` |
| `harness.py` | the clip loop (`run`, in the VM), `dry-run` (a stub app, no macOS), `summarize`, `compare`, `set-input` |
| `bench.sh` | host side: VM clone/start/stop, setup, app install, runs, scoring, `publish` |
| `judge.py`, `judges.py` | the scorer; one cache per judge and model in `.judge-cache/` |
| `test_harness.py`, `test_judge.py`, `test.sh` | offline tests (standard library only): the loop on a simulated clock, the scorer on fixed strings |
| `results/` | what is published: `summary.csv`, `summary.md`, and per run `meta.json` (app setup), `host_load.txt`, `vm_load.txt` |

`clips/`, `work/` (per-clip results, transcripts) and `.judge-cache/` are git-ignored and never published.

## Requirements

- An Apple-silicon Mac with [Lume](https://github.com/trycua/cua/tree/main/libs/lume) (macOS VMs on Apple's
  Virtualization framework), `jq`, `ssh` and [uv](https://docs.astral.sh/uv/).
- The judge's CLI, logged in (`claude` for the default judge).

## The VM image (once)

The published runs used a VM with this setup; anything equivalent works.

1. `lume create bench-base --os macos --ipsw latest --cpu 4 --memory 8GB --disk-size 80GB` (the runs used
   macOS 15.6.1), finish Setup Assistant with a local user (`lume`), and turn on automatic login and Remote
   Login (ssh) with your key.
2. Install [BlackHole 2ch](https://github.com/ExistentialAudio/BlackHole) (0.6.x, the signed installer
   package), then restart. Leave the system **output** on the VM's speakers, so alert sounds can't reach the
   microphone.
3. System Settings → Privacy & Security → **Accessibility**: turn on Terminal (the harness posts keys and
   reads TextEdit from Terminal). Allow Terminal to control TextEdit if asked.
4. Never use this image for runs; clone it (`BENCH_BASE_IMAGE=bench-base ./bench.sh vm-create`).

`bench.sh vm-setup` then installs uv in the VM, copies the harness and clips, makes BlackHole the default
input, and turns off the Mac-wide automatic corrections (capitalisation, spelling, quotes, dashes, text
replacement, period substitution, inline prediction).

## Reproduce

```sh
export BENCH_VM=bench-vm BENCH_BASE_IMAGE=bench-base BENCH_SSH_KEY=~/.ssh/<your VM key>
./bench.sh fetch                          # 151 clips from the original source
./bench.sh vm-create && ./bench.sh vm-start --timed && ./bench.sh vm-setup
./bench.sh install ~/Downloads/Handy_0.10.1_aarch64.dmg
# in the VM: open the app, go through its onboarding, allow Microphone and Accessibility, pick the model
# (table below), set the input to BlackHole 2ch if the app has its own picker, quit every other app
./bench.sh check handy                    # preflight: default input, corrections off, app running
./bench.sh run handy_local handy local --limit 3      # a trial
./bench.sh run handy_local handy local                # all 151; rerun the same command to resume
./bench.sh meta handy_local app=handy mode=local model="Parakeet Unified EN 0.6B Q8_0" cleanup=off \
    source=release version=0.10.1 commit=aca8985190 date=$(date +%F)
./bench.sh score handy_local
./bench.sh publish                        # results/summary.csv and summary.md
```

`python3 harness.py dry-run` checks the loop without a VM; `./test.sh` runs the tests.

**Quiet machine.** Local models are sensitive to load on the host: run with no other VM running
(`vm-start --timed` refuses otherwise), no builds, and check `host_load.txt` (load average and free memory
at the start and end of each run); rerun an app if the host was busy.

## Apps

The app key, mode and model are each app's own default or recommendation (from its source at the release
tag). Only the app under test runs. Where an app has its own microphone picker, it is set to BlackHole 2ch.

| App | Release | Key, mode | Local model |
| --- | --- | --- | --- |
| [Handy](https://github.com/cjpais/Handy) | 0.10.1 (`aca8985190`) | Option+Space, hold | Parakeet Unified EN 0.6B, Q8_0 GGUF (731 MB) |
| [OpenWhispr](https://github.com/OpenWhispr/openwhispr) | 1.12.0 (`ad74240688`) | Fn (Globe), tap | the model its onboarding recommends (see `results/<run>/meta.json`) |
| [Muesli](https://github.com/Muesli-HQ/muesli) | 0.8.6 (`ecc2af2654`) | Right Option, hold | Parakeet Unified EN 0.6B, Core ML (~565 MB) |
| [FluidVoice](https://github.com/altic-dev/FluidVoice) | 1.6.13 (`9912eca0d9`) | Right Option, tap | Parakeet TDT 0.6B, Core ML (~450 MB) |
| [VoiceInk](https://github.com/Beingpax/VoiceInk) | 2.27 (`b5d1be9a7c`) | set in onboarding | Parakeet TDT 0.6B v3, Core ML (~450 MB) |
| [Vocet](https://github.com/gargankit90/dictation-app) | `main` at the run (commit in `meta.json`) | Right Option, hold | OrukQwen 1.7B + Gemma 4 E2B cleanup; cloud: OpenRouter |

Releases are the maintainers' signed builds from each project's GitHub releases page; Vocet is built from
source. Cloud modes run only where an app can use OpenRouter or an OpenAI-compatible endpoint, with a
spend-capped key the person running the bench enters in the app inside the VM.

**The VM understates a real Mac.** Four virtual CPUs, 8 GB, a paravirtualised GPU and no Neural Engine:
Core ML models fall back to the GPU or CPU, so local latencies are upper bounds, and the ranking between local
apps may differ on a real Mac. Cloud latencies depend on the network more than on the VM.

## Data

Clips and references: *Dictation Dataset (public test split)*, Hugging Face `VoiceArena/Dictation_Dataset`,
revision `ab671bb2`, 151 clips, CC BY-NC 4.0. Fetched at run time by `fetch_clips.py`; never redistributed
here.
