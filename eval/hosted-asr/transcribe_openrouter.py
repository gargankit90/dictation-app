# /// script
# requires-python = ">=3.11"
# dependencies = ["numpy", "soundfile>=0.12", "soxr>=0.3"]
# ///
"""Transcribe the Voice Arena public set (151 clips) with hosted speech-to-text models on OpenRouter.

Each clip is sent the way Vocet uploads a recording: a 16 kHz mono 16-bit PCM WAV
(dictation_core::wav), language "en", no prompt, temperature 0. By default the clip is padded with
the silence Vocet's own recording had in the Voice Arena harness run (0.8 s before: the 0.5 s start
delay plus the harness's 300 ms pad; 4.3 s after: the 300 ms pad plus the 4 s tail), so a model's
behaviour on trailing silence (Whisper's invented "Thank you.") is tested the same way Groq was.
--lead-s 0 --tail-s 0 sends the bare clip.

Request: POST {base}/audio/transcriptions with a JSON body (input_audio base64, format wav),
OpenRouter's documented STT route (docs/guides/overview/multimodal/stt). OpenRouter does not apply
provider routing (order/only) to transcription requests, so it picks the provider; after each clip
the script looks up the generation's provider name (GET {base}/generation?id=...), keeping only
whitelisted metadata fields.

Privacy and secrets:
  - The key is read only from the OPENROUTER_API_KEY environment variable. It goes only into the
    Authorization header; it is never printed, logged or written.
  - Response bodies are never printed. On an error only the HTTP status is recorded. From a
    successful reply only the transcript text and the usage numbers are kept.
  - Audio is read from the git-ignored clips/ folder and only sent, never written.

Output, per model, in <out>/<label>/:
  transcripts.jsonl     the cache: one line per attempt outcome; a rerun skips clips already "ok"
  harness_results.csv   one row per clip in the columns score_claude_cli.py / score_codex_cli.py read
                        (clip_id, status, transcript, finalize_s = API seconds), plus provider, cost

    OPENROUTER_API_KEY=... uv run transcribe_openrouter.py --model nvidia/nemotron-3.5-transcribe
"""
import argparse
import base64
import concurrent.futures as cf
import csv
import io
import json
import os
import pathlib
import random
import sys
import threading
import time
import urllib.error
import urllib.parse
import urllib.request

import numpy as np
import soundfile as sf
import soxr

RATE = 16_000
DEFAULT_BASE = "https://openrouter.ai/api/v1"
HERE = pathlib.Path(__file__).resolve().parent
DEFAULT_CLIPS = HERE.parent / "voicearena" / "clips"
DEFAULT_OUT = HERE / "results"
# Fields kept from GET /generation (metadata only; never the request or response content).
GEN_FIELDS = ("provider_name", "model", "latency", "generation_time", "total_cost", "upstream_id")
# The highest per-second price among the endpoints serving each model (OpenRouter endpoints API,
# 2026-11-19), used to estimate a request's cost before it is sent and when a reply has no
# usage.cost. Unknown models count at FALLBACK_PRICE.
MAX_PRICE_PER_S = {
    "nvidia/nemotron-3.5-transcribe": 0.0000333,      # DeepInfra; Prime Intellect 0.0000166667
    "qwen/qwen3-asr-1.7b": 0.0000075,                 # Prime Intellect, DeepInfra
    "qwen/qwen3-asr-0.6b": 0.00000333,                # Prime Intellect, DeepInfra
    "nvidia/parakeet-tdt-0.6b-v3": 0.000025,          # Together; Prime Intellect 0.0000166667
    "moondream/parakeet-ultra": 0.0000166667,         # Prime Intellect
    "mistralai/voxtral-small-24b-2507-stt": 0.00005,  # DeepInfra
    "openai/whisper-large-v3-turbo": 0.0000111111,    # Groq, Prime Intellect; DeepInfra 0.00000333
    "openai/whisper-large-v3": 0.0000308333,          # Groq; Together 0.000025, DeepInfra 0.0000075
    "mistralai/voxtral-mini-3b-2507": 0.0000166667,   # DeepInfra
}
FALLBACK_PRICE = 0.0001
_print_lock = threading.Lock()
# soxr segfaults when two threads resample at once (seen with soxr 0.5 on macOS arm64), so audio is
# prepared one clip at a time.
_audio_lock = threading.Lock()


