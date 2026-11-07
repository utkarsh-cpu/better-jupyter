"""Owned UI comparison fixtures and native capture; no user notebooks/profile."""
import ctypes
from ctypes import wintypes
import hashlib
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import time

from kernelledger.cli import alive, ensure_supervisor, rpc
from kernelledger.notebook import Notebook

OUT = Path(__file__).resolve().parent
REPO = OUT.parents[1]
STATE = OUT / 'session.json'
user = ctypes.windll.user32
user.SetProcessDPIAware()
user.GetWindowThreadProcessId.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.DWORD)]
user.GetDpiForWindow.argtypes = [wintypes.HWND]
user.MoveWindow.argtypes = [wintypes.HWND, ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_int, wintypes.BOOL]
user.PostMessageW.argtypes = [wintypes.HWND, wintypes.UINT, wintypes.WPARAM, wintypes.LPARAM]

INTRO = '# Sales analysis\nCompare a small dataset, inspect the result, and diagnose an error.'
SOURCE = 'sales = [120, 180, 150, 210]\ntotal = sum(sales)\nprint(f"Total sales: {total}")\nprint(f"Average: {total / len(sales):.1f}")'

def native(state, code):
    hwnd = state['kl_hwnd']
    actual = wintypes.DWORD()
    user.GetWindowThreadProcessId(hwnd, ctypes.byref(actual))
    assert actual.value == state['kl_ui'], 'Owned window no longer exists'
    prelude = f"""
    $ErrorActionPreference = 'Stop'
    [Console]::OutputEncoding = New-Object System.Text.UTF8Encoding $false
    Add-Type -AssemblyName UIAutomationClient
    $root = [System.Windows.Automation.AutomationElement]::FromHandle([IntPtr]{hwnd})
    """
    result = subprocess.run(['powershell', '-NoProfile', '-Command', prelude + code], capture_output=True, text=True, encoding='utf-8', timeout=25)
    if result.returncode:
        raise RuntimeError(result.stderr)
    return result.stdout

def launch():
    root = OUT / 'kernelledger-notebook'
    nb = Notebook(root)
    nb.init()
    nb.add('markdown', INTRO, 'intro')
    nb.add('code', SOURCE, 'summary')
    nb.add('code', '1 / 0', 'error')
    ensure_supervisor(root, python=sys.executable)
    for cell in ['summary', 'error']:
        print(cell, rpc(root, {'command': 'run', 'cell_id': cell})['status'])
    profile = OUT / 'profile/KernelLedger/Desktop'
    profile.mkdir(parents=True, exist_ok=True)
    (profile / 'appearance.txt').write_text('light')
    (profile / 'workbench.json').write_text(json.dumps({'schema': 2, 'ui_scale': 1, 'comfortable': False}))
    exe = REPO / 'desktop/target/debug/kernelledger-desktop.exe'
    ui = subprocess.Popen([str(exe), str(root)], cwd=REPO, env={**os.environ, 'LOCALAPPDATA': str(OUT / 'profile'), 'KL_RUNTIME_PYTHON': sys.executable}, creationflags=0x08000000, stdout=(OUT / 'kernel.stdout.log').open('wb'), stderr=(OUT / 'kernel.stderr.log').open('wb'))
    hwnd = None
    for _ in range(150):
        windows = []
        @ctypes.WINFUNCTYPE(ctypes.c_bool, wintypes.HWND, wintypes.LPARAM)
        def visit(h, _):
            pid = wintypes.DWORD()
            user.GetWindowThreadProcessId(h, ctypes.byref(pid))
            if pid.value == ui.pid and user.IsWindowVisible(h):
                windows.append(h)
            return True
        user.EnumWindows(visit, 0)
        if windows:
            hwnd = windows[0]
            break
        assert ui.poll() is None, 'Native application exited'
        time.sleep(0.1)
    assert hwnd
    dpi = user.GetDpiForWindow(hwnd)
    user.MoveWindow(hwnd, 0, 0, round(1280 * dpi / 96), round(850 * dpi / 96), True)
    version = subprocess.run([str(REPO / 'artifacts/marimo-memory-2026-10-18/venv/Scripts/python.exe'), '-c', 'import marimo; print(marimo.__version__)'], capture_output=True, text=True, check=True).stdout.strip()
    notebook = OUT / 'sales_analysis.py'
    notebook.write_text('import marimo\n\napp = marimo.App(width="medium")\n\n@app.cell\ndef _():\n    import marimo as mo\n    mo.md(' + repr(INTRO) + ')\n    return\n\n@app.cell\ndef _():\n' + ''.join('    ' + line + '\n' for line in SOURCE.splitlines()) + '    return\n\n@app.cell\ndef _():\n    1 / 0\n    return\n\nif __name__ == "__main__":\n    app.run()\n', encoding='utf-8')
    with socket.socket() as listener:
        listener.bind(('127.0.0.1', 0))
        port = listener.getsockname()[1]
    marimo = subprocess.Popen([str(REPO / 'artifacts/marimo-memory-2026-10-18/venv/Scripts/python.exe'), '-m', 'marimo', 'edit', str(notebook), '--headless', '--no-token', '--host', '127.0.0.1', '--port', str(port), '--skip-update-check', '--no-sandbox'], cwd=OUT, env={**os.environ, 'XDG_CONFIG_HOME': str(OUT / 'marimo-config'), 'MARIMO_SKIP_UPDATE_CHECK': '1'}, creationflags=0x08000000, stdout=(OUT / 'marimo.stdout.log').open('wb'), stderr=(OUT / 'marimo.stderr.log').open('wb'))
    state = {'kl_ui': ui.pid, 'kl_hwnd': hwnd, 'marimo': marimo.pid, 'port': port, 'dpi': dpi, 'native_logical_size': [1280, 850], 'marimo_version': version, 'kernel_exe_sha256': hashlib.sha256(exe.read_bytes()).hexdigest()}
    STATE.write_text(json.dumps(state, indent=2))
    print(json.dumps(state))

