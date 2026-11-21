"""The three judges for the Voice Arena AMEND / ZER metric, behind one interface (eval/judge.py is the command).

    judge = make_judge("claude")      # or "codex", "gemini"

Every judge answers the same question, the amend-va v1 prompt, with the same schema ({"fixed": string});
only the CLI that carries the request differs:
  claude  Claude Opus 5.5 through `claude -p` (Claude Code subscription), effort max: amend-va's defaults.
  codex   GPT-6 Astra through `codex exec` (ChatGPT plan), effort max, read-only sandbox, tools off.
  gemini  `agy -p` (Antigravity, owner subscription): GEMINI_JUDGE_MODEL, default a Flash model, not Argon
          (Argon's quota ran out partway through the Voxtral/omni run, eval/reports/voxtral-omni.md).
Each ask() checks that the answer came from the requested model before it is accepted.
"""
from __future__ import annotations

import json
import os
import pathlib
import re
import shutil
import subprocess
import tempfile

SCHEMA = {"type": "object", "properties": {"fixed": {"type": "string"}},
          "required": ["fixed"], "additionalProperties": False}
SYSTEM = "You are evaluating dictation output. Follow the user's instructions exactly."
PROMPT_VERSION = "v1"


class UsageLimit(RuntimeError):
    """The judge's plan reports a usage or rate limit: stop the run, keep the cache."""


class Judge:
    """name: short id (the cache file and --judge value). model/effort: what the answer must come from.
    label: the unofficial-result label stored on every row. via: how the request travels."""

    name = ""
    model = ""
    effort = ""
    label = ""
    default_workers = 4

    def via(self) -> str:
        return self.name

    def ask(self, prompt: str, intended: str, workdir: str) -> dict:
        """Send the prompt, check the answer came from self.model, return {"fixed": str, ...extras}.
        Raise UsageLimit on a usage limit, RuntimeError on any other failure (the caller retries)."""
        raise NotImplementedError

    def close(self) -> None:
        pass


def clean_env() -> dict:
    # Orca's hooks report agent status when ORCA_* is set; a judge call must not.
    return {k: v for k, v in os.environ.items() if not k.startswith("ORCA_")}


def parse_fixed(structured, text: str, intended: str) -> str:
    """amend-va's parsing: structured output first, else the JSON text; an empty answer for a non-empty
    intended text is a failure (amend-va retries these)."""
    out = structured
    if not isinstance(out, dict) or not isinstance(out.get("fixed"), str):
        out = json.loads(str(text).strip())
    fixed = out["fixed"].strip()
    if not fixed and intended.strip():
        raise RuntimeError("empty answer for a non-empty intended text (amend-va retries these)")
    return fixed


def slug(s: str) -> str:
    return re.sub(r"[^A-Za-z0-9._-]+", "-", s).strip("-")



# --- claude ------------------------------------------------------------
USAGE_CLAUDE = re.compile(r"usage limit|rate limit|quota|too many requests|\b429\b", re.I)


class ClaudeJudge(Judge):
    name = "claude"
    effort = "max"
    label = "unofficial: single judge (Claude Opus 5.5 via Claude Code), public set only"

    def __init__(self, model: str | None = None):
        self.model = model or "claude-opus-5-5"

    def via(self) -> str:
        return "claude -p (Claude Code CLI)"

    def command(self) -> list[str]:
        return ["claude", "-p", "--model", self.model, "--effort", self.effort, "--tools", "",
                "--no-session-persistence", "--setting-sources", "", "--strict-mcp-config",
                "--system-prompt", SYSTEM, "--output-format", "json", "--json-schema", json.dumps(SCHEMA)]

    def ask(self, prompt: str, intended: str, workdir: str, timeout: int = 900) -> dict:
        p = subprocess.run(self.command(), input=prompt, capture_output=True, text=True, cwd=workdir,
                           timeout=timeout)
        if p.returncode != 0:
            if USAGE_CLAUDE.search(p.stderr):
                raise UsageLimit(p.stderr[-300:])
            raise RuntimeError(f"claude exited {p.returncode}: {p.stderr[-300:]}")
        d = json.loads(p.stdout)
        if d.get("is_error") or d.get("subtype") != "success":
            raise RuntimeError(f"claude error: {d.get('subtype')} {str(d.get('result'))[:200]}")
        usage = d.get("modelUsage") or {}
        if self.model not in usage:
            raise RuntimeError(f"the answer did not come from {self.model}: {list(usage)}")
        fixed = parse_fixed(d.get("structured_output"), d.get("result", ""), intended)
        return {"fixed": fixed, "model": self.model, "output_tokens": usage[self.model].get("outputTokens"),
                "cost_usd": d.get("total_cost_usd")}



