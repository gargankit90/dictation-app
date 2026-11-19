#!/usr/bin/env bash
# Score one offline cleanup run with the Voice Arena judges, unchanged, after seeding its judge cache
# with every judge answer already given (this branch's eval/voicearena/results, this folder's other
# runs, and the Groq cleanup eval worktree's results, read only). The cache is keyed by the sha256 of
# the full judge prompt (intended + draft + spoken), so only a byte-identical draft reuses an answer:
# an unchanged draft keeps its verdict across runs and costs no judge call. Same method as
# eval/groq-cleanup/score.sh on eval/groq-cleanup.
#
#   eval/cleanup-offline/score.sh <label> [claude|codex]   # results/<label>/results.csv -> scored[_codex].csv
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd)
VA=$HERE/../voicearena
OTHER=${GROQ_EVAL_RESULTS:-$HERE/../../../groq-cleanup-eval/eval/voicearena/results}
label=${1:?run label}
judge=${2:-claude}
out=$HERE/results/$label
case "$judge" in
  claude) name=scored ;;
  codex) name=scored_codex ;;
  *) echo "judge: claude or codex" >&2; exit 2 ;;
esac
cache=$out/$name.judge.jsonl
seed=$(mktemp)
for f in "$VA"/results/*/$name.judge.jsonl "$HERE"/results/*/$name.judge.jsonl "$OTHER"/*/$name.judge.jsonl; do
  [ -f "$f" ] && [ "$f" != "$cache" ] && cat "$f"
done >"$seed" || true
[ -f "$cache" ] && cat "$cache" >>"$seed"
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
cd "$VA"
if [ "$judge" = claude ]; then
  uv run --quiet score_claude_cli.py "$out/results.csv" clips/manifest.csv -o "$out/scored.csv" --label "$label"
else
  uv run --quiet score_codex_cli.py "$out/results.csv" clips/manifest.csv -o "$out/scored_codex.csv" \
    --label "$label" --max-calls "${CODEX_MAX_CALLS:-160}"
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
