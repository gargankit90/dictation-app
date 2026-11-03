#!/usr/bin/env bash
# Role/hook/guard/settings policy only. GitHub and pre-push enforce push rules.
# Never execute the command being inspected. Exit 2 makes PreToolUse refuse it.
set -uo pipefail
command -v python3 >/dev/null || { echo 'guard-git: python3 missing; ask the Lead.' >&2; exit 2; }
input=$(cat) || exit 2
python3 - "$input" <<'PY'
import json
import os
from pathlib import Path
import re
import shlex
import sys


def block(reason):
    print(f"guard-git: blocked: {reason}. The Code Reviewer merges; the owner changes guards. Ask the Lead.", file=sys.stderr)
    sys.exit(2)


try:
    payload = json.loads(sys.argv[1])
    command = payload.get("tool_input", {}).get("command") or ""
    if not isinstance(command, str):
        raise ValueError("command is not a string")
except (ValueError, AttributeError):
    block("unreadable hook input")


def protected(path):
    path = path.lower().rstrip("/")
    return bool(re.search(
        r"(^|/)(guard-git\.sh|guard-push\.sh|install-hooks\.sh|pre-push|"
        r"\.?lefthook(?:-local)?\.(?:ya?ml|json|toml))$|"
        r"(^|/)\.claude/(?:hooks(?:/.*)?|settings(?:\.local)?\.json)$|"
        r"(^|/)(?:settings(?:\.local)?\.json|\.claude|scripts|hooks)$|\.git/hooks(?:/.*)?$", path))


def inspect(text, depth=0):
    if depth > 12:
        block("shell nesting cannot be checked")
    # Inspect executable substitutions, even when embedded in a message value.
    for match in re.finditer(r"\$\(([^()]*)\)|`([^`]*)`", text):
        inspect(match.group(1) or match.group(2), depth + 1)
    try:
        lexer = shlex.shlex(text.replace("\\\n", ""), posix=True, punctuation_chars=";&|()<>\n")
        lexer.whitespace = " \t\r"
        lexer.whitespace_split = True
        words = list(lexer)
    except ValueError:
        block("shell syntax cannot be checked")
    segment = []
    for word in words + [";"]:
        if word and all(char in ";&|()\n" for char in word):
            check(segment, depth)
            segment = []
        else:
            segment.append(word)


def environment(value):
    if re.match(r"(?i)^LEFTHOOK=(0|false)$|^LEFTHOOK_EXCLUDE=.+", value):
        block("disabling hooks")
    if re.match(r"GIT_CONFIG_(?:PARAMETERS|KEY_\d+)=", value) and "core.hookspath" in value.lower():
        block("disabling hooks through git environment configuration")


def unwrap(args, depth):
    # Consume only executable prefixes: quoted arguments of ordinary commands
    # remain data. Each wrapper has its own options that consume a value.
    wrappers = {
        "env": {"-u", "--unset", "-C", "--chdir"},
        "sudo": {"-u", "--user", "-g", "--group", "-h", "--host", "-p", "--prompt", "-C", "-T"},
        "command": set(), "exec": {"-a"}, "time": {"-f", "--format", "-o", "--output"},
        "timeout": {"-s", "--signal", "-k", "--kill-after"},
        "nohup": set(), "nice": {"-n", "--adjustment"},
        "xargs": {"-I", "-n", "-P", "-L", "-s", "-d", "-E", "--replace", "--max-args", "--max-procs", "--max-lines", "--max-chars", "--delimiter", "--eof"},
        "caffeinate": {"-t", "-w"}, "stdbuf": {"-i", "-o", "-e", "--input", "--output", "--error"},
    }
    while args:
        value = args[0]
        if re.match(r"^[A-Za-z_][A-Za-z0-9_]*=", value):
            environment(value)
            args = args[1:]
            continue
        exe = os.path.basename(value).lstrip("\\")
        if exe == "export":
            for value in args[1:]:
                environment(value)
            return []
        if exe == "!":
            args = args[1:]
            continue
        if exe not in wrappers:
            return args
        args = args[1:]
        while args and args[0].startswith("-"):
            option = args.pop(0)
            if option == "--":
                break
            if exe == "env" and option in ("-S", "--split-string") and args:
                inspect(args.pop(0), depth + 1)
            elif option in wrappers[exe] and args:
                args.pop(0)
        if exe == "timeout" and args:
            args.pop(0)  # duration, after timeout options
    return args


