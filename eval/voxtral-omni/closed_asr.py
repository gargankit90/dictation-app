# /// script
# requires-python = ">=3.11"
# dependencies = ["numpy", "soundfile>=0.12", "soxr>=0.3"]
# ///
"""Closed models (Google Gemini, OpenAI) on the Voice Arena public set: transcription, and one-pass
transcribe-and-clean where the model can do it. CLOSED MODELS: our hosted shortlist has been open
models; the owner asked for this comparison (2026-11-19).

    GEMINI_API_KEY=... OPENAI_API_KEY=... uv run eval/voxtral-omni/closed_asr.py [--model KEY ...] [--limit N]

A provider whose key is not set is skipped. Models (Google's and OpenAI's docs, read 2026-11-19):

  gemini_transcribe   gemini-3.5-transcribe, Google's dedicated speech-to-text model (Interactions API).
                      plain = mode "verbatim"; onepass = mode "smart" (Google's own clean-up: removes
                      disfluencies, resolves self-corrections, formats).
  gemini_flash_lite   gemini-3.5-flash-lite, "fastest, most cost-effective" audio-input model;
                      thinking "minimal". plain / onepass with the same prompts as part B.
  gemini_flash        gemini-3.8-flash, "most intelligent Flash model"; thinking "low" (minimal is not
                      offered on it). plain / onepass.
  openai_transcribe   gpt-transcribe, OpenAI's recommended speech-to-text model (best quality).
  openai_mini         gpt-4o-mini-transcribe-2025-12-15, the cheapest listed transcription model
                      (deprecated, shuts down 2027-02-26).
  (whisper-1 is left out: we already have Whisper data via Groq.) The OpenAI models transcribe only
  (plain); the clean-up on top runs text-only through run_matrix.py (--only openai_transcribe__luna_v4 ...).

Audio goes the way Vocet uploads it and part B sent it (audio_llm.wav_bytes: 16 kHz mono 16-bit WAV,
0.8 s of silence before, 4.3 s after). The one-pass reply goes through the app's guard (guard.py)
against the same model's plain transcript, as in part B; results are written with and without it.

Secrets and privacy: each key is read only from its environment variable and goes only into its
request header (x-goog-api-key / Authorization); it is never printed, logged or written. Response
bodies are never printed or stored; an error records only the HTTP status (and for Google, the
`error.status` enum such as INVALID_ARGUMENT). From a reply only the text and usage numbers are kept.

Budget: one ledger and cap per provider (budget.py; results/budget_gemini.jsonl and
results/budget_openai.jsonl, $3.00 each, apart from the OpenRouter ledger). Each request reserves a
worst-case price first and settles to the price computed from the reply's usage (list prices below;
Google and OpenAI don't return a cost). Each model also has a sub-cap and a projection check after 3
paid clips. HTTP 429/5xx are retried with Retry-After or exponential backoff; a model stops after 3
clips in a row that still got 429, and on 400/404 (request shape or model id not accepted). 401/403
stop that provider.

Cache: results/<label>/calls.jsonl, keyed by model, mode, prompt/config and clip; a rerun only asks
what has no ok answer yet. Output per model, in the columns the judges read (finalize_s = API seconds):
  results/<label>_plain/harness_results.csv, and for models with a one-pass mode
  results/<label>_onepass/harness_results.csv and results/<label>_onepass_guard/harness_results.csv.
"""
from __future__ import annotations

import argparse
import base64
import concurrent.futures as cf
import csv
import hashlib
import json
import os
import pathlib
import random
import sys
import threading
import time
import urllib.error
import urllib.request
import uuid

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import budget  # noqa: E402
import guard  # noqa: E402
from audio_llm import wav_bytes  # noqa: E402

CLIPS = HERE.parent / "voicearena" / "clips"
RESULTS = HERE / "results"
CAP_USD = 3.00
PROMPTS = {"plain": HERE / "prompts" / "audio_plain.txt", "onepass": HERE / "prompts" / "audio_onepass_v4.txt"}
GEMINI_BASE = "https://generativelanguage.googleapis.com/v1beta"
OPENAI_BASE = "https://api.openai.com/v1"
GEMINI_REVISION = "2026-05-20"  # the Api-Revision the audio docs' examples send
AUDIO_TOK_PER_S = 32  # Google's audio page (the pricing footnote says 25; the larger is the worst case)

