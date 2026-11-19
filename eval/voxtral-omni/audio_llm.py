# /// script
# requires-python = ">=3.11"
# dependencies = ["numpy", "soundfile>=0.12", "soxr>=0.3"]
# ///
"""Part B: audio-input chat models on OpenRouter, two prompts per clip on the Voice Arena public set.

    OPENROUTER_API_KEY=... uv run eval/voxtral-omni/audio_llm.py [--model KEY ...] [--limit N]

For every clip and every model, two chat requests with the clip as an `input_audio` part:
  plain     prompts/audio_plain.txt: transcribe verbatim
  onepass   prompts/audio_onepass_v4.txt: transcribe and clean up in one pass (prompt v4's rules,
            adapted for audio: the ASR-error section becomes "never write words nobody said")
Then the app's guard (guard.py) judges each one-pass reply against the same model's plain transcript
of the same clip, as the app judges a cleanup against the raw text: a refused reply pastes the plain
transcript. Results are written with and without the guard.

Audio is sent the way Vocet uploads it and the way eval/hosted-asr sent it (wav_bytes() is copied
from eval/hosted-asr/transcribe_openrouter.py, branch eval/hosted-asr, PR #330): 16 kHz mono 16-bit
PCM WAV, 0.8 s of silence before and 4.3 s after. Requests: temperature 0, usage {"include": true}
so each reply carries its cost, and reasoning switched off where the model has a switch (if the
route rejects that with HTTP 400, the model falls back to {"effort": "low", "exclude": true} once,
for the rest of the run, and the fallback is recorded on every row).

Secrets and privacy: the key is read only from OPENROUTER_API_KEY; it goes only into the
Authorization header and is never printed, logged or written. Response bodies are never printed;
an error records only the HTTP status. From a reply only the text, finish reason, provider name and
usage numbers are kept. Audio is read from the git-ignored eval/voicearena/clips/ and never written.

Budget: every request reserves a worst-case price in the shared ledger (budget.py, results/
budget.jsonl, $2.00 for parts A and B together) before it is sent and settles to usage.cost after.
Each model also has a sub-cap; after 3 paid clips a model stops when its projected spend would pass
it. HTTP 429 is retried with Retry-After or exponential backoff; a model stops after 3 clips in a row
that still got 429, and on 404 (no route, e.g. the account's data policy excludes a free endpoint).
401/402/403 stop everything.

Cache: results/<label>/calls.jsonl, one line per outcome, keyed by model, mode, prompt, reasoning
setting and clip; a rerun only asks what has no ok answer yet.
Output per model:
  results/<label>_plain/harness_results.csv           the verbatim transcripts
  results/<label>_onepass/harness_results.csv         one-pass replies, unguarded
  results/<label>_onepass_guard/harness_results.csv   one-pass replies through the guard
in the columns score_claude_cli.py / score_codex_cli.py read (finalize_s = API seconds).
"""
from __future__ import annotations

import argparse
import base64
import concurrent.futures as cf
import csv
import hashlib
import io
import json
import os
import pathlib
import random
import sys
import threading
import time
import urllib.error
import urllib.request

import numpy as np
import soundfile as sf
import soxr

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import budget  # noqa: E402
import guard  # noqa: E402

RATE = 16_000
DEFAULT_BASE = "https://openrouter.ai/api/v1"
CLIPS = HERE.parent / "voicearena" / "clips"
RESULTS = HERE / "results"
CAP_USD = 2.00
MODES = {"plain": HERE / "prompts" / "audio_plain.txt", "onepass": HERE / "prompts" / "audio_onepass_v4.txt"}
OFF = {"enabled": False}
FALLBACK = {"effort": "low", "exclude": True}

