"""Close only processes launched by this benchmark."""
import ctypes
from ctypes import wintypes
import json
from pathlib import Path
import subprocess
import time

from kernelledger.cli import alive, rpc

out = Path(__file__).resolve().parent
state = json.loads((out / "session.json").read_text())
user = ctypes.windll.user32
user.GetWindowThreadProcessId.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.DWORD)]
user.PostMessageW.argtypes = [wintypes.HWND, wintypes.UINT, wintypes.WPARAM, wintypes.LPARAM]
actual = wintypes.DWORD()
user.GetWindowThreadProcessId(state["kl_hwnd"], ctypes.byref(actual))
if actual.value:
    assert actual.value == state["kl_ui"]
    user.PostMessageW(state["kl_hwnd"], 0x10, 0, 0)
root = out / "kernelledger-notebook"
if alive(root):
    rpc(root, {"command": "stop"})

# Windows' venv launcher and spawned kernel are children of this owned PID.
query = subprocess.run(
    ["powershell", "-NoProfile", "-Command", f"Get-CimInstance Win32_Process -Filter 'ProcessId={state['marimo']}' | Select-Object -ExpandProperty CommandLine"],
    capture_output=True, text=True, creationflags=0x08000000,
)
if query.stdout.strip():
    assert str(out / "venv/Scripts/python.exe").lower() in query.stdout.strip().lower()
    subprocess.run(["taskkill", "/PID", str(state["marimo"]), "/T", "/F"], check=True, creationflags=0x08000000)
time.sleep(0.5)
print("Closed benchmark UI and runtimes; installation and evidence retained.")