def api(args):
    # Unlike command-wide matching, option parsing preserves headers, fields,
    # and templates as data and checks every actual positional endpoint.
    options = {"-H", "--header", "-q", "--jq", "-t", "--template", "--hostname",
               "-p", "--preview", "--cache", "-f", "-F", "--field", "--raw-field", "--input", "-X", "--method"}
    method = None
    data = False
    endpoints, queries, inputs = [], [], []
    i = 0
    positional = False
    while i < len(args):
        token = args[i]
        i += 1
        if token == "--":
            positional = True
            continue
        option, value = token, None
        if not positional and token.startswith("--") and "=" in token:
            option, value = token.split("=", 1)
        elif not positional and token[:2] in options and len(token) > 2 and not token.startswith("--"):
            option, value = token[:2], token[2:].lstrip("=")
        if not positional and option in options:
            if value is None:
                if i == len(args):
                    block("API option value cannot be checked")
                value = args[i]
                i += 1
            if option in ("-X", "--method"):
                method = value.upper()
            elif option in ("-f", "-F", "--field", "--raw-field", "--input"):
                data = True
                if option == "--input":
                    inputs.append(value)
                elif value.startswith("query="):
                    queries.append(value[6:])
        elif positional or not token.startswith("-"):
            endpoints.append(re.split(r"[?#]", token, 1)[0].rstrip("/"))
    method = method or ("POST" if data else "GET")
    if method == "GET":
        return
    for endpoint in endpoints:
        # Share host/path normalization between REST and GraphQL, including GHES.
        endpoint = re.sub(r"^(?:https?://[^/]+)?/*(?:api/)?", "", endpoint, flags=re.I)
        if (method == "PUT" and re.search(r"/pulls/\d+/merge$", endpoint)) or (method == "POST" and endpoint.endswith("/merges")):
            block("merging through the API")
        if method in ("DELETE", "PATCH", "PUT") and re.search(r"(?:^|/)repos/[^/]+/[^/]+(?:$|/(?:rules|rulesets|branches|tags|git/refs|settings|interaction-limits)(?:/|$))", endpoint):
            block("destructive repository API request")
        if endpoint != "graphql":
            continue
        for filename in inputs:
            if filename == "-":
                block("GraphQL input cannot be checked")
            payload = json.loads(Path(filename).read_text())
            if not isinstance(payload, dict) or not isinstance(payload.get("query"), str):
                block("GraphQL input has no readable query")
            queries.append(payload["query"])
        for query in queries:
            if query.startswith("@"):
                query = Path(query[1:]).read_text()
            # Ignore strings and comments so ordinary query data remains data.
            query = re.sub(r'"(?:\\.|[^"\\])*"|#[^\n]*', "", query)
            mutations = r"mergePullRequest|enablePullRequestAutoMerge|enqueuePullRequest|mergeBranch|createCommitOnBranch|(?:create|update|delete)(?:Refs?|Repository|RepositoryRuleset|BranchProtectionRule)"
            if re.search(r"\bmutation\b", query) and re.search(r"\b(?:" + mutations + r")\s*\(", query):
                block("GraphQL merge, ref or repository mutation")


