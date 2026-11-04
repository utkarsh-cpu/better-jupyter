"""Windows native probe: window identity, minimum sizing, UIA and startup timing.

This does not certify NVDA, mixed DPI, IME or task usability. It uses a disposable
notebook/profile and closes only the application process that it launches.
"""

import argparse
import ctypes
import hashlib
import json
import os
import platform
import subprocess
import sys
import tempfile
import time
from ctypes import wintypes
from pathlib import Path

from kernelledger.cli import alive, ensure_supervisor, rpc
from kernelledger.jupyter_adapter import RuntimeConnection
from kernelledger.ledger import Ledger
from kernelledger.notebook import Notebook

p = argparse.ArgumentParser()
p.add_argument("--exe", type=Path, default=Path("desktop/target/debug/kernelledger-desktop.exe"))
p.add_argument("--output", type=Path, default=Path("artifacts/frontend-v3"))
p.add_argument("--theme", choices=["light", "dark", "system", "contrast"], default="light")
p.add_argument(
    "--window-capture-exe",
    type=Path,
    help="Capture only the probe window using Windows Graphics Capture",
)
p.add_argument("--ui-scale", type=float, choices=[1, 1.25, 1.5, 2], default=1)
p.add_argument("--comfortable", action="store_true")
p.add_argument(
    "--exercise-runtime",
    action="store_true",
    help="Invoke native Run and Create checkpoint controls and verify durable results",
)
p.add_argument(
    "--exercise-interactivity",
    action="store_true",
    help="Invoke Variables and Console through native accessibility controls",
)
p.add_argument(
    "--exercise-advanced",
    action="store_true",
    help="Exercise opt-in debugger, widgets and native static renderers",
)
args = p.parse_args()
if args.exercise_advanced:
    args.exercise_interactivity = True
    os.environ["KL_EXPERIMENTAL_INTERACTIVITY"] = "1"
if os.name != "nt":
    raise SystemExit("Windows native probe only")
