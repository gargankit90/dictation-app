# /// script
# requires-python = ">=3.10,<3.14"
# dependencies = ["sounddevice", "soundfile", "numpy", "soxr", "pyobjc-framework-Quartz",
#                 "pyobjc-framework-ApplicationServices", "pyobjc-framework-Cocoa"]
# ///
"""Vocet driver for Voice Arena's Dictation harness (github.com/VoiceArenaAI/Dictation-harness, MIT).

Copy this file next to the harness's dictation_common.py and audio_input.py and run it the same way as
the other run_<tool>_dictation.py drivers:

    uv run run_vocet_dictation.py clips --check
    uv run run_vocet_dictation.py clips --limit 5 -o results_vocet.csv
    uv run run_vocet_dictation.py clips -o results_vocet.csv      # rerun the same command to resume

Vocet is hold-to-talk: Right Option down starts a recording, Right Option up stops it, then Vocet sends
the audio to the configured engine (Groq here) and pastes the text into the focused app (TextEdit).

Microphone: Vocet records from the system default input, so the default input must be BlackHole 2ch
(System Settings > Sound > Input). The batch check re-reads it every 5 clips.

Ready signal: Vocet's recording pill, a small borderless panel owned by the Vocet process that is on screen
only while Vocet is recording, transcribing or showing a result; it is read through CGWindowList (owner
name, layer and size only, no Screen Recording permission needed). After the pill appears the harness waits
the usual --start-delay (0.5 s). --no-pill falls back to the fixed delay alone.

Timing: Vocet keeps its microphone open ("warm") for 30 s after a recording so the next one starts
instantly, so "macOS reports the capture ended" can't be used as the stop-received signal the way the
precise drivers do. This driver therefore runs in the harness's plain mode (precise_timing = False) and
latency is the harness's finalize_s: from the moment the key-up is sent to the last change of the text.
For a hotkey app the key-up reaches the app in well under a millisecond, so this is directly the
engine round trip plus paste.
"""
import json
import pathlib
import subprocess
import time

from Quartz import (
    CGEventCreateKeyboardEvent,
    CGEventPost,
    CGEventSetFlags,
    CGEventSetType,
    CGWindowListCopyWindowInfo,
    kCGEventFlagMaskAlternate,
    kCGEventFlagsChanged,
    kCGHIDEventTap,
    kCGNullWindowID,
    kCGWindowListOptionOnScreenOnly,
)

import audio_input
from dictation_common import DictationError, Driver, run_tool

PROC = "Vocet"
SETTINGS = pathlib.Path.home() / "Library/Application Support/com.dictationapp.desktop/settings.json"
RIGHT_OPTION = 61
# NX_DEVICERALTKEYMASK (0x40) marks the right-hand Option; 0x100 is NX_NONCOALSESCEDMASK.
RIGHT_OPTION_DOWN = kCGEventFlagMaskAlternate | 0x40 | 0x100
NO_FLAGS = 0x100
# The pill is a 420 x 72 point panel titled "overlay". Vocet's menu-bar icon is also a small window at a
# raised layer (about 37 x 24, always on screen), so the pill is matched by size, not by "small".
PILL_W, PILL_H = (300, 520), (50, 150)


def _right_option(down: bool) -> float:
    ev = CGEventCreateKeyboardEvent(None, RIGHT_OPTION, down)
    CGEventSetType(ev, kCGEventFlagsChanged)
    CGEventSetFlags(ev, RIGHT_OPTION_DOWN if down else NO_FLAGS)
    t = time.monotonic()
    CGEventPost(kCGHIDEventTap, ev)
    return t


def pill_visible() -> bool:
    for w in CGWindowListCopyWindowInfo(kCGWindowListOptionOnScreenOnly, kCGNullWindowID) or []:
        if w.get("kCGWindowOwnerName") == PROC and (w.get("kCGWindowLayer") or 0) > 0:
            b = w.get("kCGWindowBounds", {})
            if PILL_W[0] <= b.get("Width", 0) <= PILL_W[1] and PILL_H[0] <= b.get("Height", 0) <= PILL_H[1]:
                return True
    return False


def wait_pill(visible: bool, timeout: float) -> float | None:
    t0 = time.monotonic()
    while time.monotonic() - t0 < timeout:
        if pill_visible() == visible:
            return time.monotonic() - t0
        time.sleep(0.01)
    return None