class Budget:
    """Total spend across all models, cached runs included; no request starts past the cap."""

    def __init__(self, cap: float):
        self.cap, self.spent, self.lock = cap, 0.0, threading.Lock()

    def reserve(self, estimate: float) -> bool:
        """Hold the estimate before a request, so parallel models can't overshoot together."""
        with self.lock:
            if self.spent + estimate > self.cap:
                return False
            self.spent += estimate
            return True

    def settle(self, estimate: float, cost: float) -> None:
        with self.lock:
            self.spent += cost - estimate

    def add(self, cost: float) -> None:
        with self.lock:
            self.spent += cost


def cost_of(r: dict, model: str) -> float:
    c = r.get("cost")
    if isinstance(c, (int, float)) and c >= 0:
        return float(c)
    return estimate(model, r.get("sent_s") or 0.0) if r.get("status") == "ok" else 0.0


def estimate(model: str, seconds: float) -> float:
    return MAX_PRICE_PER_S.get(model, FALLBACK_PRICE) * seconds


class Fatal(Exception):
    """Stop the whole run: the key is rejected or the account can't pay."""


def say(msg: str) -> None:
    with _print_lock:
        print(msg, flush=True)


def label_for(model: str) -> str:
    return model.replace("/", "__").replace(":", "_")


def wav_bytes(path: pathlib.Path, lead_s: float, tail_s: float) -> tuple[bytes, float]:
    """The clip as Vocet uploads audio: mono, 16 kHz, 16-bit PCM WAV with a 44-byte header."""
    with _audio_lock:
        audio, rate = sf.read(str(path), dtype="float32", always_2d=True)
        mono = audio.mean(axis=1)
        if rate != RATE:
            mono = soxr.resample(mono, rate, RATE, quality="VHQ")
    mono = np.concatenate([np.zeros(int(RATE * lead_s), "float32"), mono.astype("float32"),
                           np.zeros(int(RATE * tail_s), "float32")])
    buf = io.BytesIO()
    sf.write(buf, np.clip(mono, -1.0, 1.0), RATE, subtype="PCM_16", format="WAV")
    return buf.getvalue(), len(mono) / RATE


def post_json(url: str, key: str, body: dict, timeout: float) -> tuple[int, dict | None, dict]:
    data = json.dumps(body).encode()
    req = urllib.request.Request(url, data=data, method="POST", headers={
        "Authorization": f"Bearer {key}", "Content-Type": "application/json",
        "X-Title": "dictation-app eval"})
    try:
        with urllib.request.urlopen(req, timeout=timeout) as r:
            raw = r.read()
            headers = {k.lower(): v for k, v in r.headers.items()}
            try:
                return r.status, json.loads(raw), headers
            except ValueError:
                return r.status, None, headers
    except urllib.error.HTTPError as e:
        headers = {k.lower(): v for k, v in (e.headers or {}).items()}
        e.close()  # the body is dropped unread: it is never logged
        return e.code, None, headers


def get_json(url: str, key: str, timeout: float) -> tuple[int, dict | None]:
    req = urllib.request.Request(url, headers={"Authorization": f"Bearer {key}"})
    try:
        with urllib.request.urlopen(req, timeout=timeout) as r:
            return r.status, json.loads(r.read())
    except urllib.error.HTTPError as e:
        e.close()
        return e.code, None
    except (urllib.error.URLError, TimeoutError, ValueError, OSError):
        return 0, None


def retry_after(headers: dict) -> float | None:
    v = headers.get("retry-after")
    try:
        return min(float(v), 60.0) if v is not None else None
    except ValueError:
        return None


def transcribe(base: str, key: str, model: str, wav: bytes, args) -> dict:
    """One clip, with retries on 429 / 5xx / network errors. Returns an outcome record."""
    body = {"model": model, "input_audio": {"data": base64.b64encode(wav).decode(), "format": "wav"},
            "language": "en", "temperature": 0}
    url = base + "/audio/transcriptions"
    last = "?"
    for attempt in range(1, args.attempts + 1):
        t0 = time.monotonic()
        try:
            status, data, headers = post_json(url, key, body, args.timeout)
        except (urllib.error.URLError, TimeoutError, OSError) as e:
            status, data, headers = 0, None, {}
            last = f"network error ({type(e).__name__})"
        api_s = time.monotonic() - t0
        if status == 200 and isinstance(data, dict) and isinstance(data.get("text"), str):
            usage = data.get("usage") if isinstance(data.get("usage"), dict) else {}
            return {"status": "ok", "text": data["text"].strip(), "api_s": round(api_s, 3),
                    "attempts": attempt, "generation_id": headers.get("x-generation-id", ""),
                    "usage_seconds": usage.get("seconds"), "cost": usage.get("cost")}
        if status in (401, 402, 403):
            raise Fatal(f"HTTP {status} from OpenRouter (key rejected or no credit); stopping")
        if status == 200:
            last = "HTTP 200 without a text field"
        elif status:
            last = f"HTTP {status}"
        retryable = status == 0 or status == 429 or status >= 500 or status == 200
        if not retryable or attempt == args.attempts:
            return {"status": "error", "error": last, "attempts": attempt, "api_s": round(api_s, 3)}
        wait = retry_after(headers) if status == 429 else None
        if wait is None:
            wait = min(args.backoff * 2 ** (attempt - 1), 60.0) * (0.75 + random.random() / 2)
        say(f"  {model}: {last}, retry {attempt}/{args.attempts - 1} in {wait:.1f}s")
        time.sleep(wait)
    return {"status": "error", "error": last, "attempts": args.attempts}


