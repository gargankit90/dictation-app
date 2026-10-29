#!/usr/bin/env bash
# Install git hooks for this clone and all its worktrees. Run once after
# cloning, and again whenever lefthook.yml changes.
set -euo pipefail

hooks="$(git rev-parse --git-common-dir)/hooks"

lefthook install

# Fail closed: if a checkout has no scripts/pre-push, the push is refused.
cat > "$hooks/pre-push" <<'HOOK'
#!/usr/bin/env bash
script="$(git rev-parse --show-toplevel)/scripts/pre-push"
if [ ! -x "$script" ]; then
  echo "pre-push: $script missing; push refused. Run scripts/install-hooks.sh from an up-to-date checkout." >&2
  exit 1
fi
exec "$script" "$@"
HOOK
chmod +x "$hooks/pre-push"
echo "Installed pre-push guard in $hooks"
