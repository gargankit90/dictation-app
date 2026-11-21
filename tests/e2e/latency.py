#!/usr/bin/env python3
"""Key-up to paste time per dictation, from the app's own log (Q1).

    python3 tests/e2e/latency.py <app.log> [--csv out.csv] [--line out.txt]

The app logs `state changed phase=<Phase>` with a timestamp. Releasing the hotkey moves the pipeline from
Recording to Transcribing; the paste finishes when it returns to Idle (after Inserting). So one dictation's
time is the gap between its `Transcribing` line and the next `Idle` line: transcription (the mock engine
here, so no network), cleanup, the guard, the cursor read and the paste: the pipeline's own overhead.
A dictation that never reaches Idle, or is cancelled (a new Recording first), is not counted.
Prints the results line the e2e summary shows, e.g.
  LATENCY  key-up to paste, 20 dictations (mock engine): p50 83 ms, p95 121 ms
Exit 1 if no dictation was found, so a scenario can't pass on an empty log.
"""
from __future__ import annotations

import argparse
import math
import re
import sys
from datetime import datetime, timezone

LINE = re.compile(r"^\s*(\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d(?:\.\d+)?)(Z|[+-]\d\d:?\d\d)?\s.*state changed phase=([A-Za-z]+)")


def parse_ts(stamp: str, zone: str | None) -> float:
    base, _, frac = stamp.partition(".")
    dt = datetime.strptime(base, "%Y-%m-%dT%H:%M:%S")
    if zone and zone != "Z":
        sign = 1 if zone[0] == "+" else -1
        digits = zone[1:].replace(":", "")
        offset = sign * (int(digits[:2]) * 3600 + int(digits[2:]) * 60)
        epoch = dt.replace(tzinfo=timezone.utc).timestamp() - offset
    else:
        epoch = dt.replace(tzinfo=timezone.utc).timestamp()
    return epoch + (float("0." + frac) if frac else 0.0)


def latencies_ms(lines) -> list[float]:
    """One value per completed dictation, in milliseconds."""
    out: list[float] = []
    key_up: float | None = None
    for line in lines:
        m = LINE.match(line)
        if not m:
            continue
        ts, phase = parse_ts(m.group(1), m.group(2)), m.group(3)
        if phase == "Transcribing":
            key_up = ts
        elif phase == "Recording":
            key_up = None  # a new recording before the last one finished: cancelled
        elif phase == "Idle" and key_up is not None:
            out.append(round((ts - key_up) * 1000, 3))
            key_up = None
    return out


def percentile(values: list[float], p: float) -> float:
    """Nearest rank."""
    s = sorted(values)
    return s[max(0, math.ceil(p / 100 * len(s)) - 1)]


def summary_line(values: list[float], engine: str = "mock engine") -> str:
    return (f"LATENCY  key-up to paste, {len(values)} dictations ({engine}): "
            f"p50 {percentile(values, 50):.0f} ms, p95 {percentile(values, 95):.0f} ms")


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("log")
    ap.add_argument("--csv", help="write one row per dictation (n,ms)")
    ap.add_argument("--line", help="write the summary line to this file")
    ap.add_argument("--expect", type=int, help="exit 1 unless exactly this many dictations were found")
    ap.add_argument("--p95-under", type=float, metavar="MS", help="exit 1 if the p95 is not under this")
    a = ap.parse_args()
    with open(a.log, encoding="utf-8", errors="replace") as fh:
        values = latencies_ms(fh)
    if not values:
        print("no completed dictation (Transcribing then Idle) in the log", file=sys.stderr)
        return 1
    line = summary_line(values)
    print(line)
    if a.csv:
        with open(a.csv, "w", encoding="utf-8") as fh:
            fh.write("n,ms\n" + "".join(f"{i},{v}\n" for i, v in enumerate(values, 1)))
    if a.line:
        with open(a.line, "w", encoding="utf-8") as fh:
            fh.write(line + "\n")
    if a.expect is not None and len(values) != a.expect:
        print(f"expected {a.expect} dictations, found {len(values)}", file=sys.stderr)
        return 1
    if a.p95_under is not None and not percentile(values, 95) < a.p95_under:
        print(f"p95 {percentile(values, 95):.0f} ms is not under {a.p95_under:.0f} ms", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
