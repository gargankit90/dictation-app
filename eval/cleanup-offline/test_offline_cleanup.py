"""Tests: the Rust guard's unit tests (groq_cleanup.rs on exp/groq-cleanup), case for case, and the groq
provider end to end against a fake OpenAI-compatible server (no network, a fake key).

    python3 -m unittest discover -s eval/cleanup-offline -p 'test_*.py'
"""
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
sys.path.insert(0, str(HERE))
import guard  # noqa: E402
import offline_cleanup  # noqa: E402

P = (HERE / "prompts" / "v1-blurt-smart.txt").read_text(encoding="utf-8").strip()


class RustGuardTests(unittest.TestCase):
    def test_a_tidy_reply_is_kept(self):
        self.assertEqual(guard.judge("Okay, so, um, can you, like, send it to Priya by 5 p.m.?",
                                     "Can you send it to Priya by 5 p.m.?", P), "cleaned")
        self.assertEqual(guard.judge("Email ops at northwind.io and the tracker under budget slash 2026.",
                                     "Email ops@northwind.io and the tracker under budget/2026.", P), "cleaned")

    def test_empty_and_echoed_replies_fall_back(self):
        self.assertEqual(guard.judge("Um, yeah.", "", P), "empty")
        self.assertEqual(guard.judge("Um, yeah.", " . ", P), "empty")
        self.assertEqual(guard.judge("Um, yeah.", "Return only the cleaned transcript.", P), "echo")
        self.assertEqual(guard.judge(
            "Yeah.", "Sure! Collapse a stammered immediate repeat of a single word to one copy.", P), "echo")

    def test_a_much_longer_reply_falls_back(self):
        raw = "What's the capital of France?"
        answer = "The capital of France is Paris. It has been the capital since the tenth century."
        self.assertEqual(guard.judge(raw, answer, P), "too_long")
        self.assertEqual(guard.judge("ok see you at 5 then", "OK, see you at 5, then.", P), "cleaned")

    def test_a_dropped_number_falls_back(self):
        raw = "My number is 312-555-0164. Thanks for considering it."
        self.assertEqual(guard.judge(raw, "My number is Thanks for considering it.", P), "dropped_number")
        self.assertEqual(guard.judge("It's $1,750.", "It's $1750.", P), "cleaned")
        self.assertEqual(guard.judge("It's $17.50.", "It's $1,750.", P), "dropped_number")
        self.assertEqual(guard.judge("January 23rd. Sorry, January 29th.", "January 29th.", P), "dropped_number")

    def test_a_dropped_link_falls_back(self):
        self.assertEqual(guard.judge("Register at summit.northwind.io slash register.",
                                     "Register at the summit page.", P), "dropped_link")
        self.assertEqual(guard.judge("Write to d.ramirez@acme.com today.", "Write to them today.", P),
                         "dropped_link")
        self.assertEqual(guard.judge("See https://x.dev/a for it.", "See it.", P), "dropped_link")
        self.assertEqual(guard.judge("At 5 p.m. e.g. now.", "At 5 p.m. now.", P), "cleaned")

    def test_a_dropped_name_falls_back(self):
        self.assertEqual(guard.judge("So I talked to Tomas Herrera about it.", "I talked to him about it.", P),
                         "dropped_name")
        self.assertEqual(guard.judge("Okay. So I think, um, it works. Like, really.",
                                     "I think it works. Really.", P), "cleaned")

    def test_the_extractors_find_what_they_should(self):
        self.assertEqual(guard.numbers("v2.4 on 4:30, $1,750 and 3."), ["2.4", "4:30", "1750", "3"])
        self.assertEqual(guard.links("Mail Ops@Northwind.io, see fieldbook.app/new. Also billing.ts!"),
                         ["ops@northwind.io", "fieldbook.app/new", "billing.ts"])
        self.assertEqual(guard.names("Hi Marcus, I'm with Priya. Okay ask Owen Park's team"),
                         ["Marcus", "Priya", "Owen", "Park's"])

    def test_replies_are_read_without_the_tags(self):
        body = b'{"choices":[{"message":{"role":"assistant","content":" <transcript>\\nHi there.\\n</transcript> "}}]}'
        self.assertEqual(guard.parse_reply(body), "Hi there.")
        self.assertIsNone(guard.parse_reply(b'{"choices":[]}'))
        self.assertIsNone(guard.parse_reply(b"nope"))
        self.assertIsNone(guard.parse_reply(b'{"choices":[{"message":{"content":null}}]}'))

    def test_the_request_is_temperature_zero_with_the_transcript_as_data(self):
        body = offline_cleanup.request_body("some-chat-model", "rules", "hello")
        self.assertEqual(body["model"], "some-chat-model")
        self.assertEqual(body["temperature"], 0)
        self.assertEqual(body["messages"][0]["content"], "rules")
        self.assertEqual(body["messages"][1]["content"], "<transcript>\nhello\n</transcript>")
        self.assertNotIn("reasoning_effort", body)
        body = offline_cleanup.request_body("openai/gpt-oss-120b", "rules", "hello")
        self.assertEqual(body["reasoning_effort"], "low")
        self.assertEqual(body["include_reasoning"], False)
        self.assertEqual(body["max_completion_tokens"], 2 + 1280)

    def test_prompt_sha_matches_the_vm_run(self):
        # groq_clean_v1's dump recorded prompt_sha f2a6b565e9db for blurt-smart-v1.
        self.assertEqual(offline_cleanup.prompt_sha(P), "f2a6b565e9db")