def check(args, depth):
    if not args:
        return
    args = unwrap(args[:], depth)
    if not args:
        return
    exe = os.path.basename(args[0]).lstrip("\\")
    if exe in ("bash", "sh", "zsh"):
        for i, value in enumerate(args[1:], 1):
            if re.fullmatch(r"-[A-Za-z]*c[A-Za-z]*", value) and i + 1 < len(args):
                inspect(args[i + 1], depth + 1)
                return
    if exe in ("eval", "source", "."):
        inspect(" ".join(args[1:]), depth + 1)
        return
    # Redirection targets are checked independently of the command.
    for i, value in enumerate(args[:-1]):
        if ">" in value and protected(args[i + 1]):
            block("overwriting guards or pre-push hook")
    if any(x.lower().endswith(".git/config") for x in args) and any("hookspath" in x.lower() for x in args) and any(">" in x for x in args):
        block("changing hook configuration")
    if exe == "git":
        # Drop message data before checking git flags and configuration keys.
        filtered = []
        skip = False
        for value in args:
            if skip:
                skip = False
                continue
            if value.startswith("--message="):
                continue
            filtered.append(value)
            skip = value == "--message" or bool(re.match(r"^-[A-Za-z]*m$", value))
        args = filtered
        if "--no-verify" in args:
            block("skipping hooks")
        if any("core.hookspath" in x.lower() for x in args):
            # Read-only queries cannot excuse a separate -c/--config-env
            # override, even when the command itself is config --get.
            override = any((x == "-c" and i + 1 < len(args) and "core.hookspath" in args[i + 1].lower()) or (x.startswith(("-c", "--config-env")) and "core.hookspath" in x.lower()) for i, x in enumerate(args))
            readonly = "config" in args and args[-1].lower() == "core.hookspath" and not override and not any(x in args for x in ("--unset", "--unset-all", "--remove-section", "--replace-all", "--add"))
            if not readonly:
                block("changing core.hooksPath")
        if "commit" in args:
            flags = args[args.index("commit") + 1:]
            # Message values are data, including combined -am/-nm flags.
            skip = False
            for flag in flags:
                if skip:
                    skip = False
                    continue
                if re.match(r"^-[A-Za-z]*n[A-Za-z]*$", flag):
                    block("git commit -n skips hooks")
                skip = flag in ("-m", "--message") or bool(re.match(r"^-[A-Za-z]*m$", flag))
    if exe == "lefthook" and "uninstall" in args:
        block("uninstalling hooks")
    if exe == "gh":
        # Strip global repository/hostname options before identifying the command.
        rest = args[1:]
        while rest and rest[0].startswith("-"):
            option = rest.pop(0)
            if option in ("-R", "--repo", "--hostname") and rest:
                rest.pop(0)
        sub = rest[1:]
        while sub and sub[0].startswith("-"):
            option = sub.pop(0)
            if option in ("-R", "--repo") and sub:
                sub.pop(0)
        if rest[:1] == ["pr"] and sub[:1] == ["merge"]:
            block("merging a PR")
        if rest[:2] in (["alias", "set"], ["alias", "import"]):
            block("defining a gh alias")
        if rest[:2] in (["repo", "delete"], ["repo", "edit"], ["release", "delete"]):
            block("changing repository settings")
        if rest[:1] == ["api"]:
            api(rest[1:])
    writers = {"tee", "cp", "mv", "rm", "ln", "unlink", "truncate", "install", "dd", "rsync", "chmod", "chflags"}
    writing = exe in writers or (exe in ("sed", "perl") and any(re.match(r"^-[A-Za-z]*i", x) or x == "--in-place" for x in args[1:]))
    writing |= exe == "git" and any(x in args for x in ("checkout", "restore", "rm", "mv"))
    targets = args[-1:] if exe == "cp" else args[1:]
    if writing and any(protected(x) for x in targets):
        block("editing guards")
    if exe in ("python", "python3", "node", "ruby") and any(re.search(r"guard-git\.sh|guard-push\.sh|install-hooks\.sh|scripts/pre-push|\.claude/(hooks|settings)|lefthook", x) for x in args[1:]):
        block("script editing guards")
    if exe == "patch" or (exe == "git" and len(args) > 1 and args[1] in ("apply", "am")):
        if any(x in args for x in ("--check", "--stat", "--numstat", "--summary", "--dry-run", "--abort", "--quit", "--show-current-patch")):
            return
        # Inspect literal patch files without executing them. Unknown inputs fail closed.
        files = [Path(x) for x in args[1:] if not x.startswith("-") and Path(x).is_file()]
        if not files:
            block("patch input cannot be checked for guard edits")
        for path in files:
            for line in path.read_text().splitlines():
                if line.startswith(("+++ ", "--- ", "diff --git ", "rename to ", "rename from ")):
                    if any(protected(x) for x in shlex.split(line)[1:]):
                        block("patch editing guards")


try:
    inspect(command)
except (OSError, ValueError):
    block("command or patch input cannot be checked")
PY
