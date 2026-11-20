"""Run one cleanup model over one engine's raw drafts with llama.cpp's llama-server, on this Mac.

    python3 clean.py --model ~/vocet-cleanup-models/Qwen3-4B-Q4_K_M.gguf --name qwen3-4b \
        --source parakeet --prompt v1 [--half tune|check|all] [--limit N]

Writes runs/<source>/<name>__<prompt>/:
  results.csv  per clip: clip_id, status, transcript (the text that would be pasted: the cleaned reply, or
               the raw draft when the guard falls back), raw, reply, fallback, cleanup_s, tokens.
               finalize_s = cleanup_s (the added time on this Mac), so report.py's latency is the cleanup's.
  meta.json    model file, size, sha256, llama.cpp version, load time, peak memory, latency, fallbacks.

Temperature 0, no thinking (--reasoning off and enable_thinking=false), one request at a time (one slot),
so latency is what one user would see. Load time is from process start until /health says ready (model in
memory, Metal set up, warm-up done); the file is usually in the OS page cache already, so a first launch
after a reboot is slower. Peak memory is /usr/bin/time -l's "peak memory footprint" of llama-server (allocations: KV cache,
Metal buffers, compute) and its "maximum resident set size" (also counts the memory-mapped weights).
"""
import argparse
import csv
import hashlib
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
sys.path.insert(0, str(HERE))
from guard import check  # noqa: E402
from prompts import PROMPTS  # noqa: E402

SOURCES = {
    "parakeet": HERE.parent / "voicearena/results/vocet_parakeet/harness_results.csv",
    "qwen3": HERE.parent / "voicearena/results/vocet_qwen3/harness_results.csv",
    "groq": HERE.parent / "voicearena/results/vocet_groq/harness_results.csv",
}


def free_port() -> int:
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


def post(url: str, body: dict, timeout: float = 300) -> dict:
    req = urllib.request.Request(url, json.dumps(body).encode(), {"Content-Type": "application/json"})
    with urllib.request.urlopen(req, timeout=timeout) as r:
        return json.loads(r.read())


def sha256(path: pathlib.Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--model", type=pathlib.Path, required=True)
    ap.add_argument("--name", required=True)
    ap.add_argument("--source", choices=SOURCES, default="parakeet")
    ap.add_argument("--prompt", default="v1", choices=PROMPTS)
    ap.add_argument("--half", choices=["all", "tune", "check"], default="all")
    ap.add_argument("--limit", type=int, default=0)
    ap.add_argument("--out", type=pathlib.Path)
    a = ap.parse_args()

    system, user_t = PROMPTS[a.prompt]
    split = {r["clip_id"]: r["half"] for r in csv.DictReader((HERE / "split.csv").open())}
    rows = [r for r in csv.DictReader(SOURCES[a.source].open(newline="", encoding="utf-8"))]
    rows = [r for r in rows if a.half == "all" or split[r["clip_id"]] == a.half]
    if a.limit:
        rows = rows[:a.limit]
    out = a.out or HERE / "runs" / a.source / f"{a.name}__{a.prompt}"
    out.mkdir(parents=True, exist_ok=True)

    port = free_port()
    log = (out / "server.log").open("w")
    cmd = ["/usr/bin/time", "-l", "llama-server", "-m", str(a.model), "--host", "127.0.0.1", "--port", str(port),
           "-ngl", "99", "-c", "4096", "-np", "1", "--no-webui", "--reasoning", "off", "--temp", "0",
           "--seed", "1", "--log-disable"]
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
        for i, r in enumerate(rows):
            raw = r["transcript"] if r["status"] == "ok" else ""
            reply, dt, usage = "", 0.0, {}
            if raw.strip():
                body = {"messages": [{"role": "system", "content": system},
                                     {"role": "user", "content": user_t.format(raw=raw)}],
                        "temperature": 0, "top_k": 1, "seed": 1, "max_tokens": len(raw) // 2 + 100,
                        "chat_template_kwargs": {"enable_thinking": False}, "cache_prompt": True}
                t = time.monotonic()
                resp = post(base + "/v1/chat/completions", body)
                dt = time.monotonic() - t
                reply = resp["choices"][0]["message"].get("content") or ""
                reply = re.sub(r"<think>.*?</think>", "", reply, flags=re.S)
                usage = resp.get("usage", {})
            final, reason = check(raw, reply, system) if raw.strip() else (raw, "empty_raw")
            results.append({"clip_id": r["clip_id"], "status": r["status"], "transcript": final,
                            "finalize_s": f"{dt:.3f}", "cleanup_s": f"{dt:.3f}", "raw": raw, "reply": reply,
                            "fallback": reason, "prompt_tokens": usage.get("prompt_tokens", ""),
                            "completion_tokens": usage.get("completion_tokens", "")})
            if (i + 1) % 25 == 0:
                print(f"  {a.name} {a.source} {a.prompt}: {i + 1}/{len(rows)}", file=sys.stderr)
    finally:
        child = subprocess.run(["pgrep", "-P", str(proc.pid)], capture_output=True, text=True).stdout.split()
        for pid in child:
            os.kill(int(pid), signal.SIGTERM)
        proc.wait(timeout=60)
        log.close()

    text = (out / "server.log").read_text(errors="replace")
    m = re.search(r"(\d+)\s+peak memory footprint", text)
    peak = int(m.group(1)) if m else None
    m = re.search(r"(\d+)\s+maximum resident set size", text)
    rss = int(m.group(1)) if m else None
    with (out / "results.csv").open("w", newline="", encoding="utf-8") as fh:
        w = csv.DictWriter(fh, list(results[0]))
        w.writeheader()
        w.writerows(results)
    lat = [float(x["cleanup_s"]) for x in results if x["raw"].strip()]
    fb = {}
    for x in results:
        if x["fallback"]:
            fb[x["fallback"]] = fb.get(x["fallback"], 0) + 1
    ver = subprocess.run(["llama-server", "--version"], capture_output=True, text=True)
    meta = {
        "name": a.name, "source": a.source, "prompt": a.prompt, "half": a.half, "n": len(results),
        "model_file": a.model.name, "model_bytes": a.model.stat().st_size, "model_sha256": sha256(a.model),
        "llama_cpp": (ver.stdout + ver.stderr).strip().splitlines()[0] if (ver.stdout + ver.stderr) else "",
        "load_s": round(load_s, 2), "peak_memory_bytes": peak, "max_rss_bytes": rss,
        "latency_s": {"median": statistics.median(lat), "p90": sorted(lat)[int(0.9 * (len(lat) - 1))],
                      "mean": statistics.mean(lat), "max": max(lat)},
        "fallbacks": fb, "fallback_total": sum(fb.values()),
        "unchanged": sum(1 for x in results if x["transcript"] == x["raw"] and not x["fallback"]),
        "settings": "temperature 0, top_k 1, seed 1, thinking off, ctx 4096, one slot, all layers on Metal",
    }
    (out / "meta.json").write_text(json.dumps(meta, indent=1))
    print(json.dumps({k: meta[k] for k in ("name", "source", "prompt", "load_s", "peak_memory_bytes", "max_rss_bytes",
                                           "latency_s", "fallbacks")}))


if __name__ == "__main__":
    main()
