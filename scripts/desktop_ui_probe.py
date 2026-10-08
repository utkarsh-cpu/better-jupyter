"""Windows native probe: window identity, minimum sizing, UIA and startup timing.

This does not certify NVDA, mixed DPI, IME or task usability. It uses a disposable
notebook/profile and closes only the application process that it launches.
"""
import argparse
import ctypes
from ctypes import wintypes
import json
import os
from pathlib import Path
import platform
import subprocess
import sys
import tempfile
import time
from kernelledger.cli import alive, rpc, ensure_supervisor

from kernelledger.notebook import Notebook

p = argparse.ArgumentParser()
p.add_argument("--exe", type=Path, default=Path("desktop/target/debug/kernelledger-desktop.exe"))
p.add_argument("--output", type=Path, default=Path("artifacts/frontend-v3"))
p.add_argument("--theme", choices=["light", "dark", "system", "contrast"], default="light")
args = p.parse_args()
if os.name != "nt": raise SystemExit("Windows native probe only")
args.output.mkdir(parents=True, exist_ok=True)
u = ctypes.windll.user32
u.SetProcessDPIAware()
with tempfile.TemporaryDirectory(prefix="kl-ui-probe-") as directory:
    project = Path(directory) / "notebook"
    notebook = Notebook(project)
    notebook.init()
    notebook.add("markdown", "# UI regression fixture\nRecorded history starts empty.", "welcome")
    notebook.add("code", "print('Hello, KernelLedger')", "hello")
    notebook.add("code", "1 / 0", "error")
    profile = Path(directory) / "profile/KernelLedger/Desktop"
    profile.mkdir(parents=True)
    (profile / "appearance.txt").write_text(args.theme)
    ensure_supervisor(project, sys.executable)
    rpc(project, {"command":"run", "cell_id":"hello"})
    started = time.perf_counter()
    app = subprocess.Popen([str(args.exe.resolve()), str(project)], cwd=Path(__file__).resolve().parents[1], env={**os.environ,"LOCALAPPDATA":str(Path(directory) / "profile"), "KL_RUNTIME_PYTHON":sys.executable}, creationflags=0x08000000)
    window = None
    def own_windows():
        found = []
        @ctypes.WINFUNCTYPE(ctypes.c_bool, wintypes.HWND, wintypes.LPARAM)
        def visit(hwnd, _):
            pid = wintypes.DWORD()
            u.GetWindowThreadProcessId(hwnd, ctypes.byref(pid))
            if pid.value == app.pid and u.IsWindowVisible(hwnd): found.append(hwnd)
            return True
        u.EnumWindows(visit, 0)
        return found
    try:
        deadline = time.monotonic() + 20
        while time.monotonic() < deadline:
            windows = own_windows()
            if windows:
                window = windows[0]
                break
            if app.poll() is not None: raise RuntimeError(f"Application exited: {app.returncode}")
            time.sleep(.02)
        if window is None: raise RuntimeError("No visible app window within 20 seconds")
        startup_ms = (time.perf_counter() - started) * 1000
        time.sleep(1)
        title = ctypes.create_unicode_buffer(1024)
        u.GetWindowTextW(window, title, len(title))
        assert title.value.startswith("KernelLedger"), title.value
        dpi = u.GetDpiForWindow(window)
        scale = dpi / 96
        # Request below the minimum; the app must enforce the supported dimensions.
        u.MoveWindow(window, 20, 20, int(500 * scale), int(400 * scale), True)
        time.sleep(.3)
        rect = wintypes.RECT()
        u.GetWindowRect(window, ctypes.byref(rect))
        width, height = rect.right - rect.left, rect.bottom - rect.top
        assert width / scale >= 800 and height / scale >= 600, (width, height, scale)
        ps = f'''
Add-Type -AssemblyName UIAutomationClient
$root = [System.Windows.Automation.AutomationElement]::FromHandle([IntPtr]{window})
$nodes = $root.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
$items = @($root) + @($nodes | ForEach-Object {{ $_ }})
$items | ForEach-Object {{ [pscustomobject]@{{ name=$_.Current.Name; role=$_.Current.ControlType.ProgrammaticName; id=$_.Current.AutomationId }} }} | ConvertTo-Json -Depth 3
'''
        result = subprocess.run(["powershell", "-NoProfile", "-Command", ps], capture_output=True, text=True, timeout=20)
        if result.returncode: raise RuntimeError(result.stderr)
        tree = json.loads(result.stdout)
        (args.output / f"uia-{args.theme}.json").write_text(json.dumps(tree, indent=2))
        names = [node["name"] for node in tree]
        for destination in ["Notebook", "History", "Checkpoints", "Runtime", "Menu", "Commands", "Outline"]:
            assert any(destination in name for name in names), destination
        assert any("Run print('Hello, KernelLedger')" in name for name in names), names
        assert any("Hello, KernelLedger\n" in name for name in names), names
        # Exercise the real compositor and repeat compact/wide transitions.
        for iteration in range(10):
            for w, h in [(800, 600), (1440, 900)]:
                u.MoveWindow(window, 0, 0, int(w * scale), int(h * scale), True)
                time.sleep(.08)
        captures = []
        for w, h in [(800, 600), (1024, 768), (1280, 850), (1440, 900)]:
            u.MoveWindow(window, 0, 0, int(w * scale), int(h * scale), True)
            time.sleep(.4)
            if u.GetForegroundWindow() != window:
                continue  # Never capture another application or take foreground ownership.
            u.GetWindowRect(window, ctypes.byref(rect))
            name = f"notebook-{args.theme}-{w}x{h}.png"
            capture = str((args.output / name).resolve()).replace("'", "''")
            script = f"""Add-Type -AssemblyName System.Drawing
$bitmap = New-Object System.Drawing.Bitmap({rect.right - max(0, rect.left)}, {rect.bottom - max(0, rect.top)})
$graphics = [System.Drawing.Graphics]::FromImage($bitmap)
$graphics.CopyFromScreen({max(0, rect.left)}, {max(0, rect.top)}, 0, 0, $bitmap.Size)
$bitmap.Save('{capture}', [System.Drawing.Imaging.ImageFormat]::Png)
$graphics.Dispose()
$bitmap.Dispose()
"""
            subprocess.run(["powershell", "-NoProfile", "-Command", script], check=True, capture_output=True, timeout=20)
            captures.append(name)
        report = {"os":platform.platform(),"python":sys.version,"executable":str(args.exe.resolve()),"theme":args.theme,"dpi":dpi,"minimum_outer_logical":[width/scale,height/scale],"time_to_visible_window_ms":round(startup_ms,2),"window_title":title.value,"uia_nodes":len(tree),"native_identity":"passed","output_semantics":"passed","screenshot":"Not captured: GPU PrintWindow unavailable; desktop capture can include another window","measurement_limit":"Time to visible window only, not editor-ready latency. No NVDA/IME/mixed-DPI qualification."}
        report["resize_cycles"] = 10
        report["screenshot"] = captures
        report["capture_limit"] = "Desktop capture cropped to owned window; verify occlusion and physical monitor bounds visually. Resize calls alone do not prove control reachability."
        (args.output / f"probe-{args.theme}.json").write_text(json.dumps(report,indent=2))
        print(json.dumps(report,indent=2))
    finally:
        if window: u.PostMessageW(window, 0x0010, 0, 0)
        try: app.wait(timeout=10)
        except subprocess.TimeoutExpired:
            app.terminate()
            app.wait(timeout=5)
        if alive(project): rpc(project, {"command":"stop"})
        # Clean up only the fixture runtime started by the probe.
        time.sleep(.5)
