"""closed_asr.py (Gemini + OpenAI) and the OpenAI-transcript cleanup cells, against fake servers.

    python3 -m unittest eval/voxtral-omni/test_closed_asr.py      (needs uv for closed_asr.py's deps)

Checks: each key reaches only its own header (x-goog-api-key / Authorization) and is never printed;
error bodies are never printed; 429 is retried; a model rate-limited on every attempt stops after 3
clips; HTTP 400 stops a model; 401 stops its provider; a provider without a key is skipped; the
guard judges one-pass replies against the plain transcript; the cache makes a rerun send nothing;
each ledger ends equal to the price computed from usage and stops requests before the cap; the
OpenAI transcripts feed run_matrix.py's cleanup cells.
"""
from __future__ import annotations

import csv
import http.server
import json
import os
import pathlib
import subprocess
import sys
import tempfile
import threading
import unittest

HERE = pathlib.Path(__file__).resolve().parent
GKEY = "AIza-test-GEMINI-DO-NOT-PRINT-0123"
OKEY = "sk-test-OPENAI-DO-NOT-PRINT-4567"
SECRET_BODY = "ERROR-BODY-MUST-NOT-BE-LOGGED"
PLAIN = "Um, send 42 copies to Alice by Friday. Thank you."
CLEAN = "Send 42 copies to Alice by Friday."


class Fake(http.server.BaseHTTPRequestHandler):
    calls: list[dict] = []
    seen: set = set()
    lock = threading.Lock()
    flash_400 = False

    def log_message(self, *a):
        pass

    def reply(self, code, body, headers=None):
        data = (json.dumps(body) if isinstance(body, dict) else body).encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        for k, v in (headers or {}).items():
            self.send_header(k, v)
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def do_POST(self):
        raw = self.rfile.read(int(self.headers["Content-Length"]))
        if self.path.endswith("/interactions"):
            return self.gemini(json.loads(raw))
        if self.path.endswith("/audio/transcriptions"):
            return self.openai(raw)
        return self.reply(404, SECRET_BODY)

    def gemini(self, body):
        key = self.headers.get("x-goog-api-key")
        model = body["model"]
        gc = body["generation_config"]
        tc = gc.get("transcription_config")
        mode = tc["mode"] if tc else ("plain" if body["system_instruction"].startswith("You are a speech") else "onepass")
        audio = body["input"][0]
        sig = (model, mode, len(audio["data"]))
        with self.lock:
            Fake.calls.append({"provider": "gemini", "key": key, "auth": self.headers.get("Authorization"),
                               "model": model, "mode": mode, "thinking": gc.get("thinking_level"),
                               "mime": audio.get("mime_type"), "tc": tc,
                               "revision": self.headers.get("Api-Revision")})
            first = sig not in Fake.seen
            Fake.seen.add(sig)
        if key != GKEY:
            return self.reply(401, {"error": {"code": 401, "message": SECRET_BODY, "status": "UNAUTHENTICATED"}})
        if model == "gemini-3.8-flash":
            if Fake.flash_400:
                return self.reply(400, {"error": {"code": 400, "message": SECRET_BODY, "status": "INVALID_ARGUMENT"}})
            return self.reply(429, {"error": {"code": 429, "message": SECRET_BODY, "status": "RESOURCE_EXHAUSTED"}},
                              {"Retry-After": "0"})
        if first and model == "gemini-3.5-flash-lite":
            return self.reply(429, {"error": {"code": 429, "message": SECRET_BODY, "status": "RESOURCE_EXHAUSTED"}},
                              {"Retry-After": "0"})
        if mode in ("verbatim", "plain"):
            text = PLAIN
        elif model == "gemini-3.5-transcribe":
            text = "Send copies to Alice by Friday."  # smart mode drops "42": the guard must refuse it
        else:
            text = CLEAN
        self.reply(200, {"id": "v1_x", "status": "completed", "model": model,
                         "steps": [{"type": "thought", "content": []},
                                   {"type": "model_output", "content": [{"type": "text", "text": text}]}],
                         "usage": {"total_input_tokens": 1000, "total_output_tokens": 20, "total_thought_tokens": 30,
                                   "input_tokens_by_modality": [{"modality": "audio", "tokens": 700},
                                                                {"modality": "text", "tokens": 300}]}})

    def openai(self, raw: bytes):
        auth = self.headers.get("Authorization")
        fields = {}
        for part in raw.split(b"--")[1:]:
            if b'name="' in part:
                name = part.split(b'name="', 1)[1].split(b'"', 1)[0].decode()
                if name != "file":
                    fields[name] = part.split(b"\r\n\r\n", 1)[1].rsplit(b"\r\n", 1)[0].decode()
                else:
                    fields["file_wav"] = b"RIFF" in part
        with self.lock:
            Fake.calls.append({"provider": "openai", "auth": auth, "key": self.headers.get("x-goog-api-key"),
                               "model": fields.get("model"), "fields": fields,
                               "ctype": self.headers.get("Content-Type", "")})
        if auth != f"Bearer {OKEY}":
            return self.reply(401, SECRET_BODY)
        usage = ({"type": "tokens", "input_tokens": 400, "input_token_details": {"audio_tokens": 400},
                  "output_tokens": 20, "total_tokens": 420} if "mini" in fields["model"] else
                 {"type": "duration", "seconds": 30})
        self.reply(200, {"text": PLAIN, "usage": usage})


