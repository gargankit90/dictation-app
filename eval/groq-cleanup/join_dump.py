"""Join Vocet's Groq-cleanup eval dump (groq-replies.jsonl) to the harness rows, by time.

    python3 -I eval/groq-cleanup/join_dump.py eval/voicearena/results/<engine> <groq-replies.jsonl>

Each dump line is written when a cleanup finishes; it belongs to the first clip whose harness row
was stamped (run_at, when the harness finished the clip, after its 2 s settle) at or after it. Writes, in the results folder:

  groq-replies.jsonl     the dump as Vocet wrote it (raw Whisper text, reply, pasted text, reason,
                         model, prompt, latency; never a key)
  groq_replies.csv       one row per clip: clip_id, raw_text, cleaned_text (the model's reply),
                         pasted_text, used, reason, model, prompt, prompt_sha, cleanup_ms, match
  harness_results.csv    the stripped harness CSV plus raw_text, cleaned_text, cleanup_reason,
                         cleanup_ms columns
and a sibling folder <engine>_raw/ with results.csv / harness_results.csv whose transcript is the raw
Whisper text, so the uncleaned drafts of the same run can be scored too.

`match` says whether the pasted text in the dump equals the text the harness read from TextEdit
(whitespace-normalised); a mismatch is reported, not hidden.
"""
import csv
import datetime as dt
import json
import pathlib
import shutil
import sys

ADDED = ["raw_text", "cleaned_text", "cleanup_reason", "cleanup_ms"]


def norm(text: str) -> str:
    return " ".join((text or "").split())


def main() -> None:
    out = pathlib.Path(sys.argv[1])
    dump = pathlib.Path(sys.argv[2])
    if dump.resolve() != (out / "groq-replies.jsonl").resolve():
        shutil.copyfile(dump, out / "groq-replies.jsonl")
    lines = [json.loads(x) for x in dump.read_text(encoding="utf-8").splitlines() if x.strip()]
    lines.sort(key=lambda d: d["time_ms"])
    results = list(csv.DictReader((out / "results.csv").open(newline="", encoding="utf-8")))
    starts = sorted(  # (finish time in ms, clip_id)
        (dt.datetime.fromisoformat(r["run_at"]).timestamp() * 1000, r["clip_id"]) for r in results if r.get("run_at")
    )
    by_clip: dict[str, dict] = {}
    unassigned = 0
    for d in lines:
        # run_at is stamped (to the second) when the harness finishes a clip, after its 2 s settle.
        owner = next((cid for end, cid in starts if d["time_ms"] - 1000 <= end <= d["time_ms"] + 20000), None)
        if owner is None:
            unassigned += 1
            continue
        by_clip[owner] = d  # a retried clip keeps its last cleanup

    rows, mismatched = [], []
    for r in results:
        d = by_clip.get(r["clip_id"])
        if d is None:
            rows.append({"clip_id": r["clip_id"], "match": "no dump line"})
            continue
        match = norm(d["pasted_text"]) == norm(r.get("transcript", ""))
        if not match:
            mismatched.append(r["clip_id"])
        rows.append({
            "clip_id": r["clip_id"], "raw_text": d["raw_text"], "cleaned_text": d.get("reply") or "",
            "pasted_text": d["pasted_text"], "used": d["used"], "reason": d["reason"], "model": d["model"],
            "prompt": d["prompt"], "prompt_sha": d["prompt_sha"], "cleanup_ms": d["cleanup_ms"],
            "match": "yes" if match else "no",
        })
    fields = ["clip_id", "raw_text", "cleaned_text", "pasted_text", "used", "reason", "model", "prompt",
              "prompt_sha", "cleanup_ms", "match"]
    with (out / "groq_replies.csv").open("w", newline="", encoding="utf-8") as fh:
        w = csv.DictWriter(fh, fields)
        w.writeheader()
        w.writerows(rows)

    joined = {x["clip_id"]: x for x in rows}
    stripped = list(csv.DictReader((out / "harness_results.csv").open(newline="", encoding="utf-8")))
    base_fields = [f for f in stripped[0].keys() if f not in ADDED]
    with (out / "harness_results.csv").open("w", newline="", encoding="utf-8") as fh:
        w = csv.DictWriter(fh, base_fields + ADDED)
        w.writeheader()
        for r in stripped:
            j = joined.get(r["clip_id"], {})
            w.writerow({**{k: r[k] for k in base_fields}, "raw_text": j.get("raw_text", ""),
                        "cleaned_text": j.get("cleaned_text", ""), "cleanup_reason": j.get("reason", ""),
                        "cleanup_ms": j.get("cleanup_ms", "")})

    raw_dir = out.parent / f"{out.name}_raw"
    raw_dir.mkdir(exist_ok=True)
    for name in ("results.csv", "harness_results.csv"):
        src = list(csv.DictReader((out / name).open(newline="", encoding="utf-8")))
        fields_n = [f for f in src[0].keys() if f not in ADDED]
        with (raw_dir / name).open("w", newline="", encoding="utf-8") as fh:
            w = csv.DictWriter(fh, fields_n)
            w.writeheader()
            for r in src:
                j = joined.get(r["clip_id"], {})
                row = {k: r[k] for k in fields_n}
                if j.get("raw_text") is not None and "raw_text" in j:
                    row["transcript"] = j["raw_text"]
                w.writerow(row)
    print(f"{len(lines)} dump lines, {len(by_clip)} clips joined, {unassigned} matched no clip, "
          f"{len(mismatched)} pasted-text mismatches {mismatched[:8]}; raw drafts in {raw_dir}")


if __name__ == "__main__":
    main()
