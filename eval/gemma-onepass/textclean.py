"""Gemma 4 two calls, one model file: clean Gemma's own plain transcription with the local-cleanup v3 text prompt.

    python3 -I textclean.py --model ~/vocet-cleanup-models/gemma-4-E2B-it-Q4_K_M.gguf \
        --source runs/gemma4-e2b__plain --name gemma4-e2b

Writes runs/<name>__plain_then_v3/. Same request settings as eval/local-cleanup/clean.py (temperature 0,
top_k 1, seed 1, thinking off, max_tokens len(raw)//2 + 100, v3 system prompt, <transcript> user turn) and the
same guard (vendor/guard.py, fallback to Gemma's plain text). The text-only model is loaded without the
audio projector here, but in an app the same llama.cpp context would serve both calls.
"""
import argparse
import collections
import csv
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

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE / "vendor"))
from guard import check  # noqa: E402
from prompts import PROMPTS  # noqa: E402


def free_port() -> int:
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


def post(url: str, body: dict) -> dict:
    req = urllib.request.Request(url, json.dumps(body).encode(), {"Content-Type": "application/json"})
    with urllib.request.urlopen(req, timeout=300) as r:
        return json.loads(r.read())


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--model", type=pathlib.Path, required=True)
    ap.add_argument("--source", type=pathlib.Path, required=True)
    ap.add_argument("--name", required=True)
    a = ap.parse_args()
    system, user_t = PROMPTS["v3"]
    out = HERE / "runs" / f"{a.name}__plain_then_v3"
    out.mkdir(parents=True, exist_ok=True)
    src = list(csv.DictReader((a.source / "results.csv").open(newline="", encoding="utf-8")))
    port = free_port()
    log = (out / "server.log").open("w")
    proc = subprocess.Popen(["/usr/bin/time", "-l", "llama-server", "-m", str(a.model), "--host", "127.0.0.1",
                             "--port", str(port), "-ngl", "99", "-c", "4096", "-np", "1", "--no-webui",
                             "--reasoning", "off", "--temp", "0", "--seed", "1", "--log-disable"],
                            stdout=log, stderr=subprocess.STDOUT)
    base = f"http://127.0.0.1:{port}"
    while True:
        if proc.poll() is not None:
            sys.exit("llama-server exited early")
        try:
            with urllib.request.urlopen(base + "/health", timeout=2) as r:
                if json.loads(r.read()).get("status") == "ok":
                    break
        except Exception:  # noqa: BLE001
            pass
        time.sleep(0.05)
    rows, fb = [], collections.Counter()
    try:
        for r in src:
            raw = r["transcript"]
            t = time.monotonic()
            resp = post(base + "/v1/chat/completions", {
                "messages": [{"role": "system", "content": system}, {"role": "user", "content": user_t.format(raw=raw)}],
                "temperature": 0, "top_k": 1, "seed": 1, "max_tokens": len(raw) // 2 + 100,
                "chat_template_kwargs": {"enable_thinking": False}, "cache_prompt": True})
            dt = time.monotonic() - t
            reply = re.sub(r"<think>.*?</think>", "", resp["choices"][0]["message"].get("content") or "", flags=re.S)
            final, reason = check(raw, reply, system)
            if reason:
                fb[reason] += 1
            rows.append({"clip_id": r["clip_id"], "status": "ok" if final.strip() else "empty", "transcript": final,
                         "raw": raw, "reply": reply, "fallback": reason, "cleanup_s": f"{dt:.3f}",
                         "transcribe_s": r["finalize_s"], "finalize_s": f"{dt + float(r['finalize_s']):.3f}"})
    finally:
        for pid in subprocess.run(["pgrep", "-P", str(proc.pid)], capture_output=True, text=True).stdout.split():
            os.kill(int(pid), signal.SIGTERM)
        proc.wait(timeout=60)
        log.close()
    with (out / "results.csv").open("w", newline="", encoding="utf-8") as fh:
        w = csv.DictWriter(fh, list(rows[0]))
        w.writeheader()
        w.writerows(rows)
    lat = [float(x["finalize_s"]) for x in rows]
    cl = [float(x["cleanup_s"]) for x in rows]
    meta = {"name": a.name, "mode": "plain_then_v3", "source": a.source.name, "model_file": a.model.name,
            "model_bytes": a.model.stat().st_size, "fallbacks": dict(fb), "fallback_total": sum(fb.values()),
            "latency_s": {"median": statistics.median(lat), "p90": sorted(lat)[int(0.9 * (len(lat) - 1))]},
            "cleanup_latency_s": {"median": statistics.median(cl), "p90": sorted(cl)[int(0.9 * (len(cl) - 1))]},
            "note": "latency = Gemma plain transcription + Gemma text cleanup; this Mac was shared, timings rough"}
    (out / "meta.json").write_text(json.dumps(meta, indent=1))
    print(json.dumps(meta))


if __name__ == "__main__":
    main()
