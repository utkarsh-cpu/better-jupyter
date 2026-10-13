"""Launch an isolated native fixture for the live design review."""
import ctypes
from ctypes import wintypes
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
from kernelledger.cli import ensure_supervisor, rpc
from kernelledger.notebook import Notebook

repo = Path(__file__).resolve().parents[3]
out = Path(__file__).resolve().parent
directory = Path(tempfile.mkdtemp(prefix="kl-design-live-"))
project = directory / "Forecast analysis - October experiment"
nb = Notebook(project)
nb.init()
nb.add("markdown", "# Forecast analysis\nCompare baseline results and save a recoverable checkpoint.", "intro")
nb.add("code", "observations = [12, 18, 15, 21]\nmean = sum(observations) / len(observations)\nprint(f'Mean: {mean:.1f}')", "baseline")
nb.add("code", "print('Preparing forecast...')\nforecast = mean / 0", "forecast")
nb.add("markdown", "## Next experiment\nCorrect the denominator, rerun the forecast, then compare its recorded source.", "notes")
nb.add("code", "for day, value in enumerate(observations, 1):\n    print(f'Day {day}: {value}')", "report")
profile = directory / "profile/KernelLedger/Desktop"
profile.mkdir(parents=True)
(profile / "appearance.txt").write_text("light")
ensure_supervisor(project, sys.executable)
for cell in ["baseline", "forecast", "report"]:
    rpc(project, {"command": "run", "cell_id": cell})
rpc(project, {"command": "checkpoint"})
with (out / "app.stdout.log").open("wb") as stdout, (out / "app.stderr.log").open("wb") as stderr:
    app = subprocess.Popen([str(repo / "desktop/target/debug/kernelledger-desktop.exe"), str(project)], cwd=repo,
        env={**os.environ, "LOCALAPPDATA": str(directory / "profile"), "KL_RUNTIME_PYTHON": sys.executable},
        stdout=stdout, stderr=stderr, creationflags=0x08000000)
u = ctypes.windll.user32
u.SetProcessDPIAware()
window = None
for attempt in range(200):
    windows = []
    @ctypes.WINFUNCTYPE(ctypes.c_bool, wintypes.HWND, wintypes.LPARAM)
    def visit(hwnd, _):
        pid = wintypes.DWORD()
        u.GetWindowThreadProcessId(hwnd, ctypes.byref(pid))
        if pid.value == app.pid and u.IsWindowVisible(hwnd):
            windows.append(hwnd)
        return True
    u.EnumWindows(visit, 0)
    if windows:
        window = windows[0]
        break
    if app.poll() is not None:
        raise RuntimeError(f"App exited {app.returncode}")
    time.sleep(.1)
if window is None:
    raise RuntimeError("No native window")
u.ShowWindow(window, 9)
width, height = u.GetSystemMetrics(0), u.GetSystemMetrics(1)
u.MoveWindow(window, 0, 0, width - 20, height - 50, True)
u.SetForegroundWindow(window)
state = {"pid": app.pid, "hwnd": window, "project": str(project), "fixture_root": str(directory),
         "screen": [width, height], "dpi": u.GetDpiForWindow(window), "python": sys.executable}
(out / "session.json").write_text(json.dumps(state, indent=2))
print(json.dumps(state, indent=2))
