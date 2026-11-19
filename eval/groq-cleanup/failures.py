"""Print the failing drafts of one half, for reading (not committed output).

    python3 -I eval/groq-cleanup/failures.py eval/voicearena/results/<engine> [tune|check] [--all]

For each clip of the half with edits > 0 (or every clip with --all): the raw Whisper text, the
model's reply, what was pasted and why, the judge's corrected text and the intended text from the
local manifest (CC BY-NC 4.0, read locally, never written out by this script).
"""
import csv
import pathlib
import sys

HERE = pathlib.Path(__file__).resolve().parent
VA = HERE.parent / "voicearena"


def main() -> None:
    d = pathlib.Path(sys.argv[1])
    half = sys.argv[2] if len(sys.argv) > 2 and not sys.argv[2].startswith("--") else "tune"
    show_all = "--all" in sys.argv
    split = {r["clip_id"]: r["half"] for r in csv.DictReader((HERE / "split.csv").open(newline=""))}
    manifest = {r["clip_id"]: r for r in csv.DictReader((VA / "clips" / "manifest.csv").open(newline="", encoding="utf-8"))}
    replies = {}
    if (d / "groq_replies.csv").exists():
        replies = {r["clip_id"]: r for r in csv.DictReader((d / "groq_replies.csv").open(newline="", encoding="utf-8"))}
    n = 0
    for r in csv.DictReader((d / "scored.csv").open(newline="", encoding="utf-8")):
        if split.get(r["clip_id"]) != half or (not show_all and r["edits"] in ("0", "")):
            continue
        n += 1
        g = replies.get(r["clip_id"], {})
        print(f'== {r["clip_id"]} {r["domain"]} edits={r["edits"]}/{r["chars"]} reason={g.get("reason", "-")}')
        print(f'  RAW:      {g.get("raw_text", "-")}')
        print(f'  REPLY:    {g.get("cleaned_text", "-")}')
        print(f'  DRAFT:    {r["draft"]}')
        print(f'  FIXED:    {r["fixed"]}')
        print(f'  INTENDED: {manifest[r["clip_id"]]["intended_text"]}')
    print(f"{n} clips")


if __name__ == "__main__":
    main()
