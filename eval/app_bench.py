# /// script
# requires-python = ">=3.11,<3.14"
# dependencies = ["sounddevice==0.5.1", "soundfile==0.12.1", "numpy==2.2.4", "soxr==0.5.0.post1",
#                 "pyobjc-framework-Quartz==11.0", "pyobjc-framework-ApplicationServices==11.0",
#                 "pyobjc-framework-Cocoa==11.0"]
# ///
"""BENCH1: one black-box harness that runs the Voice Arena clips through any Mac dictation app.

For each clip, in the VM's GUI session (Terminal, which has Accessibility):

  1. empty a plain TextEdit document and bring it to the front;
  2. press the app's dictation key (hold mode: key down; toggle mode: one tap);
  3. wait until some process opens the BlackHole microphone (CoreAudio "running somewhere"; an app that
     keeps the microphone warm is ready at once), then the fixed start delay;
  4. play the clip, padded with silence, into BlackHole 2ch (the VM's default input);
  5. wait the fixed tail, release the key (or tap it again);
  6. read the document through Accessibility every 20 ms until the text has not changed for --settle
     seconds; latency = key-up -> last change of the text (finalize_s), first_text_s = key-up -> first text.

Every app gets the same audio, padding and timing; only the key differs (`APPS`). Vocet's
`--debug-audio-file` shortcut (`--driver vocet-shortcut`, debug builds) skips the microphone: the clip is
written to the one WAV path the app was started with, then `--press` / `--release`, and the same TextEdit
read; it exists to check that the black-box path measures the same thing (`agree`).

    uv run eval/app_bench.py run --app handy --clips ~/bench/clips -o ~/bench/out/handy_local/results.csv
    uv run eval/app_bench.py run --driver vocet-shortcut --app vocet --clips ... -o ...
    python3 eval/app_bench.py dry-run                 # the whole loop against a stub app, no macOS needed
    python3 eval/app_bench.py summarize eval/work/bench1/*/ [--format md|csv]
    python3 eval/app_bench.py agree eval/work/bench1/vocet_cloud eval/work/bench1/vocet_cloud_shortcut

The results CSV holds transcripts of CC BY-NC clips: it lives under eval/work/ (git-ignored) and only the
aggregates from `summarize` are committed. A rerun with the same -o resumes (clips already `ok` are kept).
Stdlib only outside `run`; `run` needs macOS and the dependencies above (uv installs them).
"""
from __future__ import annotations

import argparse
import csv
import json
import math
import os
import pathlib
import platform
import subprocess
import sys
import time
from dataclasses import dataclass, field
from datetime import datetime, timezone

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

FIELDS = ["app", "mode", "driver", "clip_id", "status", "attempts", "transcript", "audio_s", "ready_s",
          "first_text_s", "finalize_s", "error", "run_at"]

# Fixed for every app (the Voice Arena harness's values, which PR #323 used for Vocet).
PAD_MS = 300
START_DELAY_S = 0.5
TAIL_S = 4.0
SETTLE_S = 2.0
TIMEOUT_S = 30.0
READY_TIMEOUT_S = 5.0
GAP_S = 1.0
POLL_S = 0.02

# --- Keys --------------------------------------------------------------------------------------------
# macOS virtual key codes, and for modifiers the CGEventFlags they set (generic mask | device bit).
SHIFT, CONTROL, ALTERNATE, COMMAND, SECONDARY_FN = 0x20000, 0x40000, 0x80000, 0x100000, 0x800000
NON_COALESCED = 0x100
MODIFIERS = {  # name: (keycode, flags)
    "lctrl": (59, CONTROL | 0x1), "rctrl": (62, CONTROL | 0x2000),
    "lshift": (56, SHIFT | 0x2), "rshift": (60, SHIFT | 0x4),
    "lcmd": (55, COMMAND | 0x8), "rcmd": (54, COMMAND | 0x10),
    "lalt": (58, ALTERNATE | 0x20), "ralt": (61, ALTERNATE | 0x40),
    "fn": (63, SECONDARY_FN),
}
KEYS = {"space": 49, "grave": 50, "return": 36, "escape": 53, "tab": 48,
        "f13": 105, "f14": 107, "f15": 113, "f16": 106, "f17": 64, "f18": 79, "f19": 80, "f20": 90}
ALIASES = {"option": "lalt", "alt": "lalt", "ropt": "ralt", "rightoption": "ralt", "command": "lcmd",
           "cmd": "lcmd", "control": "lctrl", "ctrl": "lctrl", "shift": "lshift", "globe": "fn"}


@dataclass(frozen=True)
class Key:
    name: str
    code: int
    flags: int = 0  # non-zero for a modifier: it is sent as flagsChanged with these flags

    @property
    def modifier(self) -> bool:
        return self.flags != 0


