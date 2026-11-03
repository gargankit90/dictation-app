#!/usr/bin/env bash
# Every relative Markdown link in a tracked .md file resolves to a file or
# directory in the repo (D3). External links, mailto and pure #anchors are
# skipped; a #fragment on a file link is ignored. Links inside code fences and
# inline code are ignored too.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

bad=0
checked=0
while IFS= read -r file; do
  dir=$(dirname "$file")
  while IFS= read -r target; do
    target=${target%%#*}
    target=${target%% *}
    [ -z "$target" ] && continue
    case "$target" in
      http://* | https://* | mailto:* | /*) continue ;;
    esac
    checked=$((checked + 1))
    if [ ! -e "$dir/$target" ]; then
      echo "broken link: $file -> $target" >&2
      bad=$((bad + 1))
    fi
  done < <(awk '/^```/ { f = !f; next } !f' "$file" | sed 's/`[^`]*`//g' |
    grep -oE '\]\([^)]+\)' | sed -E 's/^\]\(//; s/\)$//' || true)
done < <(git ls-files '*.md' ':!:third_party' ':!:node_modules')

if [ "$bad" -ne 0 ]; then
  echo "check-doc-links: $bad broken link(s)" >&2
  exit 1
fi
echo "check-doc-links: $checked relative links ok"