def ledger_usd(path: pathlib.Path) -> float:
    return sum(json.loads(l)["microdollars"] for l in path.read_text().splitlines() if l.strip()) / 1e6


class Closed(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Fake)
        threading.Thread(target=cls.server.serve_forever, daemon=True).start()
        cls.base = f"http://127.0.0.1:{cls.server.server_port}"

    @classmethod
    def tearDownClass(cls):
        cls.server.shutdown()

    def setUp(self):
        Fake.calls.clear()
        Fake.seen.clear()
        Fake.flash_400 = False
        self.tmp = pathlib.Path(tempfile.mkdtemp())

    def run_c(self, *extra, gkey=GKEY, okey=OKEY):
        env = {k: v for k, v in os.environ.items() if k not in ("GEMINI_API_KEY", "OPENAI_API_KEY")}
        if gkey:
            env["GEMINI_API_KEY"] = gkey
        if okey:
            env["OPENAI_API_KEY"] = okey
        return subprocess.run(["uv", "run", "--quiet", str(HERE / "closed_asr.py"), "--gemini-base", self.base,
                               "--openai-base", self.base, "--limit", "3", "--out", str(self.tmp),
                               "--backoff", "0.01", *extra], capture_output=True, text=True, env=env, timeout=300)

    def assertClean(self, p, *keys):
        for stream in (p.stdout, p.stderr):
            for k in (GKEY, OKEY, SECRET_BODY, *keys):
                self.assertNotIn(k, stream)

    def rows(self, label):
        with open(self.tmp / label / "harness_results.csv", newline="") as f:
            return list(csv.DictReader(f))

    def test_all_models(self):
        p = self.run_c()
        self.assertClean(p)
        self.assertEqual(p.returncode, 1, p.stdout + p.stderr)  # gemini_flash never succeeds
        g = [c for c in Fake.calls if c["provider"] == "gemini"]
        o = [c for c in Fake.calls if c["provider"] == "openai"]
        self.assertTrue(all(c["key"] == GKEY and c["auth"] is None for c in g))
        self.assertTrue(all(c["auth"] == f"Bearer {OKEY}" and c["key"] is None for c in o))
        self.assertTrue(all(c["mime"] == "audio/wav" and c["revision"] for c in g))
        # Gemini Transcribe: verbatim + smart, English, no thinking level; smart drops "42" -> guard refuses.
        gt = [c for c in g if c["model"] == "gemini-3.5-transcribe"]
        self.assertEqual(sorted(c["mode"] for c in gt), ["smart"] * 3 + ["verbatim"] * 3)
        self.assertTrue(all(c["tc"]["language_codes"] == ["en"] and c["thinking"] is None for c in gt))
        self.assertEqual(self.rows("gemini_transcribe_plain")[0]["transcript"], PLAIN)
        self.assertEqual(self.rows("gemini_transcribe_plain")[0]["provider"], "Google (closed model)")
        gg = self.rows("gemini_transcribe_onepass_guard")
        self.assertEqual({r["reason"] for r in gg}, {"dropped_number"})
        self.assertEqual(gg[0]["transcript"], PLAIN)
        # Flash-Lite: one 429 per request then OK; thinking minimal; guard accepts the cleaned reply.
        fl = [c for c in g if c["model"] == "gemini-3.5-flash-lite"]
        self.assertEqual(len(fl), 12)
        self.assertEqual({c["thinking"] for c in fl}, {"minimal"})
        fg = self.rows("gemini_flash_lite_onepass_guard")
        self.assertEqual({r["reason"] for r in fg}, {"cleaned"})
        self.assertEqual(fg[0]["transcript"], CLEAN)
        # Gemini Flash: 429 on every attempt -> stops after 3 clips; thinking low.
        self.assertIn("gemini_flash: stopped: HTTP 429 on 3 clips", p.stdout)
        self.assertEqual(len([c for c in g if c["model"] == "gemini-3.8-flash"]), 3 * 6)
        self.assertEqual({c["thinking"] for c in g if c["model"] == "gemini-3.8-flash"}, {"low"})
        # OpenAI: plain only, WAV upload, gpt-transcribe gets languages[], mini gets language.
        self.assertEqual(len(o), 6)
        self.assertTrue(all(c["fields"]["file_wav"] and c["ctype"].startswith("multipart/form-data") for c in o))
        self.assertEqual({c["fields"].get("languages[]") for c in o if c["model"] == "gpt-transcribe"}, {"en"})
        self.assertEqual({c["fields"].get("language") for c in o if "mini" in c["model"]}, {"en"})
        self.assertFalse((self.tmp / "openai_transcribe_onepass").exists())
        otr = self.rows("openai_transcribe_plain")
        self.assertEqual([r["status"] for r in otr], ["ok"] * 3)
        self.assertEqual(otr[0]["provider"], "OpenAI (closed model)")
        # Ledgers = prices from usage (Gemini: 1000 in x price + 50 out x price per call).
        gl = 6 * (1000 * 2.0e-6 + 50 * 12e-6) + 6 * (1000 * 0.30e-6 + 50 * 2.5e-6)
        self.assertAlmostEqual(ledger_usd(self.tmp / "budget_gemini.jsonl"), gl, places=5)
        ol = 3 * 30 / 60 * 0.0045 + 3 * (400 * 1.25e-6 + 20 * 5e-6)
        self.assertAlmostEqual(ledger_usd(self.tmp / "budget_openai.jsonl"), ol, places=5)
        self.assertAlmostEqual(sum(float(r["cost_usd"]) for r in otr), 3 * 30 / 60 * 0.0045, places=6)
        # Rerun: everything that succeeded is cached.
        Fake.calls.clear()
        p = self.run_c("--model", "gemini_transcribe", "--model", "gemini_flash_lite", "--model", "openai_transcribe",
                       "--model", "openai_mini")
        self.assertClean(p)
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertEqual(Fake.calls, [])

    def test_400_stops_the_model_and_logs_only_the_status(self):
        Fake.flash_400 = True
        p = self.run_c("--model", "gemini_flash")
        self.assertClean(p)
        self.assertIn("HTTP 400 INVALID_ARGUMENT", p.stdout)
        self.assertEqual(len(Fake.calls), 1)

    def test_missing_key_skips_the_provider(self):
        p = self.run_c("--model", "gemini_transcribe", "--model", "openai_mini", okey=None)
        self.assertClean(p)
        self.assertIn("OPENAI_API_KEY is not set", p.stdout)
        self.assertEqual({c["provider"] for c in Fake.calls}, {"gemini"})

    def test_bad_key_stops_the_provider(self):
        p = self.run_c("--model", "gemini_transcribe", "--model", "gemini_flash_lite", "--model", "openai_mini",
                       gkey="AIza-wrong-key-also-secret")
        self.assertClean(p, "AIza-wrong-key-also-secret")
        self.assertIn("HTTP 401 UNAUTHENTICATED", p.stdout)
        self.assertLessEqual(len([c for c in Fake.calls if c["provider"] == "gemini"]), 2)
        self.assertEqual(len([c for c in Fake.calls if c["provider"] == "openai"]), 3)

    def test_stops_before_cap(self):
        (self.tmp / "budget_gemini.jsonl").write_text(json.dumps({"microdollars": 2_999_990, "tag": "x"}) + "\n")
        p = self.run_c("--model", "gemini_flash_lite")
        self.assertClean(p)
        self.assertEqual(Fake.calls, [])
        self.assertIn("would pass the $3.00 cap", p.stdout)

    def test_estimate_sends_nothing(self):
        p = self.run_c("--estimate", gkey=None, okey=None)
        self.assertEqual(p.returncode, 0, p.stderr)
        self.assertEqual(Fake.calls, [])
        self.assertIn("gemini_transcribe", p.stdout)


