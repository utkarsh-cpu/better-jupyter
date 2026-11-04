"""Disposable N0 comparison. IPython is imported only by its explicit spike child."""

import argparse
import contextlib
import hashlib
import importlib
import importlib.util
import io
import json
import platform
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def child(mode):
    started = time.perf_counter()
    spec = importlib.util.spec_from_file_location(
        "kl_spike_worker", ROOT / "src/kernelledger/worker.py"
    )
    worker_module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(worker_module)

    class Channel:
        def __init__(self):
            self.events = []

        def send(self, kind, **fields):
            self.events.append({"kind": kind, **fields})

    channel = Channel()
    worker = worker_module.Worker(channel)
    namespace = worker.namespace
    temporary = tempfile.TemporaryDirectory(prefix="kl-engine-spike-")
    shell = None
    if mode == "ipython":
        shell_module = importlib.import_module("IPython.core.interactiveshell")
        config_module = importlib.import_module("traitlets.config")
        config = config_module.Config()
        config.HistoryManager.enabled = False
        config.InteractiveShell.cache_size = 0
        shell = shell_module.InteractiveShell(
            user_ns=namespace, ipython_dir=temporary.name, config=config
        )
    startup = time.perf_counter() - started
    cases = [
        "values = []\nalias = values\nvalues.append(1)",
        "def square(x):\n    return x*x\nclass Point:\n    def __init__(self, x):\n        self.x=x\npoint=Point(7)",
        "square(8)",
        "%time values.append(2)",
        "%%time\ndef timed(x):\n    return x+1\ntimed(4)",
        "from __future__ import annotations\ndef future(x: Missing):\n    return x",
        "import asyncio\nawait asyncio.sleep(0, result=42)",
        "raise ValueError('fixture error')",
    ]
    outcomes = []
    output = io.StringIO()
    with contextlib.redirect_stdout(output), contextlib.redirect_stderr(output):
        for ordinal, source in enumerate(cases, 1):
            filename = f"cells/fixture.py@{ordinal}"
            try:
                tree = worker.index.add(filename, source, ordinal)
                if shell:
                    result = shell.run_cell(source, store_history=False, silent=True)
                    result.raise_error()
                else:
                    worker.ordinal = ordinal
                    worker.engine.execute(tree, source, filename)
                outcomes.append("ok")
            except BaseException as error:
                outcomes.append(type(error).__name__)
    manifest = worker_module.GraphEncoder(worker.index).checkpoint(
        namespace, [], worker.internal_bindings
    )
    process = importlib.import_module("psutil").Process()
    memory = process.memory_full_info()
    report = {
        "engine": mode,
        "python": sys.version,
        "startup_seconds": startup,
        "rss_bytes": memory.rss,
        "private_bytes": getattr(memory, "private", None),
        "uss_bytes": getattr(memory, "uss", None),
        "outcomes": outcomes,
        "checkpoint_exclusions": manifest["excluded"],
        "module_count": len(sys.modules),
        "default_adopted": False,
    }
    if shell:
        report["dependency_version"] = importlib.import_module("IPython").__version__
        shell.atexit_operations()
    worker.engine.close()
    temporary.cleanup()
    print(json.dumps(report))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--child", choices=("custom", "ipython"))
    parser.add_argument(
        "--output", type=Path, default=ROOT / "artifacts/notebook-interactivity/engine-spike.json"
    )
    parser.add_argument("--trials", type=int, default=3)
    args = parser.parse_args()
    if args.child:
        child(args.child)
        return
    samples = []
    for _ in range(args.trials):
        for mode in ("custom", "ipython"):
            started = time.perf_counter()
            result = subprocess.run(
                [sys.executable, __file__, "--child", mode],
                capture_output=True,
                text=True,
                timeout=30,
            )
            sample = (
                json.loads(result.stdout)
                if result.returncode == 0
                else {"engine": mode, "error": result.stderr[-4096:]}
            )
            sample["process_wall_seconds"] = time.perf_counter() - started
            samples.append(sample)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(
        json.dumps(
            {
                "schema": 1,
                "environment": {
                    "platform": platform.platform(),
                    "processor": platform.processor(),
                    "python": sys.version,
                },
                "worker_sha256": hashlib.sha256(
                    (ROOT / "src/kernelledger/worker.py").read_bytes()
                ).hexdigest(),
                "interleaved_trials": samples,
            },
            indent=2,
        )
        + "\n",
        encoding="utf-8",
    )
    print(args.output)


if __name__ == "__main__":
    main()
