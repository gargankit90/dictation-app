"""Policy comparison regression tests; no network or token needed."""
from contextlib import redirect_stderr
from copy import deepcopy
import io
import importlib.util
import json
import sys

sys.dont_write_bytecode = True
from pathlib import Path

spec = importlib.util.spec_from_file_location("rulesets", Path(__file__).with_name("check-rulesets.py"))
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
expected = json.loads((Path(__file__).resolve().parent.parent / ".github/rulesets-expected.json").read_text())
module.compare(expected, list(reversed(expected)), True)
hidden = [{k: v for k, v in r.items() if k != "bypass_actors"} for r in expected]
module.compare(expected, hidden)


def refused(actual, require=False):
    try:
        with redirect_stderr(io.StringIO()):
            module.compare(expected, actual, require)
    except (ValueError, KeyError):
        return
    raise AssertionError("changed or unverifiable policy passed")


refused(hidden, True)
refused(expected[:-1])
refused(expected + [dict(expected[0], name="unexpected")])
for key, value in (("name", "renamed"), ("enforcement", "disabled"), ("target", "tag"), ("conditions", {}), ("rules", []), ("bypass_actors", [{"actor_id": 1, "actor_type": "RepositoryRole", "bypass_mode": "always"}])):
    changed = deepcopy(expected)
    changed[0][key] = value
    refused(changed)
print("ruleset comparison: 11 checks passed")
