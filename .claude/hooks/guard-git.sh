#!/usr/bin/env bash
# Best-effort plain-command checks, not a shell sandbox. Never read command files.
command -v python3 >/dev/null || exit 0
input=$(cat) || exit 0
python3 - "$input" <<'PY'
import json, re, shlex, sys
try:
    command = json.loads(sys.argv[1])["tool_input"]["command"]
    if not isinstance(command, str) or "<<" in command: sys.exit(0)
    lexer = shlex.shlex(command, posix=True, punctuation_chars=";&|<>\n")
    lexer.whitespace = " \t\r"
    words = list(lexer)
except Exception:  # Input and tokenization failures must allow ordinary work.
    sys.exit(0)
def block():
    print("guard-git: blocked: the Code Reviewer merges; the owner changes guards. Ask the Lead.", file=sys.stderr)
    sys.exit(2)
protected = r"(?:^|/)(?:\.claude/(?:hooks(?:/.*)?|settings[^/]*\.json)|scripts/(?:guard-push\.sh|pre-push|install-hooks\.sh)|\.?lefthook[^/]*\.yml|\.git/hooks(?:/.*)?)$"
segments, args = [], []
for word in words + [";"]:
    if word and all(c in ";&|\n" for c in word): segments.append(args); args = []
    else: args.append(word)
for args in segments:
    if not args: continue
    exe = args[0].rsplit("/", 1)[-1]
    if any(a in (">", ">>", "&>") and re.search(protected, b) and re.search(r"(?<!['\"])>{1,2}\s*['\"]?" + re.escape(b), command) for a, b in zip(args, args[1:])): block()
    if exe in ("tee", "cp", "mv", "install", "ln", "rm", "unlink", "touch", "truncate", "chmod") and any(re.search(protected, p) for p in (args[-1:] if exe in ("cp", "install", "ln") else args[1:])): block()
    if exe == "sed" and any(a == "--in-place" or a.startswith("-i") for a in args) and any(re.search(protected, p) for p in args[1:]): block()
    if exe != "gh": continue
    text = " ".join(args[1:])
    if re.match(r"(?:-R\s+\S+\s+|--repo(?:=|\s+)\S+\s+)?(?:pr(?:\s+(?:-R|--repo)\s+\S+)?\s+merge|repo\s+(?:delete|edit)|release\s+delete)\b", text): block()
    if args[1:2] != ["api"]: continue
    method = re.search(r"(?:^|\s)(?:-X\s*|--method(?:=|\s+))(\w+)", text)
    method = method[1].upper() if method else ("POST" if any(a in ("-f", "-F", "--field", "--raw-field", "--input") or a.startswith(("--field=", "--raw-field=", "--input=")) for a in args) else "GET")
    if method == "GET": continue
    if re.search(r"(?:^|[ /])repos/[^/\s]+/[^/\s?]+(?:/?(?:[\s?#]|$)|/(?:rules|rulesets|branches|tags|git/refs|settings|interaction-limits|merges|pulls/\d+/merge)(?:[/\s?#]|$))", text): block()
    if "graphql" in text and re.search(r"\bmutation\b", text) and re.search(r"\b(?:mergePullRequest|enablePullRequestAutoMerge|enqueuePullRequest|mergeBranch|createCommitOnBranch|(?:create|update|delete)(?:Refs?|Repository|RepositoryRuleset|BranchProtectionRule))\s*\(", text): block()
PY
status=$?
[ "$status" -eq 2 ] && exit 2
exit 0
