#!/usr/bin/env bash
# The eval tests, offline. Plain python3 (what the gate runs): standard library only, the numpy tests
# skip themselves. `uv run pytest eval` runs the same with every dependency, so none skip.
set -euo pipefail
cd "$(cd "$(dirname "$0")/.." && pwd)"
python3 -W ignore::ResourceWarning -m unittest discover -s eval -p 'test_*.py'
python3 -W ignore::ResourceWarning -m unittest discover -s tests/e2e -p 'test_latency.py'
