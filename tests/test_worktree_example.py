"""The worktree example must restore from committed artifacts without a donor runtime."""

import json
import shutil
import subprocess
import sys
from pathlib import Path

import pytest

from kernelledger.sharing import Bundle


@pytest.mark.skipif(shutil.which("git") is None, reason="Git is required for worktree examples")
def test_worktrees(tmp_path):
    script = Path(__file__).resolve().parents[1] / "examples/worktrees/demo.py"
    destination = tmp_path / "work trees"
    result = subprocess.run(
        [sys.executable, str(script), "--output", str(destination)],
        capture_output=True,
        text=True,
        timeout=180,
    )
    assert result.returncode == 0, result.stdout + result.stderr
    summary = json.loads((destination / "summary.json").read_text())
    assert "fresh clone restore" in summary["verified"]
    records = {}
    for name in ("baseline", "learning-rate", "dataset", "restored-clone"):
        root = destination / name
        records[name] = record = json.loads((root / "experiment.json").read_text())
        assert record["checkpoint_status"] == "complete" and not record["excluded"]
        assert not (root / ".kl/supervisor.json").exists()
        tracked = subprocess.check_output(
            ["git", "-C", str(root), "ls-files"], text=True
        ).splitlines()
        assert not any(name.startswith(".kl/") for name in tracked)
        assert "experiment.json" in tracked and "parameters.json" in tracked
        bundle = Bundle(root / record["bundle"])
        try:
            report = bundle.inspect()
            assert not report["hash_mismatches"]
            assert report["checkpoint_ids"] == [record["checkpoint_id"]]
        finally:
            bundle.close()
    base = records["baseline"]
    assert base["result"]["training_steps"] == 10
    for name in ("learning-rate", "dataset"):
        assert records[name]["parent_checkpoint_id"] == base["checkpoint_id"]
        assert records[name]["result"]["training_steps"] == 20
    assert records["learning-rate"]["parameters"]["learning_rate"] == 0.01
    assert records["dataset"]["parameters"]["dataset"] == "data/shifted.json"
    assert records["learning-rate"]["result"]["model"] != records["dataset"]["result"]["model"]
    assert records["restored-clone"] == records["learning-rate"]
    parameter_diff = subprocess.check_output(
        [
            "git",
            "-C",
            str(destination / "baseline"),
            "diff",
            "--numstat",
            "baseline",
            "experiment/learning-rate",
            "--",
            "parameters.json",
        ],
        text=True,
    )
    assert parameter_diff.strip() == "1\t1\tparameters.json"

    # Re-running the demo must not overwrite an existing experiment repository.
    sentinel = destination / "keep.txt"
    sentinel.write_text("keep")
    repeated = subprocess.run(
        [sys.executable, str(script), "--output", str(destination)],
        capture_output=True,
        text=True,
        timeout=15,
    )
    assert repeated.returncode != 0 and "must not exist" in repeated.stderr
    assert sentinel.read_text() == "keep"
