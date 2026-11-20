# /// script
# requires-python = ">=3.11"
# dependencies = ["numpy", "soundfile>=0.12", "soxr>=0.3"]
# ///
"""End-to-end tests of transcribe_openrouter.py against a fake local OpenRouter.

    uv run eval/asr/hosted/test_transcribe_openrouter.py

The fake server checks each request (auth header, JSON body, a 16 kHz mono 16-bit WAV of the right
length, language en, temperature 0, no prompt) and plays back rate limits, server errors, a rejected
request whose body holds a sentinel, out-of-credit and generation lookups with extra fields. The
tests check retries, resume, the budget stop, the 402 stop, and that neither the key nor any error
body nor any non-whitelisted field ever reaches stdout, stderr or the output files.
"""
import base64
import csv
import io
import json
import os
import pathlib
import subprocess
import sys
import tempfile
import threading
import unittest
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

try:
    import numpy as np
    import soundfile as sf
except ImportError as e:  # the gate's plain python3 has neither; `uv run pytest eval` does
    raise unittest.SkipTest(f"needs numpy and soundfile: {e}")

HERE = pathlib.Path(__file__).resolve().parent
SCRIPT = HERE / "transcribe_openrouter.py"
KEY = "fake-key-" + "SENTINEL-0123456789"  # not key-shaped, so the hygiene scan stays clean
BODY_SENTINEL = "ERROR-BODY-SENTINEL"
GEN_SENTINEL = "GENERATION-EXTRA-SENTINEL"
CLIP_S = {"dict_0001": 1.5, "dict_0002": 2.0, "dict_0003": 1.0, "dict_0004": 2.5}


class Fake:
    """Shared state of the fake server: scripted replies per clip and every request seen."""

    def __init__(self):
        self.lock = threading.Lock()
        self.requests = []          # (model, clip duration in the WAV, attempt number)
        self.attempts = {}          # (model, rounded duration) -> count
        self.script = {}            # rounded duration -> list of statuses to play before 200
        self.always = {}            # rounded duration -> status returned forever
        self.problems = []


def make_handler(fake: Fake):
    class H(BaseHTTPRequestHandler):
        def log_message(self, *a):
            pass

        def _send(self, status, body, headers=None):
            raw = json.dumps(body).encode()
            self.send_response(status)
            self.send_header("Content-Type", "application/json")
            for k, v in (headers or {}).items():
                self.send_header(k, v)
            self.send_header("Content-Length", str(len(raw)))
            self.end_headers()
            self.wfile.write(raw)

        def _auth_ok(self):
            if self.headers.get("Authorization") != f"Bearer {KEY}":
                fake.problems.append("bad auth header")
                self._send(401, {"error": BODY_SENTINEL})
                return False
            return True

        def do_GET(self):
            if not self._auth_ok():
                return
            if self.path.startswith("/api/v1/generation?id=gen-"):
                gid = self.path.split("id=")[1]
                self._send(200, {"data": {"id": gid, "provider_name": "FakeProvider", "latency": 321,
                                          "total_cost": 0.0001, "secret_extra": GEN_SENTINEL}})
            else:
                self._send(404, {"error": BODY_SENTINEL})

        def do_POST(self):
            if not self._auth_ok():
                return
            if self.path != "/api/v1/audio/transcriptions":
                fake.problems.append(f"path {self.path}")
                return self._send(404, {"error": BODY_SENTINEL})
            body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
            if body.get("language") != "en" or body.get("temperature") != 0 or "prompt" in body:
                fake.problems.append(f"body fields {sorted(k for k in body if k != 'input_audio')}")
            ia = body["input_audio"]
            if ia.get("format") != "wav":
                fake.problems.append("format")
            wav = base64.b64decode(ia["data"])
            info = sf.info(io.BytesIO(wav))
            if (info.samplerate, info.channels, info.subtype, info.format) != (16000, 1, "PCM_16", "WAV"):
                fake.problems.append(f"wav {info.samplerate} {info.channels} {info.subtype}")
            if wav[:4] != b"RIFF" or wav[36:40] != b"data":
                fake.problems.append("not a 44-byte-header WAV")
            dur = round(info.duration, 1)
            model = body["model"]
            with fake.lock:
                n = fake.attempts.get((model, dur), 0) + 1
                fake.attempts[(model, dur)] = n
                fake.requests.append((model, dur, n))
                plan = fake.script.get(dur, [])
                status = fake.always.get(dur) or (plan[n - 1] if n <= len(plan) else 200)
            if status == 200:
                return self._send(200, {"text": f" {model} heard {dur:.1f} seconds ",
                                        "usage": {"seconds": dur, "cost": 0.001}},
                                  {"X-Generation-Id": f"gen-{model.replace('/', '-')}-{dur}-{n}"})
            hdr = {"Retry-After": "0"} if status == 429 else {}
            self._send(status, {"error": {"message": BODY_SENTINEL, "key": KEY}}, hdr)
    return H


