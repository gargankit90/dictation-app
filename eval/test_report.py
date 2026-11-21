"""Tests for eval/report.py, eval/evalcommon.py and eval/regress/: the numbers, the split, the dry run.
No network, no key, no CLI.
"""
import contextlib
import csv
import io
import hashlib
import json
import os
import pathlib
import subprocess
import sys
import tempfile
import unittest

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
sys.path.insert(0, str(HERE / "regress"))
import evalcommon  # noqa: E402
import report  # noqa: E402
import regress  # noqa: E402


def scored(rows):
    return [{"clip_id": c, "edits": str(e), "chars": str(ch)} for c, e, ch in rows]


class ReportTests(unittest.TestCase):
    def test_zer_and_amend_are_the_whitepaper_definitions(self):
        z, a = report.zer_amend(scored([("a", 0, 100), ("b", 5, 100), ("c", 0, 100), ("d", 1, 100)]))
        self.assertEqual(z, 50.0)
        self.assertAlmostEqual(a, 1.5)  # pooled: 6 edits / 400 characters

    def test_unjudged_rows_are_skipped(self):
        rows = scored([("a", 0, 10)]) + [{"clip_id": "b", "edits": "", "chars": ""}]
        self.assertEqual(report.zer_amend(rows)[0], 100.0)

    def test_percentiles_nearest_rank(self):
        self.assertEqual(report.percentile([float(i) for i in range(1, 101)], 95), 95.0)
        self.assertEqual(report.percentile([3.0, 1.0, 2.0], 50), 2.0)
        self.assertIsNone(report.percentile([], 50))

    def test_paired_wins_losses_ties(self):
        base = {r["clip_id"]: r for r in scored([("a", 2, 9), ("b", 0, 9), ("c", 0, 9), ("d", 3, 9)])}
        run = {r["clip_id"]: r for r in scored([("a", 0, 9), ("b", 4, 9), ("c", 0, 9), ("d", 1, 9)])}
        self.assertEqual(report.paired(run, base), (1, 1, 2))  # a won, b lost; c and d tied (d still wrong)

    def test_cost_and_latency_come_from_the_engine_files_when_present(self):
        with tempfile.TemporaryDirectory() as d:
            d = pathlib.Path(d)
            with open(d / "results.csv", "w", newline="") as fh:
                w = csv.DictWriter(fh, ["clip_id", "cost_usd", "finalize_s"])
                w.writeheader()
                for i in range(1, 21):
                    w.writerow({"clip_id": f"c{i}", "cost_usd": "0.0005", "finalize_s": str(i / 10)})
            (d / "cleanup.jsonl").write_text("".join(json.dumps({"usage": {"cost": 0.0001}}) + "\n" for _ in range(20)))
            out = report.cost_latency(d, 20)
            self.assertAlmostEqual(out["cost_per_1000_usd"], 0.6)
            self.assertEqual((out["latency_p50_s"], out["latency_p95_s"]), (1.0, 1.9))
            self.assertEqual(report.cost_latency(d / "missing", 20)["cost_per_1000_usd"], None)

    def test_the_committed_summary_holds_the_published_numbers(self):
        # PR #323 / #332: Groq raw ZER 60.3, gpt-4o-mini-transcribe + GLM Flash ZER 88.1, on the Claude judge.
        with open(HERE / "summary.csv", newline="") as fh:
            rows = {(r["group"], r["run"], r["judge"]): r for r in csv.DictReader(fh)}
        self.assertEqual(rows["voicearena", "vocet_groq", "claude"]["zer"], "60.3")
        self.assertEqual(rows["voxtral-omni", "gpt4o_mini__glm_v4", "claude"]["zer"], "88.1")
        self.assertEqual(rows["voicearena", "vocet_groq", "claude"]["clips"], "151")

    def test_a_scored_csv_with_both_halves_summarizes(self):
        with tempfile.TemporaryDirectory() as d:
            d = pathlib.Path(d)
            split = evalcommon.load_split()
            with open(d / "scored.csv", "w", newline="") as fh:
                w = csv.DictWriter(fh, ["clip_id", "edits", "chars"])
                w.writeheader()
                for i, cid in enumerate(sorted(split)):
                    w.writerow({"clip_id": cid, "edits": i % 2, "chars": 100})
            s = report.summarize(d, "claude", split, None)
            self.assertEqual(s["clips"], 151)
            self.assertAlmostEqual(s["zer"], 100 * 76 / 151)
            self.assertIn("ZER tune", report.table([s]))
            out = subprocess.run([sys.executable, str(HERE / "report.py"), str(d), "--format", "csv"],
                                 capture_output=True, text=True)
            self.assertEqual(out.returncode, 0, out.stderr)
            row = next(csv.DictReader(out.stdout.splitlines()))
            self.assertEqual((row["clips"], row["cost_per_1000_usd"]), ("151", ""))


