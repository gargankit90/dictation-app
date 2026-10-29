#!/usr/bin/env bash
# Claude Code PreToolUse hook for Bash. Blocks commands that push to main,
# force push, skip git hooks, merge PRs, or tamper with the guards themselves.
# Exit code 2 blocks the call and shows the message to the agent.
set -uo pipefail

cmd=$(jq -r '.tool_input.command // ""')
# Join backslash-newline continuations, so a flag on the next line still counts.
cont=$'\\\n'
cmd=${cmd//"$cont"/ }

block() {
  echo "guard-git: blocked: $1. Developers open PRs; the Tester merges. Ask the Lead if this is really needed." >&2
  exit 2
}

# The checks run on $scan: $cmd with literal message values blanked to '', so
# words in a commit message or PR comment don't trip them. Blanked: the value
# of -m/--message (also -am, -nm) after git commit|tag|stash|notes|merge, and
# of -b/--body/-t/--title after gh pr|issue create|comment|edit|review, if it
# has no $, backtick or backslash. A command with $( ${ $' << or a backtick, an
# unclosed quote, or over 20000 characters is scanned as it is, because the
# quote tracking below could get it wrong.
scan=$(awk '
function reset() { st = 0; kind = ""; sub1 = ""; sub2 = ""; skip = 0; flag = 0 }
function msgcmd() {
  if (kind == "git") return sub1 ~ /^(commit|tag|stash|notes|merge)$/
  if (kind == "gh") return sub1 ~ /^(pr|issue)$/ && sub2 ~ /^(create|comment|edit|review)$/
  return 0
}
function msgflag(x) {
  if (kind == "git") return x == "--message" || x ~ /^-[a-zA-Z]*m$/
  return x ~ /^(-b|--body|-t|--title)$/
}
function endword(   v, isval, name) {
  if (w == "") return
  v = w; isval = flag; flag = 0
  if (isval && !wexp) v = "\047\047"
  else if (!wexp && msgcmd() && kind == "git" && match(w, /^--message=/)) v = substr(w, 1, RLENGTH) "\047\047"
  else if (!wexp && msgcmd() && kind == "gh" && match(w, /^--(body|title)=/)) v = substr(w, 1, RLENGTH) "\047\047"
  if (st == 0) {
    # Skip VAR=value and keywords to find the command name.
    if (w !~ /^[A-Za-z_][A-Za-z0-9_]*=/ && w !~ /^(!|[{]|if|then|else|elif|do|while|until|time)$/) {
      name = w; sub(/.*\//, "", name)
      kind = (name == "git" || name == "gh") ? name : "other"
      st = 1
    }
  } else if (!isval) {
    if (kind == "git" && sub1 == "") {
      if (skip) skip = 0
      else if (w ~ /^(-C|-c|--git-dir|--work-tree|--namespace)$/) skip = 1
      else if (w !~ /^-/) sub1 = w
    } else if (kind == "gh" && w !~ /^-/) {
      if (sub1 == "") sub1 = w
      else if (sub2 == "") sub2 = w
    }
    if (msgcmd() && msgflag(w)) flag = 1
  }
  out = out v; w = ""; wexp = 0
}
{ s = s (NR > 1 ? "\n" : "") $0 }
END {
  n = length(s)
  if (n > 20000 || index(s, "$(") || index(s, "${") || index(s, "$\047") || index(s, "<<") || index(s, "`")) {
    printf "%s", s; exit
  }
  reset()
  for (i = 1; i <= n; i++) {
    c = substr(s, i, 1)
    if (q == 1) { w = w c; if (c == "\047") q = 0; continue }
    if (q == 2) {
      w = w c
      if (c == "\\") { i++; w = w substr(s, i, 1); wexp = 1 }
      else if (c == "\"") q = 0
      else if (c == "$") wexp = 1
      continue
    }
    if (c == "\\") { i++; w = w c substr(s, i, 1); wexp = 1; continue }
    if (c == "\047") { q = 1; w = w c; continue }
    if (c == "\"") { q = 2; w = w c; continue }
    if (c == "#" && w == "") { # comment: copy to the end of the line
      j = index(substr(s, i), "\n"); if (j == 0) j = n - i + 2
      out = out substr(s, i, j - 1); i += j - 2; continue
    }
    if (c == " " || c == "\t") { endword(); out = out c; continue }
    if (index(";&|()<>\n", c)) { endword(); out = out c; if (c != "<" && c != ">") reset(); continue }
    if (c == "$") wexp = 1
    w = w c
  }
  if (q) { printf "%s", s; exit }
  endword()
  printf "%s", out
}' <<<"$cmd") || scan=$cmd
[ -n "$scan" ] || scan=$cmd

has() { grep -Eq -- "$1" <<<"$scan"; }

# git as a command: also /usr/bin/git, \git, "git", (git, $(git and `git.
git_cmd='(^|[^[:alnum:]_.-])git["'\'']?'

if has "$git_cmd"'([[:space:]]+-[^[:space:]]+([[:space:]]+[^-[:space:]][^[:space:]]*)?)*[[:space:]]+push\b'; then
  has '(--force|--force-with-lease|--force-if-includes|--mirror|--delete|--prune)\b' && block "force/delete push"
  has '[[:space:]]-[a-zA-Z]*[fd]\b' && block "force/delete push (-f/-d)"
  has '[[:space:]]\+[^[:space:]]' && block "force refspec (+ref)"
  has '[[:space:]]:[^[:space:]]' && block "delete refspec (:ref)"
  has '(^|[[:space:]:/"'\''(])main([[:space:]"'\'';&|)]|$)' && block "push to main"
  has '--all\b' && block "push --all"
fi

has '--no-verify' && block "--no-verify"
has "$git_cmd"'[[:space:]].*commit([[:space:]].*)?[[:space:]]-[a-zA-Z]*n' && block "git commit -n (skips hooks)"
has 'core\.hooksPath|LEFTHOOK=0|LEFTHOOK_EXCLUDE|lefthook[[:space:]]+uninstall|\.git/hooks' && block "disabling git hooks"
has 'gh[[:space:]]+pr[[:space:]]+merge' && block "merging a PR"
has 'gh[[:space:]]+api.*(/merge\b|merges|git/refs|/protection|rulesets)' && block "changing refs or merging through the GitHub API"
has 'gh[[:space:]]+(repo[[:space:]]+(delete|edit)|release[[:space:]]+delete)' && block "changing repo settings"

if has '(guard-git\.sh|guard-push\.sh|scripts/pre-push|install-hooks\.sh|lefthook\.yml|\.claude/settings)' &&
   has '(>|(^|[[:space:];&|])(tee|rm|mv|cp|ln|chmod|truncate|install|dd)[[:space:]]|sed[[:space:]]+-i|perl[[:space:]]+-[a-z]*i|python|node|ruby|git[[:space:]]+(checkout|restore|rm|mv))'; then
  block "editing the git guards"
fi

exit 0
