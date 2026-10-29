#!/usr/bin/env bash
# Claude Code PreToolUse hook for Bash. Blocks commands that push to main,
# force push, skip git hooks, merge PRs, or tamper with the guards themselves.
# Exit code 2 blocks the call and shows the message to the agent.
set -uo pipefail

cmd=$(jq -r '.tool_input.command // ""')

block() {
  echo "guard-git: blocked: $1. Developers open PRs; the Tester merges. Ask the Lead if this is really needed." >&2
  exit 2
}

has() { grep -Eq -- "$1" <<<"$cmd"; }

if has '(^|[;&|[:space:]])git([[:space:]]+-[^[:space:]]+([[:space:]]+[^-[:space:]][^[:space:]]*)?)*[[:space:]]+push\b'; then
  has '(--force|--force-with-lease|--force-if-includes|--mirror|--delete|--prune)\b' && block "force/delete push"
  has '[[:space:]]-[a-zA-Z]*[fd]\b' && block "force/delete push (-f/-d)"
  has '[[:space:]]\+[^[:space:]]' && block "force refspec (+ref)"
  has '[[:space:]]:[^[:space:]]' && block "delete refspec (:ref)"
  has '(^|[[:space:]:/])main([[:space:]]|$)' && block "push to main"
  has '--all\b' && block "push --all"
fi

has '--no-verify' && block "--no-verify"
has '(^|[;&|[:space:]])git[[:space:]].*commit([[:space:]].*)?[[:space:]]-[a-zA-Z]*n' && block "git commit -n (skips hooks)"
has 'core\.hooksPath|LEFTHOOK=0|LEFTHOOK_EXCLUDE|lefthook[[:space:]]+uninstall|\.git/hooks' && block "disabling git hooks"
has 'gh[[:space:]]+pr[[:space:]]+merge' && block "merging a PR"
has 'gh[[:space:]]+api.*(/merge\b|merges|git/refs|/protection|rulesets)' && block "changing refs or merging through the GitHub API"
has 'gh[[:space:]]+(repo[[:space:]]+(delete|edit)|release[[:space:]]+delete)' && block "changing repo settings"

if has '(guard-git\.sh|guard-push\.sh|lefthook\.yml|\.claude/settings)' &&
   has '(>|[[:space:]]tee[[:space:]]|sed[[:space:]]+-i|perl[[:space:]]+-[a-z]*i|[[:space:]]rm[[:space:]]|^rm[[:space:]]|[[:space:]]mv[[:space:]]|^mv[[:space:]]|[[:space:]]cp[[:space:]]|chmod|truncate|python|git[[:space:]]+(checkout|restore|rm))'; then
  block "editing the git guards"
fi

exit 0