def parse_hotkey(spec: str) -> list[Key]:
    """'ralt' or 'lalt+space' -> the keys in press order (released in reverse). Modifiers come first."""
    keys = []
    for part in spec.lower().replace(" ", "").split("+"):
        name = ALIASES.get(part, part)
        if name in MODIFIERS:
            code, flags = MODIFIERS[name]
            keys.append(Key(name, code, flags))
        elif name in KEYS:
            keys.append(Key(name, KEYS[name]))
        else:
            raise ValueError(f"unknown key '{part}' in hotkey '{spec}'")
    plain = [i for i, k in enumerate(keys) if not k.modifier]
    if plain and (len(plain) > 1 or plain[0] != len(keys) - 1):
        raise ValueError(f"at most one non-modifier key, and it goes last: '{spec}'")
    return keys


def key_events(keys: list[Key], down: bool) -> list[tuple[int, bool, int, bool]]:
    """The events for pressing (down) or releasing a chord: (keycode, is_down, flags, is_flags_changed).
    Modifiers carry the cumulative flags, as the hardware sends them; the main key carries the held ones."""
    events, held = [], 0
    order = keys if down else list(reversed(keys))
    if down:
        for k in order:
            if k.modifier:
                held |= k.flags
                events.append((k.code, True, held | NON_COALESCED, True))
            else:
                events.append((k.code, True, held, False))
    else:
        held = 0
        for k in keys:
            held |= k.flags
        for k in order:
            if k.modifier:
                held &= ~k.flags
                events.append((k.code, False, (held | NON_COALESCED), True))
            else:
                events.append((k.code, False, held, False))
    return events


# --- Apps --------------------------------------------------------------------------------------------
@dataclass(frozen=True)
class App:
    process: str           # pgrep -x name, for the preflight
    hotkey: str            # parse_hotkey spec
    mode: str = "hold"     # hold: down..up; toggle: tap to start, tap to stop
    bundle: str = ""       # where it is installed in the VM, for the record
    notes: str = ""


APPS: dict[str, App] = {
    "vocet": App("Vocet", "ralt", "hold", "~/Applications/Vocet.app", "hold-to-talk, Right Option (default)"),
    "handy": App("handy", "lalt+space", "hold", "/Applications/Handy.app",
                 "push-to-talk on (default), Option+Space (default)"),
    "openwhispr": App("OpenWhispr", "rcmd", "hold", "/Applications/OpenWhispr.app", "push-to-talk"),
    "muesli": App("Muesli", "rcmd", "hold", "/Applications/Muesli.app", "hold-to-talk"),
    "fluidvoice": App("FluidVoice", "rcmd", "hold", "/Applications/FluidVoice.app", "press and hold"),
    "voiceink": App("VoiceInk", "rcmd", "hold", "/Applications/VoiceInk.app", "push-to-talk"),
    "freeflow": App("FreeFlow", "fn", "hold", "/Applications/FreeFlow.app", "hold Fn (default)"),
    "stub": App("stub", "ralt", "hold", "", "the dry-run's simulated app"),
}


# --- The loop ----------------------------------------------------------------------------------------
@dataclass
class Clip:
    clip_id: str
    path: pathlib.Path
    audio_s: float
    reference: str = ""  # only the stub app uses it (it "transcribes" perfectly)


@dataclass
class Timing:
    start_delay: float = START_DELAY_S
    tail: float = TAIL_S
    settle: float = SETTLE_S
    timeout: float = TIMEOUT_S
    ready_timeout: float = READY_TIMEOUT_S
    gap: float = GAP_S
    poll: float = POLL_S
    pad_ms: int = PAD_MS


class ClipError(Exception):
    """This attempt failed (focus lost, document not empty...); the clip is retried."""


class Clock:
    def now(self) -> float:
        return time.monotonic()

    def sleep(self, s: float) -> None:
        if s > 0:
            time.sleep(s)


class Target:
    """Where the text lands. prepare() empties it and brings it to the front; read() returns its text,
    or raises ClipError if it lost the focus."""

    def prepare(self) -> None: ...
    def read(self) -> str: ...


class Driver:
    """How the app is started and stopped, and how the audio reaches it."""

    def before_clip(self, clip: Clip) -> None: ...
    def start(self) -> None: ...
    def mic_open(self) -> bool | None:
        """True once the app is capturing; None if this driver can't tell (then the fixed delay only)."""
        return None
    def feed(self, clip: Clip) -> None: ...
    def stop(self) -> None: ...
    def reset(self) -> None:
        """After a failed attempt: make sure no key is left held."""


@dataclass
class Result:
    status: str  # ok | empty | error
    transcript: str = ""
    ready_s: float | None = None
    first_text_s: float | None = None
    finalize_s: float | None = None
    error: str = ""


def wait_text(target: Target, clock: Clock, t_up: float, timing: Timing) -> Result:
    """Polls until the text is non-empty and unchanged for `settle` seconds, or `timeout` after key-up."""
    text, first, last = "", None, None
    while True:
        now = clock.now()
        cur = target.read()
        if cur != text:
            text, last = cur, now
            if first is None and cur.strip():
                first = now
        if text.strip() and last is not None and now - last >= timing.settle:
            break
        if now - t_up >= timing.timeout:
            break
        clock.sleep(timing.poll)
    if not text.strip():
        return Result("empty", error=f"no text within {timing.timeout:g} s of key-up")
    return Result("ok", text.strip(), first_text_s=round(first - t_up, 3), finalize_s=round(last - t_up, 3))


