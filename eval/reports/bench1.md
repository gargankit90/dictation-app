# BENCH1: Vocet and other open-source Mac dictation apps on the same 151 clips

Spec: `docs/history/milestones/BENCH1.md`. Harness: `eval/app_bench.py` (runs in the VM) and
`eval/app_bench.sh` (host). Aggregates only. The clips, their reference texts and every app's per-clip
transcripts are CC BY-NC 4.0 (Voice Arena) and stay in the git-ignored `eval/clips/` and `eval/work/bench1/`.

**Unofficial:** one judge (Claude Opus 5.5 through `eval/judge.py`, amend-va 0.2.1, prompt `v1`), the public
set only, one run per app and mode, in a VM.

## Results

_Pending: the timed runs start when the Lead sends "go" (a quiet Mac, one VM)._

## Method

Every app gets the same path, timing and scorer:

1. **Audio in.** Each clip, padded with 300 ms of silence at each end, is played into **BlackHole 2ch**
   (in the VM only), the VM's default input. Every app records from the default input (or is set to
   BlackHole where it has its own picker), so all apps hear the same samples.
2. **Start.** The harness empties a plain-text TextEdit document, brings it to the front, and presses the
   app's dictation key with real `CGEvent`s at the HID tap (a modifier as `flagsChanged`, as the keyboard
   sends it). Hold-to-talk apps get key-down; toggle apps get one tap.
