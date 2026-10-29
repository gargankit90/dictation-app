#!/usr/bin/env bash
# Install git hooks for this clone and all its worktrees. Run once after
# cloning, and again whenever lefthook.yml or scripts/guard-push.sh changes.
set -euo pipefail

root=$(git rev-parse --show-toplevel)
hooks="$(git rev-parse --git-common-dir)/hooks"

lefthook install

# The hooks dir is shared by every worktree, including ones on branches from
# before the guards existed. So the hook runs its own installed copy of the
# push guard, which works in any checkout, and fails closed without it. Then
# it runs the checkout's scripts/pre-push (the guard again and the quality
# gate) if the checkout has one.
install -m 0755 "$root/scripts/guard-push.sh" "$hooks/guard-push.sh"

cat > "$hooks/pre-push" <<'HOOK'
#!/usr/bin/env bash
# Installed by scripts/install-hooks.sh; re-run that to update this hook and
# the guard-push.sh copy next to it.
set -euo pipefail

guard="$(dirname "$0")/guard-push.sh"
if [ ! -x "$guard" ]; then
  echo "pre-push: $guard missing; push refused. Run scripts/install-hooks.sh." >&2
  exit 1
fi

refs=$(cat)
[ -n "$refs" ] || exit 0 # nothing to push

printf '%s\n' "$refs" | "$guard" "$@"

root=$(git rev-parse --show-toplevel)
if [ -e "$root/scripts/guard-push.sh" ] && ! cmp -s "$guard" "$root/scripts/guard-push.sh"; then
  echo "pre-push: note: $guard differs from this checkout's scripts/guard-push.sh; run scripts/install-hooks.sh if the guard changed." >&2
fi
if [ -e "$root/scripts/pre-push" ]; then
  printf '%s\n' "$refs" | "$root/scripts/pre-push" "$@"
fi
HOOK
chmod +x "$hooks/pre-push"
echo "Installed pre-push guard in $hooks"