def make_clips(d: pathlib.Path) -> None:
    d.mkdir(parents=True, exist_ok=True)
    rows = []
    for cid, secs in CLIP_S.items():
        t = np.arange(int(44100 * secs)) / 44100
        stereo = np.stack([0.1 * np.sin(2 * np.pi * 220 * t)] * 2, axis=1)  # stereo 44.1 kHz in
        sf.write(d / f"{cid}.flac", stereo, 44100, subtype="PCM_16")
        rows.append({"file": f"{cid}.flac", "clip_id": cid, "task_type": "email", "speaker_id": "1",
                     "audio_duration_s": secs})
    with open(d / "manifest.csv", "w", newline="") as f:
        w = csv.DictWriter(f, list(rows[0]))
        w.writeheader()
        w.writerows(rows)


class E2E(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.dir = pathlib.Path(self.tmp.name)
        self.clips = self.dir / "clips"
        self.out = self.dir / "out"
        make_clips(self.clips)
        self.fake = Fake()
        self.srv = ThreadingHTTPServer(("127.0.0.1", 0), make_handler(self.fake))
        threading.Thread(target=self.srv.serve_forever, daemon=True).start()
        self.base = f"http://127.0.0.1:{self.srv.server_port}/api/v1"

    def tearDown(self):
        self.srv.shutdown()
        self.srv.server_close()
        self.tmp.cleanup()

    def run_script(self, *extra, models=("nvidia/nemotron-3.5-transcribe", "qwen/qwen3-asr-1.7b"),
                   env_key=KEY):
        cmd = ["uv", "run", "--quiet", str(SCRIPT), "--clips", str(self.clips), "--out", str(self.out),
               "--base-url", self.base, "--backoff", "0.01", "--gen-delay-scale", "0.01", *extra]
        for m in models:
            cmd += ["--model", m]
        env = {k: v for k, v in os.environ.items() if k != "OPENROUTER_API_KEY"}
        if env_key is not None:
            env["OPENROUTER_API_KEY"] = env_key
        p = subprocess.run(cmd, capture_output=True, text=True, env=env, timeout=300)
        self.assert_clean(p.stdout + p.stderr)
        return p

    def assert_clean(self, text: str) -> None:
        for s in (KEY, BODY_SENTINEL, GEN_SENTINEL):
            self.assertNotIn(s, text)

    def outputs_clean(self):
        for f in self.out.rglob("*"):
            if f.is_file():
                self.assert_clean(f.read_text())

    def rows(self, model):
        path = self.out / model.replace("/", "__") / "harness_results.csv"
        with open(path) as f:
            return {r["clip_id"]: r for r in csv.DictReader(f)}

    def test_full_run_retries_resume_and_privacy(self):
        # 2.0 s clip (+0.8 + 4.3 = 7.1 s sent): 429 then 500 then OK; 1.0 s clip (6.1 s): always 400.
        self.fake.script[7.1] = [429, 500]
        self.fake.always[6.1] = 400
        p = self.run_script()
        self.assertEqual(p.returncode, 1, p.stdout + p.stderr)  # one clip failed per model
        self.assertEqual(self.fake.problems, [])
        for m in ("nvidia/nemotron-3.5-transcribe", "qwen/qwen3-asr-1.7b"):
            rows = self.rows(m)
            self.assertEqual(len(rows), 4)
            self.assertEqual(rows["dict_0002"]["status"], "ok")
            self.assertEqual(rows["dict_0002"]["attempts"], "3")
            self.assertEqual(rows["dict_0002"]["transcript"], f"{m} heard 7.1 seconds")
            self.assertEqual(rows["dict_0002"]["provider"], "FakeProvider")
            self.assertEqual(rows["dict_0002"]["provider_latency_s"], "0.321")
            self.assertEqual(rows["dict_0002"]["sent_s"], "7.1")
            self.assertGreater(float(rows["dict_0002"]["finalize_s"]), 0)
            self.assertEqual(rows["dict_0003"]["status"], "error")
            self.assertEqual(rows["dict_0003"]["error"], "HTTP 400")
            self.assertEqual(rows["dict_0003"]["attempts"], "1")  # a 400 is not retried
            self.assertIn(f"{m}: 3 ok, 1 failed -> ", p.stdout)
        self.outputs_clean()

        # Rerun: only the failed clip is sent again; when it now succeeds the run is complete.
        n_before = len(self.fake.requests)
        del self.fake.always[6.1]
        p = self.run_script()
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        sent = self.fake.requests[n_before:]
        self.assertEqual(sorted(d for _, d, _ in sent), [6.1, 6.1])
        for m in ("nvidia/nemotron-3.5-transcribe", "qwen/qwen3-asr-1.7b"):
            self.assertEqual({r["status"] for r in self.rows(m).values()}, {"ok"})
        self.assertIn("3 cached, 1 to do", p.stdout)
        self.outputs_clean()

    def test_bare_clip_option(self):
        p = self.run_script("--lead-s", "0", "--tail-s", "0", "--limit", "1", models=("qwen/qwen3-asr-0.6b",))
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertEqual([d for _, d, _ in self.fake.requests], [1.5])

    def test_budget_stops_before_the_cap(self):
        # Each reply costs $0.001 and the next clip is estimated at ~$0.0002 (6.6 s at the dearest
        # Nemotron price), so a $0.0021 cap allows two requests.
        p = self.run_script("--max-usd", "0.0021", models=("nvidia/nemotron-3.5-transcribe",))
        self.assertEqual(p.returncode, 1)
        self.assertEqual(len(self.fake.requests), 2)
        self.assertIn("stopping, the next clip would pass the $0.00 budget (spent about $0.002)", p.stdout)
        # Cached spend counts on a rerun: nothing more is sent.
        p = self.run_script("--max-usd", "0.0021", models=("nvidia/nemotron-3.5-transcribe",))
        self.assertEqual(len(self.fake.requests), 2)

    def test_out_of_credit_stops_everything(self):
        for d in (5.6, 6.6, 7.1, 6.1, 7.6):
            self.fake.always[d] = 402
        p = self.run_script()
        self.assertEqual(p.returncode, 1)
        self.assertIn("HTTP 402 from OpenRouter (key rejected or no credit); stopping", p.stdout)
        self.assertLessEqual(len(self.fake.requests), 2)  # each model stops at its first 402
        self.outputs_clean()

    def test_ids_file_limits_the_run_to_the_listed_clips(self):
        cid = sorted(CLIP_S)[-1]
        ids = self.dir / "ids.txt"
        ids.write_text(cid + "\n")
        p = self.run_script("--ids-file", str(ids), models=("qwen/qwen3-asr-0.6b",))
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        with open(next(self.out.glob("*/harness_results.csv")), newline="") as f:
            self.assertEqual([r["clip_id"] for r in csv.DictReader(f)], [cid])
        self.outputs_clean()

    def test_no_key(self):
        p = self.run_script(env_key=None)
        self.assertEqual(p.returncode, 2)
        self.assertIn("OPENROUTER_API_KEY is not set", p.stderr)
        self.assertEqual(self.fake.requests, [])


if __name__ == "__main__":
    unittest.main(verbosity=2)