# Open-weight models with a commercial-use licence whose OpenRouter route takes audio input
# (catalog and endpoints API, 2026-11-19; REPORT.md lists the others and why they were left out).
# usd_per_s: worst-case audio price per second of sent audio, taking the larger of "per second" and
# "per audio token at <= 100 tokens/s" readings of the listed price. in/out: USD per text token.
# cap: this model's sub-cap for both modes together.
MODELS = {
    "voxtral_small_chat": {"model": "mistralai/voxtral-small-24b-2507", "reasoning": None,
                           "usd_per_s": 0.0001 * 12.5, "in": 0.11e-6, "out": 0.33e-6, "cap": 0.90,
                           "licence": "Apache-2.0"},
    "nemotron_omni": {"model": "nvidia/nemotron-3-nano-omni-30b-a3b-reasoning:free", "reasoning": OFF,
                      "usd_per_s": 0.0, "in": 0.0, "out": 0.0, "cap": 0.02, "min_interval_s": 3.2,
                      "licence": "NVIDIA Open Model License"},
    "mimo_v25": {"model": "xiaomi/mimo-v2.5", "reasoning": OFF,
                 "usd_per_s": 100 * 0.4e-6, "in": 0.4e-6, "out": 2e-6, "cap": 0.25, "licence": "MIT"},
    "inkling_small": {"model": "thinkingmachines/inkling-small", "reasoning": OFF,
                      "usd_per_s": 100 * 0.45e-6, "in": 0.45e-6, "out": 1.2e-6, "cap": 0.20,
                      "licence": "Apache-2.0"},
}
MAX_TOKENS = {False: 1024, True: 4096}  # without / with (fallback) reasoning
_print_lock = threading.Lock()
_audio_lock = threading.Lock()  # soxr is not thread-safe on macOS arm64 (see transcribe_openrouter.py)


class Fatal(Exception):
    """Stop the whole run: the key is rejected or the account can't pay."""


class StopModel(Exception):
    """Stop this model only."""


def say(msg: str) -> None:
    with _print_lock:
        print(msg, flush=True)


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


def request_body(model: str, prompt: str, wav: bytes, reasoning: dict | None) -> dict:
    body = {"model": model, "temperature": 0, "max_tokens": MAX_TOKENS[reasoning is FALLBACK],
            "messages": [{"role": "system", "content": prompt},
                         {"role": "user", "content": [
                             {"type": "input_audio",
                              "input_audio": {"data": base64.b64encode(wav).decode(), "format": "wav"}}]}],
            "usage": {"include": True}}
    if reasoning is not None:
        body["reasoning"] = reasoning
    return body


def worst_case(cfg: dict, prompt: str, sent_s: float, reasoning: dict | None) -> float:
    return (sent_s * cfg["usd_per_s"] + (256 + len(prompt.encode())) * cfg["in"]
            + MAX_TOKENS[reasoning is FALLBACK] * cfg["out"])


def post(url: str, key: str, body: dict, timeout: float) -> tuple[int, dict | None, dict]:
    req = urllib.request.Request(url, data=json.dumps(body).encode(), method="POST", headers={
        "Authorization": f"Bearer {key}", "Content-Type": "application/json", "X-Title": "dictation-app eval"})
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


def parse(data) -> tuple[str | None, str, dict, str]:
    """(text or None, finish_reason, usage subset, provider) from a chat completion."""
    try:
        choice = data["choices"][0]
        content = choice["message"].get("content")
        finish = str(choice.get("finish_reason") or "")
    except (KeyError, IndexError, TypeError, AttributeError):
        return None, "", {}, ""
    if isinstance(content, list):  # content parts
        content = "".join(p.get("text", "") for p in content if isinstance(p, dict))
    if content is not None and not isinstance(content, str):
        content = None
    usage = data.get("usage") if isinstance(data.get("usage"), dict) else {}
    pdet = usage.get("prompt_tokens_details") or {}
    cdet = usage.get("completion_tokens_details") or {}
    kept = {"prompt_tokens": usage.get("prompt_tokens"), "completion_tokens": usage.get("completion_tokens"),
            "audio_tokens": pdet.get("audio_tokens"), "reasoning_tokens": cdet.get("reasoning_tokens"),
            "cost": usage.get("cost")}
    provider = data.get("provider") if isinstance(data.get("provider"), str) else ""
    return (guard.strip_tags(content) if content is not None else None), finish, kept, provider


