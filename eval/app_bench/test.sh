#!/usr/bin/env bash
# The bench's offline tests: standard library only, no macOS, network, key or clips.
set -euo pipefail
cd "$(dirname "$0")"
python3 -W ignore::ResourceWarning -m unittest discover -s . -p 'test_*.py'
python3 harness.py dry-run
