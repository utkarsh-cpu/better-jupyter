"""Opt-in mixed supervisor/worker minor qualification, not cross-minor restore."""

import json
import os
import sys
import time
from pathlib import Path

import pytest

from kernelledger.cli import alive, ensure_supervisor, rpc
from kernelledger.notebook import Notebook


def test_selected_worker_minor_roundtrip(tmp_path):
    selected = os.environ.get("KL_QUALIFIED_WORKER")
    if not selected:
        pytest.skip("set KL_QUALIFIED_WORKER to qualify a supervisor/worker combination")
    interpreter = Path(selected)
    assert interpreter.is_absolute() and interpreter.is_file()
    Notebook(tmp_path).init()
    source = """
import dataclasses, random, sys, pathlib, json
@dataclasses.dataclass
class Point:
    x: int
def outer(value):
    def inner():
        return value
    return inner
point = Point(7)
closure = outer(9)
cycle = []
cycle.append(cycle)
alias = cycle
random.seed(5)
saved_random = random.getstate()
pathlib.Path('worker-version.json').write_text(json.dumps(list(sys.version_info[:3])))
"""
    try:
        ensure_supervisor(tmp_path, str(interpreter))
        assert rpc(tmp_path, {"command": "run", "source": source})["status"] == "ok"
        status = alive(tmp_path)
        assert os.path.samefile(status["worker_python"], interpreter)
        assert json.loads((tmp_path / "worker-version.json").read_text()) >= [3, 11, 0]
        checkpoint = rpc(tmp_path, {"command": "checkpoint"})
        assert checkpoint["status"] == "complete", checkpoint
        restored = rpc(
            tmp_path, {"command": "restore", "checkpoint_id": checkpoint["checkpoint_id"]}
        )
        assert not restored["excluded"], restored
        verified = rpc(
            tmp_path,
            {
                "command": "run",
                "source": "assert point.x == 7\nassert closure() == 9\nassert cycle is alias is cycle[0]\nassert random.getstate() == saved_random",
            },
        )
        assert verified["status"] == "ok", {"supervisor": sys.version, "worker": selected}
    finally:
        if alive(tmp_path):
            rpc(tmp_path, {"command": "stop"})
        deadline = time.monotonic() + 10
        while (tmp_path / ".kl/supervisor.json").exists() and time.monotonic() < deadline:
            time.sleep(0.02)
