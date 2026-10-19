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
p.add_argument("--window-capture-exe", type=Path, help="Capture only the probe window using Windows Graphics Capture")
p.add_argument("--ui-scale", type=float, choices=[1, 1.25, 1.5, 2], default=1)
p.add_argument("--comfortable", action="store_true")
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
    notebook.add("code", "print('Hello, KernelLedger')\nanswer = 42", "hello")
    notebook.add("code", "1 / 0", "error")
    profile = Path(directory) / "profile/KernelLedger/Desktop"
    profile.mkdir(parents=True)
    (profile / "appearance.txt").write_text(args.theme)
    (profile / "workbench.json").write_text(json.dumps({"schema":2, "ui_scale":args.ui_scale, "comfortable":args.comfortable}))
    ensure_supervisor(project, sys.executable)
    rpc(project, {"command":"run", "cell_id":"hello"})
    rpc(project, {"command":"run", "cell_id":"error"})
    rpc(project, {"command":"checkpoint"})
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
[Console]::OutputEncoding = New-Object System.Text.UTF8Encoding $false
Add-Type -AssemblyName UIAutomationClient
$root = [System.Windows.Automation.AutomationElement]::FromHandle([IntPtr]{window})
$nodes = $root.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
$items = @($root) + @($nodes | ForEach-Object {{ $_ }})
$items | ForEach-Object {{ [pscustomobject]@{{ name=$_.Current.Name; role=$_.Current.ControlType.ProgrammaticName; id=$_.Current.AutomationId }} }} | ConvertTo-Json -Depth 3
'''
        # A visible window can precede the asynchronous snapshot/output load.
        # Wait for the actual output semantics rather than a fixed startup sleep.
        ready_deadline = time.monotonic() + 20
        while True:
            result = subprocess.run(["powershell", "-NoProfile", "-Command", ps], capture_output=True, text=True, encoding="utf-8", timeout=20)
            if result.returncode: raise RuntimeError(result.stderr)
            tree = json.loads(result.stdout)
            if any("Hello, KernelLedger\n" in node["name"] for node in tree): break
            if time.monotonic() >= ready_deadline:
                raise RuntimeError("Recorded output did not become accessible within 20 seconds")
            time.sleep(.1)
        (args.output / f"uia-{args.theme}.json").write_text(json.dumps(tree, indent=2))
        names = [node["name"] for node in tree]
        # Icon controls retain their accessible names in compact layouts.
        for destination in ["Notebook", "History", "Checkpoints", "Menu", "Outline"]:
            assert any(destination in name for name in names), destination
        assert any("Run print('Hello, KernelLedger')" in name for name in names), names
        assert any("Hello, KernelLedger\n" in name for name in names), names
        # Comfortable overflow and compact 200% fold Runtime into Menu at 800px.
        runtime_on_bar = not (
            (args.comfortable and 800 < 560 * args.ui_scale)
            or (not args.comfortable and args.ui_scale >= 2)
        )
        if runtime_on_bar:
            assert any(name.startswith("Runtime: Python") for name in names), names
        assert "Run all" in names, names
        assert "Run selected" not in names and "Interrupt" not in names, names
        assert not any(name in ("[ ]", "[1]", "[2]") for name in names), names
        navigation_script = f'''
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient
if (-not ('ProbeOwnedKeys' -as [type])) {{
    Add-Type -TypeDefinition 'using System; using System.Runtime.InteropServices; public static class ProbeOwnedKeys {{ [DllImport("user32.dll")] public static extern bool PostMessageW(IntPtr hwnd, uint msg, IntPtr w, IntPtr l); }}'
}}
$root = [System.Windows.Automation.AutomationElement]::FromHandle([IntPtr]{window})
function Invoke-Named([string] $label) {{
    $condition = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::NameProperty, $label)
    $element = $root.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $condition)
    if ($null -eq $element) {{
        $condition = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::NameProperty, "$label, current destination")
        $element = $root.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $condition)
    }}
    if ($null -eq $element) {{ throw "Missing action: $label" }}
    $pattern = $null
    if ($element.TryGetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern, [ref]$pattern)) {{
        $pattern.Invoke()
    }} elseif ($element.TryGetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern, [ref]$pattern)) {{
        $pattern.Toggle()
    }} else {{ throw "Action has no activation pattern: $label" }}
    Start-Sleep -Milliseconds 150
}}
Invoke-Named 'Menu'
$nodes = $root.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
$names = @($nodes | ForEach-Object {{ $_.Current.Name }})
foreach ($label in @('Settings','Help','Diagnostics','Runtimes')) {{
    if ($names -notcontains $label) {{ throw "Missing menu destination: $label" }}
}}
# Escape closes even when the final menu row is below the scrolling viewport.
[ProbeOwnedKeys]::PostMessageW([IntPtr]{window}, 0x0100, [IntPtr]0x1b, [IntPtr]::Zero) | Out-Null
[ProbeOwnedKeys]::PostMessageW([IntPtr]{window}, 0x0101, [IntPtr]0x1b, [IntPtr]::Zero) | Out-Null
Start-Sleep -Milliseconds 150
Invoke-Named 'Checkpoints'
$nodes = $root.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
$names = @($nodes | ForEach-Object {{ $_.Current.Name }})
if ($names -notcontains 'Create checkpoint') {{ throw 'Missing checkpoint creation action' }}
if ($names -contains 'Outline') {{ throw 'Notebook outline leaked onto Checkpoints' }}
Invoke-Named 'History'
Invoke-Named 'Notebook'
'Native menu and destination navigation passed'
'''
        navigation = subprocess.run(["powershell", "-NoProfile", "-Command", navigation_script], capture_output=True, text=True, timeout=20)
        if navigation.returncode: raise RuntimeError(navigation.stderr)
        # Exercise the real compositor and repeat compact/wide transitions.
        for iteration in range(10):
            for w, h in [(800, 600), (1440, 900)]:
                u.MoveWindow(window, 0, 0, int(w * scale), int(h * scale), True)
                time.sleep(.08)
        captures = []
        alignment = []
        caption_checks = []
        for w, h in [(800, 600), (1024, 768), (1280, 850), (1440, 900)]:
            u.MoveWindow(window, 0, 0, int(w * scale), int(h * scale), True)
            time.sleep(.4)
            measurement_script = f'''