def vocet_pid() -> int | None:
    out = subprocess.run(["pgrep", "-x", PROC], capture_output=True, text=True).stdout.split()
    return int(out[0]) if out else None


class Vocet(Driver):
    name = "vocet"
    extra_fields = ["vocet_provider", "vocet_model", "vocet_language", "vocet_hotkey", "vocet_transcription",
                    "vocet_local_model", "default_input"]
    precise_timing = False

    def add_args(self, p) -> None:
        p.add_argument("--start-timeout", type=float, default=5.0,
                       help="max wait for Vocet's recording pill after key down (s)")
        p.add_argument("--hide-timeout", type=float, default=20.0,
                       help="max wait for the pill to go away before the next clip (s)")
        p.add_argument("--no-pill", action="store_true",
                       help="don't wait for the pill: key down, then only --start-delay")
        p.add_argument("--provider", default="groq",
                       help="the cloud provider Vocet must be set to (settings.json 'provider'): groq, openai, "
                            "assemblyai")
        p.add_argument("--local-model",
                       help="expect Vocet's on-device engine with this model (settings.json 'transcription' "
                            "local and 'local_model'), e.g. parakeet-unified-en-0.6b-q4_k_m; overrides --provider")

    def preflight(self, args) -> None:
        self.start_timeout, self.hide_timeout, self.no_pill = args.start_timeout, args.hide_timeout, args.no_pill
        self.provider = args.provider
        self.local_model = args.local_model
        self.key_down = False
        if vocet_pid() is None:
            raise SystemExit("Vocet isn't running. Open it (past onboarding, Groq key saved) and rerun.")
        if pill_visible():
            raise SystemExit("Vocet's pill is on screen: a dictation is running. Let it finish first.")

    def env(self) -> dict:
        try:
            s = json.loads(SETTINGS.read_text())
        except Exception as e:  # noqa: BLE001 - reported as a setting problem
            s = {"_error": str(e)}
        try:
            mic = audio_input.device_name(audio_input.get_default_input())
        except Exception as e:  # noqa: BLE001
            mic = f"? ({e})"
        return {"vocet_provider": s.get("provider", ""), "vocet_model": s.get("model", ""),
                "vocet_language": s.get("language") or "auto", "vocet_hotkey": s.get("hotkey", ""),
                "vocet_transcription": s.get("transcription", "cloud"),
                "vocet_local_model": s.get("local_model", ""), "default_input": mic}

    def env_problem(self, env: dict) -> str | None:
        if self.local_model:
            if env.get("vocet_transcription") != "local" or env.get("vocet_local_model") != self.local_model:
                return (f"Vocet's engine is {env.get('vocet_transcription')}/{env.get('vocet_local_model')}, "
                        f"not local/{self.local_model} (Settings > Speech)")
        elif env.get("vocet_transcription") != "cloud" or env.get("vocet_provider") != self.provider:
            return (f"Vocet's engine is {env.get('vocet_transcription')}/{env.get('vocet_provider')}, "
                    f"not cloud/{self.provider} (Settings > Speech)")
        hotkey = env.get("vocet_hotkey", "").lower().replace(" ", "")
        if hotkey not in ("", "rightalt", "rightoption"):
            return f"Vocet's hotkey is '{env.get('vocet_hotkey')}', not Right Option"
        if "blackhole" not in env.get("default_input", "").lower():
            return f"the default input is '{env.get('default_input')}', not BlackHole 2ch (System Settings > Sound)"
        return None

    def before_clip(self) -> None:
        if pill_visible() and wait_pill(False, self.hide_timeout) is None:
            raise DictationError("Vocet's pill is still on screen from an earlier attempt")

    def start(self) -> float | None:
        t0 = _right_option(True)
        self.key_down = True
        if self.no_pill:
            return time.monotonic() - t0
        waited = wait_pill(True, self.start_timeout)
        if waited is None:
            self.stop()
        return waited

    def stop(self) -> float | None:
        if not self.key_down:
            return None
        self.stop_sent_t = _right_option(False)
        self.key_down = False
        return 0.0

    def reset(self) -> None:
        self.stop()


if __name__ == "__main__":
    run_tool(Vocet())
