"""Exercise wheel scrolling in an isolated native release window, with no global input."""
import ctypes
from ctypes import wintypes
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time

repo = Path(__file__).resolve().parents[2]
output = Path(__file__).resolve().parent
exe = repo / "desktop/target/release/kernelledger-desktop.exe"
capture = repo / "artifacts/frontend-bars-polish/capture/target/debug/kl-window-capture.exe"
user = ctypes.windll.user32
user.SetProcessDPIAware()
user.SendMessageW.argtypes = [wintypes.HWND, wintypes.UINT, wintypes.WPARAM, wintypes.LPARAM]
user.SendMessageW.restype = wintypes.LPARAM

with tempfile.TemporaryDirectory(prefix="kl-palette-scroll-") as temporary:
    app = subprocess.Popen(
        [str(exe)], cwd=repo,
        env={**os.environ, "LOCALAPPDATA": temporary}, creationflags=0x08000000,
    )
    hwnd = None
    try:
        deadline = time.monotonic() + 20
        while hwnd is None and time.monotonic() < deadline:
            found = []

            @ctypes.WINFUNCTYPE(ctypes.c_bool, wintypes.HWND, wintypes.LPARAM)
            def visit(candidate, _):
                pid = wintypes.DWORD()
                user.GetWindowThreadProcessId(candidate, ctypes.byref(pid))
                if pid.value == app.pid and user.IsWindowVisible(candidate):
                    found.append(candidate)
                return True

            user.EnumWindows(visit, 0)
            hwnd = found[0] if found else None
            if app.poll() is not None:
                raise RuntimeError(f"App exited: {app.returncode}")
            time.sleep(0.05)
        assert hwnd, "Native window did not appear"

        def inspect(activate=False):
            script = f"""
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient
$root = [System.Windows.Automation.AutomationElement]::FromHandle([IntPtr]{hwnd})
"""
            if activate:
                script += """
$name = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::NameProperty, 'Commands')
$role = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ControlTypeProperty, [System.Windows.Automation.ControlType]::Button)
$condition = New-Object System.Windows.Automation.AndCondition($name, $role)
$button = $root.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $condition)
if ($null -eq $button) { throw 'Commands button missing' }
$button.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
Start-Sleep -Milliseconds 200
"""
            script += """
$root.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition) | ForEach-Object {
    $b = $_.Current.BoundingRectangle
    [pscustomobject]@{name=$_.Current.Name; role=$_.Current.ControlType.ProgrammaticName; x=$b.X; y=$b.Y; width=$b.Width; height=$b.Height; offscreen=$_.Current.IsOffscreen}
} | ConvertTo-Json -Depth 3
"""
            result = subprocess.run(
                ["powershell", "-NoProfile", "-Command", script],
                check=True, capture_output=True, text=True, encoding="utf-8", timeout=20,
            )
            return json.loads(result.stdout)

        initial = inspect(activate=True)
        (output / "palette-initial.uia.json").write_text(json.dumps(initial, indent=2))
        first = next(node for node in initial if node["name"].startswith("Notebook, selected"))
        assert first["height"] > 0
        # Target the middle of an actual command row, using native screen coordinates.
        x = round(first["x"] + first["width"] / 2)
        y = round(first["y"] + first["height"] / 2)
        origin = wintypes.POINT()
        user.ClientToScreen(hwnd, ctypes.byref(origin))
        user.SendMessageW(hwnd, 0x0200, 0, ((y - origin.y) << 16) | ((x - origin.x) & 0xffff))
        for _ in range(30):
            user.SendMessageW(hwnd, 0x020A, (0xff88 << 16), (y << 16) | (x & 0xffff))
            time.sleep(0.01)
        time.sleep(0.15)
        bottom = inspect()
        (output / "palette-bottom.uia.json").write_text(json.dumps(bottom, indent=2))
        last = next(node for node in bottom if node["name"].startswith("Move cell down"))
        assert last["height"] > 0 and not last["offscreen"], last
        assert not any(node["name"].startswith("Notebook, selected") for node in bottom)
        subprocess.run([str(capture), str(hwnd), str(output / "palette-bottom.png")], check=True, capture_output=True, timeout=15)

        # Keyboard navigation must still reveal the first option after manual scrolling.
        user.PostMessageW(hwnd, 0x0100, 0x24, 0x01470001)
        user.PostMessageW(hwnd, 0x0101, 0x24, 0xc1470001)
        time.sleep(0.15)
        restored = inspect()
        (output / "palette-restored.uia.json").write_text(json.dumps(restored, indent=2))
        assert any(node["name"].startswith("Notebook, selected") and not node["offscreen"] for node in restored)
        subprocess.run([str(capture), str(hwnd), str(output / "palette-top.png")], check=True, capture_output=True, timeout=15)
        report = {"executable": str(exe), "wheel_to_last_option": "passed", "hidden_rows_unmounted": "passed", "home_reveals_first_option": "passed", "input_scope": "owned HWND only"}
        (output / "palette-native-report.json").write_text(json.dumps(report, indent=2))
        print(json.dumps(report, indent=2))
    finally:
        if hwnd:
            user.PostMessageW(hwnd, 0x0010, 0, 0)
        try:
            app.wait(timeout=10)
        except subprocess.TimeoutExpired:
            app.terminate()
            app.wait(timeout=5)