[Console]::OutputEncoding = New-Object System.Text.UTF8Encoding $false
Add-Type -AssemblyName UIAutomationClient
$root = [System.Windows.Automation.AutomationElement]::FromHandle([IntPtr]{window})
$nodes = $root.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
$nodes | Where-Object {{ $_.Current.ControlType -eq [System.Windows.Automation.ControlType]::Button -and ($_.Current.Name -in @('Notebook','Notebook, current destination','History','Checkpoints','Outline','Commands','Menu','Run all') -or $_.Current.Name.StartsWith('Notebook switcher:') -or $_.Current.Name.StartsWith('Runtime: Python')) }} | ForEach-Object {{ $b=$_.Current.BoundingRectangle; [pscustomobject]@{{ name=$_.Current.Name; x=$b.X; y=$b.Y; width=$b.Width; height=$b.Height; center=$b.Y+$b.Height/2 }} }} | ConvertTo-Json -Depth 3
'''
            measurement = subprocess.run(["powershell", "-NoProfile", "-Command", measurement_script], check=True, capture_output=True, text=True, encoding="utf-8", timeout=20)
            buttons = json.loads(measurement.stdout)
            centers = [b["center"] for b in buttons]
            spread = (max(centers) - min(centers)) / scale
            assert spread <= 1, (w, h, buttons)
            alignment.append({"logical_size": [w, h], "vertical_center_spread_logical_px": spread, "buttons": buttons})
            client = wintypes.RECT()
            origin = wintypes.POINT()
            u.GetClientRect(window, ctypes.byref(client))
            u.ClientToScreen(window, ctypes.byref(origin))
            # Exercise hit testing only inside the owned window. No global input.
            def hit_test(x, y):
                local_x, local_y = round(x - origin.x), round(y - origin.y)
                u.SendMessageW(window, 0x0200, 0, (local_y << 16) | (local_x & 0xffff))
                time.sleep(.04)
                return u.SendMessageW(window, 0x0084, 0, (round(y) << 16) | (round(x) & 0xffff))
            for b in buttons:
                assert b["x"] >= origin.x and b["x"] + b["width"] <= origin.x + client.right - 102 * scale, (w, h, b)
                result = hit_test(b["x"] + b["width"]/2, b["center"])
                assert result == 1, (b["name"], "Control must use client hit testing", result)
            caption_y = origin.y + (56 if args.comfortable else 52) * args.ui_scale * scale / 2
            codes = {}
            for label, offset, expected in [("minimize",85,8), ("maximize",51,9), ("close",17,20)]:
                result = hit_test(origin.x + client.right - offset * scale, caption_y)
                assert result == expected, (label, result, expected)
                codes[label] = result
            last_destination = next(b for b in buttons if b["name"] == "Checkpoints")
            first_action = next(b for b in buttons if b["name"] == "Commands" or b["name"].startswith("Runtime: Python") or b["name"] == "Run all")
            drag_x = (last_destination["x"] + last_destination["width"] + first_action["x"]) / 2
            assert hit_test(drag_x, caption_y) == 2, "Blank title bar must allow native dragging"
            caption_checks.append({"logical_size":[w,h], "hit_test_codes":codes, "interactive_controls":"client", "blank_chrome":"drag"})
            if args.window_capture_exe:
                name = f"notebook-{args.theme}-{w}x{h}.png"
                subprocess.run([str(args.window_capture_exe.resolve()), str(window),
                    str((args.output / name).resolve())], check=True, capture_output=True, timeout=15)
                captures.append(name)
                continue
            if u.GetForegroundWindow() != window:
                continue  # Never capture another application or take foreground ownership.
            u.GetWindowRect(window, ctypes.byref(rect))
            name = f"notebook-{args.theme}-{w}x{h}.png"
            (args.output / name).unlink(missing_ok=True)
            capture = str((args.output / name).resolve()).replace("'", "''")
            script = f"""Add-Type -AssemblyName System.Drawing
Add-Type -TypeDefinition 'using System; using System.Runtime.InteropServices; public static class CaptureOwner {{ [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow(); }}'
if ([CaptureOwner]::GetForegroundWindow() -ne [IntPtr]{window}) {{ exit 0 }}
$bitmap = New-Object System.Drawing.Bitmap({rect.right - max(0, rect.left)}, {rect.bottom - max(0, rect.top)})
$graphics = [System.Drawing.Graphics]::FromImage($bitmap)
$graphics.CopyFromScreen({max(0, rect.left)}, {max(0, rect.top)}, 0, 0, $bitmap.Size)
if ([CaptureOwner]::GetForegroundWindow() -eq [IntPtr]{window}) {{ $bitmap.Save('{capture}', [System.Drawing.Imaging.ImageFormat]::Png) }}
$graphics.Dispose()
$bitmap.Dispose()
"""
            subprocess.run(["powershell", "-NoProfile", "-Command", script], check=True, capture_output=True, timeout=20)
            if (args.output / name).exists(): captures.append(name)
        # Capture the native Settings composition as well as the notebook.
        # Invoke by accessible name; crop only our own foreground window.
        u.MoveWindow(window, 0, 0, int(1280 * scale), int(850 * scale), True)
        time.sleep(.2)
        client = wintypes.RECT()
        origin = wintypes.POINT()
        u.GetClientRect(window, ctypes.byref(client))
        u.ClientToScreen(window, ctypes.byref(origin))
        settings_capture = str((args.output / f"settings-{args.theme}.png").resolve()).replace("'", "''")
        settings_tree = str((args.output / f"settings-{args.theme}.uia.json").resolve()).replace("'", "''")
        settings_script = navigation_script + f'''