# Prices: USD per token (Gemini: paid Standard tier, audio input listed with text/image/video; output
# includes thinking) or per audio minute (OpenAI). cap: sub-cap for the model's modes together.
MODELS = {
    "gemini_transcribe": {"provider": "gemini", "kind": "transcribe", "model": "gemini-3.5-transcribe",
                          "modes": {"plain": "verbatim", "onepass": "smart"},
                          "in": 2.0e-6, "out": 12.0e-6, "max_out": 1024, "cap": 1.00},
    "gemini_flash_lite": {"provider": "gemini", "kind": "chat", "model": "gemini-3.5-flash-lite",
                          "modes": {"plain": "plain", "onepass": "onepass"}, "thinking": "minimal",
                          "in": 0.30e-6, "out": 2.50e-6, "max_out": 1024, "cap": 0.40},
    "gemini_flash": {"provider": "gemini", "kind": "chat", "model": "gemini-3.8-flash",
                     "modes": {"plain": "plain", "onepass": "onepass"}, "thinking": "low",
                     "in": 0.75e-6, "out": 3.75e-6, "max_out": 2048, "cap": 1.20},
    "openai_transcribe": {"provider": "openai", "kind": "transcription", "model": "gpt-transcribe",
                          "modes": {"plain": "plain"}, "usd_per_min": 0.0045, "cap": 0.60},
    "openai_mini": {"provider": "openai", "kind": "transcription", "model": "gpt-4o-mini-transcribe-2025-12-15",
                    "modes": {"plain": "plain"}, "usd_per_min": 0.003, "in": 1.25e-6, "out": 5.0e-6,
                    "cap": 0.40},
}
KEYS = {"gemini": "GEMINI_API_KEY", "openai": "OPENAI_API_KEY"}
_print_lock = threading.Lock()


class StopProvider(Exception):
    """Stop every model of this provider: the key is rejected."""


class StopModel(Exception):
    """Stop this model only."""


def say(msg: str) -> None:
    with _print_lock:
        print(msg, flush=True)


# ---- requests -----------------------------------------------------------------------------------

def gemini_body(cfg: dict, mode: str, prompt: str | None, wav: bytes) -> dict:
    audio = {"type": "audio", "data": base64.b64encode(wav).decode(), "mime_type": "audio/wav"}
    body = {"model": cfg["model"], "input": [audio], "generation_config": {"max_output_tokens": cfg["max_out"]}}
    if cfg["kind"] == "transcribe":
        body["generation_config"]["transcription_config"] = {"language_codes": ["en"], "mode": cfg["modes"][mode]}
    else:
        body["system_instruction"] = prompt
        body["generation_config"]["thinking_level"] = cfg["thinking"]
    return body


def multipart(fields: dict, wav: bytes) -> tuple[bytes, str]:
    b = uuid.uuid4().hex
    out = []
    for k, v in fields.items():
        out.append(f'--{b}\r\nContent-Disposition: form-data; name="{k}"\r\n\r\n{v}\r\n'.encode())
    out.append(f'--{b}\r\nContent-Disposition: form-data; name="file"; filename="audio.wav"\r\n'
               f"Content-Type: audio/wav\r\n\r\n".encode() + wav + b"\r\n")
    out.append(f"--{b}--\r\n".encode())
    return b"".join(out), f"multipart/form-data; boundary={b}"


def send(url: str, headers: dict, data: bytes, timeout: float) -> tuple[int, dict | None, dict, str]:
    """(status, JSON or None, lower-case headers, Google error.status or ''). Error bodies are only
    parsed for Google's error.status enum, never printed or kept."""
    req = urllib.request.Request(url, data=data, method="POST", headers=headers)
    try:
        with urllib.request.urlopen(req, timeout=timeout) as r:
            raw = r.read()
            hdrs = {k.lower(): v for k, v in r.headers.items()}
            try:
                return r.status, json.loads(raw), hdrs, ""
            except ValueError:
                return r.status, None, hdrs, ""
    except urllib.error.HTTPError as e:
        hdrs = {k.lower(): v for k, v in (e.headers or {}).items()}
        enum = ""
        try:
            err = json.loads(e.read(65536)).get("error", {})
            s = err.get("status") if isinstance(err, dict) else None
            if isinstance(s, str) and s.replace("_", "").isalpha() and s.isupper() and len(s) < 40:
                enum = s
        except Exception:  # noqa: BLE001 - the body is dropped either way
            pass
        e.close()
        return e.code, None, hdrs, enum


