"""Launch owned, tiny notebook instances without touching the user's runtime."""
import ctypes
from ctypes import wintypes
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import time

from kernelledger.cli import ensure_supervisor, rpc
from kernelledger.notebook import Notebook

out = Path(__file__).resolve().parent
repo = out.parents[1]
root = out / "kernelledger-notebook"
notebook = Notebook(root)
if not notebook.path.exists():
    notebook.init()
    notebook.add("markdown", "# Memory benchmark\nA small idle notebook.", "intro")
    notebook.add("code", "print(1 + 2)", "arithmetic")
ensure_supervisor(root, python=sys.executable)
result = rpc(root, {"command": "run", "cell_id": "arithmetic"})
assert result["status"] == "ok", result
supervisor = json.loads((root / ".kl/supervisor.json").read_text())

profile = out / "profile/KernelLedger/Desktop"
profile.mkdir(parents=True, exist_ok=True)
(profile / "appearance.txt").write_text("dark")
with (out / "kl.stdout.log").open("wb") as stdout, (out / "kl.stderr.log").open("wb") as stderr:
    ui = subprocess.Popen(
        [str(repo / "desktop/target/debug/kernelledger-desktop.exe"), str(root)],
        cwd=repo,
        env={**os.environ, "LOCALAPPDATA": str(out / "profile"), "KL_RUNTIME_PYTHON": sys.executable},
        stdout=stdout, stderr=stderr, creationflags=0x08000000,
    )
user = ctypes.windll.user32
window = None
deadline = time.monotonic() + 20
while time.monotonic() < deadline:
    windows = []
    @ctypes.WINFUNCTYPE(ctypes.c_bool, wintypes.HWND, wintypes.LPARAM)
    def visit(hwnd, _):
        process_id = wintypes.DWORD()
        user.GetWindowThreadProcessId(hwnd, ctypes.byref(process_id))
        if process_id.value == ui.pid and user.IsWindowVisible(hwnd):
            windows.append(hwnd)
        return True
    user.EnumWindows(visit, 0)
    if windows:
        window = windows[0]
        break
    if ui.poll() is not None:
        raise RuntimeError("Measurement UI exited")
    time.sleep(0.1)
assert window is not None, "Measurement UI did not open"

with socket.socket() as listener:
    listener.bind(("127.0.0.1", 0))
    port = listener.getsockname()[1]
with (out / "marimo.stdout.log").open("wb") as stdout, (out / "marimo.stderr.log").open("wb") as stderr:
    marimo = subprocess.Popen(
        [str(out / "venv/Scripts/python.exe"), "-m", "marimo", "edit", str(out / "notebook.py"),
         "--headless", "--no-token", "--host", "127.0.0.1", "--port", str(port),
         "--skip-update-check", "--no-sandbox"],
        cwd=out, stdout=stdout, stderr=stderr, creationflags=0x08000000,
        env={**os.environ, "XDG_CONFIG_HOME": str(out / "config"), "MARIMO_SKIP_UPDATE_CHECK": "1"},
    )
state = {"kl_ui": ui.pid, "kl_hwnd": window, "kl_supervisor": supervisor["pid"],
         "marimo": marimo.pid, "port": port}
(out / "session.json").write_text(json.dumps(state, indent=2))
print(json.dumps(state))