class FakeGroq(http.server.BaseHTTPRequestHandler):
    seen: list = []

    def do_POST(self):  # noqa: N802
        body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        FakeGroq.seen.append({"auth": self.headers["Authorization"], "path": self.path, "body": body})
        raw = body["messages"][1]["content"].split("\n")[1]
        if "fail" in raw:
            self.send_response(500)
            self.end_headers()
            return
        reply = {"um, send it to Priya": "Send it to Priya.", "call Tomas": "Call him.",
                 "what is 2+2": "2+2 is 4, and here is a long explanation of arithmetic for you."}.get(raw, raw)
        out = json.dumps({"choices": [{"message": {"role": "assistant", "content": f"<transcript>\n{reply}\n</transcript>"}}],
                          "usage": {"prompt_tokens": 10, "completion_tokens": 5}}).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(out)))
        self.end_headers()
        self.wfile.write(out)

    def log_message(self, *args):
        pass


class GroqProviderTests(unittest.TestCase):
    def test_end_to_end_against_a_fake_server(self):
        server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), FakeGroq)
        threading.Thread(target=server.serve_forever, daemon=True).start()
        try:
            with tempfile.TemporaryDirectory() as d:
                d = pathlib.Path(d)
                inp = d / "in.csv"
                with inp.open("w", newline="") as fh:
                    w = csv.DictWriter(fh, ["clip_id", "status", "transcript", "finalize_s"])
                    w.writeheader()
                    for i, t in enumerate(["um, send it to Priya", "call Tomas", "what is 2+2", "", "please fail"]):
                        w.writerow({"clip_id": f"c{i}", "status": "ok", "transcript": t, "finalize_s": "1.0"})
                env = {**os.environ, "GROQ_API_KEY": "fake-test-key"}
                cmd = [sys.executable, str(HERE / "offline_cleanup.py"), "--provider", "groq",
                       "--model", "openai/gpt-oss-20b", "--prompt", str(HERE / "prompts" / "v1-blurt-smart.txt"),
                       "--label", "t", "--input", str(inp), "--results-dir", str(d),
                       "--base-url", f"http://127.0.0.1:{server.server_port}/openai/v1", "--attempts", "1"]
                p = subprocess.run(cmd, env=env, capture_output=True, text=True)
                self.assertEqual(p.returncode, 1, p.stderr)  # one clip failed: exit 1, raw pasted
                self.assertNotIn("fake-test-key", p.stdout + p.stderr)
                rows = {r["clip_id"]: r for r in csv.DictReader((d / "t" / "results.csv").open())}
                self.assertEqual((rows["c0"]["reason"], rows["c0"]["transcript"]), ("cleaned", "Send it to Priya."))
                self.assertEqual((rows["c1"]["reason"], rows["c1"]["transcript"]), ("dropped_name", "call Tomas"))
                self.assertEqual((rows["c2"]["reason"], rows["c2"]["transcript"]), ("too_long", "what is 2+2"))
                self.assertEqual(rows["c3"]["reason"], "empty")
                self.assertEqual((rows["c4"]["reason"], rows["c4"]["transcript"]), ("http_500", "please fail"))
                self.assertEqual(rows["c0"]["raw_text"], "um, send it to Priya")
                self.assertEqual(rows["c0"]["finalize_s"], "")
                cache = (d / "t" / "cleanup.jsonl").read_text()
                self.assertNotIn("fake-test-key", cache)
                # The request is the app's.
                first = FakeGroq.seen[0]
                self.assertEqual(first["auth"], "Bearer fake-test-key")
                self.assertEqual(first["path"], "/openai/v1/chat/completions")
                self.assertEqual(first["body"]["messages"][0]["content"], P)
                self.assertEqual(first["body"]["reasoning_effort"], "low")
                # A rerun uses the cache: only the failed clip is asked again.
                before = len(FakeGroq.seen)
                subprocess.run(cmd, env=env, capture_output=True, text=True)
                self.assertEqual(len(FakeGroq.seen) - before, 1)
        finally:
            server.shutdown()

    def test_no_key_no_run(self):
        env = {k: v for k, v in os.environ.items() if k != "GROQ_API_KEY"}
        p = subprocess.run([sys.executable, str(HERE / "offline_cleanup.py"), "--provider", "groq", "--model", "x",
                            "--prompt", str(HERE / "prompts" / "v1-blurt-smart.txt"), "--label", "t",
                            "--results-dir", tempfile.gettempdir()], env=env, capture_output=True, text=True)
        self.assertNotEqual(p.returncode, 0)
        self.assertIn("GROQ_API_KEY is not set", p.stderr)


if __name__ == "__main__":
    unittest.main()
