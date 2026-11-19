"""Offline LLM cleanup of saved Whisper text: raw text -> prompt + model -> the app's guard -> a
harness-format results CSV that the judges (eval/voicearena/score_claude_cli.py, score_codex_cli.py)
score unchanged.

    python3 eval/cleanup-offline/offline_cleanup.py --provider codex --model gpt-6-luna \\
        --prompt eval/cleanup-offline/prompts/v1-blurt-smart.txt --label luna_v1
    GROQ_API_KEY=... python3 eval/cleanup-offline/offline_cleanup.py --provider groq \\
        --model openai/gpt-oss-20b --prompt eval/cleanup-offline/prompts/v1-blurt-smart.txt --label oss20b_v1

Input: input/raw_whisper.csv, the stripped harness_results.csv of the Groq cleanup v1 VM run's raw
Whisper drafts (groq_clean_v1_raw, 151 clips). Its `transcript` is the raw Whisper text.

Output, in results/<label>/:
  results.csv          the input rows with `transcript` = the pasted text (cleaned reply, or the raw
                       text when the guard refused it), plus raw_text, cleaned_text (the model's reply),
                       reason, model_ms, cleanup_provider, cleanup_model, cleanup_effort, prompt_name,
                       prompt_sha. finalize_s is blank: offline there is no key-up-to-paste latency.
  cleanup.jsonl        the cache: one reply per request, keyed by the sha256 of provider, model, effort,
                       system prompt and user message, so a rerun only asks what it hasn't asked.

The request is the app's (crates/transcribe/src/groq_cleanup.rs on exp/groq-cleanup): the prompt file's
text, trimmed, as the system prompt; `<transcript>\\n{raw}\\n</transcript>` as the user message; the reply
trimmed and stripped of the tags; then guard.judge. An empty raw text is pasted as is without a request.

Providers:
  codex  `codex exec` per clip (the owner's ChatGPT plan), read-only sandbox, tools and web search off,
         the cleanup prompt as Codex's model instructions (replacing Codex's own agent prompt), the user
         message on stdin, the last message as the reply. Codex still adds its tool definitions and a
         role note, and its timing is a CLI round trip, not API latency: this measures quality only.
         Stops at the first usage or rate-limit message.
  groq   OpenAI-compatible POST {base_url}/chat/completions with the app's request body (temperature 0,
         max_completion_tokens = chars/2 + 1280 and reasoning_effort "low" for gpt-oss). The key is read
         from GROQ_API_KEY in this process's environment only; it is never printed, logged or stored.
         The app's 3 s timeout is --timeout (default 30 s offline; pass 3 to mimic the app).
  openrouter  The same chat request to https://openrouter.ai/api/v1 (OpenAI-compatible) for open-weight
         models: temperature 0, max_tokens = chars/2 + 1280, reasoning {"effort": "low", "exclude": true}
         (models without reasoning ignore it), usage {"include": true} so each reply carries its cost.
         Key from OPENROUTER_API_KEY only. --max-cost (USD) stops every run once the OpenRouter spend
         recorded in all results/*/cleanup.jsonl reaches it. model_ms here is real API time (through
         OpenRouter's router, default provider routing).
"""
from __future__ import annotations

import argparse
import concurrent.futures as cf
import csv
import hashlib
import json
import os
import pathlib
import re
import subprocess
import sys
import tempfile
import threading
import time
import urllib.error
import urllib.request

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import guard  # noqa: E402

USAGE_LIMIT = re.compile(r"usage limit|rate limit|quota|too many requests|429", re.I)
# Codex features that add tools or context a cleanup doesn't need (as score_codex_cli.py).
OFF = ["hooks", "apps", "plugins", "shell_tool", "unified_exec", "goals", "view_image", "image_generation",
       "browser_use", "browser_use_external", "computer_use", "tool_suggest", "sleep_tool", "skill_search",
       "code_mode_host", "multi_agent", "remote_plugin", "plugin_discovery_prefetch", "in_app_browser"]
_lock = threading.Lock()
_stop = threading.Event()


class UsageLimit(RuntimeError):
    pass


class Failed(RuntimeError):
    """A request that got no usable answer; .reason is the app's reason name (timeout, http_429...)."""

    def __init__(self, reason: str, detail: str = ""):
        super().__init__(f"{reason}: {detail}")
        self.reason = reason


def prompt_sha(prompt: str) -> str:
    return hashlib.sha256(prompt.encode()).hexdigest()[:12]


# --- codex -----------------------------------------------------------------------------------------

