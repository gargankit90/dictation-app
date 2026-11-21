"""What must never be under eval/: audio (CC BY-NC dataset, not ours to publish) and API keys.

Scans the files git tracks under eval/ (or, outside a checkout, every file there), and checks that
.gitignore keeps the clip folders and the judge cache out. The scanner is itself tested on planted
examples, so it can't pass by finding nothing.
"""
import pathlib
import re
import subprocess
import sys
import unittest

HERE = pathlib.Path(__file__).resolve().parent
REPO = HERE.parent
AUDIO = {".flac", ".wav", ".mp3", ".m4a", ".ogg", ".opus", ".aac", ".aiff", ".aif", ".wma", ".webm", ".mp4",
         ".parquet"}
# Shapes of real keys: OpenRouter, Groq, OpenAI, Google, PostHog, bearer tokens, and key-looking assignments.
KEY_PATTERNS = [re.compile(p) for p in (
    r"sk-or-v1-[A-Za-z0-9]{20,}", r"gsk_[A-Za-z0-9]{20,}", r"sk-(?:proj-)?[A-Za-z0-9_-]{32,}",
    r"AIza[0-9A-Za-z_-]{30,}", r"ph[cx]_[A-Za-z0-9]{30,}", r"Bearer [A-Za-z0-9._-]{30,}",
    r"(?i)(?:api[_-]?key|secret|token)[\"']?\s*[=:]\s*[\"'][A-Za-z0-9_\-]{24,}[\"']")]


def tracked(root: pathlib.Path) -> list[pathlib.Path]:
    try:
        out = subprocess.run(["git", "-C", str(REPO), "ls-files", "-z", "--", "eval"], capture_output=True,
                             check=True).stdout.decode().split("\0")
        files = [REPO / f for f in out if f]
        if files:
            return [f for f in files if f.exists()]
    except (OSError, subprocess.CalledProcessError):
        pass
    return [p for p in root.rglob("*") if p.is_file() and "__pycache__" not in p.parts]


def audio_files(files) -> list[str]:
    return [str(f) for f in files if f.suffix.lower() in AUDIO]


def key_hits(files) -> list[str]:
    hits = []
    for f in files:
        try:
            text = f.read_text(encoding="utf-8", errors="ignore")
        except OSError:
            continue
        for pat in KEY_PATTERNS:
            m = pat.search(text)
            if m:
                hits.append(f"{f.relative_to(REPO)}: matches {pat.pattern[:30]}")
                break
    return hits


class HygieneTests(unittest.TestCase):
    def test_no_audio_is_tracked_under_eval(self):
        self.assertEqual(audio_files(tracked(HERE)), [])

    def test_no_api_key_is_tracked_under_eval(self):
        self.assertEqual(key_hits(tracked(HERE)), [])

    def test_the_scanners_find_what_they_should(self):
        import tempfile
        with tempfile.TemporaryDirectory() as d:
            d = pathlib.Path(d)
            (d / "clip.flac").write_bytes(b"fLaC")
            fake = "sk-or-v1-" + "a" * 40  # built here so this file holds no key-shaped text itself
            (d / "notes.txt").write_text(f"export OPENROUTER_API_KEY={fake}\n")
            (d / "ok.txt").write_text("OPENROUTER_API_KEY is read from the environment\n")
            files = sorted(d.iterdir())
            self.assertEqual(len(audio_files(files)), 1)
            REPO_BACKUP = globals()["REPO"]
            globals()["REPO"] = d
            try:
                hits = key_hits(files)
            finally:
                globals()["REPO"] = REPO_BACKUP
            self.assertEqual(len(hits), 1)
            self.assertIn("notes.txt", hits[0])

    def test_gitignore_covers_clips_audio_and_the_judge_cache(self):
        for path in ("eval/voicearena/clips/dict_0007.flac", "eval/voicearena/clips/manifest.csv",
                     "eval/asr/clips/x.wav", "eval/anything/else.flac", "eval/.judge-cache/claude-x.jsonl",
                     "eval/regress/work/scored.csv"):
            p = subprocess.run(["git", "-C", str(REPO), "check-ignore", "-q", path])
            self.assertEqual(p.returncode, 0, f"{path} is not git-ignored")


if __name__ == "__main__":
    unittest.main()
