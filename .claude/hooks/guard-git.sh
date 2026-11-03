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


def check(args, depth):
    if not args:
        return
    for value in args:
        if re.match(r"(?i)^LEFTHOOK=(0|false)$|^LEFTHOOK_EXCLUDE=.+", value):
            block("disabling hooks")
        if "=" not in value and value not in ("env", "export", "sudo", "command", "exec", "time"):
            break
    # Locate the executable after environment assignments and common wrappers.
    start = 0
    while start < len(args) and ("=" in args[start] or args[start] in ("env", "sudo", "command", "exec", "time", "!")):
        start += 1
    if start == len(args):
        return
    args = args[start:]
    exe = os.path.basename(args[0]).lstrip("\\")
    if exe in ("bash", "sh", "zsh") and "-c" in args:
        at = args.index("-c")
        if at + 1 < len(args):
            inspect(args[at + 1], depth + 1)
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
        if any(re.match(r"(?i)^core\.hookspath(?:=|$)", x) for x in args):
            # Read-only config queries are harmless.
            readonly = "config" in args and args[-1].lower() == "core.hookspath" and not any(x in args for x in ("-c", "--unset", "--unset-all", "--remove-section"))
            if not readonly and not any(x in args for x in ("--get", "--get-all", "--get-regexp")):
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
            method = "GET"
            endpoint = ""
            skip = False
            for i, value in enumerate(rest[1:], 1):
                if skip:
                    skip = False
                    continue
                if value in ("-X", "--method") and i + 1 < len(rest):
                    method = rest[i + 1].upper()
                    skip = True
                elif value.startswith("--method=") or value.startswith("-X"):
                    method = value.split("=", 1)[1].upper() if "=" in value else value[2:].upper()
                elif value in ("-f", "-F", "--field", "--raw-field", "--input"):
                    if method == "GET":
                        method = "POST"
                    skip = True
                elif not value.startswith("-") and not endpoint:
                    endpoint = value.rstrip("/")
            # Explicit GET always wins over field options, regardless of order.
            if any(x in ("GET", "--method=GET", "-XGET") for x in rest):
                method = "GET"
            if method == "GET":
                return
            if (method == "PUT" and re.search(r"/pulls/\d+/merge$", endpoint)) or (method == "POST" and endpoint.endswith("/merges")):
                block("merging through the API")
            if method in ("DELETE", "PATCH", "PUT") and re.search(r"(?:^|/)repos/[^/]+/[^/]+(?:$|/(?:rules|rulesets|branches|tags|git/refs|settings|interaction-limits)(?:/|$))", endpoint):
                block("destructive repository API request")
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
