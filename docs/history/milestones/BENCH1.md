# BENCH1: same 151 clips through Vocet and other open-source dictation apps, in the VM

Owner, 2026-11-21: REL2 gate 1 runs the 151 public Voice Arena clips through the shipped Vocet app. Also run the
same clips through the other open-source Mac dictation apps cloned next to this repo, so we can compare quality and
speed fairly: Handy, OpenWhispr, FreeFlow, Muesli, FluidVoice, VoiceInk (and blurt if it is a Mac dictation app).

Starting point: the closed experiment PR #323 (`eval/voicearena`, Voice Arena bench harness) and `eval/` scorers
(ZER and latency). Clips: the 151 public clips the eval already uses (CC BY-NC): never commit audio or per-clip
text; commit only aggregate numbers and the harness.

## Scope
1. **One black-box harness** that drives any app the same way in dictation-vm-2: route each clip into the app as
   microphone input (a virtual audio device in the VM, e.g. BlackHole; install only in the VMs, never on the owner's
   Mac), hold the app's dictation key for the clip plus a fixed tail, release, read the text pasted into a plain
   TextEdit document through Accessibility, measure key-up → text latency, score with the existing ZER scorer. Vocet
   runs through the same path (not its `--debug-audio-file` shortcut) so the comparison is fair; also record Vocet's
   shortcut result to check the two agree.
2. **Per app**: build from the local clone (or the maintainer's signed release if building is impractical; say which),
   configure its default/recommended **local** model, note model name and size, run all 151 clips. Cloud modes only
   where the app can use OpenRouter or an OpenAI-compatible endpoint; the owner puts a capped key into the VM
   through the Keychain, and agents never read or print it. Skip an app that cannot be driven headlessly within its time
   box and say why.
3. **Vocet**: cloud (Connect OpenRouter: gpt-4o-mini-transcribe + cleanup) and local (OrukQwen + Gemma), main at the
   time of the run.
4. **Report** `eval/reports/bench1.md` (aggregates only): per app × mode: ZER, p50/p95 latency, cleanup on/off, model,
   machine (VM spec), date, app commit; plus how each was configured, so anyone can reproduce. Call out where the VM
   understates real-Mac speed.

Licences: running GPL apps (FluidVoice, VoiceInk) for measurement is fine; never copy their code into Vocet. Credit
nothing in NOTICE unless code is adapted (it shouldn't be).

## Acceptance criteria
1. [auto] Harness and scorer have tests (scorer on fixed strings; harness dry-run against a stub app).
2. [auto] Vocet black-box vs shortcut results agree within noise (state the tolerance).
3. [manual] Lead reviews the report; numbers reproducible from the documented config.

Developer: Opus 5.5 (VM audio routing is platform work). Time box: 1.5 days; report what's done at the box.
Runs in dictation-vm-2 only; coordinate with Testers through the Lead (one VM job at a time).