class ModelRun:
    def __init__(self, key_name: str, cfg: dict, args, api_key: str, ledger: budget.Ledger,
                 stop_all: threading.Event):
        self.name, self.cfg, self.args, self.key, self.ledger, self.stop_all = \
            key_name, cfg, args, api_key, ledger, stop_all
        self.dir = args.out / key_name
        self.dir.mkdir(parents=True, exist_ok=True)
        self.cache_path = self.dir / "calls.jsonl"
        self.prompts = {m: p.read_text(encoding="utf-8").strip() for m, p in MODES.items()}
        self.reasoning = cfg["reasoning"]
        self.records = [json.loads(l) for l in self.cache_path.read_text(encoding="utf-8").splitlines()
                        if l.strip()] if self.cache_path.exists() else []
        if any(r.get("reasoning") == FALLBACK for r in self.records):
            self.reasoning = FALLBACK
        self.spent = sum(float((r.get("usage") or {}).get("cost") or 0) for r in self.records)
        self.paid = sum(1 for r in self.records if r.get("status") == "ok")
        self.last_start = 0.0
        self.rate_limited_in_a_row = 0

    def ckey(self, clip_id: str, mode: str, reasoning) -> str:
        return hashlib.sha256(json.dumps([self.cfg["model"], mode, self.prompts[mode], reasoning, clip_id,
                                          self.args.lead_s, self.args.tail_s]).encode()).hexdigest()

    def done(self, clip_id: str, mode: str) -> dict | None:
        keys = {self.ckey(clip_id, mode, r) for r in (self.cfg["reasoning"], FALLBACK)}
        for r in reversed(self.records):
            if r["key"] in keys and r["status"] == "ok":
                return r
        return None

    def last(self, clip_id: str, mode: str) -> dict | None:
        for r in reversed(self.records):
            if r["clip_id"] == clip_id and r["mode"] == mode:
                return r
        return None

    def save(self, rec: dict) -> None:
        with self.cache_path.open("a", encoding="utf-8") as fh:
            fh.write(json.dumps(rec, ensure_ascii=False) + "\n")
        self.records.append(rec)

    def check_projection(self, remaining: int) -> None:
        if self.paid >= 3 and self.spent + self.spent / self.paid * remaining > self.cfg["cap"]:
            raise StopModel(f"projected ${self.spent + self.spent / self.paid * remaining:.3f} passes the "
                            f"${self.cfg['cap']:.2f} sub-cap after {self.paid} calls (${self.spent:.4f})")

    def call(self, clip: dict, mode: str, wav: bytes, sent_s: float) -> dict:
        prompt = self.prompts[mode]
        url = self.args.base_url + "/chat/completions"
        fell_back = False
        last = "?"
        for attempt in range(1, self.args.attempts + 1):
            if self.stop_all.is_set():
                raise StopModel("stopped")
            reasoning = self.reasoning
            est = worst_case(self.cfg, prompt, sent_s, reasoning)
            if self.spent + est > self.cfg["cap"]:
                raise StopModel(f"next call would pass the ${self.cfg['cap']:.2f} sub-cap (${self.spent:.4f})")
            try:
                held = self.ledger.reserve(est, self.name)
            except budget.OverBudget as e:
                raise StopModel(str(e)) from None
            wait = self.cfg.get("min_interval_s", 0) * self.args.interval_scale - (time.monotonic() - self.last_start)
            if wait > 0:
                time.sleep(wait)
            self.last_start = t0 = time.monotonic()
            try:
                status, data, headers = post(url, self.key, request_body(self.cfg["model"], prompt, wav, reasoning),
                                             self.args.timeout)
            except (urllib.error.URLError, TimeoutError, OSError) as e:
                status, data, headers = 0, None, {}
                last = f"network error ({type(e).__name__})"
            api_s = time.monotonic() - t0
            if status == 200:
                text, finish, usage, provider = parse(data)
                self.ledger.settle(held, usage.get("cost"), self.name)
                cost = usage.get("cost")
                self.spent += float(cost) if cost is not None else held / 1e6
                self.rate_limited_in_a_row = 0
                if text is not None and (text or finish in ("stop", "")):
                    self.paid += 1
                    return {"status": "ok", "text": text, "finish_reason": finish, "api_s": round(api_s, 3),
                            "attempts": attempt, "usage": usage, "provider": provider, "reasoning": reasoning}
                # Billed but unusable (e.g. the token budget went to reasoning): not retried.
                return {"status": "error", "error": f"no text (finish {finish or '?'})", "api_s": round(api_s, 3),
                        "attempts": attempt, "usage": usage, "provider": provider, "reasoning": reasoning}
            # Not a 200: an HTTP error isn't billed; a dropped connection might be, so it keeps its hold.
            if status:
                self.ledger.settle(held, 0.0, self.name)
                last = f"HTTP {status}"
            if status in (401, 402, 403):
                self.stop_all.set()
                raise Fatal(f"HTTP {status} from OpenRouter (key rejected or no credit); stopping everything")
            if status == 404:
                raise StopModel("HTTP 404: no endpoint for this model and account (data policy or route gone)")
            if status == 400 and reasoning is not None and reasoning is not FALLBACK and not fell_back:
                say(f"  {self.name}: HTTP 400 with reasoning off; falling back to low, excluded reasoning")
                self.reasoning, fell_back = FALLBACK, True
                continue
            retryable = status in (0, 408, 429) or status >= 500
            if not retryable or attempt == self.args.attempts:
                if status == 429:
                    self.rate_limited_in_a_row += 1
                    if self.rate_limited_in_a_row >= 3:
                        raise StopModel("HTTP 429 on 3 clips in a row after retries (rate or daily limit)")
                return {"status": "error", "error": last, "attempts": attempt, "api_s": round(api_s, 3),
                        "reasoning": reasoning}
            ra = headers.get("retry-after")
            try:
                delay = min(float(ra), 60.0) if ra is not None else None
            except ValueError:
                delay = None
            if delay is None:
                delay = min(self.args.backoff * 2 ** (attempt - 1), 60.0) * (0.75 + random.random() / 2)
            say(f"  {self.name}: {last}, retry {attempt}/{self.args.attempts - 1} in {delay:.1f}s")
            time.sleep(delay)
        return {"status": "error", "error": last, "attempts": self.args.attempts}

    def run(self, clips: list[dict]) -> str:
        todo = [(c, m) for c in clips for m in MODES if not self.done(c["clip_id"], m)]
        say(f"{self.name} ({self.cfg['model']}): {len(clips) * len(MODES) - len(todo)} cached, {len(todo)} to do, "
            f"spent so far ${self.spent:.4f}")
        outcome = "done"
        wavs: dict[str, tuple[bytes, float]] = {}
        try:
            for i, (c, mode) in enumerate(todo, 1):
                self.check_projection(len(todo) - i + 1)
                if c["clip_id"] not in wavs:
                    wavs.clear()
                    wavs[c["clip_id"]] = wav_bytes(self.args.clips / c["file"], self.args.lead_s, self.args.tail_s)
                wav, sent_s = wavs[c["clip_id"]]
                r = self.call(c, mode, wav, sent_s)
                r.update({"key": self.ckey(c["clip_id"], mode, r.get("reasoning")), "clip_id": c["clip_id"],
                          "mode": mode, "model": self.cfg["model"], "sent_s": round(sent_s, 3),
                          "run_at": time.strftime("%Y-%m-%dT%H:%M:%S%z")})
                self.save(r)
                if r["status"] != "ok":
                    say(f"  {self.name}: {c['clip_id']} {mode} failed: {r['error']}")
                if i % 20 == 0 or i == len(todo):
                    say(f"{self.name}: {i}/{len(todo)}, ${self.spent:.4f}")
        except StopModel as e:
            outcome = f"stopped: {e}"
            say(f"{self.name}: {outcome}")
        self.write(clips)
        return outcome

    def write(self, clips: list[dict]) -> None:
        cols = ["tool", "clip_id", "file", "status", "attempts", "transcript", "task_type", "speaker_id",
                "audio_duration_s", "sent_s", "finalize_s", "error", "model", "provider", "cost_usd",
                "finish_reason", "reasoning", "run_at"]
        got = {m: {c["clip_id"]: (self.done(c["clip_id"], m) or self.last(c["clip_id"], m)) for c in clips}
               for m in MODES}

        def row(c, r):
            r = r or {"status": "missing"}
            return {"tool": "openrouter-chat", "clip_id": c["clip_id"], "file": c["file"], "status": r["status"],
                    "attempts": r.get("attempts", ""), "transcript": r.get("text", ""),
                    "task_type": c.get("task_type", ""), "speaker_id": c.get("speaker_id", ""),
                    "audio_duration_s": c.get("audio_duration_s", ""), "sent_s": r.get("sent_s", ""),
                    "finalize_s": r.get("api_s", ""), "error": r.get("error", ""), "model": self.cfg["model"],
                    "provider": r.get("provider", ""), "cost_usd": (r.get("usage") or {}).get("cost", ""),
                    "finish_reason": r.get("finish_reason", ""),
                    "reasoning": json.dumps(r.get("reasoning")) if "reasoning" in r else "",
                    "run_at": r.get("run_at", "")}

        for mode in MODES:
            d = self.args.out / f"{self.name}_{mode}"
            d.mkdir(parents=True, exist_ok=True)
            with open(d / "harness_results.csv", "w", newline="", encoding="utf-8") as f:
                w = csv.DictWriter(f, cols)
                w.writeheader()
                for c in clips:
                    w.writerow(row(c, got[mode][c["clip_id"]]))
        # The guard: the one-pass reply replaces the model's own plain transcript only if the app's
        # guard accepts it. finalize_s is the slower of the two calls (they can run in parallel) and
        # cost_usd their sum, since a guarded one-pass needs both.
        d = self.args.out / f"{self.name}_onepass_guard"
        d.mkdir(parents=True, exist_ok=True)
        with open(d / "harness_results.csv", "w", newline="", encoding="utf-8") as f:
            w = csv.DictWriter(f, cols + ["raw_text", "cleaned_text", "reason"])
            w.writeheader()
            for c in clips:
                p, o = got["plain"][c["clip_id"]], got["onepass"][c["clip_id"]]
                p_ok, o_ok = bool(p and p["status"] == "ok"), bool(o and o["status"] == "ok")
                out = row(c, o if o_ok else p)
                if p_ok and o_ok:
                    reason = guard.judge(p["text"], o["text"], self.prompts["onepass"]) if p["text"].strip() else (
                        "empty_plain")
                    out["transcript"] = o["text"] if reason in ("cleaned", "empty_plain") else p["text"]
                    out["finalize_s"] = max(p["api_s"], o["api_s"])
                    out["cost_usd"] = sum(float((x.get("usage") or {}).get("cost") or 0) for x in (p, o))
                elif o_ok:
                    reason = "no_plain"
                elif p_ok:
                    reason = "no_onepass"
                else:
                    reason = "missing"
                    out["status"] = "missing"
                out.update(raw_text=p.get("text", "") if p_ok else "", cleaned_text=o.get("text", "") if o_ok else "",
                           reason=reason)
                w.writerow(out)


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--model", action="append", choices=sorted(MODELS), help="default: all")
    ap.add_argument("--clips", type=pathlib.Path, default=CLIPS)
    ap.add_argument("--out", type=pathlib.Path, default=RESULTS)
    ap.add_argument("--ledger", type=pathlib.Path, default=RESULTS / "budget.jsonl")
    ap.add_argument("--base-url", default=os.environ.get("OPENROUTER_BASE_URL", DEFAULT_BASE))
    ap.add_argument("--limit", type=int, help="only the first N clips (a trial)")
    ap.add_argument("--lead-s", type=float, default=0.8)
    ap.add_argument("--tail-s", type=float, default=4.3)
    ap.add_argument("--attempts", type=int, default=6)
    ap.add_argument("--backoff", type=float, default=2.0, help="first retry delay (s), doubling")
    ap.add_argument("--timeout", type=float, default=90.0)
    ap.add_argument("--interval-scale", type=float, default=1.0, help=argparse.SUPPRESS)  # tests
    ap.add_argument("--write-only", action="store_true", help="rewrite the CSVs from the caches, no requests")
    args = ap.parse_args()
    args.base_url = args.base_url.rstrip("/")

    key = os.environ.get("OPENROUTER_API_KEY", "").strip()
    if not key and not args.write_only:
        print("OPENROUTER_API_KEY is not set", file=sys.stderr)
        return 2
    manifest = args.clips / "manifest.csv"
    if not manifest.exists():
        print(f"{manifest} is missing: run eval/voicearena/bench.sh fetch first", file=sys.stderr)
        return 2
    clips = sorted(csv.DictReader(open(manifest, newline="", encoding="utf-8")), key=lambda c: c["clip_id"])
    if args.limit:
        clips = clips[: args.limit]
    ledger = budget.Ledger(args.ledger, CAP_USD)
    stop_all = threading.Event()
    names = args.model or list(MODELS)
    runs = [ModelRun(n, MODELS[n], args, key, ledger, stop_all) for n in names]
    if args.write_only:
        for r in runs:
            r.write(clips)
        return 0
    say(f"part B: {len(clips)} clips x {len(MODES)} prompts x {len(runs)} models; ledger ${ledger.used_usd():.4f} "
        f"of ${CAP_USD:.2f}")
    failed = False
    with cf.ThreadPoolExecutor(max_workers=len(runs)) as ex:
        futs = {ex.submit(r.run, clips): r for r in runs}
        for fut in cf.as_completed(futs):
            r = futs[fut]
            try:
                outcome = fut.result()
            except Fatal as e:
                say(f"{r.name}: {e}")
                r.write(clips)
                failed = True
                continue
            ok = {m: sum(1 for c in clips if r.done(c["clip_id"], m)) for m in MODES}
            say(f"{r.name}: {outcome}; ok {ok}; ${r.spent:.4f}")
            failed |= min(ok.values()) < len(clips)
    say(f"part B done; ledger ${ledger.used_usd():.4f} of ${CAP_USD:.2f}")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