Invoke-Named 'Menu'
Invoke-Named 'Settings'
$nodes = $root.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
$nodes | ForEach-Object {{ [pscustomobject]@{{ name=$_.Current.Name; role=$_.Current.ControlType.ProgrammaticName; enabled=$_.Current.IsEnabled; bounds=$_.Current.BoundingRectangle.ToString() }} }} | ConvertTo-Json -Depth 3 | Set-Content -Encoding UTF8 '{settings_tree}'
Add-Type -AssemblyName System.Drawing
Add-Type -TypeDefinition 'using System; using System.Runtime.InteropServices; public static class SettingsCaptureOwner {{ [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow(); }}'
if ([SettingsCaptureOwner]::GetForegroundWindow() -eq [IntPtr]{window}) {{
    $bitmap = New-Object System.Drawing.Bitmap({client.right}, {client.bottom})
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    $graphics.CopyFromScreen({origin.x}, {origin.y}, 0, 0, $bitmap.Size)
    if ([SettingsCaptureOwner]::GetForegroundWindow() -eq [IntPtr]{window}) {{ $bitmap.Save('{settings_capture}', [System.Drawing.Imaging.ImageFormat]::Png) }}
    $graphics.Dispose()
    $bitmap.Dispose()
}}
Invoke-Named 'Notebook'
'''
        subprocess.run(["powershell", "-NoProfile", "-Command", settings_script], check=True, capture_output=True, timeout=20)
        if args.window_capture_exe:
            # The settings script has already returned to Notebook.
            invoke_settings = navigation_script + "Invoke-Named 'Menu'\nInvoke-Named 'Settings'"
            subprocess.run(["powershell", "-NoProfile", "-Command", invoke_settings], check=True, capture_output=True, timeout=20)
            time.sleep(.3)
            subprocess.run([str(args.window_capture_exe.resolve()), str(window), settings_capture], check=True, capture_output=True, timeout=15)
        if (args.output / f"settings-{args.theme}.png").exists(): captures.append(f"settings-{args.theme}.png")
        report = {"os":platform.platform(),"python":sys.version,"executable":str(args.exe.resolve()),"theme":args.theme,"dpi":dpi,"minimum_outer_logical":[width/scale,height/scale],"time_to_visible_window_ms":round(startup_ms,2),"window_title":title.value,"uia_nodes":len(tree),"native_identity":"passed","output_semantics":"passed","screenshot":"Not captured: GPU PrintWindow unavailable; desktop capture can include another window","measurement_limit":"Time to visible window only, not editor-ready latency. No NVDA/IME/mixed-DPI qualification."}
        report["resize_cycles"] = 10
        report["native_menu_and_destinations"] = "passed"
        report["bar_alignment"] = alignment
        report["caption_controls"] = caption_checks
        report["ui_scale"] = args.ui_scale
        report["comfortable"] = args.comfortable
        report["screenshot"] = captures
        report["capture_limit"] = "Desktop capture cropped to owned window; verify occlusion and physical monitor bounds visually. Resize calls alone do not prove control reachability."
        if args.window_capture_exe:
            report["capture_limit"] = "Windows Graphics Capture restricted to the disposable probe HWND; no foreground or desktop capture."
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
        deadline = time.monotonic() + 5
        while (project / ".kl/supervisor.json").exists() and time.monotonic() < deadline:
            time.sleep(.05)
        # The registry disappears before Windows closes inherited log handles.
        time.sleep(2)