def run_clip(clip: Clip, driver: Driver, target: Target, clock: Clock, timing: Timing) -> Result:
    target.prepare()
    driver.before_clip(clip)
    t_down = clock.now()
    driver.start()
    ready = None
    while True:
        state = driver.mic_open()
        if state is None:
            break
        if state:
            ready = round(clock.now() - t_down, 3)
            break
        if clock.now() - t_down >= timing.ready_timeout:
            driver.stop()
            raise ClipError(f"the app didn't open the microphone within {timing.ready_timeout:g} s")
        clock.sleep(0.005)
    clock.sleep(timing.start_delay)
    driver.feed(clip)
    clock.sleep(timing.tail)
    t_up = clock.now()
    driver.stop()
    res = wait_text(target, clock, t_up, timing)
    res.ready_s = ready
    return res


def load_done(path: pathlib.Path) -> dict[str, dict]:
    if not path.exists():
        return {}
    with path.open(newline="", encoding="utf-8") as fh:
        return {r["clip_id"]: r for r in csv.DictReader(fh)}


def write_rows(path: pathlib.Path, rows: dict[str, dict]) -> None:
    tmp = path.with_suffix(".tmp")
    with tmp.open("w", newline="", encoding="utf-8") as fh:
        w = csv.DictWriter(fh, FIELDS, extrasaction="ignore")
        w.writeheader()
        for r in rows.values():
            w.writerow(r)
    tmp.replace(path)


def fmt(v) -> str:
    return "" if v is None else str(v)