def provider_of(base: str, key: str, gen_id: str, args) -> dict:
    """The provider OpenRouter routed the request to (generation stats can lag a few seconds)."""
    if not gen_id:
        return {}
    url = base + "/generation?" + urllib.parse.urlencode({"id": gen_id})
    for delay in (0.5, 1.5, 3.0, 6.0):
        time.sleep(delay * args.gen_delay_scale)
        status, data = get_json(url, key, args.timeout)
        d = data.get("data") if isinstance(data, dict) else None
        if status == 200 and isinstance(d, dict):
            return {k: d.get(k) for k in GEN_FIELDS if d.get(k) is not None}
    return {}


def wav_seconds(path: pathlib.Path, lead_s: float, tail_s: float) -> float:
    return sf.info(str(path)).duration + lead_s + tail_s


def cached_records(cache: pathlib.Path) -> list[dict]:
    out = []
    if cache.exists():
        for line in cache.read_text().splitlines():
            try:
                out.append(json.loads(line))
            except ValueError:
                pass
    return out


def load_done(cache: pathlib.Path) -> dict:
    done = {}
    if cache.exists():
        for line in cache.read_text().splitlines():
            try:
                r = json.loads(line)
            except ValueError:
                continue
            if r.get("status") == "ok":
                done[r["clip_id"]] = r
            elif r["clip_id"] not in done:
                done.setdefault("_errors", {})[r["clip_id"]] = r
    return done


def write_results(out: pathlib.Path, model: str, clips: list[dict], done: dict, errors: dict) -> None:
    cols = ["tool", "clip_id", "file", "status", "attempts", "transcript", "task_type", "speaker_id",
            "audio_duration_s", "sent_s", "finalize_s", "error", "model", "provider", "provider_latency_s",
            "cost_usd", "run_at"]
    with open(out / "harness_results.csv", "w", newline="") as f:
        w = csv.DictWriter(f, cols)
        w.writeheader()
        for c in clips:
            r = done.get(c["clip_id"]) or errors.get(c["clip_id"]) or {"status": "missing"}
            gen = r.get("generation") or {}
            lat = gen.get("latency")
            w.writerow({"tool": "openrouter", "clip_id": c["clip_id"], "file": c["file"],
                        "status": r["status"], "attempts": r.get("attempts", ""),
                        "transcript": r.get("text", ""), "task_type": c.get("task_type", ""),
                        "speaker_id": c.get("speaker_id", ""), "audio_duration_s": c.get("audio_duration_s", ""),
                        "sent_s": r.get("sent_s", ""), "finalize_s": r.get("api_s", ""),
                        "error": r.get("error", ""), "model": model, "provider": gen.get("provider_name", ""),
                        "provider_latency_s": f"{lat / 1000:.3f}" if isinstance(lat, (int, float)) else "",
                        "cost_usd": r.get("cost") if r.get("cost") is not None else gen.get("total_cost", ""),
                        "run_at": r.get("run_at", "")})


