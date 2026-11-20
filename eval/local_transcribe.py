#!/usr/bin/env python3
"""Transcribe the public set with an on-device engine behind a loopback OpenAI-style endpoint.

    python3 eval/local_transcribe.py --base-url http://127.0.0.1:8080/v1 --model qwen3-asr-1.7b \\
        --label qwen3_local [--clips eval/voicearena/clips] [--limit 5]

Works with anything that serves POST {base-url}/audio/transcriptions with a multipart `file` (llama.cpp's
server, whisper.cpp's, transcribe.cpp's): the ASR screen of PR #331 used llama.cpp that way. Start the
engine yourself; this only sends the clips, one at a time, and times each request (finalize_s = the
engine's seconds per clip, on this machine). The base URL must be loopback: nothing leaves the machine.
Audio is read from the git-ignored clips/ folder and only sent, never copied. No key is needed or read.

Output: eval/work/asr/<label>/harness_results.csv, in the columns judge.py reads (clip_id, status,
transcript, finalize_s, plus model and speaker_id), so `uv run eval/judge.py` and eval/report.py take it as is.
(The ASR screen's own runner was a script in /tmp that was never committed; this is a small replacement,
so its numbers are comparable but not byte-identical. The screen's numbers are in eval/reports/asr-screen.md.)
"""
from __future__ import annotations

import argparse
import csv
import json
import pathlib
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
import uuid

HERE = pathlib.Path(__file__).resolve().parent
DEFAULT_CLIPS = HERE / "clips"
COLUMNS = ["clip_id", "file", "status", "attempts", "transcript", "speaker_id", "model", "finalize_s", "error"]


def require_loopback(url: str) -> None:
    host = urllib.parse.urlparse(url).hostname or ""
    if host not in ("127.0.0.1", "localhost", "::1"):
        sys.exit(f"{url}: not a loopback address; this runner is for on-device engines only")


def multipart(fields: dict[str, str], filename: str, data: bytes) -> tuple[bytes, str]:
    boundary = uuid.uuid4().hex
    out = b""
    for k, v in fields.items():
        out += f'--{boundary}\r\nContent-Disposition: form-data; name="{k}"\r\n\r\n{v}\r\n'.encode()
    out += (f'--{boundary}\r\nContent-Disposition: form-data; name="file"; filename="{filename}"\r\n'
            "Content-Type: application/octet-stream\r\n\r\n").encode() + data + f"\r\n--{boundary}--\r\n".encode()
    return out, f"multipart/form-data; boundary={boundary}"


def transcribe(base_url: str, model: str, path: pathlib.Path, timeout: float) -> tuple[str, float]:
    body, ctype = multipart({"model": model, "language": "en", "response_format": "json", "temperature": "0"},
                            path.name, path.read_bytes())
    req = urllib.request.Request(base_url.rstrip("/") + "/audio/transcriptions", data=body,
                                 headers={"Content-Type": ctype})
    t0 = time.monotonic()
    with urllib.request.urlopen(req, timeout=timeout) as resp:
        text = json.loads(resp.read())["text"]
    return text.strip(), time.monotonic() - t0


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--base-url", required=True, help="loopback root, e.g. http://127.0.0.1:8080/v1")
    ap.add_argument("--model", required=True)
    ap.add_argument("--label", required=True)
    ap.add_argument("--clips", type=pathlib.Path, default=DEFAULT_CLIPS)
    ap.add_argument("--out", type=pathlib.Path, default=HERE / "work" / "asr")
    ap.add_argument("--limit", type=int)
    ap.add_argument("--timeout", type=float, default=120.0)
    a = ap.parse_args()
    require_loopback(a.base_url)
    manifest = a.clips / "manifest.csv"
    if not manifest.exists():
        print(f"{manifest} is missing: run `uv run eval/fetch_dataset.py eval/clips` first", file=sys.stderr)
        return 2
    with manifest.open(newline="", encoding="utf-8") as fh:
        clips = list(csv.DictReader(fh))
    if a.limit:
        clips = clips[: a.limit]
    out_dir = a.out / a.label
    out_dir.mkdir(parents=True, exist_ok=True)
    failed = 0
    with (out_dir / "harness_results.csv").open("w", newline="", encoding="utf-8") as fh:
        w = csv.DictWriter(fh, COLUMNS, lineterminator="\n")
        w.writeheader()
        for c in clips:
            row = {"clip_id": c["clip_id"], "file": c["file"], "speaker_id": c["speaker_id"], "model": a.model,
                   "attempts": 1}
            try:
                text, secs = transcribe(a.base_url, a.model, a.clips / c["file"], a.timeout)
                row.update(status="ok", transcript=text, finalize_s=f"{secs:.3f}")
            except (urllib.error.URLError, OSError, ValueError, KeyError) as e:
                failed += 1
                row.update(status="error", transcript="", finalize_s="", error=type(e).__name__)
            w.writerow(row)
    print(f"{len(clips) - failed}/{len(clips)} clips -> {out_dir / 'harness_results.csv'}")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
