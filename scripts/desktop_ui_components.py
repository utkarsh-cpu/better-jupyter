"""Capture a native screenshot of every desktop surface, panel and overlay.

Each capture addresses only the disposable application window that this script
launches: navigation uses its own UI Automation tree, and the pixels come from
Windows Graphics Capture (or PrintWindow PW_RENDERFULLCONTENT) restricted to
that HWND. The notebook, profile and runtime live under a temporary directory
and are removed afterwards.
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
from contextlib import contextmanager
from ctypes import wintypes
from pathlib import Path

SRC_DIR = str(Path(__file__).resolve().parents[1] / "src")
if SRC_DIR not in sys.path:
    sys.path.insert(0, SRC_DIR)
os.environ["PYTHONPATH"] = SRC_DIR + (
    os.pathsep + os.environ["PYTHONPATH"] if os.environ.get("PYTHONPATH") else ""
)

from kernelledger.cli import alive, ensure_supervisor, rpc
from kernelledger.notebook import Notebook

p = argparse.ArgumentParser()
p.add_argument("--exe", type=Path, default=Path("desktop/target/debug/kernelledger-desktop.exe"))
p.add_argument(
    "--capture-exe",
    type=Path,
    default=Path("artifacts/frontend-bars-polish/capture/target/debug/kl-window-capture.exe"),
)
p.add_argument("--output", type=Path, default=Path("artifacts/ui-components"))
p.add_argument("--theme", choices=["light", "dark", "contrast", "system"], default="light")
p.add_argument("--width", type=int, default=1280)
p.add_argument("--height", type=int, default=850)
p.add_argument("--only", default="", help="Comma separated capture names; default is all")
p.add_argument("--ui-scale", type=float, default=1.0, choices=[1.0, 1.25, 1.5, 2.0])
p.add_argument("--editor-size", type=float, default=14.0)
p.add_argument("--comfortable", action="store_true", help="Comfortable density")
p.add_argument("--standard-runtime", action="store_true", help="Disable experimental interactivity to capture the standard Compact gutter")
p.add_argument("--show-cell-duration", action="store_true", help="Seed the optional recorded cell duration preference")
args = p.parse_args()
if os.name != "nt":
    raise SystemExit("Windows native capture only")
args.output.mkdir(parents=True, exist_ok=True)
selected = {name.strip() for name in args.only.split(",") if name.strip()}

u = ctypes.windll.user32
gdi = ctypes.windll.gdi32
u.SetProcessDPIAware()


class BITMAPINFOHEADER(ctypes.Structure):
    _fields_ = [
        ("biSize", wintypes.DWORD),
        ("biWidth", wintypes.LONG),
        ("biHeight", wintypes.LONG),
        ("biPlanes", wintypes.WORD),
        ("biBitCount", wintypes.WORD),
        ("biCompression", wintypes.DWORD),
        ("biSizeImage", wintypes.DWORD),
        ("biXPelsPerMeter", wintypes.LONG),
        ("biYPelsPerMeter", wintypes.LONG),
        ("biClrUsed", wintypes.DWORD),
        ("biClrImportant", wintypes.DWORD),
    ]


def wanted(name):
    return not selected or name in selected


PREAMBLE = """
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient
if (-not ('CaptureKeys' -as [type])) {
    Add-Type -TypeDefinition 'using System; using System.Runtime.InteropServices; public static class CaptureKeys { [DllImport("user32.dll")] public static extern bool PostMessageW(IntPtr hwnd, uint msg, IntPtr w, IntPtr l); }'
}
$root = [System.Windows.Automation.AutomationElement]::FromHandle([IntPtr]@WINDOW@)
function Try-Name([string] $label) {
    $condition = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::NameProperty, $label)
    $node = $root.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $condition)
    if ($null -ne $node) { return $node }
    $current = "$label, current destination"
    $condition = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::NameProperty, $current)
    return $root.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $condition)
}
function Find-Named([string] $label, [int] $attempts = 40) {
    for ($attempt = 0; $attempt -lt $attempts; $attempt++) {
        $node = Try-Name $label
        if ($null -ne $node) { return $node }
        Start-Sleep -Milliseconds 100
    }
    throw "Missing accessible element: $label"
}
function Find-Prefix([string] $prefix, [int] $attempts = 40) {
    for ($attempt = 0; $attempt -lt $attempts; $attempt++) {
        $nodes = $root.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
        foreach ($node in $nodes) { if ($node.Current.Name.StartsWith($prefix)) { return $node } }
        Start-Sleep -Milliseconds 100
    }
    throw "Missing accessible element starting with: $prefix"
}
function Activate($element) {
    $pattern = $null
    if ($element.TryGetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern, [ref]$pattern)) { $pattern.Invoke() }
    elseif ($element.TryGetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern, [ref]$pattern)) { $pattern.Toggle() }
    elseif ($element.TryGetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern, [ref]$pattern)) { $pattern.Select() }
    else { throw "Action has no activation pattern: $($element.Current.Name)" }
    Start-Sleep -Milliseconds 250
}
function Invoke-Named([string] $label) { Activate (Find-Named $label) }
function Invoke-Prefix([string] $prefix) { Activate (Find-Prefix $prefix) }
function Press-Key([int] $code) {
    [CaptureKeys]::PostMessageW([IntPtr]@WINDOW@, 0x0100, [IntPtr]$code, [IntPtr]::Zero) | Out-Null
    [CaptureKeys]::PostMessageW([IntPtr]@WINDOW@, 0x0101, [IntPtr]$code, [IntPtr]::Zero) | Out-Null
    Start-Sleep -Milliseconds 300
}
function Close-Menu { Press-Key 0x1b }
# Narrow, Comfortable, and enlarged layouts move title-bar actions into Menu.
function Invoke-Or-Menu([string] $label, [string] $menuItem, [switch] $Prefix) {
    for ($attempt = 0; $attempt -lt 5; $attempt++) {
        if ($Prefix) {
            $nodes = $root.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
            foreach ($node in $nodes) { if ($node.Current.Name.StartsWith($label)) { Activate $node; return } }
        } else {
            $node = Try-Name $label
            if ($null -ne $node) { Activate $node; return }
        }
        Start-Sleep -Milliseconds 100
    }
    Invoke-Named 'Menu'
    Invoke-Named $menuItem
}
function Open-Commands { Invoke-Or-Menu 'Commands' 'Commands' }
function Open-Runtime { Invoke-Or-Menu 'Runtime: Python' 'Runtime' -Prefix }
function Open-Switcher { Invoke-Or-Menu 'Notebook switcher: notebook' 'Switch notebook…' }
function Open-Export { Invoke-Or-Menu 'Export notebook' 'Export notebook…' }
function Open-Settings { Invoke-Or-Menu 'Settings' 'Settings' }
function Select-Command([string] $label) {
    Open-Commands
    $condition = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::NameProperty, 'Type a command')
    $query = $root.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $condition)
    if ($null -eq $query) { throw 'Command query is missing' }
    $query.SetFocus()
    $query.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).SetValue($label)
    Start-Sleep -Milliseconds 300
    Press-Key 0x0d
    Start-Sleep -Milliseconds 450
}
function Set-Text([string] $label, [string] $text) {
    $element = Find-Named $label
    $element.SetFocus()
    $element.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).SetValue($text)
    Start-Sleep -Milliseconds 300
}
function Focus-Cell([string] $text) {
    Invoke-Named 'Notebook'
    $condition = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::NameProperty, 'Filter cells')
    $input = $root.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $condition)
    if ($null -eq $input) { Invoke-Named 'Outline'; $input = $root.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $condition) }
    if ($null -eq $input) { throw 'Outline filter is missing' }
    $input.SetFocus()
    $input.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).SetValue($text)
    Start-Sleep -Milliseconds 400
    Press-Key 0x0d
}
"""


class NativeApp:
    """A disposable KernelLedger window driven only through its own UIA tree."""

    def __init__(self, project, local_app_data, env):
        self.project = project
        self.local_app_data = local_app_data
        self.env = env
        self.process = None
        self.hwnd = None
        self.scale = 1.0

    def __enter__(self):
        self.process = subprocess.Popen(
            [str(args.exe.resolve())] + ([str(self.project)] if self.project else []),
            cwd=Path(__file__).resolve().parents[1],
            # profile_path() reads %LOCALAPPDATA%/KernelLedger/Desktop.
            env={**os.environ, **self.env, "LOCALAPPDATA": str(self.local_app_data)},
            creationflags=0x08000000,
        )
        deadline = time.monotonic() + 25
        while time.monotonic() < deadline:
            for hwnd in self.windows():
                cls = ctypes.create_unicode_buffer(64)
                u.GetClassNameW(hwnd, cls, len(cls))
                if u.IsWindowVisible(hwnd) and cls.value == "Zed::Window":
                    self.hwnd = hwnd
                    break
            if self.hwnd:
                break
            if self.process.poll() is not None:
                raise RuntimeError(f"Application exited: {self.process.returncode}")
            time.sleep(0.05)
        if not self.hwnd:
            raise RuntimeError("No visible application window within 25 seconds")
        self.scale = u.GetDpiForWindow(self.hwnd) / 96
        time.sleep(1.0)
        return self

    def __exit__(self, *_):
        if self.hwnd:
            u.PostMessageW(self.hwnd, 0x0010, 0, 0)
        if self.process:
            try:
                self.process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                self.process.terminate()
                self.process.wait(timeout=5)

    def windows(self):
        found = []

        @ctypes.WINFUNCTYPE(ctypes.c_bool, wintypes.HWND, wintypes.LPARAM)
        def visit(hwnd, _):
            pid = wintypes.DWORD()
            u.GetWindowThreadProcessId(hwnd, ctypes.byref(pid))
            if pid.value == self.process.pid:
                found.append(hwnd)
            return True

        u.EnumWindows(visit, 0)
        return found

    def resize(self, width, height):
        u.MoveWindow(
            self.hwnd,
            0,
            0,
            min(int(width * self.scale), u.GetSystemMetrics(0)),
            min(int(height * self.scale), u.GetSystemMetrics(1)),
            True,
        )
        time.sleep(0.4)

    def uia(self, script):
        result = subprocess.run(
            ["powershell", "-NoProfile", "-Command", PREAMBLE.replace("@WINDOW@", str(self.hwnd)) + script],
            capture_output=True,
            text=True,
            encoding="utf-8",
            timeout=120,
        )
        if result.returncode:
            raise RuntimeError(result.stderr.strip() or result.stdout.strip())
        return result.stdout

    def capture(self, name):
        path = (args.output / f"{name}.png").resolve()
        path.unlink(missing_ok=True)
        try:
            subprocess.run(
                [str(args.capture_exe.resolve()), str(self.hwnd), str(path)],
                check=True,
                capture_output=True,
                timeout=30,
            )
        except Exception:
            from PIL import Image

            rect = self.window_rect()
            w, h = rect.right - rect.left, rect.bottom - rect.top
            hdc_win = u.GetWindowDC(self.hwnd)
            hdc_mem = gdi.CreateCompatibleDC(hdc_win)
            hbmp = gdi.CreateCompatibleBitmap(hdc_win, w, h)
            gdi.SelectObject(hdc_mem, hbmp)
            u.PrintWindow(self.hwnd, hdc_mem, 2)
            bmi = BITMAPINFOHEADER(
                ctypes.sizeof(BITMAPINFOHEADER), w, -h, 1, 32, 0, 0, 0, 0, 0, 0
            )
            buf = ctypes.create_string_buffer(w * h * 4)
            gdi.GetDIBits(hdc_mem, hbmp, 0, h, buf, ctypes.byref(bmi), 0)
            img = Image.frombuffer("RGBA", (w, h), buf, "raw", "BGRA", 0, 1).convert("RGB")
            img.save(path)
            gdi.DeleteObject(hbmp)
            gdi.DeleteDC(hdc_mem)
            u.ReleaseDC(self.hwnd, hdc_win)
        if not path.exists():
            raise RuntimeError(f"Graphics capture produced no file for {name}")
        return path

    def bounds(self, names):
        """Return accessible elements' screen bounds for the named anchors."""
        # Doubling a quote is the only escape inside a PowerShell single-quoted
        # string; accessible names routinely contain apostrophes from source text.
        query = "\n".join(f"$names += '{name.replace(chr(39), chr(39) * 2)}'" for name in names)
        script = f"""
$names = @()
{query}
$result = @()
foreach ($name in $names) {{
    $node = Try-Name $name
    if ($null -ne $node) {{
        $r = $node.Current.BoundingRectangle
        if ($r.Width -gt 0 -and $r.Height -gt 0) {{
            $result += [pscustomobject]@{{ name = $name; x = $r.X; y = $r.Y; width = $r.Width; height = $r.Height }}
        }}
    }}
}}
ConvertTo-Json -InputObject @($result) -Compress
"""
        result = subprocess.run(
            ["powershell", "-NoProfile", "-Command", PREAMBLE.replace("@WINDOW@", str(self.hwnd)) + script],
            capture_output=True,
            text=True,
            encoding="utf-8",
            timeout=60,
        )
        if result.returncode:
            raise RuntimeError(result.stderr.strip() or result.stdout.strip())
        payload = result.stdout.strip()
        return json.loads(payload) if payload and payload != "null" else []

    def window_rect(self):
        rect = wintypes.RECT()
        u.GetWindowRect(self.hwnd, ctypes.byref(rect))
        return rect

    def client_box(self):
        """Client-area origin on screen and its pixel size."""
        client = wintypes.RECT()
        origin = wintypes.POINT()
        u.GetClientRect(self.hwnd, ctypes.byref(client))
        u.ClientToScreen(self.hwnd, ctypes.byref(origin))
        return origin.x, origin.y, client.right, client.bottom