args.output.mkdir(parents=True, exist_ok=True)
u = ctypes.windll.user32
u.SetProcessDPIAware()
# A late inherited Windows log handle must not mask a behavioral failure.
with tempfile.TemporaryDirectory(prefix="kl-ui-probe-", ignore_cleanup_errors=True) as directory:
    project = Path(directory) / "notebook"
    notebook = Notebook(project)
    notebook.init()
    notebook.add("markdown", "# UI regression fixture\nRecorded history starts empty.", "welcome")
    notebook.add("code", "print('Hello, KernelLedger')\nanswer = 42", "hello")
    notebook.add("code", "1 / 0", "error")
    if args.exercise_advanced:
        notebook.add(
            "code",
            "display_fixture = True\ndisplay('Display before', display_id='probe-display')",
            "displays",
        )
        notebook.add(
            "code",
            "update_fixture = True\nupdate_display('Display after', 'probe-display')",
            "display_update",
        )
        notebook.add(
            "code", "debug_value = 40\ndebug_value += 2\nprint('Debugger finished')", "debug"
        )
        notebook.add(
            "code",
            "widget_fixture = True\nclicks = 0\ndef clicked(value):\n    global clicks\n    clicks += 1\ncontrol = widget('button', label='Increment', callback=clicked)\ndisplay(control)",
            "widgets",
        )
        notebook.add(
            "code",
            "rich_fixture = True\nclass Equation:\n    def _repr_latex_(self):\n        return r'$\\frac{1}{2} + \\sqrt{x_i^2}$'\ndisplay(Equation())\ndisplay_table([['alpha', 42]], columns=['Name', 'Value'])\ndisplay_media(b'RIFF' + b'\\0' * 40, 'audio/wav')",
            "rich",
        )
    profile = Path(directory) / "profile/KernelLedger/Desktop"
    profile.mkdir(parents=True)
    (profile / "appearance.txt").write_text(args.theme)
    (profile / "workbench.json").write_text(
        json.dumps({"schema": 2, "ui_scale": args.ui_scale, "comfortable": args.comfortable})
    )
    ensure_supervisor(project, sys.executable)
    rpc(project, {"command": "run", "cell_id": "hello"})
    rpc(project, {"command": "run", "cell_id": "error"})
    rpc(project, {"command": "checkpoint"})
    if args.exercise_advanced:
        assert rpc(project, {"command": "run", "cell_id": "displays"})["status"] == "ok"
        assert rpc(project, {"command": "run", "cell_id": "widgets"})["status"] == "ok"
        assert rpc(project, {"command": "run", "cell_id": "rich"})["status"] == "ok"
    started = time.perf_counter()
    app = subprocess.Popen(
        [str(args.exe.resolve()), str(project)],
        cwd=Path(__file__).resolve().parents[1],
        env={
            **os.environ,
            "LOCALAPPDATA": str(Path(directory) / "profile"),
            "KL_RUNTIME_PYTHON": sys.executable,
        },
        creationflags=0x08000000,
    )
    window = None

    def own_windows():
        found = []

        @ctypes.WINFUNCTYPE(ctypes.c_bool, wintypes.HWND, wintypes.LPARAM)
        def visit(hwnd, _):
            pid = wintypes.DWORD()
            u.GetWindowThreadProcessId(hwnd, ctypes.byref(pid))
            if pid.value == app.pid and u.IsWindowVisible(hwnd):
                found.append(hwnd)
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
            if app.poll() is not None:
                raise RuntimeError(f"Application exited: {app.returncode}")
            time.sleep(0.02)
        if window is None:
            raise RuntimeError("No visible app window within 20 seconds")
        startup_ms = (time.perf_counter() - started) * 1000
        time.sleep(1)
        title = ctypes.create_unicode_buffer(1024)
        u.GetWindowTextW(window, title, len(title))
        assert title.value.startswith("KernelLedger"), title.value
        dpi = u.GetDpiForWindow(window)
        scale = dpi / 96
        # Request below the minimum; the app must enforce the supported dimensions.
        u.MoveWindow(window, 20, 20, int(500 * scale), int(400 * scale), True)
        time.sleep(0.3)
        rect = wintypes.RECT()
        u.GetWindowRect(window, ctypes.byref(rect))
        width, height = rect.right - rect.left, rect.bottom - rect.top
        assert width / scale >= 800 and height / scale >= 600, (width, height, scale)
        ps = f"""
[Console]::OutputEncoding = New-Object System.Text.UTF8Encoding $false
Add-Type -AssemblyName UIAutomationClient
$root = [System.Windows.Automation.AutomationElement]::FromHandle([IntPtr]{window})
$nodes = $root.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
$items = @($root) + @($nodes | ForEach-Object {{ $_ }})
$items | ForEach-Object {{ [pscustomobject]@{{ name=$_.Current.Name; role=$_.Current.ControlType.ProgrammaticName; id=$_.Current.AutomationId }} }} | ConvertTo-Json -Depth 3
"""
        # A visible window can precede the asynchronous snapshot/output load.
        # Wait for the actual output semantics rather than a fixed startup sleep.
        ready_deadline = time.monotonic() + 20
        while True:
            result = subprocess.run(
                ["powershell", "-NoProfile", "-Command", ps],
                capture_output=True,
                text=True,
                encoding="utf-8",
                timeout=20,
            )
            if result.returncode:
                raise RuntimeError(result.stderr)
            tree = json.loads(result.stdout)
            if any("Hello, KernelLedger\n" in (node.get("name") or "") for node in tree):
                break
            if time.monotonic() >= ready_deadline:
                raise RuntimeError("Recorded output did not become accessible within 20 seconds")
            time.sleep(0.1)
        (args.output / f"uia-{args.theme}.json").write_text(json.dumps(tree, indent=2))
        names = [node.get("name") or "" for node in tree]
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
        navigation_helpers = f"""
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient
if (-not ('ProbeOwnedKeys' -as [type])) {{
    Add-Type -TypeDefinition 'using System; using System.Runtime.InteropServices; public static class ProbeOwnedKeys {{ [DllImport("user32.dll")] public static extern bool PostMessageW(IntPtr hwnd, uint msg, IntPtr w, IntPtr l); [DllImport("user32.dll")] public static extern IntPtr SendMessageW(IntPtr hwnd, uint msg, IntPtr w, IntPtr l); }}'
}}
$root = [System.Windows.Automation.AutomationElement]::FromHandle([IntPtr]{window})
function Invoke-Named([string] $label) {{
    for ($attempt = 0; $attempt -lt 40; $attempt++) {{
    $condition = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::NameProperty, $label)
    $element = $root.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $condition)
    if ($null -eq $element) {{
        $condition = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::NameProperty, "$label, current destination")
        $element = $root.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $condition)
    }}
    if ($null -ne $element -and $element.Current.IsEnabled) {{ break }}
    Start-Sleep -Milliseconds 100
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
function Scroll-Top {{
    $b = $root.Current.BoundingRectangle
    $x = [int]($b.Width * .65)
    $y = [int]($b.Height * .65)
    [ProbeOwnedKeys]::SendMessageW([IntPtr]{window}, 0x0200, [IntPtr]::Zero, [IntPtr](($y -shl 16) -bor $x)) | Out-Null
    for ($i = 0; $i -lt 5; $i++) {{
        [ProbeOwnedKeys]::SendMessageW([IntPtr]{window}, 0x020A, [IntPtr](720 -shl 16), [IntPtr]((([int]($b.Y+$y)) -shl 16) -bor [int]($b.X+$x))) | Out-Null
        Start-Sleep -Milliseconds 100
    }}
}}
"""
        navigation_script = (
            navigation_helpers
            + f"""
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
Scroll-Top
$nodes = $root.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
$names = @($nodes | ForEach-Object {{ $_.Current.Name }})
if ($names -notcontains 'Create checkpoint') {{ throw 'Missing checkpoint creation action' }}
if ($names -contains 'Outline') {{ throw 'Notebook outline leaked onto Checkpoints' }}
Invoke-Named 'History'
Invoke-Named 'Notebook'
'Native menu and destination navigation passed'
"""
        )
        navigation = subprocess.run(
            ["powershell", "-NoProfile", "-Command", navigation_script],
            capture_output=True,
            text=True,
            timeout=20,
        )
        if navigation.returncode:
            raise RuntimeError(navigation.stderr)
        runtime_actions = {}
        if args.exercise_runtime:

            def recorded(sql):
                ledger = Ledger(project / ".kl/ledger.sqlite", readonly=True)
                try:
                    return ledger.rows(sql)
                finally:
                    ledger.close()

            def await_record(sql, count):
                deadline = time.monotonic() + 30
                while time.monotonic() < deadline:
                    rows = recorded(sql)
                    if len(rows) > count:
                        return rows
                    time.sleep(0.05)
                raise RuntimeError("UI action did not publish its durable result")

            runs = "SELECT * FROM executions WHERE cell_id='hello' AND status='ok' ORDER BY ordinal"
            before = len(recorded(runs))
            invoke_run = navigation_script + "\nInvoke-Named \"Run print('Hello, KernelLedger')\""
            subprocess.run(
                ["powershell", "-NoProfile", "-Command", invoke_run],
                check=True,
                capture_output=True,
                timeout=25,
            )
            rows = await_record(runs, before)
            runtime_actions["run_from_ui"] = {"status": "passed", "exec_id": rows[-1]["exec_id"]}
            checkpoints = "SELECT * FROM checkpoints ORDER BY timestamp"
            before = len(recorded(checkpoints))
            invoke_checkpoint = (
                navigation_script + "\nInvoke-Named 'Checkpoints'\nInvoke-Named 'Create checkpoint'"
            )
            subprocess.run(
                ["powershell", "-NoProfile", "-Command", invoke_checkpoint],
                check=True,
                capture_output=True,
                timeout=25,
            )
            rows = await_record(checkpoints, before)
            assert rows[-1]["status"] == "complete", rows[-1]
            runtime_actions["checkpoint_from_ui"] = {
                "status": "passed",
                "checkpoint_id": rows[-1]["checkpoint_id"],
            }
            previous_session = rpc(project, {"command": "status"})["session_id"]
            inspect_checkpoint = (
                navigation_script
                + "\nInvoke-Named 'Checkpoints'\nInvoke-Named 'Inspect checkpoint'"
            )
            subprocess.run(
                ["powershell", "-NoProfile", "-Command", inspect_checkpoint],
                check=True,
                capture_output=True,
                timeout=25,
            )
            # Reach the report footer through the owned window's scroll path.
            # UIA can expose a clipped action whose center is not clickable yet.
            body = wintypes.RECT()
            point = wintypes.POINT()
            u.GetClientRect(window, ctypes.byref(body))
            u.ClientToScreen(window, ctypes.byref(point))
            x, y = int(body.right * 0.65), int(body.bottom * 0.65)
            u.SendMessageW(window, 0x0200, 0, (y << 16) | x)
            for _ in range(5):
                u.SendMessageW(window, 0x020A, -720 << 16, ((point.y + y) << 16) | (point.x + x))
                time.sleep(0.1)
            invoke_restore = (
                navigation_helpers
                + "\nInvoke-Named ('Restore checkpoint' + [char]0x2026)\nInvoke-Named 'Allow saved Python code to run'\nInvoke-Named 'Restore checkpoint'"
            )
            restored = subprocess.run(
                ["powershell", "-NoProfile", "-Command", invoke_restore],
                capture_output=True,
                text=True,
                timeout=30,
            )
            if restored.returncode:
                failed_tree = subprocess.run(
                    ["powershell", "-NoProfile", "-Command", ps],
                    capture_output=True,
                    text=True,
                    encoding="utf-8",
                    timeout=20,
                )
                (args.output / "restore-failure-uia.json").write_text(
                    failed_tree.stdout, encoding="utf-8"
                )
                raise RuntimeError(restored.stderr)
            deadline = time.monotonic() + 30
            while time.monotonic() < deadline:
                current_session = rpc(project, {"command": "status"})["session_id"]
                if current_session and current_session != previous_session:
                    break
                time.sleep(0.05)
            else:
                raise RuntimeError("UI restore did not create a replacement session")
            assert (
                rpc(project, {"command": "run", "source": "assert answer == 42"})["status"] == "ok"
            )
            runtime_actions["restore_from_ui"] = {"status": "passed", "session_id": current_session}
            subprocess.run(
                [
                    "powershell",
                    "-NoProfile",
                    "-Command",
                    navigation_helpers + "\nInvoke-Named 'Notebook'",
                ],
                check=True,
                capture_output=True,
                timeout=25,
            )
        interactivity_actions = {}
        if args.exercise_interactivity:
            palette_helpers = (
                navigation_helpers
                + f"""
function Select-Command([string] $label) {{
    Invoke-Named 'Commands'
    $condition = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::NameProperty, 'Type a command')
    $query = $root.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $condition)
    if ($null -eq $query) {{ throw 'Command query is missing' }}
    $query.SetFocus()
    $value = $query.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern)
    $value.SetValue($label)
    Start-Sleep -Milliseconds 200
    [ProbeOwnedKeys]::PostMessageW([IntPtr]{window}, 0x0100, [IntPtr]0x0d, [IntPtr]::Zero) | Out-Null
    [ProbeOwnedKeys]::PostMessageW([IntPtr]{window}, 0x0101, [IntPtr]0x0d, [IntPtr]::Zero) | Out-Null
    Start-Sleep -Milliseconds 200
}}
"""
            )
            interactive_script = (
                palette_helpers
                + """
Select-Command 'Variables'
Invoke-Named 'Refresh live variables'
$found = $false
for ($attempt = 0; $attempt -lt 30; $attempt++) {
    $nodes = $root.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
    $names = @($nodes | ForEach-Object { $_.Current.Name })
    if (@($names | Where-Object { $_.StartsWith('answer') }).Count -gt 0) { $found = $true; break }
    Start-Sleep -Milliseconds 100
}
if (-not $found) { throw 'Live variable answer was not exposed through UIA' }
Select-Command 'Console'
$condition = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::NameProperty, 'Python code (same notebook worker)')
$input = $root.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $condition)
if ($null -eq $input) { throw 'Console input is missing' }
$value = $input.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern)
$value.SetValue("console_probe = answer + 1`nprint('Console output confirmed')")
Invoke-Named 'Run in notebook session'
$found = $false
for ($attempt = 0; $attempt -lt 50; $attempt++) {
    $nodes = $root.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
    if (@($nodes | Where-Object { $_.Current.Name.StartsWith('Recorded output: Console output confirmed') }).Count -gt 0) { $found = $true; break }
    Start-Sleep -Milliseconds 100
}
if (-not $found) { throw 'Console output was not exposed through UIA' }
Invoke-Named 'Notebook'
"""
            )
            interactive = subprocess.run(
                ["powershell", "-NoProfile", "-Command", interactive_script],
                capture_output=True,
                text=True,
                encoding="utf-8",
                timeout=30,
            )
            if interactive.returncode:
                failed_tree = subprocess.run(
                    ["powershell", "-NoProfile", "-Command", ps],
                    capture_output=True,
                    text=True,
                    encoding="utf-8",
                    timeout=20,
                )
                (args.output / "interactivity-failure-uia.json").write_text(
                    failed_tree.stdout, encoding="utf-8"
                )
                raise RuntimeError(interactive.stderr)
            deadline = time.monotonic() + 10
            while time.monotonic() < deadline:
                checked = rpc(project, {"command": "run", "source": "assert console_probe == 43"})
                if checked["status"] == "ok":
                    break
                time.sleep(0.1)
            else:
                raise RuntimeError("Console did not share the notebook namespace")
            interactivity_actions = {
                "variables_refresh_from_ui": "passed",
                "console_execution_from_ui": "passed",
                "shared_namespace": "passed",
                "console_output_from_ui": "passed",
                "completion_debugger_widgets": "not exercised by this OS probe",
            }
        if args.exercise_advanced:
            if args.exercise_runtime:
                # A namespace restore intentionally does not revive widget callbacks.
                assert rpc(project, {"command": "run", "cell_id": "displays"})["status"] == "ok"
                assert rpc(project, {"command": "run", "cell_id": "widgets"})["status"] == "ok"
                assert rpc(project, {"command": "run", "cell_id": "rich"})["status"] == "ok"
            # Use a larger owned window so the source, gutter and output are all
            # visible; later minimum-size/resize checks still run as usual.
            u.MoveWindow(window, 0, 0, int(1440 * scale), int(900 * scale), True)
            time.sleep(0.3)
            advanced_helpers = (
                palette_helpers
                + f"""
function Select-Cell([string] $filter) {{
    Invoke-Named 'Notebook'
    $condition = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::NameProperty, 'Filter cells')
    $input = $root.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $condition)
    if ($null -eq $input) {{ Invoke-Named 'Outline'; $input = $root.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $condition) }}
    if ($null -eq $input) {{ throw 'Outline filter is missing' }}
    $input.SetFocus()
    $input.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).SetValue($filter)
    Start-Sleep -Milliseconds 200
    [ProbeOwnedKeys]::PostMessageW([IntPtr]{window}, 0x0100, [IntPtr]0x0d, [IntPtr]::Zero) | Out-Null
    [ProbeOwnedKeys]::PostMessageW([IntPtr]{window}, 0x0101, [IntPtr]0x0d, [IntPtr]::Zero) | Out-Null
    Start-Sleep -Milliseconds 250
}}
function Find-Named([string] $label) {{
    $condition = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::NameProperty, $label)
    for ($attempt = 0; $attempt -lt 30; $attempt++) {{
        $node = $root.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $condition)
        if ($null -ne $node) {{ return $node }}
        Start-Sleep -Milliseconds 100
    }}
    throw "Missing accessible element: $label"
}}
"""
            )

            def advanced_ui(script):
                result = subprocess.run(
                    ["powershell", "-NoProfile", "-Command", advanced_helpers + script],
                    capture_output=True,
                    text=True,
                    encoding="utf-8",
                    timeout=30,
                )
                if result.returncode:
                    tree = subprocess.run(
                        ["powershell", "-NoProfile", "-Command", ps],
                        capture_output=True,
                        text=True,
                        encoding="utf-8",
                        timeout=15,
                    )
                    (args.output / "advanced-failure-uia.json").write_text(
                        tree.stdout, encoding="utf-8"
                    )
                    dialogs = subprocess.run(
                        [
                            "powershell",
                            "-NoProfile",
                            "-Command",
                            ps.split("$nodes =")[0]
                            + f"""
$condition = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ProcessIdProperty, {app.pid})
$windows = [System.Windows.Automation.AutomationElement]::RootElement.FindAll([System.Windows.Automation.TreeScope]::Children, $condition)
$windows | ForEach-Object {{
    @($_) + @($_.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)) | ForEach-Object {{
        [pscustomobject]@{{ name=$_.Current.Name; role=$_.Current.ControlType.ProgrammaticName; id=$_.Current.AutomationId }}
    }}
}} | ConvertTo-Json -Depth 3
""",
                        ],
                        capture_output=True,
                        text=True,
                        encoding="utf-8",
                        timeout=15,
                    )
                    (args.output / "advanced-dialog-failure-uia.json").write_text(
                        dialogs.stdout, encoding="utf-8"
                    )
                    raise RuntimeError(result.stderr)

            control = RuntimeConnection(project, "3f6fb61e-2ef7-41ba-952f-a1365a8abf7a")

            def await_pause(line):
                deadline = time.monotonic() + 15
                while time.monotonic() < deadline:
                    status = control.call("runtime.status")
                    if status.get("debug") and status["debug"]["stack"][0]["line"] == line:
                        return status
                    time.sleep(0.05)
                raise RuntimeError(f"Debugger did not pause at line {line}")

            try:
                advanced_ui(
                    "Select-Cell 'debug_value = 40'\nInvoke-Named 'Add breakpoint at line 3 in cell debug'\nSelect-Command 'Debug selected cell'"
                )
                await_pause(1)
                advanced_ui("Scroll-Top\nInvoke-Named 'Step over'")
                await_pause(2)
                # The source link must return to the current saved cell/caret.
                advanced_ui("Invoke-Named '<module> · line 2'")
                advanced_ui("""
$editor = Find-Named 'code cell debug'
$editor.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).SetValue("debug_value = 99`n# edited while paused")
Select-Command 'Debugger'
Invoke-Named '<module> · line 2'
if (-not (Find-Named 'Recorded source preview')) { throw 'Changed source did not open recorded preview' }
""")
                advanced_ui("Select-Command 'Debugger'\nScroll-Top\nInvoke-Named 'Continue'")
                await_pause(3)
                advanced_ui("Scroll-Top\nInvoke-Named 'Continue'")
                deadline = time.monotonic() + 15
                while control.call("runtime.status")["worker_state"] != "idle":
                    if time.monotonic() > deadline:
                        raise RuntimeError("Debugger did not resume")
                    time.sleep(0.05)
                assert (
                    rpc(project, {"command": "run", "source": "assert debug_value == 42"})["status"]
                    == "ok"
                )
                assert "debug_value = 99" in notebook.source(notebook.cell("debug"))
                advanced_ui("Select-Cell 'widget_fixture'\nInvoke-Named 'Increment: Click'")
                deadline = time.monotonic() + 10
                while (
                    rpc(project, {"command": "run", "source": "assert clicks == 1"})["status"]
                    != "ok"
                ):
                    if time.monotonic() > deadline:
                        raise RuntimeError("Widget callback was not executed once")
                    time.sleep(0.1)
                advanced_ui(
                    "Select-Cell 'display_fixture'\nFind-Named \"Recorded output: 'Display before'\" | Out-Null\nSelect-Cell 'update_fixture'\nInvoke-Named 'Run update_fixture = True'"
                )
                advanced_ui(
                    "Select-Cell 'display_fixture'\nFind-Named \"Recorded output: 'Display after'\" | Out-Null"
                )
                from kernelledger.desktop.helper import Helper
                from kernelledger.sharing import notebook_outputs

                ledger = Ledger(project / ".kl/ledger.sqlite", readonly=True)
                try:
                    original = ledger.one(
                        "SELECT * FROM executions WHERE cell_id='displays' ORDER BY rowid DESC LIMIT 1"
                    )
                    recorded = Helper(project).dispatch(
                        {"method": "fold.get", "exec_id": original["exec_id"]}
                    )
                    assert (
                        recorded["outputs"][0]["bundle"]["text/plain"]["value"]
                        == "'Display before'"
                    )
                    exported = notebook_outputs(
                        ledger, Helper(project).blobs, original["session_id"]
                    )
                    assert (
                        exported["displays"]["outputs"][0]["data"]["text/plain"]
                        == "'Display after'"
                    )
                    ordinal = exported["displays"]["outputs"][0]["metadata"][
                        "kernelledger_display_update_origin"
                    ]["ordinal"]
                    advanced_ui(f"Find-Named 'Display updated by execution {ordinal}' | Out-Null")
                finally:
                    ledger.close()
                advanced_ui(
                    "Select-Cell 'display_fixture'\nInvoke-Named 'Run display_fixture = True'"
                )
                advanced_ui("Find-Named \"Recorded output: 'Display before'\" | Out-Null")
                interactivity_actions["cross_cell_display_updates"] = (
                    "UI update run changed the earlier display, kept raw History unchanged, exported the updated value with origin, and rerun retired the old target"
                )
                previous_ids = {cell["id"] for cell in notebook.read()["cells"]}
                advanced_ui("Select-Command 'Add code cell'")
                deadline = time.monotonic() + 10
                while True:
                    created = [
                        cell["id"]
                        for cell in notebook.read()["cells"]
                        if cell["id"] not in previous_ids
                    ]
                    if created:
                        break
                    if time.monotonic() > deadline:
                        raise RuntimeError("Add cell did not publish source")
                    time.sleep(0.05)
                advanced_ui(f"""
$editor = Find-Named 'code cell {created[0]}'
$editor.SetFocus()
$editor.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).SetValue("display('New cell output')")
Start-Sleep -Milliseconds 700
Invoke-Named "Run display('New cell output')"
Find-Named "Recorded output: 'New cell output'" | Out-Null
""")
                interactivity_actions["new_cell_output"] = (
                    "UI add/edit/run displayed output after the source membership changed"
                )
                ledger = Ledger(project / ".kl/ledger.sqlite", readonly=True)
                try:
                    executions_before = ledger.one("SELECT COUNT(*) AS n FROM executions")["n"]
                    completion_capture = str(
                        args.output.resolve() / "completion-before-escape.uia.json"
                    ).replace("'", "''")
                    advanced_ui(
                        f"""
$editor = Find-Named 'code cell {created[0]}'
$editor.SetFocus()
$editor.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).SetValue('ans')
[ProbeOwnedKeys]::PostMessageW([IntPtr]{window}, 0x0100, [IntPtr]0x23, [IntPtr]::Zero) | Out-Null
[ProbeOwnedKeys]::PostMessageW([IntPtr]{window}, 0x0101, [IntPtr]0x23, [IntPtr]::Zero) | Out-Null
[ProbeOwnedKeys]::PostMessageW([IntPtr]{window}, 0x0102, [IntPtr][int][char]'w', [IntPtr]::Zero) | Out-Null
Start-Sleep -Milliseconds 1200
[ProbeOwnedKeys]::PostMessageW([IntPtr]{window}, 0x0100, [IntPtr]0x0d, [IntPtr]::Zero) | Out-Null
[ProbeOwnedKeys]::PostMessageW([IntPtr]{window}, 0x0101, [IntPtr]0x0d, [IntPtr]::Zero) | Out-Null
Start-Sleep -Milliseconds 300
$value = $editor.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).Current.Value
if ($value -ne 'answer') {{ throw "Completion Enter did not accept the live binding: $value" }}
$editor.SetFocus()
$editor.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).SetValue('ans')
[ProbeOwnedKeys]::PostMessageW([IntPtr]{window}, 0x0100, [IntPtr]0x23, [IntPtr]::Zero) | Out-Null
[ProbeOwnedKeys]::PostMessageW([IntPtr]{window}, 0x0101, [IntPtr]0x23, [IntPtr]::Zero) | Out-Null
[ProbeOwnedKeys]::PostMessageW([IntPtr]{window}, 0x0102, [IntPtr][int][char]'w', [IntPtr]::Zero) | Out-Null
Start-Sleep -Milliseconds 1200
$value = $editor.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).Current.Value
if ($value -ne 'answ') {{ throw "Completion dismissal prefix was not typed: $value" }}
$root.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition) | ForEach-Object {{
    [pscustomobject]@{{ name=$_.Current.Name; role=$_.Current.ControlType.ProgrammaticName; focused=$_.Current.HasKeyboardFocus; offscreen=$_.Current.IsOffscreen }}
}} | ConvertTo-Json -Depth 3 | Out-File -LiteralPath '{completion_capture}' -Encoding utf8
[ProbeOwnedKeys]::PostMessageW([IntPtr]{window}, 0x0100, [IntPtr]0x1b, [IntPtr]::Zero) | Out-Null
[ProbeOwnedKeys]::PostMessageW([IntPtr]{window}, 0x0101, [IntPtr]0x1b, [IntPtr]::Zero) | Out-Null
Start-Sleep -Milliseconds 150
[ProbeOwnedKeys]::PostMessageW([IntPtr]{window}, 0x0100, [IntPtr]0x0d, [IntPtr]::Zero) | Out-Null
[ProbeOwnedKeys]::PostMessageW([IntPtr]{window}, 0x0101, [IntPtr]0x0d, [IntPtr]::Zero) | Out-Null
Start-Sleep -Milliseconds 300
$value = $editor.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).Current.Value
if ($value.Replace("`r", '') -ne "answ`n") {{ throw "Escape did not dismiss completion and return Enter to editing: $value" }}
"""
                    )
                    assert ledger.one("SELECT COUNT(*) AS n FROM executions")["n"] == executions_before
                finally:
                    ledger.close()
                interactivity_actions["completion_keyboard"] = (
                    "Typed a namespace-only prefix, Enter accepted its live completion, Escape dismissed the next popup and Enter edited without execution; IME/screen reader not qualified"
                )
                advanced_ui("""
Select-Cell 'rich_fixture'
$found = $false
for ($attempt = 0; $attempt -lt 30; $attempt++) {
    $nodes = $root.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
    $names = @($nodes | ForEach-Object { $_.Current.Name })
    if (@($names | Where-Object { $_.StartsWith('Equation: fraction') }).Count -and @($names | Where-Object { $_.StartsWith('Save audio/wav') }).Count -and $names -contains 'Recorded table') { $found = $true; break }
    Start-Sleep -Milliseconds 100
}
if (-not $found) { throw 'Equation, table or media save control was not accessible' }
""")
                media_bytes = b"RIFF" + b"\0" * 40
                media_digest = hashlib.sha256(media_bytes).hexdigest()
                media_path = project / f"output-{media_digest[:12]}.wav"
                advanced_ui(
                    """
$b = $root.Current.BoundingRectangle
$x = [int]($b.Width * .65)
$y = [int]($b.Height * .65)
[ProbeOwnedKeys]::SendMessageW([IntPtr]@WINDOW@, 0x0200, [IntPtr]::Zero, [IntPtr](($y -shl 16) -bor $x)) | Out-Null
$visible = $false
for ($i = 0; $i -lt 40; $i++) {
    $save = Find-Named ('Save audio/wav' + [char]0x2026)
    $r = $save.Current.BoundingRectangle
    if ($r.Height -gt 0 -and $r.Top -ge $b.Top + 160 -and $r.Bottom -le $b.Bottom - 80) { $visible = $true; break }
    $delta = if ($r.Top -lt $b.Top + 160) { 120 } else { -120 }
    [ProbeOwnedKeys]::SendMessageW([IntPtr]@WINDOW@, 0x020A, [IntPtr]($delta -shl 16), [IntPtr]((([int]($b.Y+$y)) -shl 16) -bor [int]($b.X+$x))) | Out-Null
    Start-Sleep -Milliseconds 100
}
if (-not $visible) { throw 'Media save control did not enter the viewport' }
Invoke-Named ('Save audio/wav' + [char]0x2026)
Start-Sleep -Milliseconds 800
""".replace("@WINDOW@", str(window))
                )
                # Some Windows common-dialog controls are absent from UIA's
                # descendant view. Address only native children of our own HWND.
                enum_children = ctypes.WINFUNCTYPE(ctypes.c_bool, wintypes.HWND, wintypes.LPARAM)
                send_message = ctypes.WINFUNCTYPE(
                    ctypes.c_ssize_t, wintypes.HWND, wintypes.UINT, wintypes.WPARAM, wintypes.LPARAM
                )(("SendMessageW", u))
                native_controls = []
                for dialog_window in own_windows():
                    children = []

                    @enum_children
                    def visit_control(hwnd, _):
                        label = ctypes.create_unicode_buffer(256)
                        u.GetClassNameW(hwnd, label, len(label))
                        children.append((hwnd, u.GetDlgCtrlID(hwnd), label.value))
                        return True

                    u.EnumChildWindows(dialog_window, visit_control, 0)
                    native_controls.extend(children)
                    filename_input = next(
                        (
                            hwnd
                            for hwnd, control_id, name in children
                            if control_id == 1001 and name == "Edit"
                        ),
                        None,
                    )
                    save_button = next(
                        (
                            hwnd
                            for hwnd, control_id, name in children
                            if control_id == 1 and name == "Button"
                        ),
                        None,
                    )
                    if filename_input and save_button:
                        u.ShowWindow(dialog_window, 9)
                        u.MoveWindow(
                            dialog_window,
                            20,
                            20,
                            min(int(900 * scale), u.GetSystemMetrics(0) - 40),
                            min(int(700 * scale), u.GetSystemMetrics(1) - 80),
                            True,
                        )
                        u.SetForegroundWindow(dialog_window)
                        time.sleep(0.3)
                        send_message(dialog_window, 0x0028, save_button, 1)
                        u.PostMessageW(save_button, 0x00F5, 0, 0)
                        break
                (args.output / "media-dialog-controls.json").write_text(
                    json.dumps(native_controls), encoding="utf-8"
                )
                deadline = time.monotonic() + 15
                while not media_path.exists():
                    if time.monotonic() > deadline:
                        advanced_ui("Find-Named 'Open file' | Out-Null")
                        raise RuntimeError("Media export did not publish a verified file")
                    time.sleep(0.05)
                assert media_path.read_bytes() == media_bytes
                advanced_ui("Find-Named 'Open file' | Out-Null")
                interactivity_actions["completion_debugger_widgets"] = (
                    "debugger entry/step/breakpoint/current and changed source/resume and one widget callback passed; completion keyboard result recorded separately"
                )
                interactivity_actions["native_renderers"] = (
                    "equation and table exposed through UIA; native dialog media save verified byte-for-byte; explicit Open file offered, system playback not launched"
                )
            finally:
                control.close()
        # Exercise the real compositor and repeat compact/wide transitions.
        for iteration in range(10):
            for w, h in [(800, 600), (1440, 900)]:
                u.MoveWindow(window, 0, 0, int(w * scale), int(h * scale), True)
                time.sleep(0.08)
        captures = []
        alignment = []
        caption_checks = []
        for w, h in [(800, 600), (1024, 768), (1280, 850), (1440, 900)]:
            u.MoveWindow(window, 0, 0, int(w * scale), int(h * scale), True)
            time.sleep(0.4)
            measurement_script = f"""
[Console]::OutputEncoding = New-Object System.Text.UTF8Encoding $false
Add-Type -AssemblyName UIAutomationClient
$root = [System.Windows.Automation.AutomationElement]::FromHandle([IntPtr]{window})
$nodes = $root.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
$nodes | Where-Object {{ $_.Current.ControlType -eq [System.Windows.Automation.ControlType]::Button -and ($_.Current.Name -in @('Notebook','Notebook, current destination','History','Checkpoints','Outline','Commands','Menu','Run all') -or $_.Current.Name.StartsWith('Notebook switcher:') -or $_.Current.Name.StartsWith('Runtime: Python')) }} | ForEach-Object {{ $b=$_.Current.BoundingRectangle; [pscustomobject]@{{ name=$_.Current.Name; x=$b.X; y=$b.Y; width=$b.Width; height=$b.Height; center=$b.Y+$b.Height/2 }} }} | ConvertTo-Json -Depth 3
"""
            measurement = subprocess.run(
                ["powershell", "-NoProfile", "-Command", measurement_script],
                check=True,
                capture_output=True,
                text=True,
                encoding="utf-8",
                timeout=20,
            )
            buttons = json.loads(measurement.stdout)
            centers = [b["center"] for b in buttons]
            spread = (max(centers) - min(centers)) / scale
            assert spread <= 1, (w, h, buttons)
            alignment.append(
                {
                    "logical_size": [w, h],
                    "vertical_center_spread_logical_px": spread,
                    "buttons": buttons,
                }
            )
            client = wintypes.RECT()
            origin = wintypes.POINT()
            u.GetClientRect(window, ctypes.byref(client))
            u.ClientToScreen(window, ctypes.byref(origin))

            # Exercise hit testing only inside the owned window. No global input.
            def hit_test(x, y):
                local_x, local_y = round(x - origin.x), round(y - origin.y)
                u.SendMessageW(window, 0x0200, 0, (local_y << 16) | (local_x & 0xFFFF))
                time.sleep(0.04)
                return u.SendMessageW(window, 0x0084, 0, (round(y) << 16) | (round(x) & 0xFFFF))

            for b in buttons:
                assert (
                    b["x"] >= origin.x
                    and b["x"] + b["width"] <= origin.x + client.right - 102 * scale
                ), (w, h, b)
                result = hit_test(b["x"] + b["width"] / 2, b["center"])
                assert result == 1, (b["name"], "Control must use client hit testing", result)
            caption_y = origin.y + (56 if args.comfortable else 52) * args.ui_scale * scale / 2
            codes = {}
            for label, offset, expected in [
                ("minimize", 85, 8),
                ("maximize", 51, 9),
                ("close", 17, 20),
            ]:
                result = hit_test(origin.x + client.right - offset * scale, caption_y)
                assert result == expected, (label, result, expected)
                codes[label] = result
            last_destination = next(b for b in buttons if b["name"] == "Checkpoints")
            first_action = next(
                b
                for b in buttons
                if b["name"] == "Commands"
                or b["name"].startswith("Runtime: Python")
                or b["name"] == "Run all"
            )
            drag_x = (last_destination["x"] + last_destination["width"] + first_action["x"]) / 2
            assert hit_test(drag_x, caption_y) == 2, "Blank title bar must allow native dragging"
            caption_checks.append(
                {
                    "logical_size": [w, h],
                    "hit_test_codes": codes,
                    "interactive_controls": "client",
                    "blank_chrome": "drag",
                }
            )
            if args.window_capture_exe:
                name = f"notebook-{args.theme}-{w}x{h}.png"
                subprocess.run(
                    [
                        str(args.window_capture_exe.resolve()),
                        str(window),
                        str((args.output / name).resolve()),
                    ],
                    check=True,
                    capture_output=True,
                    timeout=15,
                )
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
            subprocess.run(
                ["powershell", "-NoProfile", "-Command", script],
                check=True,
                capture_output=True,
                timeout=20,
            )
            if (args.output / name).exists():
                captures.append(name)
        # Capture the native Settings composition as well as the notebook.
        # Invoke by accessible name; crop only our own foreground window.
        u.MoveWindow(window, 0, 0, int(1280 * scale), int(850 * scale), True)
        time.sleep(0.2)
        client = wintypes.RECT()
        origin = wintypes.POINT()
        u.GetClientRect(window, ctypes.byref(client))
        u.ClientToScreen(window, ctypes.byref(origin))
        settings_capture = str((args.output / f"settings-{args.theme}.png").resolve()).replace(
            "'", "''"
        )
        settings_tree = str((args.output / f"settings-{args.theme}.uia.json").resolve()).replace(
            "'", "''"
        )
        settings_script = (
            navigation_script
            + f"""
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
"""
        )
        subprocess.run(
            ["powershell", "-NoProfile", "-Command", settings_script],
            check=True,
            capture_output=True,
            timeout=20,
        )
        if args.window_capture_exe:
            # The settings script has already returned to Notebook.
            invoke_settings = navigation_script + "Invoke-Named 'Menu'\nInvoke-Named 'Settings'"
            subprocess.run(
                ["powershell", "-NoProfile", "-Command", invoke_settings],
                check=True,
                capture_output=True,
                timeout=20,
            )
            time.sleep(0.3)
            subprocess.run(
                [str(args.window_capture_exe.resolve()), str(window), settings_capture],
                check=True,
                capture_output=True,
                timeout=15,
            )
        if (args.output / f"settings-{args.theme}.png").exists():
            captures.append(f"settings-{args.theme}.png")
        report = {
            "os": platform.platform(),
            "python": sys.version,
            "executable": str(args.exe.resolve()),
            "theme": args.theme,
            "dpi": dpi,
            "minimum_outer_logical": [width / scale, height / scale],
            "time_to_visible_window_ms": round(startup_ms, 2),
            "window_title": title.value,
            "uia_nodes": len(tree),
            "native_identity": "passed",
            "output_semantics": "passed",
            "screenshot": "Not captured: GPU PrintWindow unavailable; desktop capture can include another window",
            "measurement_limit": "Time to visible window only, not editor-ready latency. No NVDA/IME/mixed-DPI qualification.",
        }
        report["resize_cycles"] = 10
        report["native_menu_and_destinations"] = "passed"
        report["bar_alignment"] = alignment
        report["caption_controls"] = caption_checks
        report["ui_scale"] = args.ui_scale
        report["comfortable"] = args.comfortable
        report["runtime_implementation"] = rpc(project, {"command": "status"}).get(
            "implementation", "python"
        )
        report["runtime_actions"] = runtime_actions
        report["interactivity_actions"] = interactivity_actions
        report["screenshot"] = captures
        report["capture_limit"] = (
            "Desktop capture cropped to owned window; verify occlusion and physical monitor bounds visually. Resize calls alone do not prove control reachability."
        )
        if args.window_capture_exe:
            report["capture_limit"] = (
                "Windows Graphics Capture restricted to the disposable probe HWND; no foreground or desktop capture."
            )
        (args.output / f"probe-{args.theme}.json").write_text(json.dumps(report, indent=2))
        print(json.dumps(report, indent=2))
    finally:
        if window:
            u.PostMessageW(window, 0x0010, 0, 0)
        try:
            app.wait(timeout=10)
        except subprocess.TimeoutExpired:
            app.terminate()
            app.wait(timeout=5)
        if alive(project):
            rpc(project, {"command": "stop"})
        # Clean up only the fixture runtime started by the probe.
        deadline = time.monotonic() + 5
        while (project / ".kl/supervisor.json").exists() and time.monotonic() < deadline:
            time.sleep(0.05)
        # The registry disappears before Windows closes inherited log handles.
        time.sleep(2)