def gemini_parse(data) -> tuple[str | None, dict]:
    """Text of the model_output steps (Interactions API) and the usage numbers."""
    if not isinstance(data, dict):
        return None, {}
    texts = []
    for step in data.get("steps") or []:
        if isinstance(step, dict) and step.get("type") == "model_output":
            for part in step.get("content") or []:
                if isinstance(part, dict) and part.get("type") == "text" and isinstance(part.get("text"), str):
                    texts.append(part["text"])
    if not texts and isinstance(data.get("output_text"), str):
        texts = [data["output_text"]]
    u = data.get("usage") if isinstance(data.get("usage"), dict) else {}
    audio = sum(int(m.get("tokens") or 0) for m in (u.get("input_tokens_by_modality") or [])
                if isinstance(m, dict) and m.get("modality") == "audio")
    usage = {"input_tokens": u.get("total_input_tokens"), "audio_tokens": audio or None,
             "output_tokens": u.get("total_output_tokens"), "thought_tokens": u.get("total_thought_tokens"),
             "status": data.get("status")}
    return ("".join(texts).strip() if texts else None), usage


def openai_parse(data) -> tuple[str | None, dict]:
    if not isinstance(data, dict) or not isinstance(data.get("text"), str):
        return None, {}
    u = data.get("usage") if isinstance(data.get("usage"), dict) else {}
    det = u.get("input_token_details") or {}
    return data["text"].strip(), {"usage_type": u.get("type"), "input_tokens": u.get("input_tokens"),
                                  "audio_tokens": det.get("audio_tokens"), "output_tokens": u.get("output_tokens"),
                                  "seconds": u.get("seconds")}


def price(cfg: dict, usage: dict, sent_s: float) -> float:
    if cfg["provider"] == "gemini":
        out = (usage.get("output_tokens") or 0) + (usage.get("thought_tokens") or 0)
        return (usage.get("input_tokens") or 0) * cfg["in"] + out * cfg["out"]
    if usage.get("usage_type") == "tokens" and "in" in cfg and usage.get("input_tokens") is not None:
        return usage["input_tokens"] * cfg["in"] + (usage.get("output_tokens") or 0) * cfg["out"]
    return (usage.get("seconds") or sent_s) / 60 * cfg["usd_per_min"]  # billed per audio minute


def worst_case(cfg: dict, prompt: str | None, sent_s: float) -> float:
    if cfg["provider"] == "gemini":
        tokens_in = sent_s * AUDIO_TOK_PER_S * 1.5 + 256 + len((prompt or "").encode())
        return tokens_in * cfg["in"] + cfg["max_out"] * cfg["out"]
    return 1.5 * sent_s / 60 * cfg["usd_per_min"] + 0.0005


# ---- one model ----------------------------------------------------------------------------------