def codex_cmd(workdir: str, model: str, effort: str, instructions: str, last: str) -> list[str]:
    cmd = ["codex", "exec", "--ephemeral", "--skip-git-repo-check", "-s", "read-only", "-C", workdir,
           "-m", model, "-c", f'model_reasoning_effort="{effort}"',
           "-c", f"model_instructions_file={json.dumps(instructions)}",
           "-c", "include_environment_context=false", "-c", "include_permissions_instructions=false",
           "-c", "include_collaboration_mode_instructions=false", "-c", "include_apps_instructions=false",
           "-c", "skills.include_instructions=false", "-c", 'web_search="disabled"', "-o", last]
    for f in OFF:
        cmd += ["-c", f"features.{f}=false"]
    return cmd + ["-"]


def _header(stderr: str, key: str) -> str | None:
    m = re.search(rf"^{re.escape(key)}: (.+)$", stderr, re.M)
    return m.group(1).strip() if m else None


def ask_codex(user: str, model: str, effort: str, workdir: str, instructions: str, timeout: float) -> dict:
    env = {k: v for k, v in os.environ.items() if not k.startswith("ORCA_")}
    with tempfile.NamedTemporaryFile("r", suffix=".txt", dir=workdir, delete=False) as fh:
        last = fh.name
    try:
        try:
            p = subprocess.run(codex_cmd(workdir, model, effort, instructions, last), input=user,
                               capture_output=True, text=True, cwd=workdir, timeout=timeout, env=env)
        except subprocess.TimeoutExpired as e:
            raise Failed("timeout", f"codex took over {timeout}s") from e
        content = pathlib.Path(last).read_text(encoding="utf-8") if os.path.exists(last) else ""
    finally:
        if os.path.exists(last):
            os.unlink(last)
    if p.returncode != 0:
        # The prompt is echoed in Codex's log, so only Codex's own error lines are matched.
        errs = "\n".join(line for line in p.stderr.splitlines() if re.match(r"^(ERROR|error|Error)", line))
        if USAGE_LIMIT.search(errs):
            raise UsageLimit(errs[-300:])
        raise Failed("codex_error", f"exit {p.returncode}: {errs[-300:] or p.stderr[-300:]}")
    got_model, got_effort, sandbox = (_header(p.stderr, k) for k in ("model", "reasoning effort", "sandbox"))
    if got_model != model or got_effort != effort or sandbox != "read-only":
        raise Failed("codex_error", f"ran {got_model}/{got_effort}/{sandbox}, not {model}/{effort}/read-only")
    tok = re.search(r"tokens used\s*\n\s*([\d,]+)", p.stderr)
    return {"content": content, "tokens_used": int(tok.group(1).replace(",", "")) if tok else None}


# --- groq (OpenAI-compatible chat completions) -------------------------------------------------------

KEY_ENV = {"groq": "GROQ_API_KEY", "openrouter": "OPENROUTER_API_KEY"}
BASE_URL = {"groq": "https://api.groq.com/openai/v1", "openrouter": "https://openrouter.ai/api/v1"}


def request_body(model: str, prompt: str, raw: str, provider: str = "groq") -> dict:
    """groq: groq_cleanup.rs request_body, field for field. openrouter: the same messages and
    temperature, OpenRouter's names for the token budget and reasoning, and the cost in the reply."""
    if provider == "openrouter":
        return {"model": model, "temperature": 0, "max_tokens": len(raw) // 2 + 1280,
                "messages": [{"role": "system", "content": prompt},
                             {"role": "user", "content": guard.user_message(raw)}],
                "reasoning": {"effort": "low", "exclude": True}, "usage": {"include": True}}
    reasoning = model.startswith("openai/gpt-oss")
    budget = len(raw) // 2 + (1280 if reasoning else 256)
    body = {"model": model, "temperature": 0, "max_completion_tokens": budget,
            "messages": [{"role": "system", "content": prompt},
                         {"role": "user", "content": guard.user_message(raw)}]}
    if reasoning:
        body["reasoning_effort"] = "low"
        body["include_reasoning"] = False
    return body


def ask_chat(raw: str, model: str, prompt: str, base_url: str, key: str, timeout: float,
             provider: str = "groq") -> dict:
    req = urllib.request.Request(
        base_url.rstrip("/") + "/chat/completions", method="POST",
        data=json.dumps(request_body(model, prompt, raw, provider)).encode(),
        headers={"Authorization": f"Bearer {key}", "Content-Type": "application/json",
                 "User-Agent": "offline-cleanup-eval"})
    try:
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            body = resp.read()
    except urllib.error.HTTPError as e:
        # The body isn't read or logged: it could echo the request.
        raise Failed(f"http_{e.code}", e.headers.get("retry-after", "") or "") from None
    except TimeoutError:
        raise Failed("timeout") from None
    except urllib.error.URLError as e:
        if isinstance(e.reason, TimeoutError):
            raise Failed("timeout") from None
        raise Failed("network", type(e.reason).__name__) from None
    text = guard.parse_reply(body)
    if text is None:
        raise Failed("bad_reply")
    usage = {}
    try:
        usage = json.loads(body).get("usage") or {}
    except ValueError:
        pass
    kept = {k: usage.get(k) for k in ("prompt_tokens", "completion_tokens")}
    if provider == "openrouter":
        kept["cost"] = usage.get("cost")
        details = usage.get("completion_tokens_details") or {}
        kept["reasoning_tokens"] = details.get("reasoning_tokens")
    return {"content": text, "usage": kept}


