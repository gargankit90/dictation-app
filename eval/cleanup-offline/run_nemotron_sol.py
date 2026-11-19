"""Owner-only OpenRouter cleanup of the saved Nemotron transcripts ($2 including retries)."""
import csv
import fcntl
import json
import math
import os
import sys
from pathlib import Path

import offline_cleanup as cleanup

HERE = Path(__file__).resolve().parent
SOURCE = HERE.parents[2] / "hosted-asr-eval/eval/hosted-asr/results/nvidia__nemotron-3.5-transcribe/harness_results.csv"
MODEL = "openai/gpt-6.2-sol"
ROOT = HERE / "paired-results"
LABEL = "nemotron_sol_v4"


def main():
    with SOURCE.open(newline="") as stream:
        rows = list(csv.DictReader(stream))
    if len(rows) != 151 or len({r["clip_id"] for r in rows}) != 151 or any(
        r["model"] != "nvidia/nemotron-3.5-transcribe" or r["status"] != "ok" or not r["transcript"].strip()
        for r in rows
    ):
        sys.exit("Expected 151 successful unique Nemotron clips")

    ROOT.mkdir(exist_ok=True)
    with (ROOT / "sol-budget.jsonl").open("a+") as ledger:
        fcntl.flock(ledger, fcntl.LOCK_EX | fcntl.LOCK_NB)
        ledger.seek(0)
        used = sum(json.loads(line)["microdollars"] for line in ledger)

        def record(amount):
            nonlocal used
            ledger.write(json.dumps({"microdollars": amount}) + "\n")
            ledger.flush()
            os.fsync(ledger.fileno())
            used += amount

        def budgeted(raw, model, prompt, base_url, key, timeout, provider):
            body = cleanup.request_body(model, prompt, raw, provider)
            # UTF-8 bytes upper-bound token count. Allow 256 framing tokens and
            # the more expensive cache-write input tariff. Reserve before network I/O.
            inputs = 256 + sum(len(m["content"].encode()) for m in body["messages"])
            reserve = math.ceil(inputs * 2.5 + body["max_tokens"] * 10)
            if used + reserve > 2_000_000:
                raise cleanup.UsageLimit("owner USD 2 budget")
            record(reserve)
            reply = cleanup.ask_chat(raw, model, prompt, base_url, key, timeout, provider)
            cost = reply["usage"]["cost"]
            if cost is not None:
                record(math.ceil(float(cost) * 1_000_000) - reserve)
            return reply

        sys.argv = [sys.argv[0], "--provider", "openrouter", "--model", MODEL, "--prompt",
                    str(HERE / "prompts/v4.txt"), "--input", str(SOURCE), "--results-dir", str(ROOT),
                    "--label", LABEL, "--workers", "1", "--attempts", "1", "--timeout", "90", "--max-cost", "2"]
        return cleanup.main(chat_request=budgeted)


if __name__ == "__main__":
    raise SystemExit(main())
