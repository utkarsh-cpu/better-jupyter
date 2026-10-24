"""Write interpreter/dependency evidence without importing optional frameworks."""

import argparse
import importlib.metadata
import json
import platform
import sys
import sysconfig
from pathlib import Path


def report():
    dependencies = {}
    for name in ("numpy", "pandas", "pyarrow", "matplotlib", "torch", "kernelledger", "pytest"):
        try:
            dependencies[name] = importlib.metadata.version(name)
        except importlib.metadata.PackageNotFoundError:
            dependencies[name] = None
    return {
        "schema": 1,
        "python": sys.version,
        "executable": sys.executable,
        "release_level": sys.version_info.releaselevel,
        "implementation": platform.python_implementation(),
        "platform": platform.platform(),
        "architecture": platform.machine(),
        "gil_enabled": getattr(sys, "_is_gil_enabled", lambda: True)(),
        "free_threaded_build": bool(sysconfig.get_config_var("Py_GIL_DISABLED")),
        "dependencies": dependencies,
        "installed_distributions": sorted(
            [
                {"name": d.metadata["Name"], "version": d.version}
                for d in importlib.metadata.distributions()
            ],
            key=lambda d: d["name"].lower(),
        ),
        "qualification": "requires test results; dependency resolution alone is not a pass",
        "gpu": "unqualified; hardware/driver/training lanes are separate",
    }


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--out", required=True)
    args = p.parse_args()
    path = Path(args.out)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(report(), indent=2) + "\n", encoding="utf-8")
