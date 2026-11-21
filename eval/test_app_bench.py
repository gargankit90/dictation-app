"""Tests for eval/app_bench.py (BENCH1): the key chords, the clip loop against the stub app on a simulated
clock (dry run), resume and retry, and the scoring path on fixed strings (harness CSV -> judge.py with a
fake judge -> summary, and amend-va's own char_edits when it is installed). No macOS, network or key.
"""
import contextlib
import csv
import io
import json
import pathlib
import sys
import tempfile
import unittest

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import app_bench as ab  # noqa: E402
import judge  # noqa: E402
from test_judges import FakeJudge, fake_tools, write_csv  # noqa: E402


def clip(i=1, seconds=3.0, ref="the quick brown fox jumps over the lazy dog"):
    return ab.Clip(f"c{i}", pathlib.Path(f"c{i}.flac"), seconds, ref)


def harness(**app_kw):
    clock = ab.FakeClock()
    app = ab.StubApp(clock, **app_kw)
    return clock, app, ab.StubDriver(app), ab.StubTarget(app)


class HotkeyTests(unittest.TestCase):
    def test_a_modifier_alone_is_one_flags_changed_key(self):
        (k,) = ab.parse_hotkey("ralt")
        self.assertEqual((k.code, k.modifier), (61, True))
        self.assertEqual(ab.parse_hotkey("Right Option"), ab.parse_hotkey("rightoption"))

    def test_a_chord_puts_modifiers_first_and_rejects_other_orders(self):
        keys = ab.parse_hotkey("option+space")
        self.assertEqual([k.name for k in keys], ["lalt", "space"])
        for bad in ("space+lalt", "space+grave", "nosuchkey", ""):
            with self.assertRaises(ValueError, msg=bad):
                ab.parse_hotkey(bad)

    def test_right_option_down_and_up_carry_the_device_bit_then_no_flags(self):
        keys = ab.parse_hotkey("ralt")
        down, up = ab.key_events(keys, True), ab.key_events(keys, False)
        self.assertEqual(down, [(61, True, ab.ALTERNATE | 0x40 | ab.NON_COALESCED, True)])
        self.assertEqual(up, [(61, False, ab.NON_COALESCED, True)])

    def test_a_chord_presses_in_order_and_releases_in_reverse_with_the_held_flags(self):
        keys = ab.parse_hotkey("lalt+space")
        alt = ab.ALTERNATE | 0x20
        self.assertEqual(ab.key_events(keys, True),
                         [(58, True, alt | ab.NON_COALESCED, True), (49, True, alt, False)])
        self.assertEqual(ab.key_events(keys, False),
                         [(49, False, alt, False), (58, False, ab.NON_COALESCED, True)])

    def test_fn_uses_the_secondary_fn_flag(self):
        self.assertEqual(ab.key_events(ab.parse_hotkey("fn"), True)[0][2], ab.SECONDARY_FN | ab.NON_COALESCED)

    def test_every_app_hotkey_parses(self):
        for name, app in ab.APPS.items():
            self.assertTrue(ab.parse_hotkey(app.hotkey), name)
            self.assertIn(app.mode, ("hold", "toggle"), name)