def capture(state, name):
    subprocess.run([str(REPO / 'artifacts/frontend-bars-polish/capture/target/debug/kl-window-capture.exe'), str(state['kl_hwnd']), str(OUT / (name + '.png'))], check=True, capture_output=True, timeout=20)
    print(str(OUT / (name + '.png')))

if __name__ == '__main__':
    action = sys.argv[1]
    if action == 'launch':
        launch()
    else:
        s = json.loads(STATE.read_text())
        if action == 'capture':
            capture(s, sys.argv[2])
        elif action == 'uia':
            text = native(s, "$nodes = $root.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition); @($nodes | ForEach-Object { [pscustomobject]@{name=$_.Current.Name; role=$_.Current.ControlType.ProgrammaticName; enabled=$_.Current.IsEnabled; rect=$_.Current.BoundingRectangle.ToString()} }) | ConvertTo-Json -Depth 3")
            (OUT / (sys.argv[2] + '.json')).write_text(text, encoding='utf-8')
            print(text)
        elif action == 'invoke':
            label = sys.argv[2].replace("'", "''")
            print(native(s, f"$condition = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::NameProperty, '{label}'); $el = $root.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $condition); if ($null -eq $el) {{ throw 'Control not found' }}; $pattern = $null; if ($el.TryGetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern, [ref]$pattern)) {{ $pattern.Invoke() }} elseif ($el.TryGetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern, [ref]$pattern)) {{ $pattern.Toggle() }} else {{ throw 'No activation pattern' }}"))
        elif action == 'resize':
            scale = s['dpi'] / 96
            user.MoveWindow(s['kl_hwnd'], 0, 0, round(int(sys.argv[2]) * scale), round(int(sys.argv[3]) * scale), True)
        elif action == 'stop':
            user.PostMessageW(s['kl_hwnd'], 0x10, 0, 0)
            if alive(OUT / 'kernelledger-notebook'):
                rpc(OUT / 'kernelledger-notebook', {'command': 'stop'})
            subprocess.run(['taskkill', '/PID', str(s['marimo']), '/T', '/F'], capture_output=True)
            print('Closed owned comparison instances')
