"""Part A: the engine x cleanup matrix. Text-only cleanup through OpenRouter on saved raw transcripts
(hosted or on-device), prompt v4 and the app's guard for every cell. Each cell is the Nemotron
pairing (eval/cleanup-offline/run_nemotron_sol.py on eval/cleanup-offline, PR #327) with another
engine's transcripts and/or another cleanup model: same request, prompt, guard, one request at a time.

    OPENROUTER_API_KEY=... python3 eval/voxtral-omni/run_matrix.py [--lane I/N] [--only LABEL ...]
    python3 eval/voxtral-omni/run_matrix.py --plan        # list the cells, priority order, no requests

Engines (input/<dir>/harness_results.csv, copied unchanged; SOURCES.md says from where):
  nemotron          Nemotron 3.5 Transcribe, hosted (OpenRouter STT)
  voxtral_mini      Voxtral Mini 3B, hosted (OpenRouter STT)
  voxtral_mini_dev  Voxtral Mini 3B, on-device (the ASR screen's raw drafts, PR #331)
  oruk_dev          OrukQwen-ASR 1.7B, on-device (the ASR screen's raw drafts, PR #331)
  voxtral_small     Voxtral Small 24B, hosted (OpenRouter STT)
Cleanups (OpenRouter chat, reasoning {"effort": "low", "exclude": true}, as offline_cleanup.py sends):
  sol   openai/gpt-6.2-sol      luna  openai/gpt-6-luna
  glm   z-ai/glm-5.3-flash      oss   openai/gpt-oss-120b
Nemotron + sol is not rerun: input/nemotron_sol_v4 is that run.

Output: results/<engine>__<cleanup>_v4/{cleanup.jsonl, results.csv} (offline_cleanup.py's format;
cleanup.jsonl also records the provider that served each reply).
Budget: each request reserves its worst-case price in the shared ledger (budget.py, results/
budget.jsonl, $3.00 for the whole task) before it is sent; each cell also has a sub-cap ($0.45 for sol,
$0.10 for the others). Cells run in the owner's priority order, so if the cap stops the run, what is
missing is the least important.
"""
from __future__ import annotations

import argparse
import csv
import json
import os
import pathlib
import sys

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import budget  # noqa: E402
import offline_cleanup as cleanup  # noqa: E402

RESULTS = HERE / "results"
ENGINES = {  # name: (input dir, expected `model` column or None for on-device drafts)
    "nemotron": ("nvidia__nemotron-3.5-transcribe", "nvidia/nemotron-3.5-transcribe"),
    "voxtral_mini": ("mistralai__voxtral-mini-3b-2507", "mistralai/voxtral-mini-3b-2507"),
    "voxtral_mini_dev": ("ondevice_voxtral3", None),
    "oruk_dev": ("ondevice_oruk", None),
    "voxtral_small": ("mistralai__voxtral-small-24b-2507-stt", "mistralai/voxtral-small-24b-2507-stt"),
    # closed_asr.py's output (OpenAI gpt-transcribe, a closed model), cleaned only on request (EXTRA)
    "openai_transcribe": ("@results/openai_transcribe_plain", "gpt-transcribe"),
    # OpenAI transcription via OpenRouter (transcribe_openrouter.py into results-or-closed/; closed models)
    "gpt4o": ("@or/openai__gpt-4o-transcribe", None),
    "gpt4o_mini": ("@or/openai__gpt-4o-mini-transcribe", None),
}
# model, worst-case USD per input token, per output token (list price x 2 for routes with several
# providers; sol: the $2.5/M cache-write tariff), sub-cap USD.
CLEANUPS = {
    "sol": ("openai/gpt-6.2-sol", 2.5e-6, 10e-6, 0.45),
    "luna": ("openai/gpt-6-luna", 0.25e-6, 1.0e-6, 0.10),
    "glm": ("z-ai/glm-5.3-flash", 0.3e-6, 1.0e-6, 0.10),
    "oss": ("openai/gpt-oss-120b", 0.3e-6, 1.2e-6, 0.10),
}
# The owner's priority order (2026-11-19): Nemotron + GLM Flash first; OrukQwen on-device + {sol, GLM,
# luna}; Voxtral Mini hosted + {sol, GLM, luna}; Nemotron + {luna, gpt-oss-120b}; then the rest.
PLAN = [("nemotron", "glm"),
        ("oruk_dev", "sol"), ("oruk_dev", "glm"), ("oruk_dev", "luna"),
        ("voxtral_mini", "sol"), ("voxtral_mini", "glm"), ("voxtral_mini", "luna"),
        ("nemotron", "luna"), ("nemotron", "oss"),
        ("voxtral_mini_dev", "sol"), ("voxtral_mini_dev", "glm"), ("voxtral_mini_dev", "luna"),
        ("oruk_dev", "oss"), ("voxtral_mini", "oss"), ("voxtral_mini_dev", "oss"),
        ("voxtral_small", "sol"), ("voxtral_small", "glm"), ("voxtral_small", "luna"), ("voxtral_small", "oss")]