class ClosedRun:
    def __init__(self, name: str, cfg: dict, args, key: str, ledger: budget.Ledger, stop: threading.Event):
        self.name, self.cfg, self.args, self.key, self.ledger, self.stop = name, cfg, args, key, ledger, stop
        self.modes = list(cfg["modes"])
        self.prompts = {m: PROMPTS[m].read_text(encoding="utf-8").strip() for m in self.modes}
        self.dir = args.out / name
        self.dir.mkdir(parents=True, exist_ok=True)
        self.cache_path = self.dir / "calls.jsonl"
        self.records = [json.loads(l) for l in self.cache_path.read_text(encoding="utf-8").splitlines()
                        if l.strip()] if self.cache_path.exists() else []
        self.spent = sum(float(r.get("cost_usd") or 0) for r in self.records)
        self.paid = sum(1 for r in self.records if r.get("status") == "ok")
        self.limited = 0

    def ckey(self, clip_id: str, mode: str) -> str:
        what = self.cfg["modes"][mode] if self.cfg["kind"] != "chat" else self.prompts[mode]
        return hashlib.sha256(json.dumps([self.cfg["model"], mode, what, self.cfg.get("thinking"), clip_id,
                                          self.args.lead_s, self.args.tail_s]).encode()).hexdigest()

    def done(self, clip_id: str, mode: str) -> dict | None:
        k = self.ckey(clip_id, mode)
        return next((r for r in reversed(self.records) if r["key"] == k and r["status"] == "ok"), None)

    def last(self, clip_id: str, mode: str) -> dict | None:
        return next((r for r in reversed(self.records) if r["clip_id"] == clip_id and r["mode"] == mode), None)

    def save(self, rec: dict) -> None:
        with self.cache_path.open("a", encoding="utf-8") as fh:
            fh.write(json.dumps(rec, ensure_ascii=False) + "\n")
        self.records.append(rec)

    def request(self, mode: str, wav: bytes) -> tuple[str, dict, bytes]:
        cfg = self.cfg
        if cfg["provider"] == "gemini":
            body = gemini_body(cfg, mode, self.prompts[mode] if cfg["kind"] == "chat" else None, wav)
            return (f"{self.args.gemini_base}/interactions",
                    {"x-goog-api-key": self.key, "Content-Type": "application/json",
                     "Api-Revision": GEMINI_REVISION}, json.dumps(body).encode())
        fields = {"model": cfg["model"], "response_format": "json", "temperature": "0"}
        if cfg["model"] == "gpt-transcribe":
            fields["languages[]"] = "en"
        else:
            fields["language"] = "en"
        data, ctype = multipart(fields, wav)
        return f"{self.args.openai_base}/audio/transcriptions", {
            "Authorization": f"Bearer {self.key}", "Content-Type": ctype}, data

    def call(self, mode: str, wav: bytes, sent_s: float) -> dict:
        prompt = self.prompts[mode] if self.cfg["kind"] == "chat" else None
        last = "?"
        for attempt in range(1, self.args.attempts + 1):
            if self.stop.is_set():
                raise StopModel("provider stopped")
            est = worst_case(self.cfg, prompt, sent_s)
            if self.spent + est > self.cfg["cap"]:
                raise StopModel(f"next call would pass the ${self.cfg['cap']:.2f} sub-cap (${self.spent:.4f})")
            try:
                held = self.ledger.reserve(est, self.name)
            except budget.OverBudget as e:
                raise StopModel(str(e)) from None
            url, headers, data = self.request(mode, wav)
            t0 = time.monotonic()
            try:
                status, js, hdrs, enum = send(url, headers, data, self.args.timeout)
            except (urllib.error.URLError, TimeoutError, OSError) as e:
                status, js, hdrs, enum = 0, None, {}, ""
                last = f"network error ({type(e).__name__})"
            api_s = time.monotonic() - t0
            if status == 200:
                text, usage = (gemini_parse if self.cfg["provider"] == "gemini" else openai_parse)(js)
                cost = price(self.cfg, usage, sent_s)
                self.ledger.settle(held, cost, self.name)
                self.spent += cost
                self.limited = 0
                if text is not None:
                    self.paid += 1
                    return {"status": "ok", "text": guard.strip_tags(text), "api_s": round(api_s, 3),
                            "attempts": attempt, "usage": usage, "cost_usd": round(cost, 8)}
                return {"status": "error", "error": "no text in the reply", "api_s": round(api_s, 3),
                        "attempts": attempt, "usage": usage, "cost_usd": round(cost, 8)}
            if status:  # an HTTP error reply is not billed; a dropped connection keeps its hold
                self.ledger.settle(held, 0.0, self.name)
                last = f"HTTP {status}" + (f" {enum}" if enum else "")
            if status in (401, 403):
                self.stop.set()
                raise StopProvider(f"{last} from {self.cfg['provider']} (key rejected); stopping this provider")
            if status in (400, 404):
                raise StopModel(f"{last}: request or model id not accepted")
            retryable = status in (0, 408, 409, 429) or status >= 500
            if not retryable or attempt == self.args.attempts:
                if status == 429:
                    self.limited += 1
                    if self.limited >= 3:
                        raise StopModel("HTTP 429 on 3 clips in a row after retries (rate, quota or spend limit)")
                return {"status": "error", "error": last, "attempts": attempt, "api_s": round(api_s, 3)}
            ra = hdrs.get("retry-after")
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
        todo = [(c, m) for c in clips for m in self.modes if not self.done(c["clip_id"], m)]
        say(f"{self.name} ({self.cfg['model']}): {len(clips) * len(self.modes) - len(todo)} cached, "
            f"{len(todo)} to do, spent so far ${self.spent:.4f}")
        outcome = "done"
        wavs: dict = {}
        try:
            for i, (c, mode) in enumerate(todo, 1):
                if self.paid >= 3 and self.spent + self.spent / self.paid * (len(todo) - i + 1) > self.cfg["cap"]:
                    raise StopModel(f"projected spend passes the ${self.cfg['cap']:.2f} sub-cap "
                                    f"after {self.paid} calls (${self.spent:.4f})")
                if c["clip_id"] not in wavs:
                    wavs = {c["clip_id"]: wav_bytes(self.args.clips / c["file"], self.args.lead_s, self.args.tail_s)}
                wav, sent_s = wavs[c["clip_id"]]
                r = self.call(mode, wav, sent_s)
                r.update({"key": self.ckey(c["clip_id"], mode), "clip_id": c["clip_id"], "mode": mode,
                          "model": self.cfg["model"], "sent_s": round(sent_s, 3),
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
                "audio_duration_s", "sent_s", "finalize_s", "error", "model", "provider", "cost_usd", "run_at"]
        got = {m: {c["clip_id"]: (self.done(c["clip_id"], m) or self.last(c["clip_id"], m)) for c in clips}
               for m in self.modes}
        tool = {"gemini": "gemini-interactions", "openai": "openai-transcriptions"}[self.cfg["provider"]]
        prov = {"gemini": "Google (closed model)", "openai": "OpenAI (closed model)"}[self.cfg["provider"]]

        def row(c, r):
            r = r or {"status": "missing"}
            return {"tool": tool, "clip_id": c["clip_id"], "file": c["file"], "status": r["status"],
                    "attempts": r.get("attempts", ""), "transcript": r.get("text", ""),
                    "task_type": c.get("task_type", ""), "speaker_id": c.get("speaker_id", ""),
                    "audio_duration_s": c.get("audio_duration_s", ""), "sent_s": r.get("sent_s", ""),
                    "finalize_s": r.get("api_s", ""), "error": r.get("error", ""), "model": self.cfg["model"],
                    "provider": prov, "cost_usd": r.get("cost_usd", ""), "run_at": r.get("run_at", "")}

        for mode in self.modes:
            d = self.args.out / f"{self.name}_{mode}"
            d.mkdir(parents=True, exist_ok=True)
            with open(d / "harness_results.csv", "w", newline="", encoding="utf-8") as f:
                w = csv.DictWriter(f, cols)
                w.writeheader()
                for c in clips:
                    w.writerow(row(c, got[mode][c["clip_id"]]))
        if "onepass" not in self.modes:
            return
        # As part B: the one-pass reply replaces the plain transcript only if the app's guard accepts it;
        # finalize_s is the slower of the two calls (they can run in parallel), cost_usd their sum. For
        # Gemini Transcribe's smart mode the guard's instructions are the audio one-pass prompt.
        gp = PROMPTS["onepass"].read_text(encoding="utf-8").strip()
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
                    reason = guard.judge(p["text"], o["text"], gp) if p["text"].strip() else "empty_plain"
                    out["transcript"] = o["text"] if reason in ("cleaned", "empty_plain") else p["text"]
                    out["finalize_s"] = max(p["api_s"], o["api_s"])
                    out["cost_usd"] = float(p.get("cost_usd") or 0) + float(o.get("cost_usd") or 0)
                else:
                    reason = "no_plain" if o_ok else ("no_onepass" if p_ok else "missing")
                    if not (p_ok or o_ok):
                        out["status"] = "missing"
                out.update(raw_text=p.get("text", "") if p_ok else "", cleaned_text=o.get("text", "") if o_ok else "",
                           reason=reason)
                w.writerow(out)


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--model", action="append", choices=sorted(MODELS), help="default: all with a key set")
    ap.add_argument("--clips", type=pathlib.Path, default=CLIPS)
    ap.add_argument("--out", type=pathlib.Path, default=RESULTS)
    ap.add_argument("--ledger-dir", type=pathlib.Path, help="default: --out")
    ap.add_argument("--gemini-base", default=os.environ.get("GEMINI_BASE_URL", GEMINI_BASE))
    ap.add_argument("--openai-base", default=os.environ.get("OPENAI_BASE_URL", OPENAI_BASE))
    ap.add_argument("--limit", type=int, help="only the first N clips (a trial)")
    ap.add_argument("--lead-s", type=float, default=0.8)
    ap.add_argument("--tail-s", type=float, default=4.3)
    ap.add_argument("--attempts", type=int, default=6)
    ap.add_argument("--backoff", type=float, default=2.0, help="first retry delay (s), doubling")
    ap.add_argument("--timeout", type=float, default=90.0)
    ap.add_argument("--write-only", action="store_true", help="rewrite the CSVs from the caches, no requests")
    ap.add_argument("--estimate", action="store_true", help="print the worst-case and expected cost, no requests")
    args = ap.parse_args()
    args.gemini_base, args.openai_base = args.gemini_base.rstrip("/"), args.openai_base.rstrip("/")
    manifest = args.clips / "manifest.csv"
    if not manifest.exists():
        print(f"{manifest} is missing: run eval/voicearena/bench.sh fetch first", file=sys.stderr)
        return 2
    clips = sorted(csv.DictReader(open(manifest, newline="", encoding="utf-8")), key=lambda c: c["clip_id"])
    if args.limit:
        clips = clips[: args.limit]
    names = args.model or list(MODELS)
    if args.estimate:
        sent = [float(c["audio_length_s"]) + args.lead_s + args.tail_s for c in clips]
        for n in names:
            cfg = MODELS[n]
            prompts = {m: (PROMPTS[m].read_text().strip() if cfg["kind"] == "chat" else None) for m in cfg["modes"]}
            worst = sum(worst_case(cfg, prompts[m], s) for m in cfg["modes"] for s in sent)
            if cfg["provider"] == "gemini":  # 32 audio tokens/s, the prompt, ~60 output (+ thinking) tokens
                think = {"minimal": 0, "low": 250}.get(cfg.get("thinking"), 0)
                exp = sum((s * AUDIO_TOK_PER_S + len((prompts[m] or "").encode()) / 4) * cfg["in"]
                          + (60 + think) * cfg["out"] for m in cfg["modes"] for s in sent)
            else:
                exp = sum(s / 60 * cfg["usd_per_min"] for s in sent)
            print(f"{n:18} {cfg['model']:36} {len(cfg['modes'])} mode(s) x {len(clips)} clips: expected "
                  f"${exp:.2f}, worst case ${worst:.2f}, sub-cap ${cfg['cap']:.2f}")
        return 0
    keys = {p: os.environ.get(v, "").strip() for p, v in KEYS.items()}
    if not args.write_only:
        for p, v in KEYS.items():
            if not keys[p] and any(MODELS[n]["provider"] == p for n in names):
                say(f"{v} is not set: skipping the {p} models")
        names = [n for n in names if keys[MODELS[n]["provider"]]]
        if not names:
            print("no key set (GEMINI_API_KEY / OPENAI_API_KEY)", file=sys.stderr)
            return 2
    ldir = args.ledger_dir or args.out
    ledgers = {p: budget.Ledger(ldir / f"budget_{p}.jsonl", CAP_USD) for p in KEYS}
    stops = {p: threading.Event() for p in KEYS}
    runs = [ClosedRun(n, MODELS[n], args, keys[MODELS[n]["provider"]], ledgers[MODELS[n]["provider"]],
                      stops[MODELS[n]["provider"]]) for n in names]
    if args.write_only:
        for r in runs:
            r.write(clips)
        return 0
    say(f"closed models: {len(clips)} clips; " + ", ".join(
        f"{p} ledger ${ledgers[p].used_usd():.4f} of ${CAP_USD:.2f}" for p in KEYS))
    failed = False
    with cf.ThreadPoolExecutor(max_workers=len(runs)) as ex:
        futs = {ex.submit(r.run, clips): r for r in runs}
        for fut in cf.as_completed(futs):
            r = futs[fut]
            try:
                outcome = fut.result()
            except StopProvider as e:
                say(f"{r.name}: {e}")
                r.write(clips)
                failed = True
                continue
            ok = {m: sum(1 for c in clips if r.done(c["clip_id"], m)) for m in r.modes}
            say(f"{r.name}: {outcome}; ok {ok}; ${r.spent:.4f}")
            failed |= min(ok.values()) < len(clips)
    say("closed models done; " + ", ".join(f"{p} ${ledgers[p].used_usd():.4f}" for p in KEYS))
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
