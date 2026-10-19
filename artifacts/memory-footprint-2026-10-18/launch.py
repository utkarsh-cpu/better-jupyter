"""Open an owned UI instance for a read-only memory snapshot."""
import ctypes
from ctypes import wintypes
import json
import os
from pathlib import Path
import subprocess
import sys
import time

out = Path(__file__).resolve().parent
repo = out.parents[1]
profile = out / "profile/KernelLedger/Desktop"
profile.mkdir(parents=True, exist_ok=True)
(profile / "appearance.txt").write_text("dark")
with (out / "app.stdout.log").open("wb") as stdout, (out / "app.stderr.log").open("wb") as stderr:
    app = subprocess.Popen(
        [str(repo / "desktop/target/debug/kernelledger-desktop.exe"), str(repo / "artifacts/frontend-design-refresh")],
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
        if process_id.value == app.pid and user.IsWindowVisible(hwnd):
            windows.append(hwnd)
        return True
    user.EnumWindows(visit, 0)
    if windows:
        window = windows[0]
        break
    if app.poll() is not None:
        raise RuntimeError(f"Owned UI exited: {app.returncode}")
    time.sleep(0.1)
if window is None:
    app.terminate()
    raise RuntimeError("Owned UI did not open")
state = {"pid": app.pid, "hwnd": window}
(out / "session.json").write_text(json.dumps(state))
print(json.dumps(state))
