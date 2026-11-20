#!/usr/bin/env bash
# Judge one run folder with the Voice Arena judges, unchanged, after seeding its judge cache with every
# verdict already given anywhere in this task's reach (this folder, the voicearena results, and the
# hosted-asr, cleanup-offline, local-cleanup and ASR-screen worktrees, read only). The cache is keyed
# by the sha256 of the full judge prompt (intended + draft + spoken), so only a byte-identical draft
# reuses a verdict. Adapted from eval/hosted-asr/score.sh (PR #330).
#
#   eval/voxtral-omni/score.sh <run dir> claude|codex
#   (reads results.csv if present, else harness_results.csv; writes scored[_codex].csv; appends the number
#    of new verdicts to judge-usage.log)
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd)
VA=$HERE/../voicearena
WS=$(cd "$HERE/../../.." && pwd)
out=$(cd "${1:?run dir}" && pwd)
judge=${2:?claude or codex}
label=$(basename "$out")
case "$judge" in
  claude) name=scored; other=_claude ;;
  codex) name=scored_codex; other=_codex ;;
  *) echo "judge: claude or codex" >&2; exit 2 ;;
esac
input=$out/harness_results.csv
[ -f "$out/results.csv" ] && input=$out/results.csv
cache=$out/$name.judge.jsonl
seed=$(mktemp)
for f in "$VA"/results/*/$name.judge.jsonl "$HERE"/results*/*/$name.judge.jsonl "$HERE"/input/*/*.judge.jsonl \
         "$WS"/hosted-asr-eval/eval/hosted-asr/results/*/$name.judge.jsonl \
         "$WS"/cleanup-offline-eval/eval/cleanup-offline/results/*/$name.judge.jsonl \
         "$WS"/cleanup-offline-eval/eval/cleanup-offline/paired-results/*/$name.judge.jsonl \
         "$WS"/local-cleanup-eval/eval/local-cleanup/judge_cache/$judge.jsonl \
         /tmp/vocet-asr-screen/runs/*$other.judge.jsonl /tmp/vocet-asr-screen/allcache-$judge; do
  [ -f "$f" ] && [ "$f" != "$cache" ] && cat "$f"
done >"$seed" || true
[ -f "$cache" ] && cat "$cache" >>"$seed"
# Keep only verdicts from this judge (the input/ folders hold both judges' caches).
python3 -I -c '
import json, sys
want = "claude" if sys.argv[3] == "claude" else "gpt"
seen, out = set(), []
for line in open(sys.argv[1], encoding="utf-8"):
    if not line.strip():
        continue
    try:
        r = json.loads(line)
    except ValueError:
        continue
    if want not in str(r.get("model", "")).lower() or r["prompt_sha256"] in seen:
        continue
    seen.add(r["prompt_sha256"]); out.append(json.dumps(r, ensure_ascii=False) + "\n")
open(sys.argv[2], "w", encoding="utf-8").writelines(out)
' "$seed" "$cache" "$judge"
rm -f "$seed"
before=$(wc -l <"$cache")
cd "$VA"
status=0
if [ "$judge" = claude ]; then
  uv run --quiet score_claude_cli.py "$input" clips/manifest.csv -o "$out/scored.csv" --label "$label" || status=$?
else
  uv run --quiet score_codex_cli.py "$input" clips/manifest.csv -o "$out/scored_codex.csv" \
    --label "$label" --max-calls "${CODEX_MAX_CALLS:-160}" || status=$?
fi
after=$(wc -l <"$cache")
echo "$(date +%FT%T) $label $judge new_verdicts=$((after - before)) exit=$status" >>"$HERE/judge-usage.log"
# Keep only the answers this run's drafts used, so the committed cache is this run's own record.
uv run --quiet --with amend-va==0.2.1 python -I - "$input" "$VA/clips/manifest.csv" "$cache" <<'PY'
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
exit $status
