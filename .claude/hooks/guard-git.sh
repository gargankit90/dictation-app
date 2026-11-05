#!/usr/bin/env bash
# Best-effort plain commands, not a sandbox; never read command-named files.
command -v python3 >/dev/null || exit 0
input=$(cat) || exit 0
python3 -I - "$input" <<'PY'
import json, re, shlex, sys
try:
    command = json.loads(sys.argv[1])["tool_input"]["command"]
    if not isinstance(command, str) or "<<" in command: sys.exit(0)
    command = command.replace("\\\n", " ")
    command = re.sub(r"'[^']*'|\"(?:\\.|[^\"\\])*\"|(?<!\S)#[^\n]*", lambda m: "" if m[0].startswith("#") else m[0], command)
    # Quoted operators are data; preserve that distinction after shlex unquotes.
    command = re.sub(r"(['\"])([;&|<>()]+)\1", "'quoted-operator-data'", command)
    lexer = shlex.shlex(command, posix=True, punctuation_chars=";&|<>()\n")
    lexer.whitespace, lexer.whitespace_split, lexer.commenters = " \t\r", True, ""
    words = list(lexer)
except Exception:  # Unreadable input or shell syntax passes.
    sys.exit(0)
def block():
    print("guard-git: blocked: the Code Reviewer merges; the owner changes guards. Ask the Lead.", file=sys.stderr)
    sys.exit(2)
protected = r"(?:^|/)(?:\.claude(?:/(?:hooks(?:/.*)?|settings[^/]*\.json))?|scripts(?:/(?:guard-push\.sh|pre-push|install-hooks\.sh))?|\.?lefthook[^/]*\.yml|\.git/hooks(?:/.*)?)$"
segments, args = [], []
for word in words + [";"]:
    if word and all(c in ";&|()\n" for c in word): segments.append(args); args = []
    else: args.append(word)
for args in segments:
    clean, skip = [], False
    for i, a in enumerate(args):
        if skip: skip = False; continue
        if re.fullmatch(r"(?:&>|[<>][<>&|]*)", a):
            if i + 1 < len(args) and ">" in a and re.search(protected, args[i + 1]): block()
            if clean and clean[-1].isdigit(): clean.pop()
            skip = True
        else: clean.append(a)
    args = clean
    while args and (args[0] in ("then", "do", "else", "elif", "if", "while", "until", "{", "!", "time", "command", "env", "sudo", "nohup", "xargs") or re.match(r"^\w+=", args[0]) or args[0] == "timeout"):
        args = args[2:] if args[0] == "timeout" else args[1:]
    if not args: continue
    exe = args[0].rsplit("/", 1)[-1]
    paths = args[-1:] if exe in ("cp", "install", "ln") else args[1:]
    if (exe in ("tee", "cp", "mv", "install", "ln", "rm", "unlink", "touch", "truncate", "chmod") or exe in ("sed", "perl") and any(re.match(r"^-[A-Za-z]*i|^--in-place", a) for a in args) or exe == "git" and args[1:2] in (["checkout"], ["restore"], ["rm"], ["mv"])) and any(re.search(protected, p) for p in paths): block()
    if exe != "gh": continue
    # Remove only real global repo options; message and API field values stay data.
    if len(args) > 2 and args[1] in ("-R", "--repo"): del args[1:3]
    elif len(args) > 1 and args[1].startswith("--repo="): del args[1]
    if re.match(r"(?:pr(?:\s+(?:-R\s+\S+|--repo(?:=|\s+)\S+))?\s+merge|repo\s+(?:delete|edit)|release\s+delete)\b", " ".join(args[1:])): block()
    if args[1:2] != ["api"]: continue
    method, data, endpoint, queries, i = None, False, "", [], 2
    options = {"-X", "--method", "-f", "-F", "--field", "--raw-field", "--input", "-H", "--header", "-q", "--jq", "-t", "--template", "--hostname", "--cache", "-p", "--preview"}
    while i < len(args):
        token = args[i]; i += 1
        option, sep, value = token.partition("=") if token.startswith("--") else (token[:2], token[2:], token[2:])
        if option in options:
            if not sep and i == len(args): sys.exit(0)
            if not sep: value = args[i]; i += 1
            if option in ("-X", "--method"): method = value.upper()
            if option in ("-f", "-F", "--field", "--raw-field", "--input"): data = True
            if option in ("-f", "-F", "--field", "--raw-field") and value.startswith("query="): queries.append(value[6:])
        elif not token.startswith("-") and not endpoint: endpoint = token
    if (method or ("POST" if data else "GET")) == "GET": continue
    endpoint = re.sub(r"(repos/)\$(?:GITHUB_REPOSITORY|\{GITHUB_REPOSITORY\})(?=/|$)", r"\1owner/repo", endpoint)
    if re.search(r"(?:^|/)repos/[^/\s]+/[^/\s?]+(?:/?(?:[?#]|$)|/(?:rules|rulesets|branches|tags|git/refs|settings|interaction-limits|merges|pulls/\d+/merge)(?:[/?#]|$))", endpoint): block()
    if "graphql" in endpoint and any(re.search(r"\bmutation\b", q) and re.search(r"\b(?:mergePullRequest|enablePullRequestAutoMerge|enqueuePullRequest|mergeBranch|createCommitOnBranch|(?:create|update|delete)(?:Refs?|Repository|RepositoryRuleset|BranchProtectionRule))\s*\(", q) for q in queries): block()
PY
status=$?
[ "$status" -eq 2 ] && exit 2
exit 0