def run_model(model: str, clips: list[dict], args, key: str, stop: threading.Event,
              budget: Budget) -> tuple[str, int, int]:
    out = args.out / label_for(model)
    out.mkdir(parents=True, exist_ok=True)
    cache = out / "transcripts.jsonl"
    done = load_done(cache)
    errors = done.pop("_errors", {})
    todo = [c for c in clips if c["clip_id"] not in done]
    say(f"{model}: {len(clips) - len(todo)} cached, {len(todo)} to do")
    for i, c in enumerate(todo, 1):
        if stop.is_set():
            break
        wav, sent_s = wav_bytes(args.clips / c["file"], args.lead_s, args.tail_s)
        held = estimate(model, sent_s)
        if not budget.reserve(held):
            say(f"{model}: stopping, the next clip would pass the ${args.max_usd:.2f} budget "
                f"(spent about ${budget.spent:.3f})")
            stop.set()
            break
        try:
            r = transcribe(args.base_url, key, model, wav, args)
        except Fatal:
            budget.settle(held, 0.0)
            stop.set()
            raise
        r.update({"clip_id": c["clip_id"], "model": model, "sent_s": round(sent_s, 3),
                  "lead_s": args.lead_s, "tail_s": args.tail_s,
                  "run_at": time.strftime("%Y-%m-%dT%H:%M:%S%z")})
        if r["status"] == "ok" and not args.no_provider_lookup:
            r["generation"] = provider_of(args.base_url, key, r.pop("generation_id", ""), args)
        else:
            r.pop("generation_id", None)
        with open(cache, "a") as f:
            f.write(json.dumps(r) + "\n")
        budget.settle(held, cost_of(r, model))
        if r["status"] == "ok":
            done[c["clip_id"]] = r
            errors.pop(c["clip_id"], None)
        else:
            errors[c["clip_id"]] = r
            say(f"  {model}: {c['clip_id']} failed: {r['error']}")
        if i % 25 == 0 or i == len(todo):
            say(f"{model}: {i}/{len(todo)}")
    write_results(out, model, clips, done, errors)
    return str(out / "harness_results.csv"), len(done), len(errors)


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--model", action="append", required=True, help="OpenRouter model slug (repeatable)")
    ap.add_argument("--clips", type=pathlib.Path, default=DEFAULT_CLIPS, help="folder with manifest.csv")
    ap.add_argument("--out", type=pathlib.Path, default=DEFAULT_OUT)
    ap.add_argument("--base-url", default=os.environ.get("OPENROUTER_BASE_URL", DEFAULT_BASE))
    ap.add_argument("--limit", type=int, help="only the first N clips (a trial)")
    ap.add_argument("--lead-s", type=float, default=0.8, help="silence before the clip (s)")
    ap.add_argument("--tail-s", type=float, default=4.3, help="silence after the clip (s)")
    ap.add_argument("--attempts", type=int, default=6)
    ap.add_argument("--backoff", type=float, default=2.0, help="first retry delay (s), doubling")
    ap.add_argument("--timeout", type=float, default=90.0, help="per request (s)")
    ap.add_argument("--max-usd", type=float, default=2.0,
                    help="stop before total spend (every run in --out, cached runs included) passes this")
    ap.add_argument("--no-provider-lookup", action="store_true")
    ap.add_argument("--gen-delay-scale", type=float, default=1.0, help=argparse.SUPPRESS)  # tests
    args = ap.parse_args()

    key = os.environ.get("OPENROUTER_API_KEY", "").strip()
    if not key:
        print("OPENROUTER_API_KEY is not set", file=sys.stderr)
        return 2
    manifest = args.clips / "manifest.csv"
    if not manifest.exists():
        print(f"{manifest} is missing: run eval/voicearena/bench.sh fetch first", file=sys.stderr)
        return 2
    clips = list(csv.DictReader(open(manifest)))
    clips.sort(key=lambda c: c["clip_id"])
    if args.limit:
        clips = clips[: args.limit]
    args.base_url = args.base_url.rstrip("/")

    stop = threading.Event()
    budget = Budget(args.max_usd)
    seconds = sum(wav_seconds(args.clips / c["file"], args.lead_s, args.tail_s) for c in clips)
    # Every model already run into --out counts against the budget (both rounds share one cap).
    if args.out.exists():
        for d in sorted(args.out.iterdir()):
            recs = cached_records(d / "transcripts.jsonl")
            budget.add(sum(cost_of(r, r.get("model", "")) for r in recs))
    for m in args.model:
        spent = sum(cost_of(r, m) for r in cached_records(args.out / label_for(m) / "transcripts.jsonl"))
        say(f"{m}: {seconds / 60:.1f} min of audio, at most ${estimate(m, seconds):.3f} "
            f"at the dearest provider; already spent ${spent:.3f}")
    say(f"budget ${args.max_usd:.2f} across every run in {args.out}, already spent ${budget.spent:.3f}")
    failed = False
    # One thread per model; clips within a model go one at a time, so API latency is what a single
    # dictation would see.
    with cf.ThreadPoolExecutor(max_workers=len(args.model)) as ex:
        futs = {ex.submit(run_model, m, clips, args, key, stop, budget): m for m in args.model}
        for fut in cf.as_completed(futs):
            m = futs[fut]
            try:
                path, ok, bad = fut.result()
            except Fatal as e:
                say(f"{m}: {e}")
                failed = True
                continue
            say(f"{m}: {ok} ok, {bad} failed -> {path}")
            failed |= ok < len(clips)
    say(f"spent about ${budget.spent:.3f} in total (usage.cost where the reply had it)")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
