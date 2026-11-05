#!/usr/bin/env python3
"""Compare repository rulesets; --require-bypass is for the Lead's admin token."""
import argparse
import difflib
import json
from pathlib import Path
import subprocess
import sys

FIELDS = ("name", "enforcement", "target", "conditions", "rules", "bypass_actors")


def canonical(value):
    if isinstance(value, dict):
        return {key: canonical(item) for key, item in sorted(value.items())}
    if isinstance(value, list):
        return sorted((canonical(item) for item in value), key=lambda item: json.dumps(item, sort_keys=True))
    return value


def compare(expected, actual, require_bypass=False):
    visible = []
    for ruleset in actual:
        fields = list(FIELDS)
        if "bypass_actors" not in ruleset:
            print(f"WARNING: {ruleset['name']}: bypass_actors not visible; bypass policy cannot be verified", file=sys.stderr)
            if require_bypass:
                raise ValueError("admin token with ruleset write access required for bypass verification")
            fields.remove("bypass_actors")
        # Missing any other field is an error, never an empty/default policy.
        visible.append({key: ruleset[key] for key in fields})
    wanted = []
    for ruleset in expected:
        fields = list(FIELDS)
        match = next((r for r in actual if r["name"] == ruleset["name"]), {})
        if "bypass_actors" not in match and not require_bypass:
            fields.remove("bypass_actors")
        wanted.append({key: ruleset[key] for key in fields})
    before = json.dumps(canonical(wanted), indent=2).splitlines(True)
    after = json.dumps(canonical(visible), indent=2).splitlines(True)
    if before != after:
        sys.stderr.writelines(difflib.unified_diff(before, after, fromfile="expected", tofile="GitHub"))
        raise ValueError("repository ruleset drift detected")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", default="gargankit90/dictation-app")
    parser.add_argument("--require-bypass", action="store_true")
    args = parser.parse_args()
    def api(path):
        return json.loads(subprocess.check_output(["gh", "api", "--paginate", "--slurp", path]))
    summaries = [item for page in api(f"repos/{args.repo}/rulesets?per_page=100") for item in page]
    actual = [api(f"repos/{args.repo}/rulesets/{item['id']}")[0] for item in summaries]
    expected = json.loads((Path(__file__).resolve().parent.parent / ".github/rulesets-expected.json").read_text())
    compare(expected, actual, args.require_bypass)
    print("rulesets: all visible policy fields match" + (" (including bypass actors)" if args.require_bypass else ""))


if __name__ == "__main__":
    try:
        main()
    except (ValueError, KeyError, TypeError, subprocess.CalledProcessError) as error:
        sys.exit(f"check-rulesets: {error}")