def openrouter_spend(results_dir: pathlib.Path) -> float:
    """USD recorded by every OpenRouter reply in results_dir/*/cleanup.jsonl."""
    total = 0.0
    for path in results_dir.glob("*/cleanup.jsonl"):
        for line in path.read_text(encoding="utf-8").splitlines():
            if '"openrouter"' in line:
                r = json.loads(line)
                if r.get("provider") == "openrouter":
                    total += float((r.get("usage") or {}).get("cost") or 0)
    return total


# --- run ----------------------------------------------------------------------------------------------

def main(chat_request=ask_chat) -> None:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--provider", choices=["codex", "groq", "openrouter"], required=True)
    ap.add_argument("--model", required=True)
    ap.add_argument("--effort", default="low", help="codex reasoning effort (default low)")
    ap.add_argument("--prompt", type=pathlib.Path, required=True)
    ap.add_argument("--label", required=True, help="results/<label>/")
    ap.add_argument("--input", type=pathlib.Path, default=HERE / "input" / "raw_whisper.csv")
    ap.add_argument("--results-dir", type=pathlib.Path, default=HERE / "results")
    ap.add_argument("--base-url", help="API root (default: the provider's)")
    ap.add_argument("--max-cost", type=float, default=2.0, help="openrouter: stop at this total USD spend")
    ap.add_argument("--timeout", type=float, help="per request, seconds (groq 30, codex 300)")
    ap.add_argument("--workers", type=int, help="parallel requests (codex 4, groq 2)")
    ap.add_argument("--attempts", type=int, default=3)
    ap.add_argument("--limit", type=int, help="only the first N clips (smoke test)")
    ap.add_argument("--max-calls", type=int, default=200, help="stop after this many model calls")
    a = ap.parse_args()

    prompt = a.prompt.read_text(encoding="utf-8").strip()  # the app trims the eval prompt file
    if not prompt:
        sys.exit("empty prompt file")
    psha = prompt_sha(prompt)
    effort = a.effort if a.provider == "codex" else (
        "low" if a.provider == "openrouter" or a.model.startswith("openai/gpt-oss") else "")
    base_url = a.base_url or BASE_URL.get(a.provider, "")
    timeout = a.timeout or (300.0 if a.provider == "codex" else 30.0)
    workers = a.workers or (4 if a.provider == "codex" else 2)
    key = ""
    if a.provider in KEY_ENV:
        key = os.environ.get(KEY_ENV[a.provider], "")
        if not key:
            sys.exit(f"{KEY_ENV[a.provider]} is not set in this process's environment")
    spent = [openrouter_spend(a.results_dir) if a.provider == "openrouter" else 0.0]
    if a.provider == "openrouter":
        print(f"OpenRouter spend so far ${spent[0]:.4f} of the ${a.max_cost:.2f} cap", file=sys.stderr)

    rows = list(csv.DictReader(a.input.open(newline="", encoding="utf-8")))
    if a.limit:
        rows = rows[: a.limit]
    out_dir = a.results_dir / a.label
    out_dir.mkdir(parents=True, exist_ok=True)
    cache_path = out_dir / "cleanup.jsonl"
    cache = {}
    if cache_path.exists():
        for line in cache_path.read_text(encoding="utf-8").splitlines():
            if line.strip():
                r = json.loads(line)
                cache[r["request_sha256"]] = r

    def rkey(raw: str) -> str:
        return hashlib.sha256(json.dumps([a.provider, a.model, effort, prompt, guard.user_message(raw)])
                              .encode()).hexdigest()

    todo = {}
    for r in rows:
        raw = r["transcript"] if r.get("status") == "ok" else ""
        if raw.strip() and rkey(raw) not in cache:
            todo[rkey(raw)] = raw
    print(f"{len(rows)} clips, {len(todo)} to clean with {a.provider}:{a.model} effort={effort or '-'} "
          f"prompt {a.prompt.name} ({psha}), {workers} workers", file=sys.stderr)

    calls = [0]
    failures: dict[str, str] = {}
    limit_hit: list[str] = []
    with tempfile.TemporaryDirectory() as workdir, cf.ThreadPoolExecutor(workers) as pool:
        instructions = os.path.join(workdir, "instructions.txt")
        pathlib.Path(instructions).write_text(prompt, encoding="utf-8")

        def job(item):
            sha, raw = item
            for attempt in range(1, a.attempts + 1):
                with _lock:
                    if a.provider == "openrouter" and spent[0] >= a.max_cost:
                        if not _stop.is_set():
                            print(f"OpenRouter spend ${spent[0]:.4f} reached the ${a.max_cost:.2f} cap, stopping",
                                  file=sys.stderr)
                            limit_hit.append("cost_cap")
                        _stop.set()
                        failures[sha] = "cost_cap"
                        return
                    if _stop.is_set() or calls[0] >= a.max_calls:
                        return
                    calls[0] += 1
                started = time.monotonic()
                try:
                    if a.provider == "codex":
                        ans = ask_codex(guard.user_message(raw), a.model, effort, workdir, instructions, timeout)
                        ans["content"] = guard.strip_tags(ans["content"])
                    else:
                        ans = chat_request(raw, a.model, prompt, base_url, key, timeout, a.provider)
                        with _lock:
                            spent[0] += float((ans.get("usage") or {}).get("cost") or 0)
                    ms = int((time.monotonic() - started) * 1000)
                    rec = {"request_sha256": sha, "provider": a.provider, "model": a.model, "effort": effort,
                           "prompt_sha": psha, "reply": ans["content"], "model_ms": ms,
                           **{k: v for k, v in ans.items() if k != "content"},
                           "at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())}
                    with _lock, cache_path.open("a", encoding="utf-8") as fh:
                        fh.write(json.dumps(rec, ensure_ascii=False) + "\n")
                        cache[sha] = rec
                    failures.pop(sha, None)
                    return
                except UsageLimit as e:
                    print(f"usage/rate limit, stopping: {e}", file=sys.stderr)
                    limit_hit.append(str(e))
                    _stop.set()
                    return
                except Failed as e:
                    failures[sha] = e.reason
                    print(f"attempt {attempt} failed: {str(e)[:300]}", file=sys.stderr)
                    if a.provider != "codex" and e.reason == "http_429" and attempt == a.attempts:
                        print("rate limit after retries, stopping", file=sys.stderr)
                        limit_hit.append("http_429")
                        _stop.set()
                        return
                    wait = 5 * attempt
                    if e.reason == "http_429":
                        try:
                            wait = max(float(str(e).split(": ", 1)[1] or 0), 15 * attempt)
                        except ValueError:
                            wait = 15 * attempt
                    time.sleep(min(wait, 120))

        done = 0
        for _ in pool.map(job, sorted(todo.items())):
            done += 1
            if done % 10 == 0:
                print(f"  cleaned {done}/{len(todo)}", file=sys.stderr)

    extra = ["raw_text", "cleaned_text", "reason", "model_ms", "cleanup_provider", "cleanup_model",
             "cleanup_effort", "prompt_name", "prompt_sha"]
    fields = list(rows[0].keys()) + [f for f in extra if f not in rows[0]]
    reasons: dict[str, int] = {}
    missing = 0
    with (out_dir / "results.csv").open("w", newline="", encoding="utf-8") as fh:
        w = csv.DictWriter(fh, fields)
        w.writeheader()
        for r in rows:
            raw = r["transcript"] if r.get("status") == "ok" else ""
            out = dict(r)
            out.update(raw_text=raw, finalize_s="", cleanup_provider=a.provider, cleanup_model=a.model,
                       cleanup_effort=effort, prompt_name=a.prompt.stem, prompt_sha=psha)
            if not raw.strip():
                out.update(transcript=raw, cleaned_text="", reason="empty", model_ms=0)
            elif rkey(raw) in cache:
                c = cache[rkey(raw)]
                reason = guard.judge(raw, c["reply"], prompt)
                out.update(transcript=c["reply"] if reason == "cleaned" else raw, cleaned_text=c["reply"],
                           reason=reason, model_ms=c["model_ms"])
            else:
                missing += 1
                # Not cleaned in this run (failure or a call limit): the raw text, as the app pastes.
                out.update(transcript=raw, cleaned_text="", reason=failures.get(rkey(raw), "not_run"), model_ms="")
            reasons[out["reason"]] = reasons.get(out["reason"], 0) + 1
            w.writerow(out)
    cost = f"  OpenRouter spend ${spent[0]:.4f}" if a.provider == "openrouter" else ""
    print(f"{a.label}: {reasons}  calls this run {calls[0]}{cost}  -> {out_dir / 'results.csv'}")
    if limit_hit:
        sys.exit(3)
    if missing:
        sys.exit(1)


if __name__ == "__main__":
    main()
