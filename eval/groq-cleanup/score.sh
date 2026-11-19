#!/usr/bin/env bash
# Score one Groq-cleanup run with the same judge as every other run (bench.sh score), after seeding
# its judge cache with every judge answer already given in eval/voicearena/results/*: the cache is
# keyed by the sha256 of the full judge prompt (intended text + draft + spoken text), so only a draft
# that is byte for byte one already judged reuses that answer. Saves judge calls and keeps an
# unchanged draft's verdict the same across versions (no judge noise on unchanged drafts).
#
#   eval/groq-cleanup/score.sh <engine> [claude|codex]   # e.g. groq_clean_v1 or groq_clean_v1_raw
# claude (default): bench.sh score -> scored.csv; codex: score_codex_cli.py (GPT-6 Astra) -> scored_codex.csv
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd)
VA=$HERE/../voicearena
engine=${1:?engine label}
judge=${2:-claude}
out=$VA/results/$engine
case "$judge" in
  claude) name=scored ;;
  codex) name=scored_codex ;;
  *) echo "judge: claude or codex" >&2; exit 2 ;;
esac
cache=$out/$name.judge.jsonl
seed=$(mktemp)
for f in "$VA"/results/*/$name.judge.jsonl; do
  [ "$f" = "$cache" ] || cat "$f"
done >"$seed"
[ -f "$cache" ] && cat "$cache" >>"$seed"
# One answer per prompt (the first one given).
python3 -I -c '
import json, sys
seen, out = set(), []
for line in open(sys.argv[1], encoding="utf-8"):
    if not line.strip():
        continue
    r = json.loads(line)
    if r["prompt_sha256"] not in seen:
        seen.add(r["prompt_sha256"]); out.append(line if line.endswith("\n") else line + "\n")
open(sys.argv[2], "w", encoding="utf-8").writelines(out)
' "$seed" "$cache"
rm -f "$seed"
if [ "$judge" = claude ]; then
  "$VA/bench.sh" score "$engine"
else
  (cd "$VA" && uv run --quiet score_codex_cli.py "$out/results.csv" clips/manifest.csv -o "$out/scored_codex.csv" \
    --label "$engine" --max-calls 200)
fi
# Keep only the answers this run's drafts used, so the committed cache is this run's own record.
uv run --quiet --with amend-va==0.2.1 python -I - "$out/results.csv" "$VA/clips/manifest.csv" "$cache" <<'PY'
import csv, hashlib, json, sys
from amend.prompts import build_prompt
results = {r["clip_id"]: r for r in csv.DictReader(open(sys.argv[1], newline="", encoding="utf-8"))}
used = set()
for m in csv.DictReader(open(sys.argv[2], newline="", encoding="utf-8")):
    r = results.get(m["clip_id"], {})
    draft = r.get("transcript", "") if r.get("status") == "ok" else ""
    used.add(hashlib.sha256(build_prompt(m["intended_text"], draft, m["spoken_text"], "v1").encode()).hexdigest())
lines = [l for l in open(sys.argv[3], encoding="utf-8") if l.strip() and json.loads(l)["prompt_sha256"] in used]
open(sys.argv[3], "w", encoding="utf-8").writelines(lines)
print(f"judge cache pruned to {len(lines)} answers used by this run")
PY
