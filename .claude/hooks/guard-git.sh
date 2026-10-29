#!/usr/bin/env bash
# Claude Code PreToolUse hook for Bash. Blocks commands that push to main,
# force push, skip git hooks, merge PRs, or tamper with the guards themselves.
# Exit code 2 blocks the call and shows the message to the agent.
set -uo pipefail

# Fail closed: a missing tool, empty input, input that isn't one JSON object,
# or a command that isn't a string blocks the call.
refuse() {
  echo "guard-git: blocked: $1, so the command can't be checked. Ask the Lead." >&2
  exit 2
}
for tool in jq awk grep sed tr; do
  command -v "$tool" >/dev/null 2>&1 || refuse "$tool not found"
done
input=$(cat) || refuse "can't read the hook input"
[ -n "$input" ] || refuse "empty hook input"
cmd=$(jq -ers 'if length == 1 and (.[0] | type) == "object"
  then .[0].tool_input.command // "" | if type == "string" then . else error("command is not a string") end
  else error("input is not one JSON object") end' <<<"$input" 2>/dev/null) ||
  refuse "unreadable hook input"
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
# has no $, backtick or backslash. A redirect (< or >) ends a message flag, so
# its target is never blanked. A newline inside quotes becomes a space, so a
# quoted value can't split a command across lines. A command with $( ${ $' <<
# or a backtick, an unclosed quote, or over 20000 characters is scanned as it
# is, because the quote tracking below could get it wrong.
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
    } else if (kind == "gh" && sub2 == "") {
      if (skip) skip = 0
      else if (w ~ /^(-R|--repo)$/) skip = 1
      else if (w !~ /^-/) { if (sub1 == "") sub1 = w; else sub2 = w }
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
    if (q == 1) { w = w (c == "\n" ? " " : c); if (c == "\047") q = 0; continue }
    if (q == 2) {
      w = w (c == "\n" ? " " : c)
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
    if (index(";&|()<>\n", c)) {
      endword(); out = out c
      if (c == "<" || c == ">") flag = 0; else reset()
      continue
    }
    if (c == "$") wexp = 1
    w = w c
  }
  if (q) { printf "%s", s; exit }
  endword()
  printf "%s", out
}' <<<"$cmd") || scan=$cmd
[ -n "$scan" ] || scan=$cmd

# grep exits 1 for no match; anything else (missing, bad pattern) fails closed.
match() { # grep-flags pattern text
  grep -Eq"$1" -- "$2" <<<"$3"
  local rc=$?
  [ "$rc" -le 1 ] || refuse "grep failed ($rc)"
  return "$rc"
}
has() { match '' "$1" "$scan"; }
has_i() { match i "$1" "$scan"; }