class OpenAICleanup(unittest.TestCase):
    """run_matrix.py cleans the OpenAI transcripts (EXTRA cells) through the OpenRouter path."""

    def test_extra_cells_read_the_closed_results(self):
        sys.path.insert(0, str(HERE))
        import run_matrix
        tmp = pathlib.Path(tempfile.mkdtemp())
        src = HERE / "input" / "nvidia__nemotron-3.5-transcribe" / "harness_results.csv"
        with open(src, newline="", encoding="utf-8") as f:
            rows = list(csv.DictReader(f))
        (tmp / "openai_transcribe_plain").mkdir()
        with open(tmp / "openai_transcribe_plain" / "harness_results.csv", "w", newline="", encoding="utf-8") as f:
            w = csv.DictWriter(f, list(rows[0]))  # closed_asr.py writes the same harness columns
            w.writeheader()
            for r in rows:
                w.writerow({**r, "model": "gpt-transcribe"})
        self.assertEqual(run_matrix.load_source("openai_transcribe", tmp),
                         tmp / "openai_transcribe_plain" / "harness_results.csv")
        self.assertNotIn(("openai_transcribe", "luna"), run_matrix.PLAN)
        p = subprocess.run([sys.executable, str(HERE / "run_matrix.py"), "--plan"], capture_output=True, text=True)
        self.assertNotIn("openai_transcribe", p.stdout)
        # The cell runs through the OpenRouter cleanup path (test_runners' fake OpenRouter).
        import test_runners
        srv = http.server.ThreadingHTTPServer(("127.0.0.1", 0), test_runners.Fake)
        threading.Thread(target=srv.serve_forever, daemon=True).start()
        try:
            test_runners.Fake.calls.clear()
            p = subprocess.run([sys.executable, str(HERE / "run_matrix.py"), "--base-url",
                                f"http://127.0.0.1:{srv.server_port}", "--results-dir", str(tmp),
                                "--only", "openai_transcribe__luna_v4"], capture_output=True, text=True,
                               env={**os.environ, "OPENROUTER_API_KEY": test_runners.KEY}, timeout=300)
        finally:
            srv.shutdown()
            srv.server_close()
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertNotIn(test_runners.KEY, p.stdout + p.stderr)
        self.assertEqual({c["model"] for c in test_runners.Fake.calls}, {"openai/gpt-6-luna"})
        with open(tmp / "openai_transcribe__luna_v4" / "results.csv", newline="") as f:
            self.assertEqual(len(list(csv.DictReader(f))), 151)


if __name__ == "__main__":
    unittest.main()
