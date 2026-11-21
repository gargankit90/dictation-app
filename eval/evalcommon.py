"""Shared by the eval scripts: the tune/check split and where the app's cleanup prompt lives."""
from __future__ import annotations

import csv
import hashlib
import os
import pathlib

EVAL = pathlib.Path(__file__).resolve().parent
REPO = EVAL.parent

# Where the app keeps its cleanup prompt once Q2 lands (a file the app compiles in), so the eval and the
# app read one text and can't drift. Until that file exists the eval copy of prompt v4 stands in.
# EVAL_APP_PROMPT overrides both (a path), e.g. to try a candidate prompt before it is in the app.
APP_PROMPT = REPO / "crates" / "core" / "prompts" / "cleanup.txt"
EVAL_PROMPT = EVAL / "prompts" / "v4.txt"


def app_prompt_path() -> pathlib.Path:
    """The prompt the app ships: EVAL_APP_PROMPT, else the app's prompt file, else the eval copy of v4."""
    env = os.environ.get("EVAL_APP_PROMPT")
    if env:
        return pathlib.Path(env)
    return APP_PROMPT if APP_PROMPT.is_file() else EVAL_PROMPT


def half_of(speaker_id: str) -> str:
    """The tune/check split (Lead, 2026-11-19): tune = speakers whose sha256(speaker_id) starts with an
    even hex digit. Split by speaker so nobody's voice is on both sides; tune is for editing prompts,
    check is for confirming they generalise."""
    digit = hashlib.sha256(str(speaker_id).encode("utf-8")).hexdigest()[0]
    return "tune" if int(digit, 16) % 2 == 0 else "check"


def load_split(path: pathlib.Path = EVAL / "split.csv") -> dict[str, str]:
    """clip_id -> "tune" | "check" from the committed split.csv (derived with half_of)."""
    with path.open(newline="", encoding="utf-8") as fh:
        return {r["clip_id"]: r["half"] for r in csv.DictReader(fh)}