class LoopTests(unittest.TestCase):
    def test_a_clip_waits_for_the_microphone_and_measures_key_up_to_text(self):
        clock, app, drv, tgt = harness(open_delay=0.3, latency=1.2)
        res = ab.run_clip(clip(), drv, tgt, clock, ab.Timing())
        self.assertEqual(res.status, "ok")
        self.assertEqual(res.transcript, clip().reference)
        self.assertAlmostEqual(res.ready_s, 0.3, delta=0.006)
        self.assertAlmostEqual(res.finalize_s, 1.2, delta=0.021)
        self.assertAlmostEqual(res.first_text_s, 1.2, delta=0.021)

    def test_a_warm_microphone_is_ready_at_once(self):
        clock, app, drv, tgt = harness(warm=True)
        self.assertEqual(ab.run_clip(clip(), drv, tgt, clock, ab.Timing()).ready_s, 0.0)

    def test_text_pasted_in_pieces_is_timed_to_the_last_change(self):
        clock, app, drv, tgt = harness(latency=1.0, paste_in_chunks=4)
        res = ab.run_clip(clip(), drv, tgt, clock, ab.Timing())
        self.assertEqual(res.transcript, clip().reference)
        self.assertAlmostEqual(res.first_text_s, 1.0, delta=0.021)
        self.assertAlmostEqual(res.finalize_s, 1.3, delta=0.021)

    def test_waiting_for_the_microphone_keeps_the_start_of_the_clip(self):
        # A driver that can't see the microphone gets only the fixed delay: an app that opens it later than
        # that loses the start of the clip (the stub pastes the words it heard); the mic-open wait doesn't.
        class Blind(ab.StubDriver):
            def mic_open(self):
                return None
        clock, app, drv, tgt = harness(open_delay=1.5)
        self.assertEqual(ab.run_clip(clip(), drv, tgt, clock, ab.Timing()).transcript, clip().reference)
        clock, app, _, tgt = harness(open_delay=1.5)
        res = ab.run_clip(clip(), Blind(app), tgt, clock, ab.Timing())
        self.assertNotEqual(res.transcript, clip().reference)
        self.assertTrue(clip().reference.startswith(res.transcript))

    def test_an_app_that_never_opens_the_microphone_fails_after_the_attempts_with_no_key_held(self):
        clock, app, drv, tgt = harness(never_opens=True)
        with tempfile.TemporaryDirectory() as d:
            rows = ab.run_all([clip()], drv, tgt, clock, ab.Timing(), pathlib.Path(d) / "r.csv", app="stub",
                              mode="t", driver_name="stub", attempts=2, log=lambda *_: None)
        r = rows["c1"]
        self.assertEqual((r["status"], r["attempts"]), ("error", 2))
        self.assertIn("microphone", r["error"])
        self.assertIsNone(app.pressed_at)

    def test_no_text_is_an_empty_result_after_the_timeout(self):
        clock, app, drv, tgt = harness()
        silent = clip(ref="")
        t0 = clock.now()
        res = ab.run_clip(silent, drv, tgt, clock, ab.Timing(timeout=5))
        self.assertEqual(res.status, "empty")
        self.assertLess(clock.now() - t0, 3 + 0.6 + 0.5 + 4 + 5 + 1)

    def test_a_rerun_resumes_and_redoes_only_what_failed(self):
        clock, app, drv, tgt = harness()
        clips = [clip(1), clip(2), clip(3)]
        with tempfile.TemporaryDirectory() as d:
            out = pathlib.Path(d) / "r.csv"
            quiet = dict(app="stub", mode="t", driver_name="stub", log=lambda *_: None)
            ab.run_all(clips, drv, tgt, clock, ab.Timing(), out, **quiet)
            rows = ab.load_done(out)
            rows["c2"]["status"] = "error"
            ab.write_rows(out, rows)
            before = app.presses
            rows = ab.run_all(clips, drv, tgt, clock, ab.Timing(), out, **quiet)
            self.assertEqual(app.presses - before, 1)
            self.assertEqual({r["status"] for r in rows.values()}, {"ok"})
            with out.open(newline="") as fh:
                self.assertEqual(set(list(csv.DictReader(fh))[0]), set(ab.FIELDS))

    def test_dry_run_cli_passes(self):
        buf = io.StringIO()
        with contextlib.redirect_stdout(buf):
            self.assertEqual(ab.main(["dry-run", "-n", "3"]), 0)
        self.assertIn("3/3 ok", buf.getvalue())

    def test_load_clips_reads_the_manifest_in_order(self):
        with tempfile.TemporaryDirectory() as d:
            p = pathlib.Path(d)
            write_csv(p / "manifest.csv", [
                {"clip_id": "b", "file": "b.flac", "audio_length_s": "2.5", "intended_text": "B"},
                {"clip_id": "a", "file": "a.flac", "audio_length_s": "1", "intended_text": "A"}])
            clips = ab.load_clips(p)
            self.assertEqual([(c.clip_id, c.path.name, c.audio_s) for c in clips],
                             [("b", "b.flac", 2.5), ("a", "a.flac", 1.0)])
            self.assertEqual([c.clip_id for c in ab.load_clips(p, only={"a"})], ["a"])