class SplitTests(unittest.TestCase):
    def test_tune_is_an_even_first_hex_digit_of_sha256_of_the_speaker(self):
        for sid in ("37861", "74259", "40358", "10007"):
            digit = int(hashlib.sha256(sid.encode()).hexdigest()[0], 16)
            self.assertEqual(evalcommon.half_of(sid), "tune" if digit % 2 == 0 else "check")

    def test_the_committed_split_csv_matches_the_rule(self):
        with open(HERE / "split.csv", newline="") as fh:
            rows = list(csv.DictReader(fh))
        self.assertEqual(len(rows), 151)
        self.assertTrue(all(evalcommon.half_of(r["speaker_id"]) == r["half"] for r in rows))
        self.assertEqual({r["half"] for r in rows}, {"tune", "check"})

    def test_a_speaker_is_never_on_both_sides(self):
        halves = {}
        for r in csv.DictReader(open(HERE / "split.csv", newline="")):
            halves.setdefault(r["speaker_id"], set()).add(r["half"])
        self.assertTrue(all(len(h) == 1 for h in halves.values()))


class AppPromptTests(unittest.TestCase):
    def test_the_env_var_wins_then_the_app_file_then_the_eval_copy(self):
        old = os.environ.pop("EVAL_APP_PROMPT", None)
        try:
            self.assertIn(evalcommon.app_prompt_path(), (evalcommon.APP_PROMPT, evalcommon.EVAL_PROMPT))
            if not evalcommon.APP_PROMPT.is_file():  # until Q2 lands
                self.assertEqual(evalcommon.app_prompt_path(), evalcommon.EVAL_PROMPT)
            os.environ["EVAL_APP_PROMPT"] = "/some/where.txt"
            self.assertEqual(evalcommon.app_prompt_path(), pathlib.Path("/some/where.txt"))
        finally:
            os.environ.pop("EVAL_APP_PROMPT", None)
            if old is not None:
                os.environ["EVAL_APP_PROMPT"] = old

    def test_the_eval_copy_is_prompt_v4_as_the_baseline_used(self):
        text = evalcommon.EVAL_PROMPT.read_text(encoding="utf-8")
        self.assertEqual(regress.prompt_sha(text), json.loads((HERE / "regress" / "baseline.json").read_text())["prompt_sha"])


