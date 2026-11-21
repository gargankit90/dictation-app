"""Tests for the unified judge (eval/judge.py, eval/judges.py): the flow, the cache and the CLI command lines,
with a fake judge. No network, no CLI, no amend-va.

    python3 -m unittest discover -s eval -p 'test_*.py'      (or: uv run pytest eval)
"""
import csv
import io
import json
import os
import pathlib
import sys
import tempfile
import unittest

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import judge  # noqa: E402
from judges import DEFAULT_MODEL, Judge, UsageLimit, make_judge, parse_fixed  # noqa: E402


def fake_tools():
    def char_edits(draft, fixed):
        return sum(1 for a, b in zip(draft, fixed) if a != b) + abs(len(draft) - len(fixed)), max(len(fixed), 1)
    return (lambda intended, draft, spoken, version: f"{version}|{intended}|{draft}", char_edits, "fp0123456789", "9.9.9")


class FakeJudge(Judge):
    name, model, effort, label = "fake", "fake-model", "max", "unofficial: fake"

    def __init__(self, answers=None, limit_after=None):
        self.calls, self.answers, self.limit_after = [], answers or {}, limit_after

    def ask(self, prompt, intended, workdir):
        if self.limit_after is not None and len(self.calls) >= self.limit_after:
            raise UsageLimit("quota")
        self.calls.append(prompt)
        return {"fixed": self.answers.get(prompt, intended)}


def write_csv(path, rows):
    with open(path, "w", newline="", encoding="utf-8") as fh:
        w = csv.DictWriter(fh, list(rows[0]))
        w.writeheader()
        w.writerows(rows)


class JudgeRunTests(unittest.TestCase):
    def setUp(self):
        self.dir = tempfile.TemporaryDirectory()
        self.d = pathlib.Path(self.dir.name)
        self.manifest = self.d / "manifest.csv"
        self.results = self.d / "results.csv"
        write_csv(self.manifest, [
            {"clip_id": "c1", "domain": "email", "speaker_id": "1", "audio_length_s": "3", "intended_text": "Hello Bob.", "spoken_text": "hello bob"},
            {"clip_id": "c2", "domain": "code", "speaker_id": "2", "audio_length_s": "4", "intended_text": "Run make.", "spoken_text": "run make"},
            {"clip_id": "c3", "domain": "code", "speaker_id": "2", "audio_length_s": "5", "intended_text": "Done.", "spoken_text": "done"}])
        write_csv(self.results, [
            {"clip_id": "c1", "status": "ok", "transcript": "Hello Bob.", "finalize_s": "0.5"},
            {"clip_id": "c2", "status": "ok", "transcript": "Run bake.", "finalize_s": "0.7"},
            {"clip_id": "c3", "status": "error", "transcript": "", "finalize_s": ""}])
        self.out = self.d / "scored.csv"
        self.cache = self.d / "cache.jsonl"

    def tearDown(self):
        self.dir.cleanup()

    def run_judge(self, j, **kw):
        return judge.judge_run(j, self.results, self.manifest, self.out, "eng", tools=fake_tools(),
                               cache_file=self.cache, backoff=0, log=io.StringIO(), **kw)

    def rows(self):
        with open(self.out, newline="", encoding="utf-8") as fh:
            return {r["clip_id"]: r for r in csv.DictReader(fh)}

    def test_scores_every_clip_and_leaves_out_the_references(self):
        j = FakeJudge()
        self.assertEqual(self.run_judge(j), 0)
        rows = self.rows()
        self.assertEqual(rows["c1"]["edits"], "0")
        self.assertEqual(rows["c1"]["zero_edit"], "1")
        self.assertNotEqual(rows["c2"]["edits"], "0")
        self.assertEqual(rows["c3"]["status"], "error")  # an errored clip is judged as an empty draft
        header = self.out.read_text(encoding="utf-8").splitlines()[0]
        for banned in ("intended", "spoken", "reference"):
            self.assertNotIn(banned, header)
        self.assertEqual(rows["c1"]["judge_model"], "fake-model")
        self.assertEqual(rows["c1"]["label"], "unofficial: fake")

    def test_the_cache_makes_a_rerun_ask_nothing(self):
        first = FakeJudge()
        self.run_judge(first)
        self.assertEqual(len(first.calls), 3)
        again = FakeJudge()
        self.assertEqual(self.run_judge(again), 0)
        self.assertEqual(again.calls, [])
        self.assertEqual(len(self.cache.read_text().splitlines()), 3)
        self.assertNotIn("key", self.cache.read_text().lower())

    def test_identical_drafts_are_judged_once(self):
        write_csv(self.manifest, [
            {"clip_id": "c1", "domain": "e", "speaker_id": "1", "audio_length_s": "3", "intended_text": "Same.", "spoken_text": "same"},
            {"clip_id": "c2", "domain": "e", "speaker_id": "1", "audio_length_s": "3", "intended_text": "Same.", "spoken_text": "same"}])
        write_csv(self.results, [{"clip_id": "c1", "status": "ok", "transcript": "Same.", "finalize_s": ""},
                                 {"clip_id": "c2", "status": "ok", "transcript": "Same.", "finalize_s": ""}])
        j = FakeJudge()
        self.run_judge(j)
        self.assertEqual(len(j.calls), 1)

    def test_a_usage_limit_stops_the_run_with_exit_3_and_keeps_what_it_has(self):
        j = FakeJudge(limit_after=1)
        self.assertEqual(self.run_judge(j, workers=1), 3)
        self.assertEqual(len(self.cache.read_text().splitlines()), 1)
        self.assertEqual(len(j.calls), 1)
        # the CSV is still written; unjudged clips have no edits
        self.assertEqual(sum(1 for r in self.rows().values() if r["edits"] == ""), 2)

    def test_a_failing_judge_is_retried_then_reported_unjudged(self):
        class Flaky(FakeJudge):
            def ask(self, prompt, intended, workdir):
                self.calls.append(prompt)
                raise RuntimeError("boom")
        j = Flaky()
        self.assertEqual(self.run_judge(j, attempts=2, workers=1), 1)
        self.assertEqual(len(j.calls), 6)  # 3 distinct prompts x 2 attempts

    def test_max_calls_limits_the_run(self):
        j = FakeJudge()
        self.run_judge(j, max_calls=1, workers=1)
        self.assertEqual(len(j.calls), 1)

    def test_limit_judges_only_the_first_clips(self):
        j = FakeJudge()
        self.run_judge(j, limit=1)
        self.assertEqual(list(self.rows()), ["c1"])