class ScoringTests(unittest.TestCase):
    """The harness CSV goes through eval/judge.py unchanged; ZER and latency come out of summarize."""

    def setUp(self):
        self.dir = tempfile.TemporaryDirectory()
        self.d = pathlib.Path(self.dir.name)

    def tearDown(self):
        self.dir.cleanup()

    def run_folder(self, name, drafts):
        run = self.d / name
        run.mkdir()
        man = self.d / "manifest.csv"
        write_csv(man, [{"clip_id": c, "domain": "email", "speaker_id": "1", "audio_length_s": "3",
                         "intended_text": ref, "spoken_text": ref.lower()} for c, ref, _ in drafts])
        rows = {c: {"app": "x", "mode": "local", "driver": "blackhole", "clip_id": c,
                    "status": "ok" if d else "empty", "attempts": 1, "transcript": d, "audio_s": 3,
                    "ready_s": "0.1", "first_text_s": f"{0.4 + i / 10:.1f}" if d else "",
                    "finalize_s": f"{0.5 + i / 10:.1f}" if d else "", "error": "", "run_at": "t"}
                for i, (c, ref, d) in enumerate(drafts)}
        ab.write_rows(run / "results.csv", rows)
        (run / "meta.json").write_text(json.dumps({"app": "x", "mode": "local", "model": "m", "cleanup": "off"}))
        code = judge.judge_run(FakeJudge(), run / "results.csv", man, run / "scored.csv", name, tools=fake_tools(),
                               cache_file=self.d / f"{name}.jsonl", backoff=0, log=io.StringIO())
        self.assertIn(code, (0, 1))
        return run

    def test_fixed_strings_score_zer_and_latency(self):
        run = self.run_folder("r", [("c1", "Hello Bob.", "Hello Bob."), ("c2", "Run make.", "Run bake."),
                                    ("c3", "Done.", "Done."), ("c4", "Ship it.", "")])
        s = ab.summarize_run(run)
        self.assertEqual((s["clips"], s["ok"], s["judged"]), (4, 3, 4))
        self.assertEqual(s["zer"], 50.0)  # c1, c3 right; c2 one edit; c4 empty counts as wrong
        self.assertEqual((s["p50_s"], s["p95_s"]), (0.6, 0.7))
        self.assertIn("| x | local | m |", ab.summary_table([s]))
        self.assertTrue(ab.summary_table([s], "csv").startswith("app,mode,"))

    @unittest.skipUnless(__import__("importlib").util.find_spec("amend"), "amend-va not installed (uv run pytest)")
    def test_amend_va_char_edits_on_fixed_strings(self):
        from amend.metric import char_edits
        self.assertEqual(char_edits("Hello Bob.", "Hello Bob.")[0], 0)
        self.assertGreater(char_edits("Run bake.", "Run make.")[0], 0)

    def test_agreement_is_within_the_noise_of_a_paired_difference(self):
        def scored(flags):
            return [{"clip_id": f"c{i}", "edits": "0" if ok else "3"} for i, ok in enumerate(flags)]
        same = scored([True] * 120 + [False] * 31)
        self.assertTrue(ab.agreement(same, same, 3.0)["agree"])
        a = scored([True] * 120 + [False] * 31)
        b = scored([True] * 116 + [False] * 4 + [True] * 2 + [False] * 29)  # 4 lost, 2 gained: -1.3 pts
        res = ab.agreement(a, b, 3.0)
        self.assertEqual((res["only_blackbox_right"], res["only_shortcut_right"]), (4, 2))
        self.assertTrue(res["agree"])
        far = scored([True] * 90 + [False] * 61)  # 30 clips worse: -19.9 pts
        self.assertFalse(ab.agreement(a, far, 3.0)["agree"])
        self.assertEqual(ab.agreement([], a, 3.0), {"n": 0, "agree": False})


if __name__ == "__main__":
    unittest.main()