class RegressTests(unittest.TestCase):
    def test_the_subset_is_40_clips_covering_both_halves_and_every_domain(self):
        ids = (HERE / "regress" / "clips.txt").read_text().split()
        self.assertEqual(len(ids), 40)
        self.assertEqual(len(set(ids)), 40)
        with open(HERE / "regress" / "baseline.csv", newline="") as fh:
            rows = list(csv.DictReader(fh))
        self.assertEqual([r["clip_id"] for r in rows], ids)
        self.assertEqual({r["half"] for r in rows}, {"tune", "check"})
        self.assertEqual(len({r["domain"] for r in rows}), 7)
        self.assertFalse((HERE / "regress" / "raw.csv").exists())  # drafts of the dataset's clips stay out
        self.assertEqual(set(rows[0]), {"clip_id", "domain", "speaker_id", "edits", "chars", "zero_edit", "half"})

    def test_the_baseline_summary_matches_its_clips(self):
        meta = json.loads((HERE / "regress" / "baseline.json").read_text())
        with open(HERE / "regress" / "baseline.csv", newline="") as fh:
            z, a = report.zer_amend(list(csv.DictReader(fh)))
        self.assertEqual(meta["zer_pct"], round(z, 1))
        self.assertEqual(meta["amend_pct"], round(a, 2))

    def run_cli(self, *args, env=None):
        e = {k: v for k, v in os.environ.items() if k not in ("OPENROUTER_API_KEY", "EVAL_APP_PROMPT")}
        e.update(env or {})
        return subprocess.run([str(HERE / "regress.sh"), *args], capture_output=True, text=True, env=e)

    def test_dry_run_prints_the_plan_and_the_cost_without_a_key_or_the_network(self):
        p = self.run_cli("--dry-run")
        self.assertEqual(p.returncode, 0, p.stderr)
        for want in ("40", "prompt", "sha c530a8a410f1", "z-ai/glm-5.3-flash", "estimated cost", "baseline", "NOT set"):
            self.assertIn(want, p.stdout)

    def test_dry_run_never_prints_a_key(self):
        secret = "fake-" + "SECRET-1234567890"
        p = self.run_cli("--dry-run", env={"OPENROUTER_API_KEY": secret})
        self.assertEqual(p.returncode, 0)
        self.assertIn("is set", p.stdout)
        self.assertNotIn(secret, p.stdout + p.stderr)

    def test_a_real_run_without_a_key_stops_before_any_request(self):
        p = self.run_cli()
        self.assertEqual(p.returncode, 2)
        self.assertIn("OPENROUTER_API_KEY is not set", p.stderr)

    def test_dry_run_reads_the_prompt_the_env_var_names(self):
        with tempfile.TemporaryDirectory() as d:
            f = pathlib.Path(d) / "p.txt"
            f.write_text("A different prompt.")
            p = self.run_cli("--dry-run", env={"EVAL_APP_PROMPT": str(f)})
            self.assertIn(str(f), p.stdout)
            self.assertNotIn("c530a8a410f1", p.stdout.split("prompt")[1].split("\n")[0])

    def test_a_real_run_calls_scripts_that_exist_in_order(self):
        calls = []
        regress.run = lambda cmd: calls.append(cmd)
        old_argv, old_env = sys.argv, dict(os.environ)
        with tempfile.TemporaryDirectory() as d:
            work = pathlib.Path(d)
            old = (regress.WORK, regress.RAW, regress.MANIFEST, regress.CLIPS, regress.compare)
            regress.WORK, regress.CLIPS = work, work / "clips"
            regress.RAW, regress.MANIFEST = work / "raw.csv", work / "clips" / "manifest.csv"
            regress.compare = lambda *a: 0
            (work / "clips").mkdir()
            with open(regress.MANIFEST, "w", newline="") as fh:
                w = csv.writer(fh)
                w.writerow(["clip_id", "intended_text"])
                w.writerow([(HERE / "regress" / "clips.txt").read_text().split()[0], "x"])
            os.environ["OPENROUTER_API_KEY"] = "fake-" + "key"
            sys.argv = ["regress.py"]
            try:
                with contextlib.redirect_stdout(io.StringIO()):
                    self.assertEqual(regress.main(), 0)
            finally:
                sys.argv = old_argv
                os.environ.clear()
                os.environ.update(old_env)
                regress.WORK, regress.RAW, regress.MANIFEST, regress.CLIPS, regress.compare = old
        scripts = [next(a for a in c if a.endswith(".py")) for c in calls]
        self.assertEqual([pathlib.Path(s).name for s in scripts],
                         ["transcribe_openrouter.py", "offline_cleanup.py", "judge.py"])
        self.assertTrue(all(pathlib.Path(s).is_file() for s in scripts))
        self.assertIn(str(evalcommon.app_prompt_path()), calls[1])

    def compare(self, edits_by_clip, threshold="5"):
        base = list(csv.DictReader(open(HERE / "regress" / "baseline.csv", newline="")))
        with tempfile.TemporaryDirectory() as d:
            path = pathlib.Path(d) / "scored.csv"
            with open(path, "w", newline="") as fh:
                w = csv.DictWriter(fh, ["clip_id", "edits", "chars", "zero_edit"])
                w.writeheader()
                for r in base:
                    e = edits_by_clip.get(r["clip_id"], int(r["edits"]))
                    w.writerow({"clip_id": r["clip_id"], "edits": e, "chars": r["chars"], "zero_edit": int(e == 0)})
            return self.run_cli("--compare-only", str(path), "--threshold", threshold)

    def test_compare_passes_when_nothing_changed(self):
        p = self.compare({})
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertIn("PASS", p.stdout)

    def test_compare_fails_when_zer_drops_more_than_the_threshold(self):
        base = list(csv.DictReader(open(HERE / "regress" / "baseline.csv", newline="")))
        passing = [r["clip_id"] for r in base if r["zero_edit"] == "1"][:4]  # 4 of 40 = 10 points
        p = self.compare({c: 3 for c in passing})
        self.assertEqual(p.returncode, 1, p.stdout)
        self.assertIn("FAIL", p.stdout)
        self.assertIn("regressed clips", p.stdout)
        self.assertEqual(self.compare({c: 3 for c in passing}, threshold="12").returncode, 0)

    def test_compare_with_clips_missing_is_a_setup_problem(self):
        base = list(csv.DictReader(open(HERE / "regress" / "baseline.csv", newline="")))
        with tempfile.TemporaryDirectory() as d:
            path = pathlib.Path(d) / "scored.csv"
            with open(path, "w", newline="") as fh:
                w = csv.DictWriter(fh, ["clip_id", "edits", "chars", "zero_edit"])
                w.writeheader()
                for r in base[:10]:
                    w.writerow({k: r[k] for k in ("clip_id", "edits", "chars", "zero_edit")})
            self.assertEqual(self.run_cli("--compare-only", str(path)).returncode, 2)


if __name__ == "__main__":
    unittest.main()
