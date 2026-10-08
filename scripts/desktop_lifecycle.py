"""Run first-use and stop regressions against the real Rust/Python service."""
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path
from kernelledger.cli import alive, rpc

repo = Path(__file__).resolve().parents[1]
cargo = shutil.which("cargo") or str(Path.home() / ".cargo/bin/cargo.exe")
with tempfile.TemporaryDirectory(prefix="kl-lifecycle-") as directory:
    project = Path(directory)
    try:
        with (project / "result.json").open("wb") as out, (project / "client.log").open("wb") as err:
            result = subprocess.run([cargo, "+1.99.0", "run", "--locked", "-q", "-p", "kernelledger-client", "--example", "lifecycle_regression", "--", str(project), sys.executable], cwd=repo / "desktop", stdout=out, stderr=err, timeout=180)
        if result.returncode:
            raise RuntimeError((project / "client.log").read_text())
        print(json.dumps(json.loads((project / "result.json").read_bytes()), indent=2))
    finally:
        time.sleep(1)
        if alive(project): rpc(project, {"command":"stop"})
        for _ in range(100):
            if not (project / ".kl/supervisor.json").exists(): break
            time.sleep(.02)
