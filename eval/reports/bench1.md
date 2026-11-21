# BENCH1: Vocet and other open-source Mac dictation apps on the same 151 clips

Spec: `docs/history/milestones/BENCH1.md`. The bench is self-contained in [`eval/app_bench/`](../app_bench/)
(MIT, to be published as its own repository): method, VM image, app versions and settings, and exact
reproduce steps are in [its README](../app_bench/README.md); published aggregates in
`eval/app_bench/results/`. This page is the milestone's report: results, the Vocet agreement check, the
load during each run, and what deviated from the plan. Aggregates only: the clips, references and
transcripts are CC BY-NC 4.0 and never committed.

**Unofficial:** one judge (Claude Opus 5.5, amend-va 0.2.1 prompt `v1`), the public set only, one run per
app and mode, in a VM.

## Results

_Pending: the timed runs start when the Lead sends "go" (a quiet Mac, one VM)._

## Vocet: black box vs `--debug-audio-file` (acceptance criterion 2)

The published Vocet numbers come from the same path as every other app (hotkey, BlackHole microphone,
TextEdit). As a check only, `eval/bench1_vocet_check.py` runs Vocet's debug build through its
`--debug-audio-file` shortcut on the same clips, with the bench's loop and TextEdit read, and
`eval/app_bench/harness.py compare` compares the two scored runs.

**Tolerance:** the two ZERs agree if they differ by no more than the noise of a paired difference on the
same clips, `2 * 100 * sqrt(discordant clips) / n` ZER points (about a 95% interval), with a floor of 3
points. Two runs of the same cloud engine on identical audio differ by a clip or two.

_Pending._

## Runs in this repo

`eval/bench1.sh` is `eval/app_bench/bench.sh` with this Mac's settings: the VM `dictation-vm-bench` (a clone of
`dictation-vm-golden`, 8 GB, 4 CPUs, macOS 15.6.1, BlackHole 2ch from the golden image), the
`dictation-vm-2` e2e lock held while it runs (so a Tester's run reports "VM busy" instead of resetting a VM
in use), and the Vocet check script copied into the VM. Host: Apple M4 Pro, 14 cores, 24 GB.

Vocet was built from `main` (`a7c7a46`, the commit this branch starts from) with the dev certificate:
`npm run tauri build -- --bundles app` for the bench, `--debug` for the shortcut check.