def run_all(clips: list[Clip], driver: Driver, target: Target, clock: Clock, timing: Timing, out: pathlib.Path,
            *, app: str, mode: str, driver_name: str, attempts: int = 2, warmup: int = 0,
            log=print) -> dict[str, dict]:
    """Runs every clip not already `ok` in `out` (resume), up to `attempts` tries each (an empty result is
    retried too), writing the CSV after each clip. First `warmup` dictations of the first clip, not
    recorded, so a local model is loaded before anything is timed. Returns the rows by clip_id."""
    out.parent.mkdir(parents=True, exist_ok=True)
    rows = load_done(out)
    for w in range(warmup if clips else 0):
        try:
            res = run_clip(clips[0], driver, target, clock, timing)
            log(f"warm-up {w + 1}/{warmup}: {res.status} finalize={fmt(res.finalize_s)} (not recorded)")
        except ClipError as e:
            driver.reset()
            log(f"warm-up {w + 1}/{warmup}: {e}")
        clock.sleep(timing.gap)
    for i, clip in enumerate(clips, 1):
        if rows.get(clip.clip_id, {}).get("status") == "ok":
            continue
        res, tries = Result("error", error="not run"), 0
        for tries in range(1, attempts + 1):
            try:
                res = run_clip(clip, driver, target, clock, timing)
            except ClipError as e:
                driver.reset()
                res = Result("error", error=str(e))
            if res.status == "ok":
                break
            clock.sleep(timing.gap + timing.settle)
        rows[clip.clip_id] = {
            "app": app, "mode": mode, "driver": driver_name, "clip_id": clip.clip_id, "status": res.status,
            "attempts": tries, "transcript": res.transcript, "audio_s": round(clip.audio_s, 3),
            "ready_s": fmt(res.ready_s), "first_text_s": fmt(res.first_text_s), "finalize_s": fmt(res.finalize_s),
            "error": res.error, "run_at": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")}
        write_rows(out, rows)
        log(f"[{i}/{len(clips)}] {clip.clip_id} {res.status} tries={tries} ready={fmt(res.ready_s)} "
            f"finalize={fmt(res.finalize_s)} chars={len(res.transcript)} {res.error}")
        clock.sleep(timing.gap)
    return rows


def load_clips(folder: pathlib.Path, limit: int | None = None, only: set[str] | None = None) -> list[Clip]:
    """Clips in manifest order (manifest.csv from fetch_dataset.py: file, clip_id, audio_length_s,
    intended_text)."""
    with (folder / "manifest.csv").open(newline="", encoding="utf-8") as fh:
        man = list(csv.DictReader(fh))
    clips = []
    for r in man:
        if only and r["clip_id"] not in only:
            continue
        p = folder / pathlib.Path(r.get("file") or f"{r['clip_id']}.flac").name
        clips.append(Clip(r["clip_id"], p, float(r.get("audio_length_s") or 0), r.get("intended_text", "")))
    return clips[:limit] if limit else clips


# --- The stub app (dry run and tests) ----------------------------------------------------------------
class FakeClock(Clock):
    def __init__(self) -> None:
        self.t = 1000.0

    def now(self) -> float:
        return self.t

    def sleep(self, s: float) -> None:
        self.t += max(s, 0.0)


@dataclass
class StubApp:
    """A dictation app on a simulated clock: the microphone opens `open_delay` after key-down (0 when warm),
    it hears what is played while it is open, and `latency` after key-up it pastes the clip's reference
    text if it heard the whole clip (a prefix otherwise, nothing if it heard nothing)."""
    clock: FakeClock
    open_delay: float = 0.3
    latency: float = 1.2
    warm: bool = False
    never_opens: bool = False
    paste_in_chunks: int = 1
    pressed_at: float | None = None
    heard: tuple[float, float] | None = None
    played: tuple[float, float, Clip] | None = None
    pending: list[tuple[float, str]] = field(default_factory=list)
    text: str = ""
    presses: int = 0

    def key(self, down: bool) -> None:
        now = self.clock.now()
        if down:
            self.presses += 1
            self.pressed_at = now
            return
        if self.pressed_at is None:
            return
        open_at = self.pressed_at + (0 if self.warm else self.open_delay)
        self.pressed_at = None
        if self.never_opens or not self.played:
            return
        p0, p1, clip = self.played
        heard = max(0.0, min(now, p1) - max(open_at, p0)) / max(p1 - p0, 1e-9)
        ref = clip.reference
        words = ref.split()
        out = " ".join(words[:round(len(words) * heard)]) if heard < 0.999 else ref
        if out:
            n = max(1, self.paste_in_chunks)
            step = math.ceil(len(out) / n)
            for i in range(n):
                self.pending.append((now + self.latency + i * 0.1, out[:step * (i + 1)]))
        self.played = None

    def mic_open(self) -> bool:
        if self.never_opens or self.pressed_at is None:
            return False
        return self.warm or self.clock.now() >= self.pressed_at + self.open_delay

    def play(self, clip: Clip, seconds: float) -> None:
        t0 = self.clock.now()
        self.clock.sleep(seconds)
        self.played = (t0, self.clock.now(), clip)

    def visible_text(self) -> str:
        now = self.clock.now()
        while self.pending and self.pending[0][0] <= now:
            self.text = self.pending.pop(0)[1]
        return self.text


class StubDriver(Driver):
    def __init__(self, app: StubApp, pad_ms: int = PAD_MS) -> None:
        self.app, self.pad_ms = app, pad_ms

    def start(self) -> None:
        self.app.key(True)

    def mic_open(self) -> bool | None:
        return self.app.mic_open()

    def feed(self, clip: Clip) -> None:
        self.app.play(clip, clip.audio_s + 2 * self.pad_ms / 1000)

    def stop(self) -> None:
        self.app.key(False)

    def reset(self) -> None:
        self.app.pressed_at = None


class StubTarget(Target):
    def __init__(self, app: StubApp) -> None:
        self.app = app

    def prepare(self) -> None:
        self.app.text, self.app.pending = "", []

    def read(self) -> str:
        return self.app.visible_text()


def dry_run(out: pathlib.Path, n: int = 5, latency: float = 1.2, log=print) -> dict[str, dict]:
    clock = FakeClock()
    app = StubApp(clock, latency=latency)
    clips = [Clip(f"stub{i:03d}", pathlib.Path(f"stub{i:03d}.flac"), 2.0 + i, f"stub clip number {i} says hello")
             for i in range(1, n + 1)]
    return run_all(clips, StubDriver(app), StubTarget(app), clock, Timing(), out, app="stub", mode="dry-run",
                   driver_name="stub", log=log)


# --- macOS: keys, audio, TextEdit, Vocet's shortcut ---------------------------------------------------
class MacKeys:
    def __init__(self, keys: list[Key]) -> None:
        import Quartz
        self.Q, self.keys, self.held = Quartz, keys, False

    def _post(self, down: bool) -> None:
        Q = self.Q
        for code, is_down, flags, is_flags in key_events(self.keys, down):
            ev = Q.CGEventCreateKeyboardEvent(None, code, is_down)
            if is_flags:
                Q.CGEventSetType(ev, Q.kCGEventFlagsChanged)
            Q.CGEventSetFlags(ev, flags)
            Q.CGEventPost(Q.kCGHIDEventTap, ev)
            time.sleep(0.01)

    def down(self) -> None:
        self.held = True
        self._post(True)

    def up(self) -> None:
        self._post(False)
        self.held = False

    def tap(self) -> None:
        self.down()
        time.sleep(0.08)
        self.up()


class CoreAudio:
    """BlackHole lookups through the CoreAudio HAL (ctypes; pyobjc doesn't wrap it)."""

    def __init__(self) -> None:
        import ctypes
        import ctypes.util
        self.c = ctypes
        self.ca = ctypes.CDLL(ctypes.util.find_library("CoreAudio"))
        self.cf = ctypes.CDLL(ctypes.util.find_library("CoreFoundation"))

        class Addr(ctypes.Structure):
            _fields_ = [("selector", ctypes.c_uint32), ("scope", ctypes.c_uint32), ("element", ctypes.c_uint32)]
        self.Addr = Addr
        u32p = ctypes.POINTER(ctypes.c_uint32)
        self.ca.AudioObjectGetPropertyDataSize.argtypes = [ctypes.c_uint32, ctypes.POINTER(Addr), ctypes.c_uint32,
                                                           ctypes.c_void_p, u32p]
        self.ca.AudioObjectGetPropertyData.argtypes = [ctypes.c_uint32, ctypes.POINTER(Addr), ctypes.c_uint32,
                                                       ctypes.c_void_p, u32p, ctypes.c_void_p]
        self.ca.AudioObjectSetPropertyData.argtypes = [ctypes.c_uint32, ctypes.POINTER(Addr), ctypes.c_uint32,
                                                       ctypes.c_void_p, ctypes.c_uint32, ctypes.c_void_p]
        self.cf.CFStringGetCString.argtypes = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_long, ctypes.c_uint32]
        self.cf.CFRelease.argtypes = [ctypes.c_void_p]

    def addr(self, sel: str, scope: str = "glob"):
        return self.Addr(int.from_bytes(sel.encode(), "big"), int.from_bytes(scope.encode(), "big"), 0)

    def u32(self, obj: int, sel: str, scope: str = "glob") -> int | None:
        c = self.c
        val, size = c.c_uint32(0), c.c_uint32(4)
        a = self.addr(sel, scope)
        err = self.ca.AudioObjectGetPropertyData(obj, c.byref(a), 0, None, c.byref(size), c.byref(val))
        return None if err else val.value

    def name(self, dev: int) -> str:
        c = self.c
        ref, size = c.c_void_p(), c.c_uint32(c.sizeof(c.c_void_p))
        a = self.addr("lnam")
        if self.ca.AudioObjectGetPropertyData(dev, c.byref(a), 0, None, c.byref(size), c.byref(ref)):
            return ""
        buf = c.create_string_buffer(512)
        self.cf.CFStringGetCString(ref, buf, 512, 0x08000100)
        self.cf.CFRelease(ref)
        return buf.value.decode()

    def inputs(self) -> dict[str, int]:
        c = self.c
        a, size = self.addr("dev#"), c.c_uint32(0)
        self.ca.AudioObjectGetPropertyDataSize(1, c.byref(a), 0, None, c.byref(size))
        n = size.value // 4
        ids = (c.c_uint32 * n)()
        self.ca.AudioObjectGetPropertyData(1, c.byref(a), 0, None, c.byref(size), ids)
        out = {}
        for d in ids:
            s, sa = c.c_uint32(0), self.addr("stm#", "inpt")
            self.ca.AudioObjectGetPropertyDataSize(d, c.byref(sa), 0, None, c.byref(s))
            if s.value:
                out[self.name(d)] = d
        return out

    def default_input_name(self) -> str:
        dev = self.u32(1, "dIn ")
        return self.name(dev) if dev else ""

    def set_default_input(self, dev: int) -> None:
        c = self.c
        val, a = c.c_uint32(dev), self.addr("dIn ")
        if self.ca.AudioObjectSetPropertyData(1, c.byref(a), 0, None, 4, c.byref(val)):
            raise SystemExit("couldn't set the default input")

    def running_somewhere(self, dev: int) -> bool:
        return bool(self.u32(dev, "gone"))


