"""One USD budget shared by every OpenRouter call in this experiment (parts A and B), across processes.

A ledger file holds signed microdollar entries. Before a request, the caller reserves a pessimistic
estimate; once the reply arrives, it settles to the reply's usage.cost (or keeps the estimate when
the reply has no cost). Reservation and settlement each take an exclusive flock on the ledger, so
two processes running at once can't overshoot the cap together. No request starts if its
reservation would pass the cap.

Pattern from eval/cleanup-offline/run_nemotron_sol.py (branch eval/cleanup-offline, PR #327), which
held the lock for a whole run; here the lock is held per operation so parts A and B can run in
parallel.
"""
from __future__ import annotations

import fcntl
import json
import math
import os
import pathlib


class OverBudget(RuntimeError):
    pass


class Ledger:
    def __init__(self, path: pathlib.Path, cap_usd: float):
        self.path = pathlib.Path(path)
        self.cap = int(round(cap_usd * 1_000_000))
        self.path.parent.mkdir(parents=True, exist_ok=True)
        self.path.touch(exist_ok=True)

    def _locked(self, fn):
        with self.path.open("a+") as fh:
            fcntl.flock(fh, fcntl.LOCK_EX)
            try:
                fh.seek(0)
                used = sum(int(json.loads(line)["microdollars"]) for line in fh if line.strip())
                return fn(fh, used)
            finally:
                fcntl.flock(fh, fcntl.LOCK_UN)

    @staticmethod
    def _write(fh, amount: int, tag: str) -> None:
        fh.seek(0, os.SEEK_END)
        fh.write(json.dumps({"microdollars": amount, "tag": tag}) + "\n")
        fh.flush()
        os.fsync(fh.fileno())

    def used_usd(self) -> float:
        return self._locked(lambda fh, used: used) / 1_000_000

    def reserve(self, usd: float, tag: str) -> int:
        """Hold usd (rounded up to a microdollar); raise OverBudget if that would pass the cap."""
        amount = max(1, math.ceil(usd * 1_000_000))

        def go(fh, used):
            if used + amount > self.cap:
                raise OverBudget(f"reserving ${usd:.6f} would pass the ${self.cap / 1e6:.2f} cap "
                                 f"(used ${used / 1e6:.4f})")
            self._write(fh, amount, tag)
            return amount

        return self._locked(go)

    def settle(self, reserved: int, cost_usd: float | None, tag: str) -> None:
        """Replace a reservation by the real cost; with no cost reported, keep the reservation."""
        if cost_usd is None:
            return
        delta = math.ceil(float(cost_usd) * 1_000_000) - reserved
        if delta:
            self._locked(lambda fh, used: self._write(fh, delta, tag))
