"""Disposable active probe. Invoked by absolute path with an isolated interpreter."""

import hashlib
import json
import platform
import sys
import tempfile
from pathlib import Path
from typing import Any


def run(device: str) -> dict[str, Any]:
    # -I deliberately excludes project paths; add only the installed package source.
    sys.path.insert(0, str(Path(__file__).resolve().parents[3]))
    import torch

    torch_module: Any = torch
    from kernelledger.artifacts import ArtifactStore
    from kernelledger.gpu.portable import PortableEncoder, restore_values

    if device != "cpu" and not torch_module.cuda.is_available():
        return {
            "status": "unavailable",
            "reason": "framework has no available GPU",
            "device": device,
            "torch": torch_module.__version__,
            "native_qualified": False,
        }
    with tempfile.TemporaryDirectory(prefix="kl-gpu-probe-") as directory:
        store = ArtifactStore(Path(directory))
        value = torch_module.arange(16, dtype=torch_module.float32, device=device).reshape(4, 4)
        snapshot = PortableEncoder(store).capture({"tensor": value, "view": value[:, ::2]})
        restored = restore_values(store, snapshot, device=device)
        if not torch_module.equal(restored["tensor"], value) or not torch_module.equal(
            restored["view"], value[:, ::2]
        ):
            raise ValueError("portable byte round-trip failed")
        restored["view"][0, 0] = 91
        if restored["tensor"][0, 0].item() != 91:
            raise ValueError("shared-storage round-trip failed")
        report = {
            "status": "experimental",
            "reason": "disposable portable tensor/view probe passed",
            "device": device,
            "python": platform.python_version(),
            "torch": torch_module.__version__,
            "cuda": torch_module.version.cuda,
            "hip": torch_module.version.hip,
            "snapshot": snapshot,
            "native_qualified": False,
        }
        if device != "cpu":
            properties = torch_module.cuda.get_device_properties(device)
            report["hardware"] = {
                "name": properties.name,
                "total_memory": properties.total_memory,
                "architecture": [properties.major, properties.minor],
                "uuid": str(getattr(properties, "uuid", "unavailable")),
            }
    return report


if __name__ == "__main__":
    try:
        result = run(sys.argv[1])
    except Exception as error:
        result = {
            "status": "unavailable",
            "device": sys.argv[1],
            "reason": str(error),
            "error_type": type(error).__name__,
            "python": platform.python_version(),
            "os_build": platform.version(),
            "native_qualified": False,
        }
    result["evidence"] = hashlib.sha256(json.dumps(result, sort_keys=True).encode()).hexdigest()
    print(json.dumps(result, sort_keys=True))