class BlackHoleDriver(Driver):
    """The black-box path: the app's hotkey, the clip played into BlackHole 2ch, the mic-open signal."""

    def __init__(self, app: App, timing: Timing, device: str = "BlackHole 2ch") -> None:
        import numpy as np
        import sounddevice as sd
        self.np, self.sd, self.app, self.timing = np, sd, app, timing
        self.keys = MacKeys(parse_hotkey(app.hotkey))
        self.ca = CoreAudio()
        inputs = self.ca.inputs()
        hit = [d for n, d in inputs.items() if device.lower() in n.lower()]
        if not hit:
            raise SystemExit(f"no input device '{device}' (inputs: {list(inputs)})")
        self.bh_in = hit[0]
        self.out_dev = next((i for i, d in enumerate(sd.query_devices())
                             if device.lower() in d["name"].lower() and d["max_output_channels"] > 0), None)
        if self.out_dev is None:
            raise SystemExit(f"no output device '{device}'")
        self.rate = int(sd.query_devices(self.out_dev)["default_samplerate"])
        self.channels = sd.query_devices(self.out_dev)["max_output_channels"]
        self.audio = None

    def before_clip(self, clip: Clip) -> None:
        import soundfile as sf
        import soxr
        np = self.np
        a, sr = sf.read(clip.path, dtype="float32", always_2d=True)
        a = a.mean(axis=1)
        if sr != self.rate:
            a = soxr.resample(a, sr, self.rate).astype("float32")
        peak = float(np.abs(a).max()) if a.size else 0.0
        if peak > 1.0:
            a /= peak
        pad = np.zeros(int(self.rate * self.timing.pad_ms / 1000), dtype="float32")
        self.audio = np.repeat(np.concatenate([pad, a, pad])[:, None], self.channels, axis=1)

    def start(self) -> None:
        if self.app.mode == "toggle":
            self.keys.tap()
        else:
            self.keys.down()

    def mic_open(self) -> bool | None:
        return self.ca.running_somewhere(self.bh_in)

    def feed(self, clip: Clip) -> None:
        self.sd.play(self.audio, self.rate, device=self.out_dev, blocking=True)

    def stop(self) -> None:
        if self.app.mode == "toggle":
            self.keys.tap()
        elif self.keys.held:
            self.keys.up()

    def reset(self) -> None:
        if self.keys.held:
            self.keys.up()