class BackendTests(unittest.TestCase):
    def test_parse_fixed_prefers_structured_output_then_json_text(self):
        self.assertEqual(parse_fixed({"fixed": " a "}, "", "x"), "a")
        self.assertEqual(parse_fixed(None, '{"fixed": "b"}', "x"), "b")

    def test_an_empty_answer_for_a_non_empty_text_is_a_failure(self):
        with self.assertRaises(RuntimeError):
            parse_fixed({"fixed": "  "}, "", "something")
        self.assertEqual(parse_fixed({"fixed": ""}, "", ""), "")

    def test_gemini_defaults_to_a_flash_model_and_honours_the_env_var(self):
        old = os.environ.pop("GEMINI_JUDGE_MODEL", None)
        try:
            j = make_judge("gemini")
            self.assertEqual(j.model, DEFAULT_MODEL)
            self.assertIn("flash", j.model)
            self.assertNotIn("argon", j.model.lower())
            os.environ["GEMINI_JUDGE_MODEL"] = "gemini-x-test"
            self.assertEqual(make_judge("gemini").model, "gemini-x-test")
            self.assertEqual(make_judge("gemini", "explicit").model, "explicit")
        finally:
            os.environ.pop("GEMINI_JUDGE_MODEL", None)
            if old is not None:
                os.environ["GEMINI_JUDGE_MODEL"] = old

    def test_command_lines_pin_the_model_and_keep_the_prompt_off_the_claude_and_codex_argv(self):
        claude = make_judge("claude")
        cmd = claude.command()
        self.assertEqual(cmd[cmd.index("--model") + 1], "claude-opus-5-5")
        self.assertIn("--no-session-persistence", cmd)
        codex = make_judge("codex")
        try:
            c = codex.command("/work", "/work/last.txt")
            self.assertEqual(c[c.index("-m") + 1], "gpt-6-astra")
            self.assertEqual(c[c.index("-s") + 1], "read-only")
            self.assertEqual(c[-1], "-")  # the prompt goes on stdin
            self.assertIn("features.shell_tool=false", c)
        finally:
            codex.close()
        g = make_judge("gemini", "m1").command("PROMPT", "/log")
        self.assertIn("--sandbox", g)
        self.assertEqual(g[g.index("--model") + 1], "m1")
        self.assertEqual(g[-1], "-p=PROMPT")  # agy reads the prompt only from argv

    def test_unknown_judge_is_an_error(self):
        with self.assertRaises(ValueError):
            make_judge("nope")

    def test_the_cache_is_per_judge_and_model(self):
        with tempfile.TemporaryDirectory() as d:
            os.environ["EVAL_JUDGE_CACHE"] = d
            try:
                a = judge.cache_path(FakeJudge())
                other = FakeJudge()
                other.model = "other/model"
                self.assertNotEqual(a, judge.cache_path(other))
                self.assertEqual(a.parent, pathlib.Path(d))
            finally:
                del os.environ["EVAL_JUDGE_CACHE"]


if __name__ == "__main__":
    unittest.main()
