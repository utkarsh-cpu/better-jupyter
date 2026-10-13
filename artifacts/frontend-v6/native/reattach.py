import ctypes
from ctypes import wintypes
import json, os, subprocess, sys, time
from pathlib import Path
repo=Path(__file__).resolve().parents[3]
out=Path(__file__).resolve().parent
previous=json.loads((out/'session.json').read_text())
project=Path(previous['project']);directory=Path(previous['fixture_root'])
with (out/'app.stdout.log').open('ab') as stdout,(out/'app.stderr.log').open('ab') as stderr:
    app=subprocess.Popen([str(repo/'desktop/target/debug/kernelledger-desktop.exe'),str(project)],cwd=repo,
        env={**os.environ,'LOCALAPPDATA':str(directory/'profile'),'KL_RUNTIME_PYTHON':sys.executable},stdout=stdout,stderr=stderr,creationflags=0x08000000)
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
