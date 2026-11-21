# /// script
# requires-python = ">=3.11,<3.14"
# dependencies = ["sounddevice==0.5.1", "soundfile==0.12.1", "numpy==2.2.4", "soxr==0.5.0.post1",
#                 "pyobjc-framework-Quartz==11.0", "pyobjc-framework-ApplicationServices==11.0",
#                 "pyobjc-framework-Cocoa==11.0", "pyobjc-framework-Metal==11.0"]
# ///
"""Can this Mac (a CI runner) run the bench? Prints one `KEY: value` line per check; never fails.

    uv run ci_probe.py ax | audio | metal | e2e --app handy --wav probe.wav --text "..."
"""
from __future__ import annotations

import argparse
import pathlib
import sys
import time

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import harness as hb  # noqa: E402


def say(k: str, v) -> None:
    print(f"{k}: {v}", flush=True)


def ax() -> None:
    import ApplicationServices as AS
    import Quartz
    say("ax_process_trusted", bool(AS.AXIsProcessTrusted()))
    wins = Quartz.CGWindowListCopyWindowInfo(Quartz.kCGWindowListOptionOnScreenOnly, Quartz.kCGNullWindowID) or []
    say("on_screen_windows", sorted({(str(w.get("kCGWindowOwnerName")), int(w.get("kCGWindowLayer", 0)))
                                     for w in wins}))
    err, app = AS.AXUIElementCopyAttributeValue(AS.AXUIElementCreateSystemWide(), "AXFocusedApplication", None)
    say("ax_focused_application", f"err={err} app={'yes' if app is not None else 'no'}")
    import AppKit
    te = AppKit.NSRunningApplication.runningApplicationsWithBundleIdentifier_("com.apple.TextEdit")
    if te:
        el = AS.AXUIElementCreateApplication(te[0].processIdentifier())
        for attr in ("AXFrontmost", "AXFocusedWindow", "AXWindows"):
            err, v = AS.AXUIElementCopyAttributeValue(el, attr, None)
            say(f"textedit_{attr}", f"err={err} {'set' if v is not None else 'none'}"
                + (f" n={len(v)}" if attr == "AXWindows" and v is not None else ""))
    try:
        t = hb.TextEditTarget(pathlib.Path.home() / "bench" / "scratch.txt")
        t.prepare()
        say("textedit_ax_area", "found and emptied")
        AS.AXUIElementSetAttributeValue(t.area, "AXValue", "probe text")
        time.sleep(0.2)
        say("textedit_ax_write_read", t.read() == "probe text")
    except Exception as e:  # noqa: BLE001 - the probe reports, never fails
        say("textedit_ax_error", f"{type(e).__name__}: {e}")


def audio(wav: pathlib.Path) -> None:
    import numpy as np
    import sounddevice as sd
    import soundfile as sf
    try:
        ca = hb.CoreAudio()
        say("input_devices", list(ca.inputs()))
        bh = [d for n, d in ca.inputs().items() if "blackhole" in n.lower()]
        if not bh:
            say("blackhole", "not present")
            return
        ca.set_default_input(bh[0])
        say("default_input", ca.default_input_name())
        devs = sd.query_devices()
        i_in = next(i for i, d in enumerate(devs) if "blackhole" in d["name"].lower() and d["max_input_channels"])
        i_out = next(i for i, d in enumerate(devs) if "blackhole" in d["name"].lower() and d["max_output_channels"])
        rate = int(devs[i_out]["default_samplerate"])
        a, sr = sf.read(wav, dtype="float32", always_2d=True)
        a = a.mean(axis=1)
        if sr != rate:
            import soxr
            a = soxr.resample(a, sr, rate).astype("float32")
        rec = sd.playrec(np.repeat(a[:, None], 2, axis=1), rate, channels=2, device=(i_in, i_out), blocking=True)
        rms_in, rms_out = float(np.sqrt((rec ** 2).mean())), float(np.sqrt((a ** 2).mean()))
        say("loopback_rms_played_recorded", f"{rms_out:.4f} {rms_in:.4f}")
        say("loopback_works", rms_in > 0.1 * rms_out)
        say("running_somewhere_after", ca.running_somewhere(bh[0]))
    except Exception as e:  # noqa: BLE001
        say("audio_error", f"{type(e).__name__}: {e}")


def metal() -> None:
    try:
        import Metal
        dev = Metal.MTLCreateSystemDefaultDevice()
        say("metal_device", dev.name() if dev else "none")
        if dev:
            say("metal_recommended_working_set_mb", int(dev.recommendedMaxWorkingSetSize() / 2**20))
    except Exception as e:  # noqa: BLE001
        say("metal_error", f"{type(e).__name__}: {e}")


def e2e(app: str, wav: pathlib.Path, text: str) -> None:
    import csv
    import tempfile
    d = pathlib.Path(tempfile.mkdtemp())
    (d / "probe.wav").write_bytes(wav.read_bytes())
    import soundfile as sf
    with (d / "manifest.csv").open("w", newline="") as fh:
        w = csv.DictWriter(fh, ["file", "clip_id", "audio_length_s", "intended_text"])
        w.writeheader()
        w.writerow({"file": "probe.wav", "clip_id": "probe", "audio_length_s": sf.info(wav).duration,
                    "intended_text": text})
    try:
        hb.mac_preflight(hb.APPS[app], check_process=True)
        timing = hb.accuracy_only()
        out = d / "out.csv"
        args = argparse.Namespace(clips=d, output=out, limit=None, only=None, attempts=2, warmup=1, app=app,
                                  mode="local", machine="github-macos-15")
        hb.run_mac(hb.BlackHoleDriver(hb.APPS[app], timing), "blackhole", timing, args)
        row = next(iter(hb.load_done(out).values()))
        got = set(row["transcript"].lower().replace(",", "").replace(".", "").split())
        want = set(text.lower().replace(",", "").replace(".", "").split())
        say("e2e_status", f"{row['status']} ready={row['ready_s']} finalize_s={row['finalize_s']} "
            f"error={row['error']}")
        say("e2e_words_matched", f"{len(got & want)}/{len(want)}")
        say("e2e_transcript", row["transcript"][:200])  # our own `say` sentence, not dataset text
    except BaseException as e:  # noqa: BLE001 - SystemExit from the preflight included
        say("e2e_error", f"{type(e).__name__}: {e}")


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("check", choices=["ax", "audio", "metal", "e2e"])
    ap.add_argument("--wav", type=pathlib.Path)
    ap.add_argument("--app", default="handy")
    ap.add_argument("--text", default="")
    a = ap.parse_args()
    {"ax": ax, "metal": metal}.get(a.check, lambda: None)()
    if a.check == "audio":
        audio(a.wav)
    if a.check == "e2e":
        e2e(a.app, a.wav, a.text)
    return 0


if __name__ == "__main__":
    sys.exit(main())
