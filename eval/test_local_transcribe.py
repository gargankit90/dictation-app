"""Tests for eval/asr/local_transcribe.py against a fake loopback engine. No network beyond 127.0.0.1."""
import csv
import http.server
import json
import pathlib
import subprocess
import sys
import tempfile
import threading
import unittest

HERE = pathlib.Path(__file__).resolve().parent
SCRIPT = HERE / "local_transcribe.py"


class Engine(http.server.BaseHTTPRequestHandler):
    seen = []

    def do_POST(self):
        body = self.rfile.read(int(self.headers["Content-Length"]))
        Engine.seen.append((self.path, self.headers.get("Authorization"), body))
        if b"FAIL" in body:
            self.send_response(500)
            self.end_headers()
            return
        out = json.dumps({"text": " hello from the engine "}).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(out)))
        self.end_headers()
        self.wfile.write(out)

    def log_message(self, *a):
        pass


class LocalTranscribeTests(unittest.TestCase):
    def setUp(self):
        Engine.seen = []
        self.srv = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Engine)
        threading.Thread(target=self.srv.serve_forever, daemon=True).start()
        self.dir = tempfile.TemporaryDirectory()
        self.d = pathlib.Path(self.dir.name)
        (self.d / "clips").mkdir()
        rows = [("c1", "c1.flac", "11", b"AUDIO-1"), ("c2", "c2.flac", "12", b"FAIL-AUDIO")]
        with open(self.d / "clips" / "manifest.csv", "w", newline="") as fh:
            w = csv.writer(fh)
            w.writerow(["clip_id", "file", "speaker_id"])
            for cid, f, sp, data in rows:
                w.writerow([cid, f, sp])
                (self.d / "clips" / f).write_bytes(data)

    def tearDown(self):
        self.srv.shutdown()
        self.srv.server_close()
        self.dir.cleanup()

    def run_script(self, base):
        return subprocess.run([sys.executable, str(SCRIPT), "--base-url", base, "--model", "m", "--label", "t",
                               "--clips", str(self.d / "clips"), "--out", str(self.d / "out")],
                              capture_output=True, text=True)

    def test_writes_the_harness_columns_and_marks_a_failed_clip(self):
        p = self.run_script(f"http://127.0.0.1:{self.srv.server_port}/v1")
        self.assertEqual(p.returncode, 1, p.stderr)  # one clip failed
        with open(self.d / "out" / "t" / "harness_results.csv", newline="") as fh:
            rows = {r["clip_id"]: r for r in csv.DictReader(fh)}
        self.assertEqual(rows["c1"]["status"], "ok")
        self.assertEqual(rows["c1"]["transcript"], "hello from the engine")
        self.assertGreaterEqual(float(rows["c1"]["finalize_s"]), 0)
        self.assertEqual(rows["c2"]["status"], "error")
        self.assertEqual(rows["c2"]["transcript"], "")
        path, auth, body = Engine.seen[0]
        self.assertEqual(path, "/v1/audio/transcriptions")
        self.assertIsNone(auth)  # no key is sent to a local engine
        self.assertIn(b"AUDIO-1", body)

    def test_refuses_a_non_loopback_url(self):
        p = self.run_script("https://example.com/v1")
        self.assertNotEqual(p.returncode, 0)
        self.assertIn("loopback", p.stderr)
        self.assertEqual(Engine.seen, [])

    def test_missing_manifest_asks_for_the_fetch(self):
        (self.d / "clips" / "manifest.csv").unlink()
        p = self.run_script(f"http://127.0.0.1:{self.srv.server_port}/v1")
        self.assertEqual(p.returncode, 2)
        self.assertIn("fetch_dataset.py", p.stderr)


if __name__ == "__main__":
    unittest.main()
