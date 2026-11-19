"""The app's cleanup guard, ported from Rust for the offline eval.

Source: branch origin/exp/groq-cleanup (draft PR #324), crates/transcribe/src/groq_cleanup.rs
(`judge`, `is_echo`, `normalized`, `numbers`, `links`, `is_link`, `names`, `word_parts`,
`parse_reply`) and crates/transcribe/src/groq_cleanup_prompt.rs (`user_message`, the tags).
Same checks, same order, same constants. Differences, all from the language, none in the rules:
  - Rust's `char::is_alphanumeric` / `is_uppercase` / `is_alphabetic` and Python's `str.isalnum` /
    `isupper` / `isalpha` both follow Unicode, but differ on a few rare code points (e.g. Rust's
    alphanumeric is Alphabetic + Numeric; Python's isalnum is isalpha/isdecimal/isdigit/isnumeric).
    The transcripts here are English ASCII text, so no clip is affected.
  - `char::to_lowercase` vs `str.lower()`: identical for this text (no Turkish dotted I etc.).
  - Rust `trim()` trims Unicode White_Space; Python `strip()` with no argument also strips a few
    extra separators (\\x1c-\\x1f). None occur in these transcripts.
The unit tests in test_offline_cleanup.py are the Rust module's tests, case for case.
"""
from __future__ import annotations

import json

OPEN_TAG = "<transcript>"
CLOSE_TAG = "</transcript>"
LENGTH_RATIO = 1.25
LENGTH_SLACK = 20
SENTENCE_FLOOR = 12


def user_message(transcript: str) -> str:
    return f"{OPEN_TAG}\n{transcript}\n{CLOSE_TAG}"


def strip_tags(content: str) -> str:
    """parse_reply's text handling: trim, then drop a leading open tag and a trailing close tag."""
    text = content.strip()
    if text.startswith(OPEN_TAG):
        text = text[len(OPEN_TAG):]
    text = text.lstrip()
    if text.endswith(CLOSE_TAG):
        text = text[: -len(CLOSE_TAG)]
    return text.rstrip()


def parse_reply(body: bytes) -> str | None:
    """The first choice's text without the tags; None if the body isn't a chat reply."""
    try:
        reply = json.loads(body)
        content = reply["choices"][0]["message"]["content"]
    except (ValueError, KeyError, IndexError, TypeError):
        return None
    if not isinstance(content, str):
        return None
    return strip_tags(content)


def judge(raw: str, reply: str, prompt: str) -> str:
    """'cleaned' if reply may replace raw, else the name of the first check it fails."""
    if not normalized(reply):
        return "empty"
    if is_echo(reply, prompt):
        return "echo"
    if len(reply) > len(raw) * LENGTH_RATIO + LENGTH_SLACK:
        return "too_long"
    kept = numbers(reply)
    if any(n not in kept for n in numbers(raw)):
        return "dropped_number"
    lower = reply.lower()
    if any(link not in lower for link in links(raw)):
        return "dropped_link"
    words = word_parts(reply)
    if any(part not in words for name in names(raw) for part in word_parts(name)):
        return "dropped_name"
    return "cleaned"


def is_echo(reply: str, prompt: str) -> bool:
    r = f" {normalized(reply)} "
    if r in f" {normalized(prompt)} ":
        return True
    pieces = [prompt]
    for sep in (".", "\n", ":", ";"):
        pieces = [x for p in pieces for x in p.split(sep)]
    for piece in pieces:
        s = normalized(piece)
        if len(s) >= SENTENCE_FLOOR and f" {s} " in r:
            return True
    return False


def normalized(text: str) -> str:
    out: list[str] = []
    gap = False
    for c in text.lower():
        if c.isalnum():
            if gap and out:
                out.append(" ")
            gap = False
            out.append(c)
        else:
            gap = True
    return "".join(out)


def numbers(text: str) -> list[str]:
    found: list[str] = []
    current = ""
    for i, c in enumerate(text):
        nxt = text[i + 1] if i + 1 < len(text) else ""
        joins = c in ".,:" and current != "" and nxt.isascii() and nxt.isdigit()
        if c.isascii() and c.isdigit():
            current += c
        elif joins:
            if c != ",":
                current += c
        elif current:
            found.append(current)
            current = ""
    if current:
        found.append(current)
    return found


def _trim(token: str, keep) -> str:
    start, end = 0, len(token)
    while start < end and not keep(token[start]):
        start += 1
    while end > start and not keep(token[end - 1]):
        end -= 1
    return token[start:end]


def links(text: str) -> list[str]:
    out = []
    for token in text.split():
        t = _trim(token, lambda c: c.isalnum() or c == "/").rstrip("/").lower()
        if is_link(t):
            out.append(t)
    return out


def is_link(token: str) -> bool:
    if "://" in token:
        return True
    if "@" in token:
        user, host = token.split("@", 1)
        return user != "" and "." in host
    host = token.split("/")[0]
    labels = host.split(".")
    return (
        len(labels) >= 2
        and all(label and all(c.isalnum() or c == "-" for c in label) for label in labels)
        and len(labels[-1]) >= 2
        and labels[-1].isalpha()
    )


def names(text: str) -> list[str]:
    found = []
    sentence_start = True
    for token in text.split():
        word = _trim(token, str.isalnum)
        capitalised = bool(word) and word[0].isupper()
        pronoun = word == "I" or word.startswith("I'") or word.startswith("I’")
        if capitalised and not sentence_start and not pronoun:
            found.append(word)
        sentence_start = token.endswith((".", "!", "?", ":", "\n")) or word == ""
    return found


def word_parts(text: str) -> list[str]:
    return [w for w in normalized(text).split(" ") if w]
