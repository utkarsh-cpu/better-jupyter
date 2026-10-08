"""Inventory never imports a framework. Active probing is a separate process."""

import importlib.metadata
import json
import os
import platform
import shutil
import subprocess
import sys
from pathlib import Path
from typing import Any, cast

from ...execution.targets import local_target


def inventory() -> dict[str, Any]:
    target = local_target()
    frameworks = {}
    for name in ("torch", "numpy"):
        try:
            frameworks[name] = importlib.metadata.version(name)
        except importlib.metadata.PackageNotFoundError:
            frameworks[name] = None
    tools = {
        name: shutil.which(name) is not None for name in ("criu", "cuda-checkpoint", "wsl", "ssh")
    }

    def feature(status: str, reason: str) -> dict[str, str | None]:
        return {"status": status, "reason": reason, "evidence": None}

    if target.kind == "local-windows":
        nvidia = feature("unsupported", "NVIDIA native checkpoint APIs require Linux")
        amd = feature("unsupported", "amdgpu/KFD CRIU snapshots require native Linux")
    elif target.kind == "wsl":
        nvidia = feature("experimental", "WSL compute does not qualify CUDA/CRIU process restore")
        amd = feature("unsupported", "WSL DXG is not the native amdgpu/KFD snapshot route")
    else:
        nvidia = feature(
            "unavailable" if not (tools["criu"] and tools["cuda-checkpoint"]) else "experimental",
            "native tools missing"
            if not (tools["criu"] and tools["cuda-checkpoint"])
            else "no qualified worker reconnect/privilege helper or hardware restore evidence",
        )
        amd = feature(
            "unavailable" if not tools["criu"] else "experimental",
            "CRIU missing"
            if not tools["criu"]
            else "KFD/plugin/firmware and real hardware restore are unqualified",
        )
    return {
        "schema": "kernelledger.experimental.capabilities/1",
        "target": target.describe(),
        "python": platform.python_version(),
        "os_build": platform.version(),
        "frameworks": frameworks,
        "tools": tools,
        "features": {
            "portable-pytorch": feature(
                "experimental" if frameworks["torch"] else "unavailable",
                "active device/round-trip probe required"
                if frameworks["torch"]
                else "PyTorch is not installed",
            ),
            "native-nvidia": nvidia,
            "native-amd": amd,
            "warm-cache": feature(
                "unavailable", "v4 worker has no authenticated snapshot generation cache"
            ),
        },
        "inspection_only": True,
    }


def probe(
    *, python: str = sys.executable, device: str = "cpu", timeout: float = 60
) -> dict[str, Any]:
    import re

    if not re.fullmatch(r"cpu|cuda:\d+", device):
        raise ValueError("unsupported probe device")
    if not 0 < timeout <= 300:
        raise ValueError("invalid probe timeout")
    script = Path(__file__).with_name("probe.py")
    environment = dict(os.environ)
    # The disposable interpreter loads our prototype, but never project source.
    environment["PYTHONPATH"] = str(Path(__file__).resolve().parents[3])
    environment.pop("PYTHONSTARTUP", None)
    try:
        result = subprocess.run(
            [python, "-I", str(script), device],
            capture_output=True,
            timeout=timeout,
            env=environment,
            cwd=script.parent,
            creationflags=0x08000000 if os.name == "nt" else 0,
        )
    except subprocess.TimeoutExpired:
        return {
            "status": "unavailable",
            "reason": "disposable probe timed out",
            "device": device,
            "native_qualified": False,
        }
    except OSError as error:
        return {
            "status": "unavailable",
            "reason": str(error),
            "device": device,
            "native_qualified": False,
        }
    if result.returncode:
        return {
            "status": "unavailable",
            "reason": "disposable probe failed",
            "exit_code": result.returncode,
            "stderr": result.stderr.decode(errors="replace")[-4000:],
            "device": device,
            "native_qualified": False,
        }
    try:
        value = json.loads(result.stdout)
        if not isinstance(value, dict) or "status" not in value:
            raise ValueError("invalid probe response")
        return cast(dict[str, Any], value)
    except (ValueError, UnicodeDecodeError):
        return {
            "status": "unavailable",
            "reason": "invalid disposable probe response",
            "device": device,
            "native_qualified": False,
        }
