#!/usr/bin/env python3
"""
Aura Z3 sidecar — minimal SMT-LIB solver over stdin/stdout.

Protocol: one JSON object per line on stdin, one JSON object per line on
stdout. The Rust side (`reasoning::z3_bridge`) spawns this script and
communicates via the resulting pipes.

Request shape:
    {"id": <any>, "smt": "<SMT-LIB 2.6 source>"}

Response shape:
    {"id": <echoed>, "result": "sat" | "unsat" | "unknown", "model": "..."?}
or on failure:
    {"id": <echoed>, "error": "<message>"}

The sidecar quits cleanly on EOF. Errors are written to stderr so the
Rust side can pick them up if it wants.

Install once:
    pip install z3-solver

Run from Rust as:
    python3 path/to/z3_sidecar.py
"""

from __future__ import annotations

import json
import sys
from typing import Any


def main() -> int:
    try:
        import z3  # noqa: F401  (we use Solver / etc. through `z3.*`)
    except ImportError as e:
        sys.stderr.write(f"z3-solver not importable: {e}\n")
        sys.stderr.write("Install with: pip install z3-solver\n")
        return 1

    # Announce ready so the Rust side knows the import succeeded.
    sys.stdout.write(json.dumps({"ready": True}) + "\n")
    sys.stdout.flush()

    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            req: dict[str, Any] = json.loads(line)
        except json.JSONDecodeError as e:
            _emit({"id": None, "error": f"bad json: {e}"})
            continue

        req_id = req.get("id")
        smt = req.get("smt", "")
        if not isinstance(smt, str):
            _emit({"id": req_id, "error": "smt must be a string"})
            continue

        try:
            solver = z3.Solver()
            solver.from_string(smt)
            verdict = solver.check()
            resp: dict[str, Any] = {"id": req_id, "result": str(verdict)}
            if verdict == z3.sat:
                resp["model"] = str(solver.model())
            _emit(resp)
        except z3.Z3Exception as e:
            _emit({"id": req_id, "error": f"z3: {e}"})
        except Exception as e:  # noqa: BLE001
            _emit({"id": req_id, "error": f"sidecar: {e}"})

    return 0


def _emit(obj: dict[str, Any]) -> None:
    sys.stdout.write(json.dumps(obj) + "\n")
    sys.stdout.flush()


if __name__ == "__main__":
    sys.exit(main())