class VocetShortcutDriver(Driver):
    """Vocet's debug-only `--debug-audio-file`: the app was started with that flag and one WAV path; it
    reads the file on each press. The clip (16 kHz mono, the same padding) is written there, then
    `--press` / `--release` are sent to the running app with `open`."""

    def __init__(self, app_path: str, wav: pathlib.Path, timing: Timing, hold: float = 0.3) -> None:
        self.app_path, self.wav, self.timing, self.hold = app_path, wav, timing, hold

    def before_clip(self, clip: Clip) -> None:
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

    def feed(self, clip: Clip) -> None:
        time.sleep(self.hold)

    def stop(self) -> None:
        self._send("--release")


class TextEditTarget(Target):
    """A plain-text TextEdit document (`scratch`), emptied and read through Accessibility."""

    def __init__(self, scratch: pathlib.Path) -> None:
        import AppKit
        import ApplicationServices as AS
        self.AK, self.AS, self.scratch = AppKit, AS, scratch
        scratch.parent.mkdir(parents=True, exist_ok=True)
        scratch.write_text("")
        self.area = None

    def _get(self, el, attr):
        err, val = self.AS.AXUIElementCopyAttributeValue(el, attr, None)
        return val if err == 0 else None

    def _find_area(self):
        apps = self.AK.NSRunningApplication.runningApplicationsWithBundleIdentifier_("com.apple.TextEdit")
        if not apps:
            raise ClipError("TextEdit isn't running")
        el = self.AS.AXUIElementCreateApplication(apps[0].processIdentifier())
        win = self._get(el, "AXFocusedWindow") or self._get(el, "AXMainWindow")

        def find(node, depth=0):
            if node is None or depth > 8:
                return None
            if self._get(node, "AXRole") == "AXTextArea":
                return node
            for ch in self._get(node, "AXChildren") or []:
                hit = find(ch, depth + 1)
                if hit is not None:
                    return hit
            return None
        return find(win)

    def prepare(self) -> None:
        subprocess.run(["open", "-a", "TextEdit", str(self.scratch)], check=False)
        for _ in range(50):
            front = self.AK.NSWorkspace.sharedWorkspace().frontmostApplication()
            if front and front.bundleIdentifier() == "com.apple.TextEdit":
                break
            time.sleep(0.1)
        else:
            raise ClipError("TextEdit didn't come to the front")
        self.area = self._find_area()
        if self.area is None:
            raise ClipError("no text area in TextEdit's front window")
        self.AS.AXUIElementSetAttributeValue(self.area, "AXFocused", True)
        self.AS.AXUIElementSetAttributeValue(self.area, "AXValue", "")
        time.sleep(0.2)
        if self.read():  # not settable here: select all and delete, as a person would
            MacKeys([Key("lcmd", *MODIFIERS["lcmd"]), Key("a", 0)]).tap()
            MacKeys([Key("delete", 51)]).tap()
            time.sleep(0.3)
        if self.read():
            raise ClipError("the document didn't empty")

    def read(self) -> str:
        """The document's text. It doesn't check the focus: a paste that went elsewhere shows as no text."""
        v = self._get(self.area, "AXValue")
        return "" if v is None else str(v)


def mac_preflight(app: App, check_process: bool) -> dict:
    """What the run depends on, for the record; exits if it's wrong."""
    env = {"macos": platform.mac_ver()[0], "machine": platform.machine()}
    for k in ("NSAutomaticSpellingCorrectionEnabled", "NSAutomaticQuoteSubstitutionEnabled",
              "NSAutomaticDashSubstitutionEnabled", "NSAutomaticTextReplacementEnabled",
              "NSAutomaticCapitalizationEnabled", "NSAutomaticPeriodSubstitutionEnabled"):
        v = subprocess.run(["defaults", "read", "-g", k], capture_output=True, text=True).stdout.strip()
        if v not in ("0", "false"):
            raise SystemExit(f"Mac-wide text substitution {k} is on: defaults write -g {k} -bool false")
    ca = CoreAudio()
    env["default_input"] = ca.default_input_name()
    if "blackhole" not in env["default_input"].lower():
        raise SystemExit(f"the default input is '{env['default_input']}', not BlackHole 2ch (bench.sh vm-setup)")
    if check_process and subprocess.run(["pgrep", "-x", app.process], capture_output=True).returncode:
        raise SystemExit(f"{app.process} isn't running: open it (past onboarding) and rerun")
    return env