# git or gh as a command: also /usr/bin/git, \git, "git", (git, $(git and `git.
git_cmd='(^|[^[:alnum:]_.-])git["'\'']?'
gh_cmd='(^|[^[:alnum:]_.-])gh["'\'']?'
# Options before a subcommand, each with an optional value: git -C <path>,
# gh -R <repo>. A value may be quoted and contain spaces (-C "/a b/c",
# -c user.name="A B", --git-dir='/x y/.git') or escaped ones (-C /a\ b).
piece='("([^"\\]|\\.)*"|'\''[^'\'']*'\''|\\.|[^[:space:]"'\''\\])'
first='("([^"\\]|\\.)*"|'\''[^'\'']*'\''|\\.|[^-[:space:]"'\''\\])'
opts="([[:space:]]+-$piece*([[:space:]]+$first$piece*)?)*"

if has "$git_cmd$opts"'[[:space:]]+push\b'; then
  has '(--force|--force-with-lease|--force-if-includes|--mirror|--delete|--prune)\b' && block "force/delete push"
  has '[[:space:]]-[a-zA-Z]*[fd]\b' && block "force/delete push (-f/-d)"
  has '[[:space:]]\+[^[:space:]]' && block "force refspec (+ref)"
  has '[[:space:]]:[^[:space:]]' && block "delete refspec (:ref)"
  has '(^|[[:space:]:/"'\''(])main([[:space:]"'\'';&|)]|$)' && block "push to main"
  has '--all\b' && block "push --all"
fi

has '--no-verify' && block "--no-verify"
has "$git_cmd"'[[:space:]].*commit([[:space:]].*)?[[:space:]]-[a-zA-Z]*n' && block "git commit -n (skips hooks)"
# git config keys are case-insensitive, and on macOS so are paths.
has_i 'hookspath|lefthook=["'\'']?(0|false)\b|lefthook_[a-z]|lefthook[[:space:]]+uninstall|lefthook-local|\.git/hooks|git-path[[:space:]=]+["'\'']?hooks' &&
  block "disabling git hooks"
# A git alias can run push --force under another name.
has_i "$git_cmd"'.*alias\.' && block "defining a git alias"
has "$gh_cmd$opts[[:space:]]+pr$opts"'[[:space:]]+merge\b' && block "merging a PR"
has "$gh_cmd$opts"'[[:space:]]+alias[[:space:]]+(set|import)\b' && block "defining a gh alias"
# gh api, flattened so a GraphQL query over several lines is one string. The
# query can't be checked when it comes from a file (-F query=@file, --input).
flat=${scan//$'\n'/ }
match '' "$gh_cmd$opts"'[[:space:]]+api\b.*(/merge\b|merges|git/refs|/protection|rulesets|mergePullRequest|enablePullRequestAutoMerge|enqueuePullRequest|mergeBranch|createCommitOnBranch|(create|update|delete)Refs?\b)' "$flat" &&
  block "changing refs or merging through the GitHub API"
match '' "$gh_cmd$opts"'[[:space:]]+api\b.*graphql.*(=@|--input)' "$flat" && block "a GraphQL query from a file"
has "$gh_cmd$opts"'[[:space:]]+(repo[[:space:]]+(delete|edit)|release[[:space:]]+delete)' && block "changing repo settings"

# A patch can change a guard without naming it, so applying one is blocked.
# Checking one (--check, --stat, --dry-run) is fine. Checked per command.
patch_cmd='(^[[:space:]]*(([A-Za-z_][A-Za-z0-9_]*=[^[:space:]]*|sudo|env|xargs|command|exec|nohup|time)[[:space:]]+)*|["'\''])([^[:space:]"'\'']*/)?patch([[:space:]]|$)'
while IFS= read -r seg; do
  match '' "$git_cmd$opts"'[[:space:]]+(apply|am)\b|'"$patch_cmd" "$seg" || continue
  match '' '[[:space:]](--check|--stat|--numstat|--summary|--dry-run|--abort|--quit|--show-current-patch)\b' "$seg" ||
    block "applying a patch (it could change the guards)"
done < <(tr ';&|()' '\n' <<<"$scan")

# Editing the guards: a guard file (also relative after a cd, and the scripts/,
# .claude/ and hooks/ dirs) together with a write. 2>&1 and >/dev/null don't
# count as writes.
guards='guard-git\.sh|guard-push\.sh|install-hooks\.sh|\.claude/(settings|hooks)|(^|[^[:alnum:]_.-])\.?lefthook(-local)?\.(ya?ml|json|toml)|(^|[/[:space:]"'\''=])(pre-push|settings(\.local)?\.json|hooks|\.claude|scripts)/?([[:space:]"'\'';&|)]|$)'
writes='>|(^|[[:space:];&|(])(sudo[[:space:]]+)?(tee|rm|mv|cp|ln|chmod|chflags|truncate|install|dd|rsync|unlink|patch)[[:space:]]|sed([[:space:]]+[^[:space:];&|]+)*[[:space:]]+(-[a-zA-Z]*i|--in-place)|perl[[:space:]]+-[a-z]*i|python|node|ruby|git[[:space:]]+(checkout|restore|rm|mv|apply)'
wscan=$(sed -E 's#[0-9]*>&[0-9-]+##g; s#[0-9&]*>>?[[:space:]]*/dev/null([^[:alnum:]_./-]|$)#\1#g' <<<"$scan") || wscan=$scan
if match '' "$guards" "$wscan" && match '' "$writes" "$wscan"; then
  block "editing the git guards"
fi

exit 0
