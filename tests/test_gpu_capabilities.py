import json
import subprocess
import sys
from pathlib import Path

import pytest

from kernelledger.cli import execute_cli, parser
from kernelledger.execution.targets import Target, runtime_identity
from kernelledger.gpu.backends import capabilities


@pytest.mark.parametrize(
    "kind, nvidia, amd",
    [
        ("local-windows", "unsupported", "unsupported"),
        ("local-linux", "unavailable", "unavailable"),
        ("wsl", "experimental", "unsupported"),
    ],
)
def test_compute_and_tool_presence_never_qualify_native(monkeypatch, kind, nvidia, amd):
    monkeypatch.setattr(
        capabilities, "local_target", lambda: Target("target", kind, "test", "x64", "kernel")
    )
    monkeypatch.setattr(capabilities.shutil, "which", lambda _: None)
    report = capabilities.inventory()
    assert report["features"]["native-nvidia"]["status"] == nvidia
    assert report["features"]["native-amd"]["status"] == amd
    monkeypatch.setattr(capabilities.shutil, "which", lambda _: "present")
    assert all(
        feature["status"] != "qualified"
        for feature in capabilities.inventory()["features"].values()
    )


def test_probe_failure_timeout_and_invalid_response(monkeypatch):
    def timeout(*args, **kwargs):
        raise subprocess.TimeoutExpired("probe", 1)

    monkeypatch.setattr(capabilities.subprocess, "run", timeout)
    assert capabilities.probe(timeout=1)["reason"] == "disposable probe timed out"
    monkeypatch.setattr(
        capabilities.subprocess,
        "run",
        lambda *args, **kwargs: subprocess.CompletedProcess([], 0, b"invalid", b""),
    )
    assert capabilities.probe()["reason"] == "invalid disposable probe response"
    with pytest.raises(ValueError, match="device"):
        capabilities.probe(device="shell; anything")


def test_cli_inventory_does_not_create_notebook_or_store(tmp_path, capsys):
    assert (
        execute_cli(parser().parse_args(["-C", str(tmp_path), "experimental-gpu", "inventory"]))
        == 0
    )
    assert json.loads(capsys.readouterr().out)["inspection_only"]
    assert list(tmp_path.iterdir()) == []
    with pytest.raises(FileNotFoundError):
        execute_cli(
            parser().parse_args(["-C", str(tmp_path), "experimental-gpu", "inspect", "0" * 64])
        )
    assert list(tmp_path.iterdir()) == []


def test_runtime_identity_separates_worktrees_target_and_generation(tmp_path):
    target = Target("local", "local-windows", "Windows", "AMD64", "11")
    identity = runtime_identity("repo", tmp_path, target, 0)
    assert identity == runtime_identity("repo", tmp_path, target, 0)
    assert identity != runtime_identity("repo", tmp_path / "other", target, 0)
    assert identity != runtime_identity("repo", tmp_path, target, 1)


def test_standalone_v4_worker_keeps_package_independence():
    source = Path(__file__).parents[1] / "src/kernelledger/worker.py"
    result = subprocess.run(
        [
            sys.executable,
            "-I",
            "-c",
            "import runpy,sys; runpy.run_path(sys.argv[1]); assert 'kernelledger' not in sys.modules",
            str(source),
        ],
        capture_output=True,
        text=True,
        timeout=15,
    )
    assert result.returncode == 0, result.stderr
