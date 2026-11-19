# /// script
# requires-python = ">=3.11"
# dependencies = ["numpy"]
# ///
"""Run Gemma 4 (E2B or E4B) on the Voice Arena clips with audio in, through llama.cpp's llama-server.

    uv run onepass.py --model ~/vocet-cleanup-models/gemma-4-E2B-it-Q4_K_M.gguf \
        --mmproj ~/vocet-onepass-models/e2b/mmproj-F16.gguf --name gemma4-e2b --mode plain|onepass \
        [--limit N] [--clips dict_0007,dict_0008]

Writes runs/<name>__<mode>/:
  results.csv  per clip: clip_id, status, transcript (the reply, parts joined with a space), parts (count),
               split_at (seconds, ";"-joined), part_replies (JSON list), audio_s, finalize_s (model time for
               the whole clip, all parts), prompt_tokens, completion_tokens.
  meta.json    model + projector files, sizes, sha256, llama.cpp version, load time, peak memory, latency.

Audio: each clip is decoded with ffmpeg to 16 kHz mono (Gemma 4's input format) and sent as 16-bit WAV
(llama.cpp converts it to float in [-1, 1]). Clips over 30 s (Gemma 4's limit) are split at the quietest
0.3 s window between 18 s and 29 s after the start of the current part (20 ms RMS frames); each part gets
0.15 s of the neighbouring audio on both sides of the cut (a short overlap inside the silence), each part is
its own request, and the replies are joined with a space.

Settings: temperature 0, top_k 1, seed 1, thinking off, one slot, all layers and the projector on Metal.
Load time: process start until /health is ok. Peak memory: /usr/bin/time -l on llama-server.
"""
import argparse
import base64
import csv
import hashlib
import io
import json
import os
import pathlib
import re
import signal
import socket
import statistics
import subprocess
import sys
import time
import urllib.request
import wave

import numpy as np

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from prompts_onepass import MODES  # noqa: E402

CLIPS = HERE.parent / "voicearena" / "clips"
SR = 16000
LIMIT_S = 30.0
OVERLAP_S = 0.15


def free_port() -> int:
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


