"""Part A: GPT-6.2-sol (reasoning low) + prompt v4 + the app's guard, through OpenRouter, on the saved
hosted Voxtral Mini 3B and Voxtral Small 24B raw transcripts. Exactly the Nemotron pairing
(eval/cleanup-offline/run_nemotron_sol.py on eval/cleanup-offline, PR #327): same request, model,
prompt, guard and one request at a time; only the input transcripts and the budget ledger differ.

    OPENROUTER_API_KEY=... python3 eval/voxtral-omni/run_two_pass.py [mini|small ...] [--base-url URL]

Input: input/<model label>/harness_results.csv, copied unchanged from
eval/hosted-asr/results/<model label>/ (branch eval/hosted-asr, PR #330): 151 ok clips each.
Output: results/<mini|small>_sol_v4/{cleanup.jsonl, results.csv} (offline_cleanup.py's format).
Every OpenRouter request reserves its worst-case price in the shared ledger (budget.py, results/
budget.jsonl, $2.00 cap for parts A and B together) before it is sent, and each run also stops
before its own sub-cap (--run-cap, default $0.45: about 1.5x the Nemotron run's $0.29).
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

MODEL = "openai/gpt-6.2-sol"
SOURCES = {
    "mini": ("mistralai__voxtral-mini-3b-2507", "mistralai/voxtral-mini-3b-2507"),
    "small": ("mistralai__voxtral-small-24b-2507-stt", "mistralai/voxtral-small-24b-2507-stt"),
}
RESULTS = HERE / "results"
CAP_USD = 2.00
# OpenRouter list prices for openai/gpt-6.2-sol (2026-11-19): $2/M input ($2.5/M cache write),
# $10/M output. A UTF-8 byte is at most one token; 256 tokens of framing on top.
IN_PER_TOKEN, OUT_PER_TOKEN = 2.5e-6, 10e-6


def worst_case(raw: str, prompt: str) -> float:
    body = cleanup.request_body(MODEL, prompt, raw, "openrouter")
    inputs = 256 + sum(len(m["content"].encode()) for m in body["messages"])
    return inputs * IN_PER_TOKEN + body["max_tokens"] * OUT_PER_TOKEN


def run(which: str, base_url: str | None, run_cap: float, ledger: budget.Ledger,
        results: pathlib.Path = RESULTS) -> int:
    label_dir, model = SOURCES[which]
    source = HERE / "input" / label_dir / "harness_results.csv"
    with source.open(newline="", encoding="utf-8") as stream:
        rows = list(csv.DictReader(stream))
    if len(rows) != 151 or len({r["clip_id"] for r in rows}) != 151 or any(
            r["model"] != model or r["status"] != "ok" or not r["transcript"].strip() for r in rows):
        sys.exit(f"Expected 151 successful unique {model} clips in {source}")
    label = f"{which}_sol_v4"
    run_spent = [_spend(results / label / "cleanup.jsonl")]

    def budgeted(raw, model_, prompt, base, key, timeout, provider):
        est = worst_case(raw, prompt)
        if run_spent[0] + est > run_cap:
            raise cleanup.UsageLimit(f"{label} sub-cap ${run_cap:.2f} (spent ${run_spent[0]:.4f})")
        try:
            held = ledger.reserve(est, label)
        except budget.OverBudget as e:
            raise cleanup.UsageLimit(str(e)) from None
        try:
            reply = cleanup.ask_chat(raw, model_, prompt, base, key, timeout, provider)
        except cleanup.Failed as e:
            # An HTTP error reply is not billed; a timeout or network error might be, so it keeps
            # its reservation.
            if e.reason.startswith("http_"):
                ledger.settle(held, 0.0, label)
            raise
        cost = reply["usage"].get("cost")
        ledger.settle(held, cost, label)
        run_spent[0] += float(cost) if cost is not None else held / 1e6
        return reply

    argv = ["--provider", "openrouter", "--model", MODEL, "--prompt", str(HERE / "prompts/v4.txt"),
            "--input", str(source), "--results-dir", str(results), "--label", label,
            "--workers", "1", "--attempts", "4", "--timeout", "90", "--max-cost", f"{CAP_USD}"]
    if base_url:
        argv += ["--base-url", base_url]
    saved = sys.argv
    sys.argv = [saved[0], *argv]
    try:
        cleanup.main(chat_request=budgeted)
    except SystemExit as e:
        return int(e.code or 0) if not isinstance(e.code, str) else 1
    finally:
        sys.argv = saved
    return 0


def _spend(path: pathlib.Path) -> float:
    if not path.exists():
        return 0.0
    return sum(float((json.loads(l).get("usage") or {}).get("cost") or 0)
               for l in path.read_text(encoding="utf-8").splitlines() if l.strip())


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("which", nargs="*", choices=sorted(SOURCES), help="default: mini small")
    ap.add_argument("--base-url", default=os.environ.get("OPENROUTER_BASE_URL"),
                    help="API root (tests point this at a fake server)")
    ap.add_argument("--run-cap", type=float, default=0.45, help="USD per transcript set")
    ap.add_argument("--results-dir", type=pathlib.Path, default=RESULTS)
    ap.add_argument("--ledger", type=pathlib.Path, help="default: <results-dir>/budget.jsonl")
    a = ap.parse_args()
    a.ledger = a.ledger or a.results_dir / "budget.jsonl"
    ledger = budget.Ledger(a.ledger, CAP_USD)
    worst = 0
    for which in a.which or ["mini", "small"]:
        code = run(which, a.base_url, a.run_cap, ledger, a.results_dir)
        print(f"part A {which}: exit {code}; ledger ${ledger.used_usd():.4f} of ${CAP_USD:.2f}", flush=True)
        worst = max(worst, code)
    return worst


if __name__ == "__main__":
    raise SystemExit(main())
