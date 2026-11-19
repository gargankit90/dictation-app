"""Apply the local-cleanup guard to a one-pass run, against the same model's plain transcription.

    python3 -I apply_guard.py runs/gemma4-e2b__plain runs/gemma4-e2b__onepass   -> runs/gemma4-e2b__onepass__guard/

The guard is vendor/guard.py, unchanged (copied from the eval/local-cleanup worktree): the one-pass reply is
kept only when it is not empty, not an echo of the instruction, not assistant-style, not much longer or
shorter than the plain text, and drops no number, email/URL/dotted name or capitalised name that the plain
text has. Otherwise the plain transcription is pasted. The one-pass folder itself is the "no guard" result.
"""
import collections
import csv
import json
import pathlib
import shutil
import sys

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE / "vendor"))
sys.path.insert(0, str(HERE))
from guard import check  # noqa: E402
from prompts_onepass import ONEPASS_SYSTEM  # noqa: E402


def main() -> None:
    plain_dir, one_dir = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
    out = one_dir.parent / f"{one_dir.name}__guard"
    out.mkdir(exist_ok=True)
    plain = {r["clip_id"]: r for r in csv.DictReader((plain_dir / "results.csv").open(newline="", encoding="utf-8"))}
    rows = list(csv.DictReader((one_dir / "results.csv").open(newline="", encoding="utf-8")))
    fb = collections.Counter()
    for r in rows:
        raw = plain[r["clip_id"]]["transcript"]
        r["onepass_reply"] = r["transcript"]
        r["plain"] = raw
        if raw.strip():
            r["transcript"], r["fallback"] = check(raw, r["onepass_reply"], ONEPASS_SYSTEM)
        else:
            r["fallback"] = "empty_plain"
        r["status"] = "ok" if r["transcript"].strip() else "empty"
        if r["fallback"]:
            fb[r["fallback"]] += 1
    with (out / "results.csv").open("w", newline="", encoding="utf-8") as fh:
        w = csv.DictWriter(fh, list(rows[0]))
        w.writeheader()
        w.writerows(rows)
    meta = json.loads((one_dir / "meta.json").read_text())
    meta.update(guard="vendor/guard.py against " + plain_dir.name, fallbacks=dict(fb), fallback_total=sum(fb.values()),
                note=meta.get("note", "") + "; with the guard the model runs twice per clip (plain + one-pass), "
                "so the real latency is the sum of both runs")
    (out / "meta.json").write_text(json.dumps(meta, indent=1))
    shutil.copy(one_dir / "server.log", out / "server.log")
    print(out, dict(fb))


if __name__ == "__main__":
    main()
