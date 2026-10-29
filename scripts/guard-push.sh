#!/usr/bin/env bash
# Pre-push guard (run by lefthook). GitHub Free can't protect branches on a
# private repo, so main is protected here: no pushes to main, no force pushes,
# no deleting branches or moving tags. Milestones reach main only by PR merge.
set -euo pipefail

zero=0000000000000000000000000000000000000000
status=0

refuse() {
  echo "guard-push: refused: $1" >&2
  status=1
}

while read -r local_ref local_sha remote_ref remote_sha; do
  case "$remote_ref" in
    refs/heads/main)
      refuse "pushing to main. Open a pull request; the Tester merges it."
      continue
      ;;
  esac

  if [ "$local_sha" = "$zero" ]; then
    refuse "deleting $remote_ref."
    continue
  fi

  [ "$remote_sha" = "$zero" ] && continue # new branch or tag

  case "$remote_ref" in
    refs/tags/*)
      refuse "moving existing tag $remote_ref."
      ;;
    *)
      if ! git cat-file -e "$remote_sha^{commit}" 2>/dev/null; then
        refuse "$remote_ref has commits you don't have. Fetch and rebase first."
      elif ! git merge-base --is-ancestor "$remote_sha" "$local_sha"; then
        refuse "force push to $remote_ref (not a fast-forward)."
      fi
      ;;
  esac
done

exit "$status"