def seed_notebook(project):
    notebook = Notebook(project)
    notebook.init()
    notebook.add(
        "markdown",
        "# Release notebook\nSource stays in ordinary files; execution history is recorded locally.",
        "intro",
    )
    notebook.add("code", "print('Hello, KernelLedger')\nanswer = 42", "hello")
    notebook.add("code", "1 / 0", "error")
    notebook.add(
        "code",
        "display_fixture = True\ndisplay('Display before', display_id='shot-display')",
        "displays",
    )
    notebook.add(
        "code",
        "update_fixture = True\nupdate_display('Display after', 'shot-display')",
        "display_update",
    )
    notebook.add(
        "code", "debug_value = 40\ndebug_value += 2\nprint('Debugger finished')", "debug"
    )
    notebook.add(
        "code",
        "clicks = 0\ndef clicked(value):\n    global clicks\n    clicks += 1\n"
        "control = widget('button', label='Increment', callback=clicked)\ndisplay(control)",
        "widgets",
    )
    notebook.add(
        "code",
        "class Equation:\n    def _repr_latex_(self):\n"
        "        return r'$\\frac{1}{2} + \\sqrt{x_i^2}$'\n"
        "display(Equation())\ndisplay_table([['alpha', 42]], columns=['Name', 'Value'])\n"
        "display_media(b'RIFF' + b'\\0' * 40, 'audio/wav')",
        "rich",
    )
    return notebook


