"""Which text Vocet Cloud (AssemblyAI) pasted for each clip: the rewrite, or the verbatim fallback.

Vocet logs one line per dictation (exp/assemblyai, round 2), with no text in it:
    INFO dictation_transcribe::assemblyai: assemblyai text source="rewrite" reason="rewrite" rewrite_error="none" chars=148
Each line is matched to the harness row that finished next (run_at, UTC, whole seconds).

    python3 -I vocet_cloud_sources.py <vocet log lines> results/<engine>/results.csv results/<engine>/vocet_cloud_sources.csv
"""
import bisect
import csv
import re
import sys
from collections import Counter
from datetime import datetime, timezone

LINE = re.compile(r'^(\S+Z)\s+INFO dictation_transcribe::assemblyai: assemblyai text source="(\w+)" '
                  r'reason="([^"]*)" rewrite_error="([^"]*)" chars=(\d+)')


def ts(s: str) -> float:
    return datetime.fromisoformat(s.replace("Z", "+00:00")).timestamp()


log_path, results_path, out_path = sys.argv[1:4]
events = [m.groups() for m in map(LINE.match, open(log_path, encoding="utf-8")) if m]
rows = sorted(csv.DictReader(open(results_path, newline="", encoding="utf-8")), key=lambda r: r["run_at"])
done = [datetime.fromisoformat(r["run_at"]).astimezone(timezone.utc).timestamp() for r in rows]
first = min(done) - 120 if done else 0
out, used = [], set()
for t, source, reason, err, chars in events:
    tt = ts(t)
    if tt < first:
        continue
    i = bisect.bisect_left(done, tt - 1)  # run_at is rounded down to the second
    if i >= len(rows) or i in used:
        continue
    used.add(i)
    out.append({"clip_id": rows[i]["clip_id"], "logged_at": t, "source": source, "reason": reason,
                "rewrite_error": err, "chars": chars})
with open(out_path, "w", newline="", encoding="utf-8") as f:
    w = csv.DictWriter(f, ["clip_id", "logged_at", "source", "reason", "rewrite_error", "chars"])
    w.writeheader()
    w.writerows(out)
c = Counter(o["source"] for o in out)
print(f"{len(out)} dictations matched to {len(rows)} clips: {dict(c)}; "
      f"reasons {dict(Counter(o['reason'] for o in out))}; errors {dict(Counter(o['rewrite_error'] for o in out))}")
