"""Explicit runtime selection, installed binary refusal, and shared-owner reuse."""

import os
import subprocess
import sys
import time
from pathlib import Path

import pytest

from kernelledger.cli import ensure_supervisor, rpc, runtime_command
from kernelledger.notebook import Notebook


def test_runtime_launch_defaults_to_python_and_rejects_unknown_mode(tmp_path, monkeypatch):
    monkeypatch.delenv("KERNELLEDGER_RUNTIME", raising=False)
    assert runtime_command(tmp_path)[0] == sys.executable
    monkeypatch.setenv("KERNELLEDGER_RUNTIME", "invalid")
    with pytest.raises(ValueError, match="must be python or native"):
        runtime_command(tmp_path)
    assert not (tmp_path / ".kl").exists()


@pytest.mark.parametrize("binary", ["", "relative-daemon", "/missing-daemon"])
def test_native_launch_requires_explicit_existing_absolute_binary(tmp_path, monkeypatch, binary):
    monkeypatch.setenv("KERNELLEDGER_RUNTIME", "native")
    monkeypatch.setenv("KERNELLEDGER_DAEMON", binary)
    with pytest.raises(ValueError, match="absolute KERNELLEDGER_DAEMON"):
        runtime_command(tmp_path)
    assert not (tmp_path / ".kl").exists()


def test_native_command_preserves_selected_interpreter_path(tmp_path, monkeypatch):
    binary = tmp_path / "daemon"
    binary.write_bytes(b"fixture")
    monkeypatch.setenv("KERNELLEDGER_RUNTIME", "native")
    monkeypatch.setenv("KERNELLEDGER_DAEMON", str(binary))
    selected = tmp_path / "venv/bin/python"
    command = runtime_command(tmp_path, str(selected))
    assert command == [
        str(binary),
        "serve-experimental",
        "--project",
        str(tmp_path.resolve()),
        "--python",
        os.path.abspath(selected),
    ]


def test_native_cli_launcher_reuses_existing_owner(tmp_path, monkeypatch):
    binary = os.environ.get("KL_NATIVE_DAEMON") or os.environ.get("KERNELLEDGER_DAEMON")
    if not binary:
        pytest.skip("native launcher requires an explicit daemon build")
    assert Path(binary).is_absolute() and Path(binary).is_file()
    monkeypatch.setenv("KERNELLEDGER_RUNTIME", "native")
    monkeypatch.setenv("KERNELLEDGER_DAEMON", binary)
    Notebook(tmp_path).init()
    try:
        ensure_supervisor(tmp_path, sys.executable)
        first = rpc(tmp_path, {"command": "status"})
        assert first["implementation"] == "rust"
        assert rpc(tmp_path, {"command": "run", "source": "assert True"})["status"] == "ok"
        monkeypatch.setenv("KERNELLEDGER_RUNTIME", "python")
        ensure_supervisor(tmp_path, sys.executable)
        assert rpc(tmp_path, {"command": "status"})["instance_id"] == first["instance_id"]
    finally:
        rpc(tmp_path, {"command": "stop"})
        deadline = time.monotonic() + 10
        while (tmp_path / ".kl/supervisor.json").exists():
            assert time.monotonic() < deadline
            time.sleep(0.02)


@pytest.mark.skipif(os.name == "nt", reason="Unix venv symlink regression")
def test_native_worker_keeps_venv_site_packages(tmp_path, monkeypatch):
    binary = os.environ.get("KL_NATIVE_DAEMON") or os.environ.get("KERNELLEDGER_DAEMON")
    if not binary:
        pytest.skip("native launcher requires an explicit daemon build")
    selected = tmp_path / "worker-venv"
    subprocess.run(
        [sys.executable, "-m", "venv", "--without-pip", str(selected)], check=True, timeout=30
    )
    python = selected / "bin/python"
    site = Path(
        subprocess.check_output(
            [str(python), "-I", "-c", "import site;print(site.getsitepackages()[0])"], text=True
        ).strip()
    )
    (site / "native_venv_marker.py").write_text("value = 73\n")
    project = tmp_path / "notebook"
    Notebook(project).init()
    monkeypatch.setenv("KERNELLEDGER_RUNTIME", "native")
    monkeypatch.setenv("KERNELLEDGER_DAEMON", binary)
    try:
        ensure_supervisor(project, str(python))
        result = rpc(
            project,
            {
                "command": "run",
                "source": "import native_venv_marker\nassert native_venv_marker.value == 73",
            },
        )
        assert result["status"] == "ok", result
    finally:
        rpc(project, {"command": "stop"})
