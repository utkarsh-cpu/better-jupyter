"""Idle release snapshot for one owned two-cell notebook. Does not touch other runtimes."""

import ctypes
from ctypes import wintypes
import json
import os
from pathlib import Path
import subprocess
import sys
import time

from kernelledger.cli import alive, ensure_supervisor, rpc
from kernelledger.notebook import Notebook

out = Path(__file__).resolve().parent
repo = out.parents[1]
root = out / "notebook"
exe = repo / "desktop/target/release/kernelledger-desktop.exe"
profile = out / "profile/KernelLedger/Desktop"
profile.mkdir(parents=True, exist_ok=True)
(profile / "appearance.txt").write_text("dark")

notebook = Notebook(root)
if not notebook.path.exists():
    notebook.init()
    notebook.add("markdown", "# Memory benchmark\nA small idle notebook.", "intro")
    notebook.add("code", "print(1 + 2)", "arithmetic")

python = Path(sys.executable)
venv = repo / ".venv" / "Scripts" / "python.exe"
worker_python = venv if venv.is_file() else python
ensure_supervisor(root, python=str(worker_python))
result = rpc(root, {"command": "run", "cell_id": "arithmetic"})
if result["status"] != "ok":
    raise SystemExit(f"benchmark cell failed: {result}")
supervisor = json.loads((root / ".kl/supervisor.json").read_text())

ui = None
try:
    with (out / "ui.stdout.log").open("wb") as stdout, (out / "ui.stderr.log").open("wb") as stderr:
        ui = subprocess.Popen(
            [str(exe), str(root)],
            cwd=repo,
            env={**os.environ, "LOCALAPPDATA": str(out / "profile"), "KL_RUNTIME_PYTHON": str(python)},
            stdout=stdout,
            stderr=stderr,
            creationflags=0x08000000,
        )
    user = ctypes.windll.user32
    window = None
    deadline = time.monotonic() + 30
    while time.monotonic() < deadline:
        found = []

        @ctypes.WINFUNCTYPE(ctypes.c_bool, wintypes.HWND, wintypes.LPARAM)
        def visit(hwnd, _):
            process_id = wintypes.DWORD()
            user.GetWindowThreadProcessId(hwnd, ctypes.byref(process_id))
            if process_id.value == ui.pid and user.IsWindowVisible(hwnd):
                found.append(hwnd)
            return True

        user.EnumWindows(visit, 0)
        if found:
            window = found[0]
            break
        if ui.poll() is not None:
            raise RuntimeError(f"release UI exited: {ui.returncode}")
        time.sleep(0.1)
    if window is None:
        raise RuntimeError("release UI did not open")
    time.sleep(3)
    state = {
        "ui": ui.pid,
        "hwnd": int(window),
        "supervisor": supervisor["pid"],
        "python": str(python),
        "worker_python": str(worker_python),
    }
    (out / "session.json").write_text(json.dumps(state, indent=2))
    measure = subprocess.run(
        [
            "powershell",
            "-NoProfile",
            "-File",
            str(out / "sample.ps1"),
        ],
        check=True,
        cwd=out,
    )
    print("sampled", measure.returncode)
finally:
    if ui is not None and ui.poll() is None:
        user = ctypes.windll.user32
        user.PostMessageW.argtypes = [wintypes.HWND, wintypes.UINT, wintypes.WPARAM, wintypes.LPARAM]
        session_path = out / "session.json"
        if session_path.exists():
            hwnd = json.loads(session_path.read_text())["hwnd"]
            user.PostMessageW(hwnd, 0x10, 0, 0)
            deadline = time.monotonic() + 8
            while ui.poll() is None and time.monotonic() < deadline:
                time.sleep(0.1)
        if ui.poll() is None:
            subprocess.run(["taskkill", "/PID", str(ui.pid), "/T", "/F"], check=False, creationflags=0x08000000)
    if alive(root):
        try:
            rpc(root, {"command": "stop"})
        except (OSError, ValueError):
            supervisor_now = json.loads((root / ".kl/supervisor.json").read_text())
            subprocess.run(
                ["taskkill", "/PID", str(supervisor_now["pid"]), "/T", "/F"],
                check=False,
                creationflags=0x08000000,
            )
