"""Debug: dump accessible names per surface for the capture script anchors."""

import ctypes
import json
import os
import subprocess
import sys
import tempfile
import time
from ctypes import wintypes
from pathlib import Path

sys.path.insert(0, "src")
os.environ["KL_EXPERIMENTAL_INTERACTIVITY"] = "1"
from kernelledger.cli import ensure_supervisor, rpc
from kernelledger.notebook import Notebook

u = ctypes.windll.user32
u.SetProcessDPIAware()
root = Path(tempfile.mkdtemp(prefix="kl-anchors-"))
project = root / "notebook"
local = root / "profile"
profile = local / "KernelLedger/Desktop"
profile.mkdir(parents=True)
(profile / "appearance.txt").write_text("light")
(profile / "workbench.json").write_text(json.dumps({"schema": 2, "ui_scale": 1}))
n = Notebook(project)
n.init()
n.add("markdown", "# Release notebook", "intro")
n.add("code", "print('Hello, KernelLedger')\nanswer = 42", "hello")
n.add("code", "1 / 0", "error")
ensure_supervisor(project, sys.executable)
for cell in ("hello", "error"):
    rpc(project, {"command": "run", "cell_id": cell})
rpc(project, {"command": "checkpoint"})
p = subprocess.Popen(
    [str(Path("desktop/target/debug/kernelledger-desktop.exe").resolve()), str(project)],
    env={**os.environ, "LOCALAPPDATA": str(local), "KL_RUNTIME_PYTHON": sys.executable},
    creationflags=0x08000000,
)
time.sleep(4)
windows = []
visit_t = ctypes.WINFUNCTYPE(ctypes.c_bool, wintypes.HWND, wintypes.LPARAM)


def visit(hwnd, _):
    pid = wintypes.DWORD()
    u.GetWindowThreadProcessId(hwnd, ctypes.byref(pid))
    if pid.value == p.pid and u.IsWindowVisible(hwnd):
        windows.append(hwnd)
    return True


u.EnumWindows(visit_t(visit), 0)
hwnd = windows[0]

PRE = """
Add-Type -AssemblyName UIAutomationClient
if (-not ('DbgKeys' -as [type])) { Add-Type -TypeDefinition 'using System; using System.Runtime.InteropServices; public static class DbgKeys { [DllImport("user32.dll")] public static extern bool PostMessageW(IntPtr h, uint m, IntPtr w, IntPtr l); }' }
$root = [System.Windows.Automation.AutomationElement]::FromHandle([IntPtr]%d)
function Find-Named([string] $label) {
    $c = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::NameProperty, $label)
    for ($i = 0; $i -lt 40; $i++) { $n = $root.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $c); if ($n) { return $n }; Start-Sleep -Milliseconds 100 }
    throw "missing $label"
}
function Invoke-Named([string] $label) {
    $e = Find-Named $label
    $p = $null
    if ($e.TryGetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern, [ref]$p)) { $p.Invoke() }
    elseif ($e.TryGetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern, [ref]$p)) { $p.Toggle() }
    else { throw "no pattern $label" }
    Start-Sleep -Milliseconds 350
}
function Press-Key([int] $c) { [DbgKeys]::PostMessageW([IntPtr]%d, 0x0100, [IntPtr]$c, [IntPtr]::Zero) | Out-Null; [DbgKeys]::PostMessageW([IntPtr]%d, 0x0101, [IntPtr]$c, [IntPtr]::Zero) | Out-Null; Start-Sleep -Milliseconds 250 }
function Dump { $root.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition) | ForEach-Object { $_.Current.Name } }
""" % (hwnd, hwnd, hwnd)


def run(script):
    r = subprocess.run(
        ["powershell", "-NoProfile", "-Command", PRE + script],
        capture_output=True,
        text=True,
        encoding="utf-8",
    )
    return (r.stdout, r.stderr)


for label, script in [
    ("Runtimes", "Invoke-Named 'Menu'\nInvoke-Named 'Runtimes'\nStart-Sleep -Milliseconds 900\nDump"),
    ("Diagnostics", "Invoke-Named 'Menu'\nInvoke-Named 'Diagnostics'\nStart-Sleep -Milliseconds 900\nDump"),
    ("Settings", "Invoke-Named 'Settings'\nStart-Sleep -Milliseconds 700\nDump"),
    ("Help", "Invoke-Named 'Menu'\nInvoke-Named 'Help'\nStart-Sleep -Milliseconds 700\nDump"),
    ("History", "Invoke-Named 'History'\nStart-Sleep -Milliseconds 800\nDump"),
    ("Checkpoints", "Invoke-Named 'Checkpoints'\nStart-Sleep -Milliseconds 800\nDump"),
    ("Variables", "Invoke-Named 'Commands'\n$q = Find-Named 'Type a command'\n$q.SetFocus()\n$q.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).SetValue('Variables')\nStart-Sleep -Milliseconds 300\nPress-Key 0x0d\nStart-Sleep -Milliseconds 800\nDump"),
    ("Assistant", "Invoke-Named 'Notebook'\nInvoke-Named 'Assistant'\nStart-Sleep -Milliseconds 900\nDump"),
]:
    out, err = run(script)
    print(f"=== {label}")
    print(out.strip()[:2000] if out.strip() else f"ERR {err.strip()[:600]}")

u.PostMessageW(hwnd, 0x0010, 0, 0)
p.wait(timeout=10)
rpc(project, {"command": "stop"})
