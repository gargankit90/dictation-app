"""Parts A and B against a fake OpenRouter server: no network, no real key.

    python3 -m unittest eval/voxtral-omni/test_runners.py      (needs uv for audio_llm.py's deps)

Checks: the key reaches only the Authorization header and is never printed; error bodies are never
printed; 429 is retried with backoff; HTTP 400 on "reasoning off" falls back once; the guard judges
one-pass replies against the plain transcript; the cache makes a rerun send nothing; the shared
ledger stops requests before the cap and ends equal to the summed usage.cost.
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
KEY = "sk-or-test-DO-NOT-PRINT-0123456789"
SECRET_BODY = "ERROR-BODY-MUST-NOT-BE-LOGGED"
COST = 0.0002


class Fake(http.server.BaseHTTPRequestHandler):
    calls: list[dict] = []
    seen_429: set = set()
    lock = threading.Lock()

    def log_message(self, *a):  # quiet
        pass

    def reply(self, code: int, body: dict | str, headers: dict | None = None):
        data = (json.dumps(body) if isinstance(body, dict) else body).encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        for k, v in (headers or {}).items():
            self.send_header(k, v)
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        auth = self.headers.get("Authorization")
        model, msgs = body["model"], body["messages"]
        user = msgs[-1]["content"]
        audio = isinstance(user, list)
        mode = "plain" if audio and msgs[0]["content"].startswith("You are a speech-to-text") else (
            "onepass" if audio else "cleanup")
        sig = (model, mode, json.dumps(body.get("reasoning")), len(json.dumps(user)) if audio else user)
        with self.lock:
            Fake.calls.append({"auth": auth, "model": model, "mode": mode, "reasoning": body.get("reasoning"),
                               "max_tokens": body.get("max_tokens"), "usage": body.get("usage")})
            first = sig not in Fake.seen_429
            Fake.seen_429.add(sig)
        if auth != f"Bearer {KEY}":
            return self.reply(401, SECRET_BODY)
        if "inkling" in model:  # always rate limited
            return self.reply(429, SECRET_BODY, {"Retry-After": "0"})
        if first and model == "mistralai/voxtral-small-24b-2507":  # one 429 per request, then OK
            return self.reply(429, SECRET_BODY, {"Retry-After": "0"})
        if "nemotron" in model and body.get("reasoning") == {"enabled": False}:
            return self.reply(400, SECRET_BODY)
        if mode == "cleanup":
            raw = user.split("\n", 1)[1].rsplit("\n", 1)[0]
            text = "<transcript>\n" + raw.replace("um ", "").replace("Um, ", "") + "\n</transcript>"
        elif mode == "plain":
            text = "Um, send 42 copies to Alice by Friday. Thank you."
        elif "mimo" in model:
            text = "Send copies to Alice by Friday."  # drops the number: the guard must refuse it
        else:
            text = "Send 42 copies to Alice by Friday."
        self.reply(200, {"choices": [{"message": {"content": text}, "finish_reason": "stop"}],
                         "provider": "FakeProvider",
                         "usage": {"prompt_tokens": 500, "completion_tokens": 12, "cost": COST,
                                   "prompt_tokens_details": {"audio_tokens": 300}}})


def ledger_usd(path: pathlib.Path) -> float:
    return sum(json.loads(l)["microdollars"] for l in path.read_text().splitlines() if l.strip()) / 1e6


class Runners(unittest.TestCase):
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
        Fake.seen_429.clear()
        self.tmp = pathlib.Path(tempfile.mkdtemp())
        self.env = {**os.environ, "OPENROUTER_API_KEY": KEY}

    def run_b(self, *extra, key=KEY):
        env = {**self.env, "OPENROUTER_API_KEY": key}
        return subprocess.run(["uv", "run", "--quiet", str(HERE / "audio_llm.py"), "--base-url", self.base,
                               "--limit", "3", "--out", str(self.tmp), "--ledger", str(self.tmp / "budget.jsonl"),
                               "--backoff", "0.01", "--interval-scale", "0.01", *extra],
                              capture_output=True, text=True, env=env, timeout=300)

    def assertClean(self, p):
        for stream in (p.stdout, p.stderr):
            self.assertNotIn(KEY, stream)
            self.assertNotIn(SECRET_BODY, stream)

    def rows(self, label):
        with open(self.tmp / label / "harness_results.csv", newline="") as f:
            return list(csv.DictReader(f))

    def test_part_b(self):
        p = self.run_b()
        self.assertClean(p)
        self.assertEqual(p.returncode, 1, p.stdout + p.stderr)  # inkling never succeeds
        self.assertTrue(all(c["auth"] == f"Bearer {KEY}" for c in Fake.calls))
        self.assertTrue(all(c["usage"] == {"include": True} for c in Fake.calls))
        # Voxtral: one 429 per request, retried, all OK; no reasoning field.
        vox = [c for c in Fake.calls if c["model"] == "mistralai/voxtral-small-24b-2507"]
        self.assertEqual(len(vox), 12)
        self.assertTrue(all(c["reasoning"] is None for c in vox))
        plain = self.rows("voxtral_small_chat_plain")
        self.assertEqual([r["status"] for r in plain], ["ok"] * 3)
        self.assertEqual(plain[0]["transcript"], "Um, send 42 copies to Alice by Friday. Thank you.")
        self.assertEqual(plain[0]["provider"], "FakeProvider")
        g = self.rows("voxtral_small_chat_onepass_guard")
        self.assertEqual({r["reason"] for r in g}, {"cleaned"})
        self.assertEqual(g[0]["transcript"], "Send 42 copies to Alice by Friday.")
        self.assertAlmostEqual(float(g[0]["cost_usd"]), 2 * COST)
        # Nemotron: 400 on reasoning off -> fallback to low/excluded with the larger token budget.
        nem = [c for c in Fake.calls if "nemotron" in c["model"]]
        self.assertEqual(nem[0]["reasoning"], {"enabled": False})
        self.assertTrue(all(c["reasoning"] == {"effort": "low", "exclude": True} for c in nem[1:]))
        self.assertEqual(sum(c["reasoning"] == {"enabled": False} for c in nem), 1)
        self.assertEqual(nem[-1]["max_tokens"], 4096)
        self.assertIn('"effort": "low"', self.rows("nemotron_omni_plain")[0]["reasoning"])
        # MiMo: the one-pass reply drops "42"; the guard pastes the plain transcript instead.
        mg = self.rows("mimo_v25_onepass_guard")
        self.assertEqual({r["reason"] for r in mg}, {"dropped_number"})
        self.assertEqual(mg[0]["transcript"], mg[0]["raw_text"])
        self.assertEqual(self.rows("mimo_v25_onepass")[0]["transcript"], "Send copies to Alice by Friday.")
        # Inkling: 429 on every attempt; stops after 3 clips in a row.
        self.assertIn("inkling_small: stopped: HTTP 429 on 3 clips", p.stdout)
        self.assertEqual(len([c for c in Fake.calls if "inkling" in c["model"]]), 3 * 6)
        # Ledger = sum of reported costs (HTTP errors released their holds).
        ok = sum(1 for d in ("voxtral_small_chat", "nemotron_omni", "mimo_v25") for _ in range(6))
        self.assertAlmostEqual(ledger_usd(self.tmp / "budget.jsonl"), ok * COST, places=6)
        # A rerun asks only what failed (inkling), with nothing else re-sent.
        Fake.calls.clear()
        p = self.run_b("--model", "voxtral_small_chat", "--model", "mimo_v25", "--model", "nemotron_omni")
        self.assertClean(p)
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertEqual(Fake.calls, [])

    def test_part_b_stops_before_cap(self):
        (self.tmp / "budget.jsonl").write_text(json.dumps({"microdollars": 2_999_990, "tag": "x"}) + "\n")
        p = self.run_b("--model", "voxtral_small_chat")
        self.assertClean(p)
        self.assertEqual(Fake.calls, [])
        self.assertIn("would pass the $3.00 cap", p.stdout)

    def test_part_b_bad_key_stops_everything(self):
        p = self.run_b("--model", "voxtral_small_chat", "--model", "mimo_v25", key="wrong-key-also-secret")
        self.assertNotIn("wrong-key-also-secret", p.stdout + p.stderr)
        self.assertNotIn(SECRET_BODY, p.stdout + p.stderr)
        self.assertIn("HTTP 401", p.stdout)
        self.assertLessEqual(len(Fake.calls), 2)

    def run_a(self, *extra):
        return subprocess.run([sys.executable, str(HERE / "run_matrix.py"), "--base-url", self.base,
                               "--results-dir", str(self.tmp), *extra], capture_output=True, text=True,
                              env=self.env, timeout=300)

    def test_part_a(self):
        p = self.run_a("--only", "oruk_dev__glm_v4", "--only", "voxtral_mini__sol_v4")
        self.assertClean(p)
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        distinct = 0
        for d in ("ondevice_oruk", "mistralai__voxtral-mini-3b-2507"):
            src = HERE / "input" / d / "harness_results.csv"
            distinct += len({r["transcript"] for r in csv.DictReader(open(src, newline=""))})
        self.assertEqual(len(Fake.calls), distinct)  # a duplicate transcript is asked once
        self.assertEqual({c["model"] for c in Fake.calls}, {"z-ai/glm-5.3-flash", "openai/gpt-6.2-sol"})
        self.assertEqual({json.dumps(c["reasoning"]) for c in Fake.calls}, {json.dumps({"effort": "low", "exclude": True})})
        self.assertEqual({json.dumps(c["usage"]) for c in Fake.calls}, {json.dumps({"include": True})})
        with open(self.tmp / "oruk_dev__glm_v4" / "results.csv", newline="") as f:
            rows = list(csv.DictReader(f))
        self.assertEqual(len(rows), 151)
        self.assertEqual({r["cleanup_model"] for r in rows}, {"z-ai/glm-5.3-flash"})
        with open(self.tmp / "oruk_dev__glm_v4" / "cleanup.jsonl") as f:
            self.assertEqual({json.loads(l)["served_by"] for l in f}, {"FakeProvider"})
        self.assertAlmostEqual(ledger_usd(self.tmp / "budget.jsonl"), distinct * COST, places=6)
        Fake.calls.clear()  # rerun: all cached
        p = self.run_a("--only", "oruk_dev__glm_v4", "--only", "voxtral_mini__sol_v4")
        self.assertEqual(p.returncode, 0)
        self.assertEqual(Fake.calls, [])

    def test_part_a_lanes_cover_the_plan_once(self):
        sys.path.insert(0, str(HERE))
        import run_matrix
        lanes = [[c for k, c in enumerate(run_matrix.PLAN) if k % 3 == i] for i in range(3)]
        self.assertEqual(sorted(sum(lanes, [])), sorted(run_matrix.PLAN))
        self.assertEqual(len(set(run_matrix.PLAN)), len(run_matrix.PLAN))
        self.assertNotIn(("nemotron", "sol"), run_matrix.PLAN)
        self.assertEqual(run_matrix.PLAN[0], ("nemotron", "glm"))

    def test_part_a_stops_before_cap(self):
        (self.tmp / "budget.jsonl").write_text(json.dumps({"microdollars": 2_999_999, "tag": "x"}) + "\n")
        p = self.run_a("--only", "nemotron__glm_v4")
        self.assertClean(p)
        self.assertNotEqual(p.returncode, 0)
        self.assertEqual(Fake.calls, [])
        self.assertIn("cap", p.stderr + p.stdout)


if __name__ == "__main__":
    unittest.main()
