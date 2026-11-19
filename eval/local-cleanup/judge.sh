#!/bin/sh
# Judge one or more cleanup runs with both judges (Claude Opus 5.5 and Codex GPT-6 Astra), the scorers from
# eval/voicearena. A clip whose pasted text equals a draft judged before (the raw engine's drafts, or any
# earlier run) is not judged again: the scorers cache by prompt sha256, and this script shares one cache per
# judge across all runs, seeded with the raw engines' judge answers from PR #323.
#
#   eval/local-cleanup/judge.sh runs/parakeet/qwen3-4b__v1 [more run dirs]   (JUDGES="claude codex" by default)
set -e
HERE=$(cd "$(dirname "$0")" && pwd)
VA=$HERE/../voicearena
CACHE=$HERE/judge_cache
mkdir -p "$CACHE"
for j in claude codex; do
  [ -f "$CACHE/$j.jsonl" ] && continue
  if [ $j = claude ]; then
    cat "$VA"/results/vocet_parakeet/scored.judge.jsonl "$VA"/results/vocet_qwen3/scored.judge.jsonl \
        "$VA"/results/vocet_groq/scored.judge.jsonl > "$CACHE/$j.jsonl"
  else
    : > "$CACHE/$j.jsonl"
  fi
done
for d in "$@"; do
  d=$(cd "$d" && pwd); label=$(basename "$(dirname "$d")")_$(basename "$d")
  for j in ${JUDGES:-claude codex}; do
    if [ $j = claude ]; then out=$d/scored.csv; script=score_claude_cli.py; else out=$d/scored_codex.csv; script=score_codex_cli.py; fi
    cp "$CACHE/$j.jsonl" "${out%.csv}.judge.jsonl"
    rc=0
    (cd "$VA" && uv run --quiet "$script" "$d/results.csv" clips/manifest.csv -o "$out" --label "$label") || rc=$?
    # Fold new answers back into the shared cache (dedupe by prompt sha256); scored*.csv keeps each
    # clip's corrected text, so the run folder needs no copy of the cache.
    python3 -I - "$CACHE/$j.jsonl" "${out%.csv}.judge.jsonl" "$out" <<'PY'
import csv, json, sys
cache_p, run_p, scored_p = sys.argv[1:]
seen, lines = set(), []
for p in (cache_p, run_p):
    for l in open(p, encoding="utf-8"):
        if l.strip():
            r = json.loads(l)
            if r["prompt_sha256"] not in seen:
                seen.add(r["prompt_sha256"]); lines.append(l if l.endswith("\n") else l + "\n")
before = len({json.loads(l)["prompt_sha256"] for l in open(cache_p, encoding="utf-8") if l.strip()})
open(cache_p, "w", encoding="utf-8").writelines(lines)
print(f"{scored_p}: {len(lines) - before} new judge answers", file=sys.stderr)
PY
    rm -f "${out%.csv}.judge.jsonl"
    if [ $rc -ne 0 ]; then echo "judge $j exited $rc for $d" >&2; [ $rc -eq 3 ] && exit 3; fi
  done
done
