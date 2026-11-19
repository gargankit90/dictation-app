#!/bin/sh
# Judge Gemma one-pass runs with both judges (Claude Opus 5.5 and Codex GPT-6 Astra), the scorers from
# eval/voicearena, amend-va 0.2.1 prompt v1. Same scheme as eval/local-cleanup/judge.sh: one shared answer
# cache per judge (judge_cache/, git-ignored, seeded with a copy of the local-cleanup eval's caches), keyed by
# the prompt's sha256, so a pasted text judged before is not judged again.
#
#   eval/gemma-onepass/judge.sh runs/gemma4-e2b__plain [more run dirs]   (JUDGES="claude codex" by default)
# Exit 3 = Codex reported a usage limit: stop.
set -e
HERE=$(cd "$(dirname "$0")" && pwd)
VA=$HERE/../voicearena
CACHE=$HERE/judge_cache
mkdir -p "$CACHE"
for j in claude codex; do [ -f "$CACHE/$j.jsonl" ] || : > "$CACHE/$j.jsonl"; done
for d in "$@"; do
  d=$(cd "$d" && pwd); label=gemma-onepass_$(basename "$d")
  for j in ${JUDGES:-claude codex}; do
    if [ $j = claude ]; then out=$d/scored.csv; script=score_claude_cli.py; else out=$d/scored_codex.csv; script=score_codex_cli.py; fi
    cp "$CACHE/$j.jsonl" "${out%.csv}.judge.jsonl"
    rc=0
    (cd "$VA" && uv run --quiet "$script" "$d/results.csv" clips/manifest.csv -o "$out" --label "$label") || rc=$?
    python3 -I - "$CACHE/$j.jsonl" "${out%.csv}.judge.jsonl" "$out" <<'PY'
import json, sys
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
