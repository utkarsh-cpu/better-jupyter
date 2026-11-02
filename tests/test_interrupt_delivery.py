"""An admitted execution must retain an interrupt sent before its worker starts."""

import concurrent.futures
import subprocess
import sys
import time
import uuid
from pathlib import Path

import pytest

from kernelledger.cli import alive, rpc
from kernelledger.jupyter_adapter import RuntimeConnection

pytest_plugins = ["test_shell"]


def delayed_execution_worker(root):
    source = (Path(__file__).resolve().parents[1] / "src/kernelledger/worker.py").read_text(
        encoding="utf-8"
    )
    entry = "    def execute(self, message: Any) -> Any:\n        self.interrupted.clear()"
    assert source.count(entry) == 1
    source = source.replace(
        entry,
        "    def execute(self, message: Any) -> Any:\n        pathlib.Path('worker-entry-delay').touch()\n        self.original_sleep(0.5)\n        self.interrupted.clear()",
    )
    path = root / "delayed-worker.py"
    path.write_text(source, encoding="utf-8")
    return path


def check_interrupt_delivery(notebook, transport):
    with concurrent.futures.ThreadPoolExecutor() as pool:
        running = pool.submit(
            rpc,
            notebook.root,
            {"command": "run", "source": "import time\ntime.sleep(2)\nfinished = True"},
        )
        deadline = time.monotonic() + 10
        while not (notebook.root / "worker-entry-delay").exists():
            if running.done():
                pytest.fail(f"Worker ended before the delayed entry: {running.result()}")
            assert time.monotonic() < deadline
            time.sleep(0.01)
        if transport == "legacy":
            assert rpc(notebook.root, {"command": "interrupt"})["interrupted"]
        else:
            client = RuntimeConnection(notebook.root, str(uuid.uuid4()))
            try:
                assert client.call("interrupt")["interrupted"]
            finally:
                client.close()
        assert running.result(timeout=5)["status"] == "interrupted"
    assert (
        rpc(notebook.root, {"command": "run", "source": "assert 'finished' not in globals()"})[
            "status"
        ]
        == "ok"
    )


@pytest.mark.parametrize("transport", ["legacy", "desktop"])
def test_interrupt_before_worker_running_ack_is_delivered_once(notebook, transport):
    worker = delayed_execution_worker(notebook.root)
    # Substitute only this process's worker launch; no production delay hook.
    code = f"""
import asyncio
from pathlib import Path
from kernelledger.supervisor import Supervisor
import kernelledger.supervisor as module
original = asyncio.create_subprocess_exec
resource = str(Path(module.__file__).parents[1] / 'worker.py')
async def launch(*args, **kwargs):
    args = tuple({str(worker)!r} if arg == resource else arg for arg in args)
    return await original(*args, **kwargs)
asyncio.create_subprocess_exec = launch
asyncio.run(Supervisor({str(notebook.root)!r}).run())
"""
    with (notebook.root / "owner-test.log").open("wb") as log:
        process = subprocess.Popen(
            [sys.executable, "-c", code],
            stdout=log,
            stderr=log,
            creationflags=0x08000000 if sys.platform == "win32" else 0,
        )
        try:
            deadline = time.monotonic() + 10
            while not alive(notebook.root):
                assert process.poll() is None, (notebook.root / "owner-test.log").read_text(
                    errors="replace"
                )
                assert time.monotonic() < deadline
                time.sleep(0.01)
            check_interrupt_delivery(notebook, transport)
        finally:
            if process.poll() is None:
                try:
                    rpc(notebook.root, {"command": "stop"})
                    process.wait(timeout=10)
                finally:
                    if process.poll() is None:
                        process.kill()
                        process.wait(timeout=5)