# Not in the default lanes: they need closed_asr.py's OpenAI transcripts first (run_all_closed.sh).
EXTRA = [("openai_transcribe", "luna"), ("openai_transcribe", "glm"), ("openai_transcribe", "sol"),
         ("gpt4o", "sol"), ("gpt4o", "glm"), ("gpt4o", "luna"),
         ("gpt4o_mini", "sol"), ("gpt4o_mini", "glm"), ("gpt4o_mini", "luna")]


def label(engine: str, clean: str) -> str:
    return f"{engine}__{clean}_v4"


def load_source(engine: str, results: pathlib.Path = RESULTS) -> pathlib.Path:
    d, model = ENGINES[engine]
    if d.startswith("@results/"):
        base = results / d[len("@results/"):]
    elif d.startswith("@or/"):
        base = HERE / "results-or-closed" / d[len("@or/"):]
    else:
        base = HERE / "input" / d
    source = base / "harness_results.csv"
    with source.open(newline="", encoding="utf-8") as stream:
        rows = list(csv.DictReader(stream))
    if len(rows) != 151 or len({r["clip_id"] for r in rows}) != 151 or any(
            r["status"] != "ok" or not r["transcript"].strip() or (model and r.get("model") != model) for r in rows):
        sys.exit(f"Expected 151 successful unique clips in {source}")
    return source


def spent(path: pathlib.Path) -> float:
    if not path.exists():
        return 0.0
    return sum(float((json.loads(l).get("usage") or {}).get("cost") or 0)
               for l in path.read_text(encoding="utf-8").splitlines() if l.strip())


def run(engine: str, clean: str, base_url: str | None, ledger: budget.Ledger, results: pathlib.Path) -> int:
    model, in_tok, out_tok, cap = CLEANUPS[clean]
    source = load_source(engine, results)
    name = label(engine, clean)
    run_spent = [spent(results / name / "cleanup.jsonl")]

    def budgeted(raw, model_, prompt, base, key, timeout, provider):
        body = cleanup.request_body(model_, prompt, raw, "openrouter")
        # A UTF-8 byte is at most one token; 256 tokens of framing on top.
        est = (256 + sum(len(m["content"].encode()) for m in body["messages"])) * in_tok + body["max_tokens"] * out_tok
        if run_spent[0] + est > cap:
            raise cleanup.UsageLimit(f"{name} sub-cap ${cap:.2f} (spent ${run_spent[0]:.4f})")
        try:
            held = ledger.reserve(est, name)
        except budget.OverBudget as e:
            raise cleanup.UsageLimit(str(e)) from None
        try:
            reply = cleanup.ask_chat(raw, model_, prompt, base, key, timeout, provider)
        except cleanup.Failed as e:
            # An HTTP error reply is not billed; a timeout or network error might be, so it keeps its hold.
            if e.reason.startswith("http_"):
                ledger.settle(held, 0.0, name)
            raise
        cost = reply["usage"].get("cost")
        ledger.settle(held, cost, name)
        run_spent[0] += float(cost) if cost is not None else held / 1e6
        return reply

    argv = ["--provider", "openrouter", "--model", model, "--prompt", str(HERE / "prompts/v4.txt"),
            "--input", str(source), "--results-dir", str(results), "--label", name,
            "--workers", "1", "--attempts", "4", "--timeout", "90", "--max-cost", f"{budget.CAP_USD}"]
    if base_url:
        argv += ["--base-url", base_url]
    saved = sys.argv
    sys.argv = [saved[0], *argv]
    cleanup._stop.clear()
    try:
        cleanup.main(chat_request=budgeted)
    except SystemExit as e:
        return 1 if isinstance(e.code, str) else int(e.code or 0)
    finally:
        sys.argv = saved
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--lane", default="0/1", help="I/N: run cells I, I+N, I+2N... of the plan (parallel lanes)")
    ap.add_argument("--only", action="append", help="run only these labels (e.g. nemotron__glm_v4)")
    ap.add_argument("--plan", action="store_true", help="print the plan and exit")
    ap.add_argument("--base-url", default=os.environ.get("OPENROUTER_BASE_URL"),
                    help="API root (tests point this at a fake server)")
    ap.add_argument("--results-dir", type=pathlib.Path, default=RESULTS)
    ap.add_argument("--ledger", type=pathlib.Path, help="default: <results-dir>/budget.jsonl")
    a = ap.parse_args()
    i, n = (int(x) for x in a.lane.split("/"))
    pool = PLAN + EXTRA if a.only else PLAN
    cells = [c for k, c in enumerate(pool) if k % n == i and (not a.only or label(*c) in a.only)]
    if a.plan:
        for k, c in enumerate(PLAN, 1):
            print(f"{k:2}. {label(*c)}  ({CLEANUPS[c[1]][0]} on {ENGINES[c[0]][0]})")
        return 0
    ledger = budget.Ledger(a.ledger or a.results_dir / "budget.jsonl", budget.CAP_USD)
    worst = 0
    for engine, clean in cells:
        code = run(engine, clean, a.base_url, ledger, a.results_dir)
        print(f"part A {label(engine, clean)}: exit {code}; ledger ${ledger.used_usd():.4f} of ${budget.CAP_USD:.2f}",
              flush=True)
        worst = max(worst, code)
        if code == 3 and ledger.used_usd() >= budget.CAP_USD - 0.01:
            break
    return worst


if __name__ == "__main__":
    raise SystemExit(main())