# --- Summaries (host) --------------------------------------------------------------------------------
def percentile(values: list[float], p: float) -> float | None:
    if not values:
        return None
    s = sorted(values)
    return s[max(0, math.ceil(p / 100 * len(s)) - 1)]


def read_csv(path: pathlib.Path) -> list[dict]:
    with path.open(newline="", encoding="utf-8") as fh:
        return list(csv.DictReader(fh))


def summarize_run(run: pathlib.Path, judge: str = "claude") -> dict:
    """One row per run folder: results.csv (this harness), scored*.csv (eval/judge.py), meta.json (how the
    app was set up). ZER is over every clip of the scored CSV (a clip with no text counts as wrong)."""
    meta = json.loads((run / "meta.json").read_text()) if (run / "meta.json").exists() else {}
    res = read_csv(run / "results.csv") if (run / "results.csv").exists() else []
    scored_name = {"claude": "scored.csv"}.get(judge, f"scored_{judge}.csv")
    scored = read_csv(run / scored_name) if (run / scored_name).exists() else []
    judged = [r for r in scored if r.get("edits", "") != ""]
    fin = [float(r["finalize_s"]) for r in res if r.get("status") == "ok" and r.get("finalize_s")]
    first = [float(r["first_text_s"]) for r in res if r.get("status") == "ok" and r.get("first_text_s")]
    out = {"run": run.name, **{k: meta.get(k, "") for k in ("app", "mode", "version", "commit", "source",
                                                             "model", "model_size", "cleanup", "driver")},
           "clips": len(res), "ok": sum(r.get("status") == "ok" for r in res),
           "judged": len(judged), "judge": judge,
           "zer": round(100 * sum(int(r["edits"]) == 0 for r in judged) / len(judged), 1) if judged else None,
           "amend": (round(100 * sum(int(r["edits"]) for r in judged) / max(1, sum(int(r["chars"]) for r in judged)),
                           2) if judged else None),
           "p50_s": percentile(fin, 50), "p95_s": percentile(fin, 95),
           "first_p50_s": percentile(first, 50), "date": meta.get("date", "")}
    return out


SUMMARY_COLS = ["app", "mode", "model", "model_size", "cleanup", "source", "version", "commit", "driver", "clips",
                "ok", "judged", "zer", "amend", "p50_s", "p95_s", "first_p50_s", "date", "run"]


def summary_table(rows: list[dict], fmt_: str = "md") -> str:
    def cell(v):
        if v is None or v == "":
            return "-"
        return f"{v:.2f}" if isinstance(v, float) and not v.is_integer() else str(v)
    if fmt_ == "csv":
        import io
        buf = io.StringIO()
        w = csv.DictWriter(buf, SUMMARY_COLS, extrasaction="ignore", lineterminator="\n")
        w.writeheader()
        w.writerows(rows)
        return buf.getvalue()
    head = "| " + " | ".join(SUMMARY_COLS) + " |\n|" + "---|" * len(SUMMARY_COLS) + "\n"
    return head + "".join("| " + " | ".join(cell(r.get(c)) for c in SUMMARY_COLS) + " |\n" for r in rows)


def agreement(a_scored: list[dict], b_scored: list[dict], tolerance_pts: float) -> dict:
    """Black-box (a) vs shortcut (b) on the clips both judged: ZER of each, the difference, the paired
    discordant clips, and whether the difference is within `tolerance_pts`. The tolerance is the noise of a
    151-clip ZER: the standard error of a paired difference, 100*sqrt(discordant)/n, doubled (about a 95%
    interval), but never below the stated floor."""
    a = {r["clip_id"]: int(r["edits"]) == 0 for r in a_scored if r.get("edits", "") != ""}
    b = {r["clip_id"]: int(r["edits"]) == 0 for r in b_scored if r.get("edits", "") != ""}
    both = sorted(set(a) & set(b))
    n = len(both)
    if not n:
        return {"n": 0, "agree": False}
    za = 100 * sum(a[c] for c in both) / n
    zb = 100 * sum(b[c] for c in both) / n
    only_a = sum(a[c] and not b[c] for c in both)
    only_b = sum(b[c] and not a[c] for c in both)
    noise = 2 * 100 * math.sqrt(only_a + only_b) / n
    tol = max(tolerance_pts, noise)
    return {"n": n, "zer_blackbox": round(za, 1), "zer_shortcut": round(zb, 1), "diff_pts": round(za - zb, 1),
            "only_blackbox_right": only_a, "only_shortcut_right": only_b, "tolerance_pts": round(tol, 1),
            "agree": abs(za - zb) <= tol}


