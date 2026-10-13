"""Exercise the real Rust helper/protocol client on a disposable notebook."""

import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

from kernelledger.cli import alive, rpc
from kernelledger.notebook import Notebook

repo = Path(__file__).resolve().parents[1]
cargo = shutil.which("cargo") or str(Path.home() / ".cargo/bin/cargo.exe")
with tempfile.TemporaryDirectory(prefix="kl-rust-smoke-") as directory:
    project = Path(directory)
    notebook = Notebook(project)
    notebook.init()
    notebook.add("code", "print('before')", "smoke")
    try:
        # Windows descendants can inherit unrelated pipe handles. Files let the test
        # finish when the client exits while its detached runtime is still alive.
        with (project / "result.json").open("wb") as output, (project / "client.log").open("wb") as errors:
            result = subprocess.run(
                [cargo, "run", "--locked", "-q", "-p", "kernelledger-client", "--example", "vertical_slice", "--", str(project), sys.executable],
                cwd=repo / "desktop", stdout=output, stderr=errors, timeout=180,
                env={**os.environ, "PYTHONIOENCODING": "utf-8"},
            )
        if result.returncode:
            raise RuntimeError((project / "client.log").read_text(encoding="utf-8"))
        report = json.loads((project / "result.json").read_bytes())
        assert report["operation"]["state"] == "completed"
        print("Rust/Python vertical slice passed: helper edit, hash-checked run, reconnect, operation lookup, fold")
    finally:
        if alive(project):
            rpc(project, {"command": "stop"})
        import time

        for _ in range(100):
            if not (project / ".kl/supervisor.json").exists():
                break
            time.sleep(0.02)
        # Registry removal precedes process exit. Let inherited Windows log
        # handles close before TemporaryDirectory attempts to remove them.
        time.sleep(0.5)
