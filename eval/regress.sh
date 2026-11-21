#!/usr/bin/env bash
# The prompt/model regression check (eval/regress/regress.py): the app's current cleanup prompt on 40 saved
# transcripts, judged, compared with the stored baseline. Costs API calls; not part of the gate.
#   eval/regress.sh --dry-run     # plan + cost estimate, no network
set -euo pipefail
exec python3 "$(cd "$(dirname "$0")" && pwd)/regress/regress.py" "$@"