def build_identity():
    """Source revision, dirty-tree state and executable hash for the manifest."""
    root = Path(__file__).resolve().parents[1]

    def git(*command):
        try:
            return subprocess.run(
                ["git", *command], cwd=root, capture_output=True, text=True, timeout=30
            ).stdout.strip()
        except (OSError, subprocess.SubprocessError):
            return ""

    digest = hashlib.sha256()
    with open(args.exe, "rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            digest.update(chunk)
    status = git("status", "--porcelain")
    return {
        "source_revision": git("rev-parse", "HEAD"),
        "dirty_tree": bool(status),
        "dirty_paths": len(status.splitlines()),
        "executable": str(args.exe),
        "executable_sha256": digest.hexdigest(),
    }


def seed_recents(root, profile, project):
    """Recent notebooks: two with the same name, a long name and a missing folder."""
    twin = root / "archive" / "notebook"
    Notebook(twin).init()
    long_name = root / "quarterly-revenue-forecast-with-seasonal-adjustment-and-scenarios"
    Notebook(long_name).init()
    missing = root / "moved-away" / "field-measurements"
    (profile / "recent.json").write_text(
        json.dumps([str(project), str(twin), str(long_name), str(missing)])
    )


class Report:
    def __init__(self):
        self.screens = []
        self.failures = []
        self.measurements = {}

    def record(self, name, description, error=None):
        if error is None:
            self.screens.append(
                {"name": name, "file": f"{name}.png", "description": description}
            )
        else:
            self.failures.append({"name": name, "description": description, "error": str(error)})


@contextmanager
def surface(app, report, name, description):
    """Capture one surface; a failure is recorded without discarding the rest."""
    if not wanted(name):
        yield False
        return
    try:
        time.sleep(0.45)
        app.capture(name)
        report.record(name, description)
        yield True
    except Exception as error:
        report.record(name, description, error)
        yield False


def main():
    report = Report()
    with tempfile.TemporaryDirectory(prefix="kl-shots-", ignore_cleanup_errors=True) as directory:
        root = Path(directory)
        local = root / "profile"
        profile = local / "KernelLedger/Desktop"
        profile.mkdir(parents=True)
        (profile / "appearance.txt").write_text(args.theme)
        (profile / "workbench.json").write_text(
            json.dumps(
                {
                    "schema": 2,
                    "ui_scale": args.ui_scale,
                    "comfortable": args.comfortable,
                    "editor_size": args.editor_size,
                    "show_cell_duration": args.show_cell_duration,
                }
            )
        )
        sample = root / "sample_module.py"
        sample.write_text(
            "def transform(values):\n"
            "    return [value * 2 for value in values]\n\n\n"
            "print(transform([1, 2, 3]))\n"
        )
        project = root / "notebook"
        seed_notebook(project)
        seed_recents(root, profile, project)
        empty = root / "empty"
        Notebook(empty).init()
        env = {
            "KL_EXPERIMENTAL_INTERACTIVITY": "0" if args.standard_runtime else "1",
            "KL_RUNTIME_PYTHON": sys.executable,
        }
        os.environ.update(env)
        ensure_supervisor(project, sys.executable)
        for cell in ("hello", "error", "displays", "widgets", "rich"):
            rpc(project, {"command": "run", "cell_id": cell})
        rpc(project, {"command": "checkpoint"})
        status = rpc(project, {"command": "status"})
        dpi = 192
        try:
            with NativeApp(project, local, env) as app:
                app.resize(args.width, args.height)
                dpi = round(app.scale * 96)
                notebook_journey(app, report, sample, project)
        finally:
            if alive(project):
                rpc(project, {"command": "stop"})
        if wanted("notebook-empty"):
            with NativeApp(empty, local, env) as app:
                app.resize(args.width, args.height)
                with surface(app, report, "notebook-empty", "Notebook with no cells yet"):
                    pass
        if wanted("welcome") or wanted("welcome-help"):
            with NativeApp(None, local, env) as app:
                app.resize(args.width, args.height)
                with surface(app, report, "welcome", "Welcome screen before a notebook is open"):
                    pass
                try:
                    app.uia("Invoke-Named 'Help and shortcuts'")
                    with surface(app, report, "welcome-help", "Help surface from the welcome screen"):
                        pass
                except Exception as error:
                    report.record("welcome-help", "Help from welcome", error)
        manifest = {
            "os": platform.platform(),
            "build": build_identity(),
            "theme": args.theme,
            "logical_size": [args.width, args.height],
            "dpi": dpi,
            "ui_scale": args.ui_scale,
            "editor_size": args.editor_size,
            "density": "comfortable" if args.comfortable else "compact",
            "experimental_interactivity": not args.standard_runtime,
            "show_cell_duration": args.show_cell_duration,
            "runtime_implementation": status.get("implementation", "python"),
            "measurements": report.measurements,
            "screens": report.screens,
            "failures": report.failures,
            "limit": (
                "Windows Graphics Capture restricted to the disposable probe HWND. "
                "Navigation uses only this application's UI Automation tree."
            ),
        }
        (args.output / "manifest.json").write_text(
            json.dumps(manifest, indent=2), encoding="utf-8"
        )
        print(json.dumps(manifest, indent=2))


def settle(app, script="Press-Key 0x1b"):
    """Best-effort navigation between steps; a failure here is not a capture failure."""
    try:
        app.uia(script)
    except Exception:
        pass


def step(app, report, name, description, script, after=None):
    """Drive one journey step, capture it, then run `after`.

    Constrained layouts and older builds can lack a control; the failure is
    recorded and the journey continues from a dismissed state.
    """
    try:
        app.uia(script)
    except Exception as error:
        if wanted(name):
            report.record(name, description, error)
        settle(app)
        return False
    with surface(app, report, name, description):
        pass
    if after:
        settle(app, after)
    return True


CELL_MENU_KEYBOARD = r"""
function Focused-Name {
    $condition = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::HasKeyboardFocusProperty, $true)
    $node = $root.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $condition)
    if ($null -eq $node) { return '' }
    return $node.Current.Name
}
if (-not ('CaptureCursor' -as [type])) {
    Add-Type -TypeDefinition 'using System.Runtime.InteropServices; public static class CaptureCursor { [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y); }'
}
function Hover($element) {
    $r = $element.Current.BoundingRectangle
    [CaptureCursor]::SetCursorPos([int]($r.X + $r.Width / 2), [int]($r.Y + $r.Height / 2)) | Out-Null
}
$result = [ordered]@{}
$opener = Find-Named "More actions for print('Hello, KernelLedger')"
$wanted = $opener.Current.Name
# The control is hidden until hover or focus. Park the pointer on it, then
# wait until keyboard focus has actually moved there before activating.
$focusedBefore = ''
for ($attempt = 0; $attempt -lt 12; $attempt++) {
    Hover $opener
    $opener.SetFocus()
    Start-Sleep -Milliseconds 120
    $focusedBefore = Focused-Name
    if ($focusedBefore -eq $wanted) { break }
}
$result.opener_focused = $focusedBefore
Invoke-Named "More actions for print('Hello, KernelLedger')"
$result.menu_open = $null -ne (Try-Name 'Duplicate cell')
$result.editing_before_assistant = $null -ne (Try-Name 'Move down')
$result.assistant_items_hidden_until_entered = $null -eq (Try-Name 'Suggest edit')
Press-Key 0x27
$result.right_enters_assistant = ($null -ne (Try-Name 'Back to cell actions')) -and ($null -ne (Try-Name 'Suggest edit'))
Press-Key 0x25
$result.left_returns_to_editing = ($null -ne (Try-Name 'Assistant actions')) -and ($null -eq (Try-Name 'Back to cell actions'))
Press-Key 0x27
Press-Key 0x1b
$result.escape_closes_from_assistant = ($null -eq (Try-Name 'Suggest edit')) -and ($null -eq (Try-Name 'Duplicate cell'))
$focusedAfter = ''
for ($attempt = 0; $attempt -lt 10; $attempt++) {
    $focusedAfter = Focused-Name
    if ($focusedAfter -eq $wanted) { break }
    Start-Sleep -Milliseconds 100
}
$result.focused_after_escape = $focusedAfter
[CaptureCursor]::SetCursorPos(0, 0) | Out-Null
ConvertTo-Json -InputObject $result -Compress
"""


def keyboard_checks(app, report):
    """Cell-menu group traversal with keys only, recorded as data rather than a picture."""
    try:
        report.measurements["cell_menu_keyboard"] = json.loads(app.uia(CELL_MENU_KEYBOARD))
    except Exception as error:
        report.measurements["cell_menu_keyboard"] = {"error": str(error)}
        settle(app, "Close-Menu")


CELL_RUN_ANCHORS = [
    "Run print('Hello, KernelLedger')",
    "Run 1 / 0",
    "Run display_fixture = True",
]


def measure_cells(app, report):
    """Vertical pitch between successive cells' Run controls, in logical pixels."""
    try:
        boxes = {b["name"]: b for b in app.bounds(CELL_RUN_ANCHORS + ["code cell hello"])}
        scale = app.scale or 1.0
        tops = [boxes[name]["y"] for name in CELL_RUN_ANCHORS if name in boxes]
        pitches = [round((b - a) / scale, 1) for a, b in zip(tops, tops[1:])]
        result = {"cell_pitch_logical_px": pitches, "anchors": CELL_RUN_ANCHORS}
        run, editor = boxes.get(CELL_RUN_ANCHORS[0]), boxes.get("code cell hello")
        if run and editor:
            result["run_to_source_logical_px"] = round(
                (editor["y"] - (run["y"] + run["height"])) / scale, 1
            )
        report.measurements["notebook"] = result
    except Exception as error:
        report.measurements["notebook"] = {"error": str(error)}


def notebook_journey(app, report, sample, project):
    # Narrow windows collapse the outline, so its cell list is absent there.
    try:
        app.uia(
            "try { Find-Named 'Notebook cells' 20 | Out-Null } "
            "catch { Find-Named 'Run all' | Out-Null }"
        )
    except RuntimeError:
        app.capture("startup-failure")
        names = app.uia(
            "$root.FindAll([System.Windows.Automation.TreeScope]::Descendants, "
            "[System.Windows.Automation.Condition]::TrueCondition) | "
            "ForEach-Object { $_.Current.Name } | Where-Object { $_ } | Select-Object -First 80"
        )
        (args.output / "startup-failure.txt").write_text(names or "", encoding="utf-8")
        raise
    with surface(app, report, "notebook", "Notebook with outline, cells, outputs and status bar"):
        pass
    measure_cells(app, report)
    components(app, report)

    # Header overlays.
    step(
        app, report, "overlay-command-palette", "Command palette over the notebook",
        "Open-Commands", after="Press-Key 0x1b",
    )
    step(
        app, report, "overlay-application-menu", "Workspace menu with files and destinations",
        "Invoke-Named 'Menu'", after="Close-Menu",
    )
    if step(
        app, report, "overlay-runtime-menu", "Runtime menu with worker state and controls",
        "Open-Runtime",
    ):
        step(
            app, report, "overlay-confirmation", "Destructive confirmation dialog",
            "Invoke-Named 'Stop runtime…'", after="Press-Key 0x1b",
        )
    settle(app, "Close-Menu")
    step(
        app, report, "overlay-notebook-switcher", "Notebook switcher menu",
        "Open-Switcher", after="Close-Menu",
    )
    step(
        app, report, "overlay-export-menu", "Export menu: PDF, Word, Jupyter and bundle",
        "Open-Export", after="Close-Menu",
    )
    step(
        app, report, "overlay-insert-menu", "Insert-cell type menu",
        "Invoke-Named 'Insert cell after cell 2'", after="Close-Menu",
    )
    if step(
        app, report, "overlay-cell-actions", "Per-cell actions menu",
        "Invoke-Prefix 'More actions for'",
    ):
        step(
            app, report, "overlay-cell-actions-assistant",
            "Assistant group inside the cell actions menu, with Back",
            "Invoke-Named 'Assistant actions'; Find-Named 'Back to cell actions' | Out-Null",
        )
    settle(app, "Close-Menu")
    keyboard_checks(app, report)

    # Notebook cell details.
    step(
        app, report, "notebook-traceback", "Failed cell with full traceback expanded",
        "Focus-Cell '1 / 0'; Invoke-Prefix 'Show full traceback'",
    )
    step(
        app, report, "notebook-widget", "Native widget output with recorded snapshot",
        "Focus-Cell 'clicks = 0'",
    )
    step(
        app, report, "notebook-rich-output", "Rendered equation, table and media attachment",
        "Focus-Cell 'class Equation'",
    )

    # Destinations.
    if step(
        app, report, "history-list", "History list with filters and recorded executions",
        "Invoke-Named 'History'; Find-Prefix 'View execution' | Out-Null",
    ):
        step(
            app, report, "history-filter-pending", "History search edited but not applied",
            "Set-Text 'Search recorded source' 'no-such-source'",
        )
        step(
            app, report, "history-no-results", "History search applied with no matching records",
            "Set-Text 'Search recorded source' 'no-such-source'; Invoke-Named 'Apply filters'",
        )
        settle(app, "Invoke-Named 'Clear filters'")
        step(
            app, report, "history-detail", "Recorded execution detail with source and outputs",
            "Invoke-Prefix 'View execution'; Find-Named 'Back to list' | Out-Null",
            after="Invoke-Named 'Back to list'",
        )

    if step(
        app, report, "checkpoints", "Checkpoint list with creation controls",
        "Invoke-Named 'Checkpoints'; Find-Named 'Create checkpoint' | Out-Null",
    ):
        step(
            app, report, "checkpoints-inspect", "Checkpoint inspection report",
            "Invoke-Named 'Inspect checkpoint'; Find-Named 'Back to list' | Out-Null",
            after="Invoke-Named 'Back to list'",
        )

    step(
        app, report, "runtimes", "Known workers and interpreters",
        "Invoke-Named 'Menu'; Invoke-Named 'Runtimes'; Start-Sleep -Milliseconds 1000",
    )
    step(
        app, report, "diagnostics", "Health checks, in-flight operations and recovery tools",
        "Invoke-Named 'Menu'; Invoke-Named 'Diagnostics'; Find-Named 'Run health check' | Out-Null",
    )
    if step(
        app, report, "help", "Shortcuts, terminology and build information",
        "Invoke-Named 'Menu'; Invoke-Named 'Help'; Find-Named 'About KernelLedger' | Out-Null",
    ):
        step(
            app, report, "overlay-about", "About dialog with support information",
            "Invoke-Named 'About KernelLedger'", after="Press-Key 0x1b",
        )

    # Sectioned Settings opens on Appearance; single-page builds show every control.
    if step(
        app, report, "settings", "Appearance, type, layout and interpreter settings",
        "Open-Settings; "
        "try { Find-Named 'Appearance settings' 20 | Out-Null } "
        "catch { Find-Named 'Interface size 100 percent' | Out-Null }",
    ):
        for section in ("Editor", "Layout & accessibility", "Runtime", "Assistant"):
            slug = section.lower().replace(" & ", "-")
            step(
                app, report, f"settings-{slug}", f"{section} settings section",
                f"Invoke-Named '{section} settings'",
            )
        settle(app, "Invoke-Named 'Appearance settings'")

    # Live worker surfaces.
    step(
        app, report, "variables", "Passive live namespace metadata",
        "Select-Command 'Variables'; Invoke-Named 'Refresh live variables'; "
        "Find-Prefix 'answer' | Out-Null",
    )
    step(
        app, report, "console", "Console sharing the notebook worker",
        "Select-Command 'Console'; "
        "Set-Text 'Python code (same notebook worker)' 'console_note = answer + 1'",
    )

    if step(
        app, report, "assistant", "Assistant inspector with evidence and composer",
        "Invoke-Named 'Notebook'; Invoke-Named 'Assistant'; Find-Named 'Review context' | Out-Null",
    ):
        step(
            app, report, "assistant-provider-settings",
            "Connect a provider leads to the Assistant settings section",
            "Invoke-Named 'Open provider settings'; Find-Named 'Test connection' | Out-Null",
        )
    settle(app, "Invoke-Named 'Notebook'")
    settle(app, "Invoke-Named 'Close'")

    # Read-only source preview from a local file.
    try:
        app.uia("Select-Command 'Load source'")
        deadline = time.monotonic() + 15
        dialog = None
        while time.monotonic() < deadline and dialog is None:
            dialog = dialog_handle(app)
            time.sleep(0.1)
        if dialog is None:
            raise RuntimeError("file dialog did not appear")
        type_into_dialog(dialog, sample)
        app.uia("Find-Named 'Insert as a new cell' | Out-Null")
        with surface(app, report, "source-preview", "Read-only preview of a local Python file"):
            pass
        app.uia("Invoke-Named 'Cancel'")
    except Exception as error:
        report.record("source-preview", "Read-only source preview", error)
        settle(app)

    try:
        debugger(app, report)
    except Exception as error:
        report.record("debugger", "Debugger journey", error)
        settle(app)


COMPONENT_ANCHORS = {
    "component-title-bar": (
        "Application title bar: outline toggle, notebook switcher, destinations, "
        "commands, runtime, run, export, settings and menu",
        [
            "Outline",
            "Notebook switcher: notebook",
            "Notebook",
            "History",
            "Checkpoints",
            "Commands",
            "Runtime: Python (Ready)",
            "Run all",
            "Export notebook",
            "Settings",
            "Menu",
        ],
    ),
    "component-outline": (
        "Notebook outline with the cell filter and cell list",
        ["Filter cells", "Notebook cells"],
    ),
    "component-cell-header": (
        "Cell header: run control, ordinal, language and outcome, with output tools",
        ["Run print('Hello, KernelLedger')", "Output tools: print('Hello, KernelLedger')"],
    ),
    "component-cell-error": (
        "Failed cell header and its recorded error output",
        ["Run 1 / 0", "Recorded execution error: ZeroDivisionError: division by zero"],
    ),
    "component-status-bar": (
        "Status bar: mode, cell count, runtime state, save state and assistant toggle",
        ["Assistant"],
    ),
}


def components(app, report):
    """Crop the running window to named components using their accessible bounds."""
    try:
        from PIL import Image
    except ImportError:
        report.record("components", "Whole-window component crops", "Pillow is unavailable")
        return
    if not (args.output / "notebook.png").exists():
        report.record("components", "Whole-window component crops", "notebook capture is missing")
        return
    source = Image.open(args.output / "notebook.png")
    origin_x, origin_y, client_width, client_height = app.client_box()
    scale = source.width / client_width
    for name, (description, anchors) in COMPONENT_ANCHORS.items():
        if not wanted(name):
            continue
        try:
            boxes = app.bounds(list(dict.fromkeys(anchors)))
            if not boxes:
                raise RuntimeError(f"no accessible bounds for {anchors}")
            left = min(b["x"] for b in boxes) - origin_x
            top = min(b["y"] for b in boxes) - origin_y
            right = max(b["x"] + b["width"] for b in boxes) - origin_x
            bottom = max(b["y"] + b["height"] for b in boxes) - origin_y
            if name in ("component-title-bar", "component-status-bar") or name.startswith(
                "component-cell-"
            ):
                left, right = 0, client_width
            if name == "component-status-bar":
                bottom = client_height
            box = tuple(
                max(0, int(round(value * scale)))
                for value in (left - 6, top - 6, right + 6, bottom + 6)
            )
            box = (
                box[0],
                box[1],
                min(source.width, box[2]),
                min(source.height, box[3]),
            )
            if box[2] - box[0] < 32 or box[3] - box[1] < 16:
                raise RuntimeError(f"degenerate crop {box}")
            source.crop(box).save(args.output / f"{name}.png")
            report.record(name, f"{description} ({box[2] - box[0]}x{box[3] - box[1]} px crop)")
        except Exception as error:
            report.record(name, description, error)


def cell_bounds(app, anchors, origin):
    """Return the full width of the cells that contain the named controls."""
    boxes = app.bounds(anchors)
    top = min(b["y"] for b in boxes) - origin[1]
    bottom = max(b["y"] + b["height"] for b in boxes) - origin[1]
    return top - 8, bottom + 8, 0, app.client_box()[2]


def debugger(app, report):
    """Pause a real debug session, then open the recorded source it points at."""
    app.uia("Select-Command 'Notebook'")
    app.uia("Focus-Cell 'debug_value = 40'")
    app.uia("Invoke-Named 'Add breakpoint at line 3 in cell debug'")
    app.uia("Select-Command 'Debug selected cell'")
    app.uia("Find-Named 'Step over' | Out-Null")
    with surface(app, report, "debugger", "Debugger paused on a live frame with stepping controls"):
        pass
    app.uia("Invoke-Named 'Step over'")
    with surface(app, report, "debugger-stepped", "Debugger after stepping to the next line"):
        pass
    # Changing the cell source makes the frame link open the immutable revision.
    app.uia("Invoke-Named 'Notebook'")
    app.uia("Set-Text 'code cell debug' 'debug_value = 99# edited while paused'")
    app.uia("Select-Command 'Debugger'")
    app.uia("Invoke-Prefix '<module> · line 2'")
    app.uia("Find-Prefix 'Recorded source' | Out-Null")
    with surface(app, report, "recorded-source", "Immutable recorded source page for a changed cell"):
        pass
    app.uia("Select-Command 'Debugger'")
    app.uia("Find-Named 'Continue' | Out-Null")
    app.uia("Invoke-Named 'Continue'")
    time.sleep(1.0)


def dialog_handle(app):
    """Return an owned file-dialog HWND, ignoring the main window."""
    for hwnd in app.windows():
        if hwnd == app.hwnd or not u.IsWindowVisible(hwnd):
            continue
        name = ctypes.create_unicode_buffer(64)
        u.GetClassNameW(hwnd, name, len(name))
        if name.value == "#32770":
            return hwnd
    return None


def type_into_dialog(hwnd, path):
    """Set the file-name field and accept the owned dialog.

    Windows common file dialogs expose some controls only as native children,
    so this addresses this application's own dialog window directly.
    """
    enum_proc = ctypes.WINFUNCTYPE(ctypes.c_bool, wintypes.HWND, wintypes.LPARAM)
    children = []

    @enum_proc
    def visit(child, _):
        name = ctypes.create_unicode_buffer(256)
        u.GetClassNameW(child, name, len(name))
        children.append((child, u.GetDlgCtrlID(child), name.value))
        return True

    u.EnumChildWindows(hwnd, visit, 0)
    field = next(
        (c for c, cid, kind in children if kind == "Edit" and cid in (1001, 1148)), None
    )
    if field is None:
        raise RuntimeError(f"file dialog has no file-name field: {children}")
    u.SendMessageW(field, 0x000C, 0, ctypes.c_wchar_p(str(path)))  # WM_SETTEXT
    time.sleep(0.4)
    # IDOK carries the entered path; the button is a fallback for shell dialogs.
    send = ctypes.WINFUNCTYPE(
        ctypes.c_ssize_t, wintypes.HWND, wintypes.UINT, wintypes.WPARAM, wintypes.LPARAM
    )(("SendMessageW", u))
    send(hwnd, 0x0111, 1, 0)  # WM_COMMAND / IDOK
    time.sleep(1.8)


if __name__ == "__main__":
    main()