# --- codex -------------------------------------------------------------
USAGE_CODEX = re.compile(r"usage limit|rate limit|quota|too many requests|429", re.I)
# Codex features that add tools or context a judge doesn't need. Unknown names are ignored by Codex.
OFF = ["hooks", "apps", "plugins", "shell_tool", "unified_exec", "goals", "view_image", "image_generation",
       "browser_use", "browser_use_external", "computer_use", "tool_suggest", "sleep_tool", "skill_search",
       "code_mode_host", "multi_agent", "remote_plugin", "plugin_discovery_prefetch", "in_app_browser"]


def header(stderr: str, key: str) -> str | None:
    m = re.search(rf"^{re.escape(key)}: (.+)$", stderr, re.M)
    return m.group(1).strip() if m else None


class CodexJudge(Judge):
    name = "codex"
    effort = "max"
    label = "unofficial: second judge (GPT-6 Astra via Codex), public set only"

    def __init__(self, model: str | None = None):
        self.model = model or "gpt-6-astra"
        self._dir = tempfile.TemporaryDirectory()
        self.schema = os.path.join(self._dir.name, "schema.json")
        self.system = os.path.join(self._dir.name, "system.txt")
        pathlib.Path(self.schema).write_text(json.dumps(SCHEMA))
        pathlib.Path(self.system).write_text(SYSTEM)

    def close(self) -> None:
        self._dir.cleanup()

    def via(self) -> str:
        return "codex exec (ChatGPT plan, read-only sandbox)"

    def command(self, workdir: str, last: str) -> list[str]:
        cmd = ["codex", "exec", "--ephemeral", "--skip-git-repo-check", "-s", "read-only", "-C", workdir,
               "-m", self.model, "-c", f'model_reasoning_effort="{self.effort}"',
               "-c", f"model_instructions_file={json.dumps(self.system)}",
               "-c", "include_environment_context=false", "-c", "include_permissions_instructions=false",
               "-c", "include_collaboration_mode_instructions=false", "-c", "include_apps_instructions=false",
               "-c", "skills.include_instructions=false", "-c", 'web_search="disabled"',
               "--output-schema", self.schema, "-o", last]
        for f in OFF:
            cmd += ["-c", f"features.{f}=false"]
        return cmd + ["-"]

    def ask(self, prompt: str, intended: str, workdir: str, timeout: int = 900) -> dict:
        with tempfile.NamedTemporaryFile("r", suffix=".txt", dir=workdir, delete=False) as fh:
            last = fh.name
        try:
            p = subprocess.run(self.command(workdir, last), input=prompt, capture_output=True, text=True,
                               cwd=workdir, timeout=timeout, env=clean_env())
            raw = pathlib.Path(last).read_text(encoding="utf-8").strip() if os.path.exists(last) else ""
        finally:
            if os.path.exists(last):
                os.unlink(last)
        if p.returncode != 0:
            # The prompt is echoed in Codex's log, so only Codex's own error lines are matched.
            errs = "\n".join(l for l in p.stderr.splitlines() if re.match(r"^(ERROR|error|Error)", l))
            if USAGE_CODEX.search(errs):
                raise UsageLimit(errs[-300:])
            raise RuntimeError(f"codex exited {p.returncode}: {p.stderr[-400:]}")
        model, effort, sandbox = (header(p.stderr, k) for k in ("model", "reasoning effort", "sandbox"))
        if model != self.model or effort != self.effort or sandbox != "read-only":
            raise RuntimeError(f"codex ran {model}/{effort}/{sandbox}, not {self.model}/{self.effort}/read-only")
        fixed = parse_fixed(None, raw, intended)
        tok = re.search(r"tokens used\s*\n\s*([\d,]+)", p.stderr)
        return {"fixed": fixed, "model": model, "tokens_used": int(tok.group(1).replace(",", "")) if tok else None}



