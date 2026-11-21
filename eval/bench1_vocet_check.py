# /// script
# requires-python = ">=3.11,<3.14"
# dependencies = ["sounddevice==0.5.1", "soundfile==0.12.1", "numpy==2.2.4", "soxr==0.5.0.post1",
#                 "pyobjc-framework-Quartz==11.0", "pyobjc-framework-ApplicationServices==11.0",
#                 "pyobjc-framework-Cocoa==11.0"]
# ///
"""BENCH1 acceptance criterion 2 only: does the black-box path change what Vocet transcribes?

Vocet's debug builds take `--debug-audio-file <wav>`: the app records that file instead of the microphone,
reading it on each press. This runs the bench's clip loop (eval/app_bench/harness.py, same clips, padding,
TextEdit read and retries) with a driver that writes each clip to that one WAV path and sends `--press` /
`--release` to the running app. Compare its scored run with the black-box run:

    open -g ~/Applications/Vocet.app --args --debug-audio-file ~/bench/shortcut.wav   # in the VM, debug build
    uv run bench1_vocet_check.py --clips ~/bench/clips -o ~/bench/out/vocet_cloud_shortcut.csv --mode cloud
    python3 eval/app_bench/harness.py compare eval/work/bench1/vocet_cloud eval/work/bench1/vocet_cloud_shortcut

Never used for the published numbers: there Vocet runs through the microphone and hotkey like every app.
Lives outside eval/app_bench/ so that folder stays app-neutral and can be published on its own.
"""
from __future__ import annotations

import argparse
import os
import pathlib
import subprocess
import sys
import time

_here = pathlib.Path(__file__).resolve().parent
sys.path[:0] = [str(_here / "app_bench"), str(_here)]  # the repo layout, or flat next to harness.py in the VM
import harness as hb  # noqa: E402


class VocetShortcutDriver(hb.Driver):
    def __init__(self, app_path: str, wav: pathlib.Path, timing: hb.Timing, hold: float = 0.3) -> None:
        self.app_path, self.wav, self.timing, self.hold = app_path, wav, timing, hold

    def before_clip(self, clip: hb.Clip) -> None:
        import numpy as np
        import soundfile as sf
        import soxr
        a, sr = sf.read(clip.path, dtype="float32", always_2d=True)
        a = a.mean(axis=1)
        if sr != 16000:
            a = soxr.resample(a, sr, 16000).astype("float32")
        pad = np.zeros(int(16000 * self.timing.pad_ms / 1000), dtype="float32")
        sf.write(self.wav, np.concatenate([pad, a, pad]), 16000, subtype="PCM_16")

    def _send(self, flag: str) -> None:
        subprocess.run(["open", "-g", "-n", os.path.expanduser(self.app_path), "--args", flag], check=True)

    def start(self) -> None:
        self._send("--press")

    def feed(self, clip: hb.Clip) -> None:
        time.sleep(self.hold)

    def stop(self) -> None:
        self._send("--release")


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--clips", type=pathlib.Path, required=True)
    ap.add_argument("-o", "--output", type=pathlib.Path, required=True)
    ap.add_argument("--mode", default="cloud")
    ap.add_argument("--app-path", default="~/Applications/Vocet.app")
    ap.add_argument("--wav", type=pathlib.Path, default=pathlib.Path.home() / "bench" / "shortcut.wav")
    ap.add_argument("--limit", type=int)
    ap.add_argument("--only")
    ap.add_argument("--attempts", type=int, default=2)
    ap.add_argument("--warmup", type=int, default=1)
    a = ap.parse_args()
    a.app = "vocet"
    hb.mac_preflight(hb.APPS["vocet"], check_process=True)
    timing = hb.Timing(tail=0.5)  # the file is read whole on --release; no audio to wait for
    return hb.run_mac(VocetShortcutDriver(a.app_path, a.wav, timing), "vocet-shortcut", timing, a)


if __name__ == "__main__":
    sys.exit(main())