# --- CLI ---------------------------------------------------------------------------------------------
def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    sub = ap.add_subparsers(dest="cmd", required=True)
    r = sub.add_parser("run", help="run the clips through an app (macOS, in the VM's Terminal)")
    r.add_argument("--app", required=True, choices=sorted(APPS))
    r.add_argument("--mode", default="local", help="label stored on each row: local, cloud, ...")
    r.add_argument("--driver", default="blackhole", choices=["blackhole", "vocet-shortcut"])
    r.add_argument("--hotkey", help="override the app's hotkey (parse_hotkey spec)")
    r.add_argument("--toggle", action="store_true", help="tap to start and tap to stop instead of holding")
    r.add_argument("--clips", type=pathlib.Path, required=True)
    r.add_argument("-o", "--output", type=pathlib.Path, required=True)
    r.add_argument("--limit", type=int)
    r.add_argument("--only", help="comma-separated clip ids")
    r.add_argument("--attempts", type=int, default=2)
    r.add_argument("--warmup", type=int, default=1, help="unrecorded dictations before timing (loads the model)")
    r.add_argument("--wav", type=pathlib.Path, default=pathlib.Path.home() / "bench" / "shortcut.wav",
                   help="vocet-shortcut: the path Vocet was started with (--debug-audio-file)")
    r.add_argument("--check", action="store_true", help="preflight only")
    si = sub.add_parser("set-input", help="make an input device the default (macOS)")
    si.add_argument("device", nargs="?", default="BlackHole 2ch")
    d = sub.add_parser("dry-run", help="the whole loop against the stub app (no macOS)")
    d.add_argument("-o", "--output", type=pathlib.Path)
    d.add_argument("-n", type=int, default=5)
    s = sub.add_parser("summarize", help="one table row per run folder")
    s.add_argument("runs", nargs="+", type=pathlib.Path)
    s.add_argument("--judge", default="claude")
    s.add_argument("--format", default="md", choices=["md", "csv", "json"])
    g = sub.add_parser("agree", help="black-box vs Vocet shortcut on the same clips")
    g.add_argument("blackbox", type=pathlib.Path)
    g.add_argument("shortcut", type=pathlib.Path)
    g.add_argument("--judge", default="claude")
    g.add_argument("--tolerance", type=float, default=3.0, help="floor of the tolerance, ZER points")
    a = ap.parse_args(argv)

    if a.cmd == "dry-run":
        import tempfile
        with tempfile.TemporaryDirectory() as tmp:
            out = a.output or pathlib.Path(tmp) / "results.csv"
            rows = dry_run(out, a.n)
        bad = [r for r in rows.values() if r["status"] != "ok"]
        print(f"dry run: {len(rows) - len(bad)}/{len(rows)} ok")
        return 1 if bad else 0
    if a.cmd == "set-input":
        ca = CoreAudio()
        hit = [d for n, d in ca.inputs().items() if a.device.lower() in n.lower()]
        if not hit:
            raise SystemExit(f"no input '{a.device}' (inputs: {list(ca.inputs())})")
        ca.set_default_input(hit[0])
        print(f"default input: {ca.default_input_name()}")
        return 0
    if a.cmd == "summarize":
        rows = [summarize_run(p, a.judge) for p in a.runs]
        print(json.dumps(rows, indent=1) if a.format == "json" else summary_table(rows, a.format), end="")
        return 0
    if a.cmd == "agree":
        name = {"claude": "scored.csv"}.get(a.judge, f"scored_{a.judge}.csv")
        res = agreement(read_csv(a.blackbox / name), read_csv(a.shortcut / name), a.tolerance)
        print(json.dumps(res, indent=1))
        return 0 if res.get("agree") else 1

    app = APPS[a.app]
    if a.hotkey or a.toggle:
        app = App(app.process, a.hotkey or app.hotkey, "toggle" if a.toggle else app.mode, app.bundle, app.notes)
    env = mac_preflight(app, check_process=True)
    print(f"preflight ok: {json.dumps(env)}; app {a.app} key {app.hotkey} ({app.mode}), driver {a.driver}")
    if a.check:
        return 0
    timing = Timing()
    if a.driver == "vocet-shortcut":
        timing.tail = 0.5  # the file is read whole on --release; no audio to wait for
        driver: Driver = VocetShortcutDriver(app.bundle, a.wav, timing)
    else:
        driver = BlackHoleDriver(app, timing)
    target = TextEditTarget(pathlib.Path.home() / "bench" / "scratch.txt")
    clips = load_clips(a.clips, a.limit, set(a.only.split(",")) if a.only else None)
    caff = subprocess.Popen(["caffeinate", "-dimsu"])
    try:
        rows = run_all(clips, driver, target, Clock(), timing, a.output, app=a.app, mode=a.mode,
                       driver_name=a.driver, attempts=a.attempts, warmup=a.warmup)
    finally:
        driver.reset()
        caff.terminate()
    ok = sum(r["status"] == "ok" for r in rows.values())
    print(f"done: {ok}/{len(rows)} ok -> {a.output}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