# --- gemini ------------------------------------------------------------
DEFAULT_MODEL = "gemini-3.8-flash"
USAGE_GEMINI = re.compile(r"usage limit|rate limit|rate-limit|quota|too many requests|resource.?exhausted|\b429\b", re.I)


def agy_bin() -> str:
    return os.environ.get("AGY_BIN") or shutil.which("agy") or os.path.expanduser("~/.local/bin/agy")


class GeminiJudge(Judge):
    name = "gemini"
    effort = "default"
    default_workers = 6

    def __init__(self, model: str | None = None):
        self.model = model or os.environ.get("GEMINI_JUDGE_MODEL") or DEFAULT_MODEL
        self.label = f"unofficial: third judge ({self.model} via Antigravity), public set only"

    def via(self) -> str:
        return "agy -p (Antigravity CLI, --sandbox --mode plan, owner subscription)"

    def command(self, prompt: str, log: str) -> list[str]:
        return [agy_bin(), "--model", self.model, "--output-format", "json", "--sandbox", "--mode", "plan",
                "--json-schema", json.dumps(SCHEMA), "--log-file", log, f"-p={prompt}"]

    def ask(self, prompt: str, intended: str, workdir: str, timeout: int = 1200) -> dict:
        with tempfile.TemporaryDirectory() as wd:
            log = os.path.join(wd, "agy.log")
            p = subprocess.run(self.command(prompt, log), stdin=subprocess.DEVNULL, capture_output=True,
                               text=True, cwd=wd, timeout=timeout, env=clean_env())
            logtext = pathlib.Path(log).read_text(encoding="utf-8", errors="replace") if os.path.exists(log) else ""
        try:
            d = json.loads(p.stdout)
        except ValueError:
            d = None
        # Only agy's own status/error text is matched, never the echoed prompt.
        status = str(d.get("status")) if isinstance(d, dict) else ""
        errtext = p.stderr if d is None else (p.stderr + " " + status + " " + str(d.get("error", "")))
        errs = "\n".join(l for l in logtext.splitlines() if l[:1] == "E" and "not logged into" not in l)
        if p.returncode != 0 or not isinstance(d, dict) or status != "SUCCESS":
            if USAGE_GEMINI.search(errtext) or USAGE_GEMINI.search(errs):
                raise UsageLimit((errtext.strip() or errs)[-300:])
            raise RuntimeError(f"agy exited {p.returncode} status={status!r}: {p.stderr[-300:]}")
        m = re.search(r'Print mode: starting \(.*?model="([^"]+)"', logtext)
        model = m.group(1) if m else None
        if model != self.model:
            raise RuntimeError(f"agy ran model {model!r}, not {self.model}")
        fixed = parse_fixed(d.get("structured_output"), d.get("response", ""), intended)
        u = d.get("usage") or {}
        return {"fixed": fixed, "model": model, "output_tokens": u.get("output_tokens"),
                "thinking_tokens": u.get("thinking_tokens"), "input_tokens": u.get("input_tokens"),
                "seconds": d.get("duration_seconds")}

# --- factory -----------------------------------------------------------
NAMES = ("claude", "codex", "gemini")


def make_judge(name: str, model: str | None = None) -> Judge:
    classes = {"claude": ClaudeJudge, "codex": CodexJudge, "gemini": GeminiJudge}
    if name not in classes:
        raise ValueError(f"unknown judge {name!r}; one of {', '.join(NAMES)}")
    return classes[name](model)