def sha256(path: pathlib.Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def load_audio(path: pathlib.Path) -> np.ndarray:
    raw = subprocess.run(["ffmpeg", "-loglevel", "error", "-i", str(path), "-ac", "1", "-ar", str(SR),
                          "-f", "f32le", "-"], capture_output=True, check=True).stdout
    return np.frombuffer(raw, dtype=np.float32)


def split_points(x: np.ndarray) -> list[float]:
    """Cut times (s) so that no part is longer than 30 s, each at the quietest 0.3 s window."""
    dur = len(x) / SR
    if dur <= LIMIT_S:
        return []
    hop = int(0.02 * SR)
    frames = len(x) // hop
    rms = np.sqrt(np.mean(x[:frames * hop].reshape(frames, hop) ** 2, axis=1) + 1e-12)
    win = 15  # 15 x 20 ms = 0.3 s
    smooth = np.convolve(rms, np.ones(win) / win, mode="same")
    cuts, start = [], 0.0
    while dur - start > LIMIT_S - 2 * OVERLAP_S:
        lo, hi = int((start + 18) / 0.02), int(min(start + 29, dur - 1) / 0.02)
        i = lo + int(np.argmin(smooth[lo:hi]))
        cut = i * 0.02
        cuts.append(round(cut, 2))
        start = cut
    return cuts


def wav_b64(x: np.ndarray) -> str:
    buf = io.BytesIO()
    with wave.open(buf, "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(SR)
        w.writeframes((np.clip(x, -1, 1) * 32767).astype("<i2").tobytes())
    return base64.b64encode(buf.getvalue()).decode()


def post(url: str, body: dict, timeout: float = 600) -> dict:
    req = urllib.request.Request(url, json.dumps(body).encode(), {"Content-Type": "application/json"})
    with urllib.request.urlopen(req, timeout=timeout) as r:
        return json.loads(r.read())


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--model", type=pathlib.Path, required=True)
    ap.add_argument("--mmproj", type=pathlib.Path, required=True)
    ap.add_argument("--name", required=True)
    ap.add_argument("--mode", choices=MODES, required=True)
    ap.add_argument("--limit", type=int, default=0)
    ap.add_argument("--clips", default="")
    ap.add_argument("--out", type=pathlib.Path)
    a = ap.parse_args()

    system, user = MODES[a.mode]
    manifest = list(csv.DictReader((CLIPS / "manifest.csv").open(newline="", encoding="utf-8")))
    if a.clips:
        want = set(a.clips.split(","))
        manifest = [m for m in manifest if m["clip_id"] in want]
    if a.limit:
        manifest = manifest[:a.limit]
    out = a.out or HERE / "runs" / f"{a.name}__{a.mode}"
    out.mkdir(parents=True, exist_ok=True)

    port = free_port()
    log = (out / "server.log").open("w")
    cmd = ["/usr/bin/time", "-l", "llama-server", "-m", str(a.model), "--mmproj", str(a.mmproj),
           "--host", "127.0.0.1", "--port", str(port), "-ngl", "99", "-c", "8192", "-np", "1", "--no-webui",
           "--jinja", "--reasoning", "off", "--temp", "0", "--seed", "1", "--log-disable"]
    t0 = time.monotonic()
    proc = subprocess.Popen(cmd, stdout=log, stderr=subprocess.STDOUT)
    base = f"http://127.0.0.1:{port}"
    while True:
        if proc.poll() is not None:
            sys.exit(f"llama-server exited early, see {out / 'server.log'}")
        try:
            with urllib.request.urlopen(base + "/health", timeout=2) as r:
                if r.status == 200 and json.loads(r.read()).get("status") == "ok":
                    break
        except Exception:  # noqa: BLE001 - not up yet
            pass
        time.sleep(0.05)
    load_s = time.monotonic() - t0

    results = []
    try:
        for i, m in enumerate(manifest):
            x = load_audio(CLIPS / m["file"])
            cuts = split_points(x)
            bounds = [0.0] + cuts + [len(x) / SR]
            replies, dt, pt, ct = [], 0.0, 0, 0
            for k in range(len(bounds) - 1):
                s = max(0.0, bounds[k] - (OVERLAP_S if k else 0))
                e = min(len(x) / SR, bounds[k + 1] + (OVERLAP_S if k < len(bounds) - 2 else 0))
                seg = x[int(s * SR):int(e * SR)]
                content = [{"type": "text", "text": user},
                           {"type": "input_audio", "input_audio": {"data": wav_b64(seg), "format": "wav"}}]
                msgs = ([{"role": "system", "content": system}] if system else []) + \
                       [{"role": "user", "content": content}]
                body = {"messages": msgs, "temperature": 0, "top_k": 1, "seed": 1,
                        "max_tokens": 160 + int(8 * (e - s)), "chat_template_kwargs": {"enable_thinking": False},
                        "cache_prompt": True}
                t = time.monotonic()
                resp = post(base + "/v1/chat/completions", body)
                dt += time.monotonic() - t
                rep = (resp["choices"][0]["message"].get("content") or "").strip()
                rep = re.sub(r"<think>.*?</think>", "", rep, flags=re.S).strip()
                replies.append(rep)
                u = resp.get("usage", {})
                pt += u.get("prompt_tokens", 0)
                ct += u.get("completion_tokens", 0)
            text = " ".join(r for r in replies if r)
            results.append({"clip_id": m["clip_id"], "status": "ok" if text else "empty", "transcript": text,
                            "parts": len(replies), "split_at": ";".join(f"{c:.2f}" for c in cuts),
                            "part_replies": json.dumps(replies, ensure_ascii=False),
                            "audio_s": f"{len(x) / SR:.2f}", "finalize_s": f"{dt:.3f}",
                            "prompt_tokens": pt, "completion_tokens": ct})
            if (i + 1) % 10 == 0:
                print(f"  {a.name} {a.mode}: {i + 1}/{len(manifest)}", file=sys.stderr, flush=True)
    finally:
        child = subprocess.run(["pgrep", "-P", str(proc.pid)], capture_output=True, text=True).stdout.split()
        for pid in child:
            os.kill(int(pid), signal.SIGTERM)
        proc.wait(timeout=60)
        log.close()
        if results:
            with (out / "results.csv").open("w", newline="", encoding="utf-8") as fh:
                w = csv.DictWriter(fh, list(results[0]))
                w.writeheader()
                w.writerows(results)

    text = (out / "server.log").read_text(errors="replace")
    mm = re.search(r"(\d+)\s+peak memory footprint", text)
    peak = int(mm.group(1)) if mm else None
    mm = re.search(r"(\d+)\s+maximum resident set size", text)
    rss = int(mm.group(1)) if mm else None
    lat = [float(r["finalize_s"]) for r in results]
    short = [float(r["finalize_s"]) for r in results if int(r["parts"]) == 1]
    rtf = [float(r["finalize_s"]) / float(r["audio_s"]) for r in results]
    ver = subprocess.run(["llama-server", "--version"], capture_output=True, text=True)
    meta = {
        "name": a.name, "mode": a.mode, "n": len(results),
        "model_file": a.model.name, "model_bytes": a.model.stat().st_size, "model_sha256": sha256(a.model),
        "mmproj_file": a.mmproj.name, "mmproj_bytes": a.mmproj.stat().st_size, "mmproj_sha256": sha256(a.mmproj),
        "llama_cpp": (ver.stdout + ver.stderr).strip().splitlines()[0] if (ver.stdout + ver.stderr) else "",
        "load_s": round(load_s, 2), "peak_memory_bytes": peak, "max_rss_bytes": rss,
        "latency_s": {"median": statistics.median(lat), "p90": sorted(lat)[int(0.9 * (len(lat) - 1))],
                      "mean": statistics.mean(lat), "max": max(lat)},
        "latency_single_part_s": {"median": statistics.median(short), "p90": sorted(short)[int(0.9 * (len(short) - 1))]}
        if short else {},
        "real_time_factor_median": statistics.median(rtf),
        "split_clips": sum(1 for r in results if int(r["parts"]) > 1),
        "empty": sum(1 for r in results if r["status"] != "ok"),
        "settings": "temperature 0, top_k 1, seed 1, thinking off, ctx 8192, one slot, all layers and the "
                    "projector on Metal, --jinja (the model's own chat template)",
        "note": "this Mac was shared with another eval running llama.cpp at the same time; timings are rough",
    }
    (out / "meta.json").write_text(json.dumps(meta, indent=1))
    print(json.dumps({k: meta[k] for k in ("name", "mode", "load_s", "peak_memory_bytes", "max_rss_bytes",
                                           "latency_s", "split_clips", "empty")}))


if __name__ == "__main__":
    main()
