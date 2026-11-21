"""Tests for tests/e2e/latency.py: no VM, no network.

    python3 -m unittest discover -s tests/e2e -p 'test_latency.py'
"""
import os
import pathlib
import subprocess
import sys
import tempfile
import unittest

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import latency  # noqa: E402


def log_line(ts: str, phase: str) -> str:
    return f"{ts}  INFO dictation_app_lib: state changed phase={phase}\n"


CYCLE = [log_line("2026-11-20T10:00:00.000000Z", "Recording"),
         log_line("2026-11-20T10:00:01.000000Z", "Transcribing"),
         log_line("2026-11-20T10:00:01.050000Z", "Inserting"),
         log_line("2026-11-20T10:00:01.120000Z", "Idle")]


class LatencyTests(unittest.TestCase):
    def test_one_dictation_is_transcribing_to_idle(self):
        self.assertEqual(latency.latencies_ms(CYCLE), [120.0])

    def test_other_lines_are_ignored(self):
        lines = ["2026-11-20T10:00:00Z  INFO dictation_app_lib: ready hotkey=RightAlt\n", "junk\n"] + CYCLE
        self.assertEqual(latency.latencies_ms(lines), [120.0])

    def test_a_cancelled_dictation_is_not_counted(self):
        cancelled = [log_line("2026-11-20T10:00:00.0Z", "Recording"), log_line("2026-11-20T10:00:01.0Z", "Transcribing"),
                     log_line("2026-11-20T10:00:02.0Z", "Recording")]  # a new press before Idle
        self.assertEqual(latency.latencies_ms(cancelled + CYCLE), [120.0])

    def test_idle_without_a_key_up_is_not_counted(self):
        self.assertEqual(latency.latencies_ms([log_line("2026-11-20T10:00:00Z", "Idle")]), [])

    def test_timezone_offsets_cancel_out(self):
        lines = [log_line("2026-11-20T02:00:01.000+08:00", "Transcribing"),
                 log_line("2026-11-20T02:00:01.250+08:00", "Idle")]
        self.assertEqual(latency.latencies_ms(lines), [250.0])

    def test_percentiles_use_nearest_rank(self):
        v = [float(i) for i in range(1, 21)]  # 1..20
        self.assertEqual(latency.percentile(v, 50), 10.0)
        self.assertEqual(latency.percentile(v, 95), 19.0)
        self.assertEqual(latency.percentile([5.0], 95), 5.0)

    def test_summary_line(self):
        self.assertEqual(latency.summary_line([100.0, 200.0, 300.0]),
                         "LATENCY  key-up to paste, 3 dictations (mock engine): p50 200 ms, p95 300 ms")

    def run_cli(self, text: str, *args: str) -> subprocess.CompletedProcess:
        with tempfile.TemporaryDirectory() as d:
            path = os.path.join(d, "app.log")
            pathlib.Path(path).write_text(text)
            return subprocess.run([sys.executable, str(HERE / "latency.py"), path, *args],
                                  capture_output=True, text=True)

    def test_cli_prints_the_line_and_writes_the_csv(self):
        with tempfile.TemporaryDirectory() as d:
            csv_path = os.path.join(d, "l.csv")
            p = self.run_cli("".join(CYCLE * 2), "--csv", csv_path, "--expect", "2", "--p95-under", "5000")
            self.assertEqual(p.returncode, 0, p.stderr)
            self.assertIn("2 dictations", p.stdout)
            self.assertEqual(pathlib.Path(csv_path).read_text(), "n,ms\n1,120.0\n2,120.0\n")

    def test_cli_fails_on_an_empty_log(self):
        self.assertEqual(self.run_cli("nothing here\n").returncode, 1)

    def test_cli_fails_on_a_wrong_count_or_a_slow_p95(self):
        self.assertEqual(self.run_cli("".join(CYCLE), "--expect", "2").returncode, 1)
        self.assertEqual(self.run_cli("".join(CYCLE), "--p95-under", "100").returncode, 1)


if __name__ == "__main__":
    unittest.main()
