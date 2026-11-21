"""Tests for eval/bench1_vocet_check.py: the shortcut driver sends --press / --release to the running app and
runs in the bench's own loop. No macOS or app needed."""
import pathlib
import sys
import tempfile
import unittest
from unittest import mock

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import bench1_vocet_check as vc  # noqa: E402


class ShortcutDriverTests(unittest.TestCase):
    def test_press_and_release_go_to_the_running_app_with_open(self):
        drv = vc.VocetShortcutDriver("~/Applications/Vocet.app", pathlib.Path("/tmp/x.wav"), vc.hb.Timing(), hold=0)
        with mock.patch.object(vc.subprocess, "run") as run:
            drv.start()
            drv.feed(None)
            drv.stop()
        flags = [c.args[0][-1] for c in run.call_args_list]
        self.assertEqual(flags, ["--press", "--release"])
        for c in run.call_args_list:
            self.assertEqual(c.args[0][:3], ["open", "-g", "-n"])
            self.assertTrue(c.args[0][3].endswith("/Applications/Vocet.app"))
            self.assertNotIn("~", c.args[0][3])

    def test_it_runs_in_the_bench_loop(self):
        # The driver is a harness Driver, so run_all drives it like any other (stub target, fake clock).
        clock = vc.hb.FakeClock()
        app = vc.hb.StubApp(clock, warm=True)

        class Wired(vc.VocetShortcutDriver):
            def before_clip(self, clip):
                pass

            def _send(self, flag):
                app.key(flag == "--press")

            def feed(self, clip):
                app.play(clip, clip.audio_s)

        clips = [vc.hb.Clip("c1", pathlib.Path("c1.flac"), 2.0, "hello there")]
        with tempfile.TemporaryDirectory() as d:
            rows = vc.hb.run_all(clips, Wired("x", pathlib.Path(d) / "s.wav", vc.hb.Timing()), vc.hb.StubTarget(app),
                                 clock, vc.hb.Timing(tail=0.5), pathlib.Path(d) / "r.csv", app="vocet", mode="cloud",
                                 driver_name="vocet-shortcut", log=lambda *_: None)
        self.assertEqual((rows["c1"]["status"], rows["c1"]["transcript"]), ("ok", "hello there"))


if __name__ == "__main__":
    unittest.main()