3. **Ready.** It waits until a process opens the BlackHole microphone (CoreAudio's "device is running
   somewhere"; an app that keeps the microphone warm is ready at once), up to 5 s, then 0.5 s more.
   Waiting for the microphone instead of a fixed delay means an app that opens it slowly doesn't lose the
   start of the clip.
4. **Speak, tail, release.** The clip plays, then a fixed 4 s tail, then key-up (or a second tap).
5. **Text out.** The document is read through Accessibility every 20 ms until its text has been unchanged for
   2 s (timeout 30 s after key-up). **Latency** is `finalize_s`: key-up to the last change of the text
   (transcription, any cleanup, and the paste). `first_text_s`, key-up to the first text, is recorded too.
6. **Score.** `eval/judge.py --judge claude` on the harness CSV; ZER is the share of the 151 clips that need no
   edit. A clip with no text counts as wrong. p50/p95 latency are over the clips that produced text.

Each clip gets up to 2 attempts (a missing microphone or no text is retried once). One unrecorded
dictation of the first clip runs before timing, so a local model is loaded and warm (`--warmup 1`). The
timing values are the Voice Arena harness's (also used by PR #323), so the numbers line up with
`reports/voicearena.md`.

**Vocet's shortcut (AC 2).** Vocet's debug-only `--debug-audio-file` records a WAV file instead of the
microphone. `--driver vocet-shortcut` writes each clip (same padding, 16 kHz) to the one path the app was
started with, then sends `--press` / `--release` to the running app and reads TextEdit the same way. It skips
the microphone, the hotkey and the timing of the audio, so it shows whether the black-box path changes
what Vocet transcribes. The **tolerance** is the noise of a paired difference between two runs on the same
151 clips: twice the standard error, `2 * 100 * sqrt(discordant clips) / 151` ZER points, with a floor of
3 points (`app_bench.py agree`). Two cloud runs differ by a clip or two even with identical audio.

## Machine

| | |
| --- | --- |
| Host | Apple M4 Pro, 14 cores, 24 GB, macOS (owner's Mac), no other VM running during timed runs |
| VM | Lume `dictation-vm-bench`, a clone of `dictation-vm-golden`: macOS 15.6.1, 4 virtual CPUs, 8 GB, paravirtualised GPU, no Neural Engine |
| Load | host load average and free memory at the start and end of each run: `host_load.txt` per run, summarised below |

**Where the VM understates a real Mac.** Four virtual CPUs instead of 14 cores, 8 GB instead of 24, and no
Neural Engine: Core ML models (FluidAudio's Parakeet in Muesli and FluidVoice) fall back to the GPU or CPU,
and Metal goes through the paravirtualised GPU. Local-model latencies here are upper bounds; the ranking
between local apps may differ on a real Mac. Cloud latencies depend on the network, not the VM.

## Apps and setup

Releases were downloaded from each project's GitHub release page (maintainer-signed builds, not built from
the local clones; the Lead approved this to fit the time box). Each app was installed into `/Applications`
in the VM, taken through its own onboarding, given Microphone and Accessibility, and set to its default or
recommended **local** model. Only the app under test runs during its run.

| App | Version (tag, commit) | Release asset (sha256) | Licence |
| --- | --- | --- | --- |
| Vocet | `main` at the time of the run (commit below), built here with the dev certificate | `npm run tauri build -- --bundles app` (black box); `--debug` (shortcut) | FSL-1.1-ALv2 |
| Handy | 0.10.1 (`v0.10.1`, `aca8985190`) | `Handy_0.10.1_aarch64.dmg` (`7cdef5de7d1b…`) | MIT |
| OpenWhispr | 1.12.0 (`v1.12.0`, `ad74240688`) | `OpenWhispr-1.12.0-arm64.dmg` (`4a3ec44a08d6…`) | MIT |
| Muesli | 0.8.6 (`v0.8.6`, `ecc2af2654`) | `Muesli-0.8.6.dmg` (`673b182fd759…`) | MIT |
| FluidVoice | 1.6.13 (`v1.6.13`, `9912eca0d9`) | `Fluid-oss-1.6.13.dmg` (`8da47aba78de…`) | GPL-3.0 (run only) |
| VoiceInk | 2.27 (`v2.27`, `b5d1be9a7c`) | `VoiceInk.dmg` (`a832f6c3a165…`) | GPL-3.0 (run only) |
| FreeFlow | 1.3.1 (`v1.3.1`, `20674709ae`) | `FreeFlow.dmg` (`17392f0dbc64…`) | MIT |

Download URLs: `https://github.com/<owner>/<repo>/releases/download/<tag>/<asset>` with `cjpais/Handy`,
`OpenWhispr/openwhispr`, `Muesli-HQ/muesli`, `altic-dev/FluidVoice`, `Beingpax/VoiceInk`,
`zachlatta/freeflow`.

What each app is set to (from the release's source; "measured as" is filled in from the VM run):

| App | Key and mode (app default) | Local model (default / recommended) | Size | Cleanup | Engine |
| --- | --- | --- | --- | --- | --- |
| Vocet local | Right Option, hold | OrukQwen 1.7B Q4_K_M + Gemma 4 E2B Q4_K_M cleanup | 1.32 GB + 3.11 GB | on (Gemma, on device) | llama.cpp-based, Metal |
| Vocet cloud | Right Option, hold | OpenRouter `openai/gpt-4o-mini-transcribe` + cleanup | - | on (cloud) | OpenRouter |
| Handy | Option+Space, push-to-talk | Parakeet Unified EN 0.6B, Q8_0 GGUF (catalogue's first recommendation) | 731 MB | none | GGUF, Metal |
| OpenWhispr | Globe/Fn, tap to toggle | Whisper `base` (the setting's default; onboarding also offers Parakeet "Orukeet", 672 MB, marked recommended) | 142 MB | off by default | whisper.cpp / sherpa-onnx |
| Muesli | Right Option, hold | Parakeet Unified EN 0.6B, Core ML (onboarding default) | ~565 MB | off by default | FluidAudio, Core ML |
| FluidVoice | Right Option, toggle | Parakeet TDT 0.6B, Core ML ("FluidVoice Pick", the Apple-silicon default) | ~450 MB | off by default | FluidAudio, Core ML |
| VoiceInk | set in onboarding | Parakeet TDT 0.6B v3, Core ML (onboarding's required model) | ~450 MB | off by default | FluidAudio, Core ML |

Muesli, FluidVoice and Vocet all default to Right Option, so only the app under test runs.

**Blurt** (AssemblyAI) is a Mac dictation app but has no local model and transcribes only through
AssemblyAI with the user's own key, not OpenRouter or an OpenAI-compatible endpoint, so it is out of scope
under the spec's cloud rule. **FreeFlow** has no local model either (Groq by default, with a configurable
OpenAI-compatible base URL), so it can only run as a cloud mode.

## Reproduce

```sh
eval/app_bench.sh fetch                          # the 151 clips (pinned revision, sha256 checked)
eval/app_bench.sh lock                           # dictation-vm-2's e2e lock, so no reset races the bench
eval/app_bench.sh vm-create                      # once: clone the golden image to dictation-vm-bench (8 GB, 4 CPUs)
eval/app_bench.sh vm-start --timed               # refuses while another VM runs
eval/app_bench.sh vm-setup                       # uv, harness, clips, BlackHole as input, substitutions off
eval/app_bench.sh install eval/work/bench1/dl/Handy_0.10.1_aarch64.dmg
# ... the app's onboarding and model, in the VM (per app below) ...
eval/app_bench.sh check handy
eval/app_bench.sh run handy_local handy local --limit 3     # trial
eval/app_bench.sh run handy_local handy local               # timed, all 151 (rerun to resume)
eval/app_bench.sh meta handy_local app=handy mode=local model=... cleanup=off source=release version=0.10.1 commit=aca8985190 date=...
eval/app_bench.sh score handy_local
eval/app_bench.sh summary
python3 eval/app_bench.py agree eval/work/bench1/vocet_cloud eval/work/bench1/vocet_cloud_shortcut
eval/app_bench.sh unlock
```
