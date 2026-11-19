"""The fallback guard: keep the model's reply only when it is a cleanup of the raw text.

Same rules as the other cleanup work (#295, the AssemblyAI rewrite guard plus the digit check the
Voice Arena report asked for): fall back to the raw text when the reply is empty, an echo of the
instruction or the delimiters, assistant-style, much longer than the input, much shorter than the input,
or drops any number, email/URL/dotted name, or capitalised name that is in the raw text.

check(raw, reply, instruction) -> (final_text, reason); reason is "" when the reply is kept.
"""
import re
import unicodedata

# Capitalised words that are not names: sentence openers and fillers the cleanup is meant to drop.
NOT_NAMES = {
    "I", "I'm", "I'll", "I've", "I'd", "Okay", "Ok", "OK", "Oh", "So", "Like", "Yeah", "Yes", "Well", "Um",
    "Uh", "Right", "Actually", "And", "But", "Hmm", "Sorry", "No", "Also", "Anyway", "Just", "Then", "Or",
    "Basically", "Alright", "Wait", "Ah", "Er", "Huh", "Mm", "Now", "Hey", "Hi",
}
ASSISTANT = re.compile(
    r"^\s*(sure|certainly|of course|here is|here's|cleaned|the cleaned|i'm sorry|i cannot|i can't|as an ai|"
    r"note:|transcript:|output:)", re.I)
NUM = re.compile(r"\d+")
DOTTED = re.compile(r"[\w+-]+(?:[.@][\w+-]+)+(?:/[\w./-]*)?")
CAP = re.compile(r"\b[A-Z][A-Za-z]*(?:['’][A-Za-z]+)?")


def norm(text: str) -> str:
    text = unicodedata.normalize("NFKC", text).lower()
    return " ".join(re.findall(r"[a-z0-9]+", text))


def sentences(instruction: str) -> list[str]:
    return [s for s in (norm(x) for x in re.split(r"[.!?\n]", instruction)) if len(s) >= 12]


def numbers(text: str) -> set[str]:
    return set(NUM.findall(text.replace(",", "")))


def dotted(text: str) -> set[str]:
    out = set()
    for t in DOTTED.findall(text):
        t = t.strip(".-/").lower()
        if re.search(r"[a-z]", t) and not re.fullmatch(r"[\d.]+", t):
            out.add(t)
    return out


def names(text: str) -> set[str]:
    out = set()
    for m in CAP.finditer(text):
        w = re.sub(r"['’]s$", "", m.group(0))
        before = text[:m.start()].rstrip()
        sentence_start = not before or before[-1] in ".!?:\"“\n"
        if sentence_start or w in NOT_NAMES or len(w) < 2:
            continue
        out.add(w)
    return out


def strip_wrapping(reply: str) -> str:
    r = reply.strip()
    r = re.sub(r"^<transcript>\s*|\s*</transcript>$", "", r).strip()
    if len(r) >= 2 and r[0] == r[-1] and r[0] in "\"'`":
        r = r[1:-1].strip()
    return r


def check(raw: str, reply: str, instruction: str) -> tuple[str, str]:
    r = strip_wrapping(reply)
    if not norm(r):
        return raw, "empty"
    if "<transcript" in r.lower() or "</" in r:
        return raw, "echo"
    padded = f" {norm(r)} "
    if any(f" {s} " in padded for s in sentences(instruction)) or padded.strip() in norm(instruction):
        return raw, "echo"
    if ASSISTANT.match(r) and not ASSISTANT.match(raw):
        return raw, "assistant"
    rw, cw = len(norm(raw).split()), len(norm(r).split())
    if len(r) > 1.2 * len(raw) + 20:
        return raw, "longer"
    if rw >= 8 and cw < 0.5 * rw:
        return raw, "shorter"
    if numbers(raw) - numbers(r):
        return raw, "number"
    low = r.lower()
    if any(d not in low for d in dotted(raw)):
        return raw, "url_email"
    if any(not re.search(r"(?<![A-Za-z])" + re.escape(n), r) for n in names(raw)):
        return raw, "name"
    return r, ""
