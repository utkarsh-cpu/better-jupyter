"""Sequential owned native candidate capture and UIA journeys, independent of branches."""
import argparse
import ctypes
from ctypes import wintypes
import hashlib
import json
import os
from pathlib import Path
import sqlite3
import subprocess
import sys
import time

from kernelledger.cli import alive, ensure_supervisor, rpc
from kernelledger.notebook import Notebook

ROOT = Path(__file__).resolve().parents[2]
CAPTURE = ROOT / 'artifacts/frontend-bars-polish/capture/target/debug/kl-window-capture.exe'
u = ctypes.windll.user32
u.SetProcessDPIAware()
u.GetWindowThreadProcessId.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.DWORD)]
u.GetDpiForWindow.argtypes = [wintypes.HWND]
u.MoveWindow.argtypes = [wintypes.HWND, ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_int, wintypes.BOOL]
u.PostMessageW.argtypes = [wintypes.HWND, wintypes.UINT, wintypes.WPARAM, wintypes.LPARAM]
u.GetClientRect.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.RECT)]
u.ClientToScreen.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.POINT)]
u.SendMessageW.argtypes = [wintypes.HWND, wintypes.UINT, wintypes.WPARAM, wintypes.LPARAM]
u.SendMessageW.restype = ctypes.c_ssize_t

INTRO = '# Sales analysis\nCompare a small dataset, inspect the result, and diagnose an error.'
SOURCE = 'sales = [120, 180, 150, 210]\ntotal = sum(sales)\nprint(f"Total sales: {total}")\nprint(f"Average: {total / len(sales):.1f}")'

class Session:
    def __init__(self, exe, out, *, theme='light', scale=1, comfortable=False, font=14, minimal=False, mixed=False):
        self.exe, self.out = exe.resolve(), out.resolve()
        self.out.mkdir(parents=True, exist_ok=False)
        self.project = self.out / 'Sales-analysis-notebook'
        nb = Notebook(self.project)
        nb.init()
        nb.add('markdown', INTRO, 'intro')
        nb.add('code', SOURCE, 'summary')
        if mixed:
            for index in range(28):
                nb.add('code', f"value_{index:02d} = {index} ** 2\nprint('Recorded sample {index:02d}:', value_{index:02d})", f'sample-{index:02d}')
            nb.add('markdown', '## Notes\nA second Markdown cell for navigation and editing checks.', 'notes')
        nb.add('code', '1 / 0', 'error')
        self.preferences = {'schema': 2, 'ui_scale': scale, 'editor_size': font, 'comfortable': comfortable, 'show_cell_duration': False}
        self.theme, self.minimal = theme, minimal
        self.hwnd = None
        self.app = None
        self.manifest = {'executable': str(self.exe), 'sha256': hashlib.sha256(self.exe.read_bytes()).hexdigest(), 'capture': 'Windows Graphics Capture, owned window only', 'fixture': str(self.project), 'screens': [], 'journeys': {}, 'runtime': 'standard; experimental debugger disabled', 'font_px': 14, 'ui_scale': 1, 'density': 'Compact'}
        self.manifest.update(font_px=font, ui_scale=scale, density='Comfortable' if comfortable else 'Compact', theme=theme, fixture_cells=32 if mixed else 3)

    def ps(self, code):
        pre = f"""$ErrorActionPreference='Stop'
[Console]::OutputEncoding = New-Object System.Text.UTF8Encoding $false
Add-Type -AssemblyName UIAutomationClient
$root=[System.Windows.Automation.AutomationElement]::FromHandle([IntPtr]{self.hwnd})
"""
        r = subprocess.run(['powershell', '-NoProfile', '-Command', pre + code], capture_output=True, text=True, encoding='utf-8', timeout=25)
        if r.returncode:
            raise RuntimeError(r.stderr.strip())
        return r.stdout.strip()

    def launch(self):
        ensure_supervisor(self.project, python=sys.executable)
        for cell in ['summary', 'error']:
            result = rpc(self.project, {'command': 'run', 'cell_id': cell})
            assert result['status'] == ('ok' if cell == 'summary' else 'error'), result
        profile = self.out / 'profile/KernelLedger/Desktop'
        profile.mkdir(parents=True)
        (profile / 'appearance.txt').write_text(self.theme)
        (profile / 'workbench.json').write_text(json.dumps(self.preferences))
        env = {k: v for k, v in os.environ.items() if k not in ['KL_EXPERIMENTAL_INTERACTIVITY', 'KL_DESKTOP_HEADLESS']}
        env.update(LOCALAPPDATA=str(self.out / 'profile'), KL_RUNTIME_PYTHON=sys.executable)
        self.app = subprocess.Popen([str(self.exe), str(self.project)], cwd=ROOT, env=env, creationflags=0x08000000, stdout=(self.out / 'stdout.log').open('wb'), stderr=(self.out / 'stderr.log').open('wb'))
        for _ in range(200):
            found = []
            @ctypes.WINFUNCTYPE(ctypes.c_bool, wintypes.HWND, wintypes.LPARAM)
            def visit(hwnd, _):
                pid = wintypes.DWORD()
                u.GetWindowThreadProcessId(hwnd, ctypes.byref(pid))
                if pid.value == self.app.pid and u.IsWindowVisible(hwnd):
                    found.append(hwnd)
                return True
            u.EnumWindows(visit, 0)
            if found:
                self.hwnd = found[0]
                break
            assert self.app.poll() is None, 'Candidate exited before opening window'
            time.sleep(.1)
        assert self.hwnd, 'No owned window'
        self.dpi = u.GetDpiForWindow(self.hwnd)
        self.manifest.update(pid=self.app.pid, hwnd=self.hwnd, dpi=self.dpi)
        self.resize(1280, 850)
        deadline = time.monotonic() + 25
        while time.monotonic() < deadline:
            text = self.ps("$root.FindAll([System.Windows.Automation.TreeScope]::Descendants,[System.Windows.Automation.Condition]::TrueCondition) | ForEach-Object {$_.Current.Name}")
            if 'Total sales: 660' in text:
                break
            time.sleep(.2)
        else:
            raise RuntimeError('Recorded output did not become accessible')

    def resize(self, w, h):
        scale = self.dpi / 96
        u.MoveWindow(self.hwnd, 0, 0, round(w*scale), round(h*scale), True)
        self.size = [w, h]
        time.sleep(.35)

    def dump(self, name):
        data = self.ps("""$nodes=$root.FindAll([System.Windows.Automation.TreeScope]::Descendants,[System.Windows.Automation.Condition]::TrueCondition)
@($nodes | ForEach-Object {
$n=$_; $v=$n.Current; $r=$v.BoundingRectangle
$item=[ordered]@{name=$v.Name; role=$v.ControlType.ProgrammaticName; enabled=$v.IsEnabled; offscreen=$v.IsOffscreen; focused=$v.HasKeyboardFocus; focusable=$v.IsKeyboardFocusable; rect=@($r.X,$r.Y,$r.Width,$r.Height); patterns=@($n.GetSupportedPatterns() | ForEach-Object {$_.ProgrammaticName})}
$p=$null
if($n.TryGetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern,[ref]$p)){$item.toggle=$p.Current.ToggleState.ToString()}
$p=$null
if($n.TryGetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern,[ref]$p)){$item.selected=$p.Current.IsSelected}
$p=$null
if($n.TryGetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern,[ref]$p)){$item.value=$p.Current.Value}
$p=$null
if($n.TryGetCurrentPattern([System.Windows.Automation.TextPattern]::Pattern,[ref]$p)){$item.text=$p.DocumentRange.GetText(4000)}
[pscustomobject]$item
}) | ConvertTo-Json -Depth 6""")
        (self.out / (name + '-uia.json')).write_text(data, encoding='utf-8')
        return json.loads(data)

    def invoke(self, name, prefix=False, focus=False):
        label = name.replace("'", "''")
        match = f"$_.Current.Name.StartsWith('{label}')" if prefix else f"$_.Current.Name -eq '{label}'"
        code = f"""$el=@($root.FindAll([System.Windows.Automation.TreeScope]::Descendants,[System.Windows.Automation.Condition]::TrueCondition) | Where-Object {{{match}}}) | Select-Object -First 1
if($null -eq $el){{throw 'Control not found: {label}'}}
"""
        if focus:
            code += '$el.SetFocus()'
        else:
            code += "$p=$null; if($el.TryGetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern,[ref]$p)){$p.Invoke()}elseif($el.TryGetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern,[ref]$p)){$p.Toggle()}elseif($el.TryGetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern,[ref]$p)){$p.Select()}else{throw 'No activation pattern'}"
        self.ps(code)
        time.sleep(.3)

    def key(self, vk):
        u.PostMessageW(self.hwnd, 0x100, vk, 0)
        u.PostMessageW(self.hwnd, 0x101, vk, 0)
        time.sleep(.25)

    def capture(self, name):
        # Park only this window's hover state on neutral caption space.
        u.PostMessageW(self.hwnd, 0x200, 0, (4 << 16) | 4)
        time.sleep(.2)
        subprocess.run([str(CAPTURE), str(self.hwnd), str(self.out / (name + '.png'))], check=True, capture_output=True, timeout=20)
        rect = wintypes.RECT()
        u.GetClientRect(self.hwnd, ctypes.byref(rect))
        self.manifest['screens'].append({'name': name, 'logical_outer_size': self.size, 'client_pixels': [rect.right, rect.bottom]})
        print('CAPTURE ' + str(self.out / (name + '.png')), flush=True)
        return self.dump(name)

    def journey(self, name, action):
        try:
            result = action()
            self.manifest['journeys'][name] = {'status': 'passed', 'result': result}
            print('PASS ' + name, flush=True)
        except Exception as e:
            self.manifest['journeys'][name] = {'status': 'failed', 'error': str(e)}
            print('FAIL ' + name + ': ' + str(e)[:250], flush=True)

    def run_summary(self):
        db = self.project / '.kl/ledger.sqlite'
        def count():
            with sqlite3.connect(db) as connection:
                return connection.execute("SELECT count(*) FROM executions WHERE cell_id='summary' AND status='ok'").fetchone()[0]
        before = count()
        self.invoke('Run sales =', prefix=True)
        for _ in range(80):
            after = count()
            if after > before:
                return {'before': before, 'after': after}
            time.sleep(.1)
        raise RuntimeError('UI Run did not record a new successful summary execution')

    def run_all(self):
        with sqlite3.connect(self.project / '.kl/ledger.sqlite') as db:
            before = db.execute('SELECT count(*) FROM executions').fetchone()[0]
        self.invoke('Run all')
        for _ in range(80):
            with sqlite3.connect(self.project / '.kl/ledger.sqlite') as db:
                after = db.execute('SELECT count(*) FROM executions').fetchone()[0]
            if after >= before + 2:
                return {'before': before, 'after': after}
            time.sleep(.1)
        raise RuntimeError('Run all did not record both code-cell executions')

    def suite(self):
        if self.minimal:
            self.capture('notebook-1280-' + self.theme)
            self.resize(800, 600)
            self.capture('notebook-800-' + self.theme)
            return
        self.capture('notebook-1280-light')
        self.journey('run-one-from-ui', self.run_summary)
        self.journey('run-all-from-ui', self.run_all)
        self.resize(800, 600)
        self.capture('notebook-800-light')
        self.resize(1280, 850)
        self.journey('expanded-traceback', lambda: (self.invoke('Show full traceback', prefix=True), self.capture('traceback-1280-light'), self.invoke('Hide traceback', prefix=True)) and None)
        self.invoke('Settings')
        self.capture('settings-appearance')
        self.invoke('Dark')
        self.invoke('Notebook')
        self.capture('notebook-1280-dark')
        self.invoke('Settings')
        self.invoke('Light')
        self.invoke('Layout & accessibility settings')
        tree = self.capture('settings-layout')
        names = [x.get('name','') for x in tree]
        timing = next((n for n in names if 'Show last recorded cell duration' in n or 'Show cell duration' in n or 'Show execution time' in n or 'Show last run time' in n), None)
        if timing:
            self.journey('enable-recorded-duration', lambda: self.invoke(timing))
        self.invoke('Notebook')
        self.capture('notebook-duration')
        self.journey('focus-contextual-actions', lambda: (self.invoke('More actions for sales =', prefix=True, focus=True), self.capture('focused-actions'), self.key(0x20), self.capture('cell-actions'), self.key(0x1b), self.dump('focus-return')) and None)
        self.invoke('Commands')
        self.capture('commands')
        self.key(0x1b)

    def timing_suite(self):
        self.invoke('Settings')
        self.invoke('Layout & accessibility settings')
        tree = self.dump('timing-setting')
        name = next(n['name'] for n in tree if 'Show last recorded cell duration' in n['name'] or 'Show last run time' in n['name'])
        self.invoke(name)
        self.invoke('Notebook')
        self.capture('timing-before-rerun')
        self.journey('timing-run-one', self.run_summary)
        self.capture('timing-after-rerun')
        self.invoke('History')
        self.capture('timing-history')
        self.invoke('Notebook')
        self.capture('timing-after-history')

    def narrow_suite(self):
        self.resize(800, 600)
        tree = self.dump('narrow-controls')
        views = next((n['name'] for n in tree if n['name'].startswith('Views')), None)
        if views:
            self.invoke(views)
        else:
            current = next(n['name'] for n in tree if n['name'].startswith('Notebook') and 'current destination' in n['name'])
            self.invoke(current)
        menu = self.capture('navigation-800')
        names = [n['name'] for n in menu]
        assert any(n.startswith('History') for n in names), names
        assert any(n.startswith('Checkpoints') for n in names), names
        self.manifest['journeys']['narrow-navigation-reachability'] = {'status':'passed'}
        self.key(0x1b)
        self.invoke('Show full traceback', prefix=True)
        self.capture('traceback-800-light')
        self.invoke('Hide traceback', prefix=True)

    def caption_suite(self):
        results = []
        for width,height in [(1280,850),(800,600)]:
            self.resize(width,height)
            tree = self.dump(f'caption-{width}')
            buttons = [n for n in tree if n['role']=='ControlType.Button' and n['rect'][1]<80 and n['rect'][2]>0 and not n['offscreen']]
            client=wintypes.RECT(); origin=wintypes.POINT()
            u.GetClientRect(self.hwnd,ctypes.byref(client));u.ClientToScreen(self.hwnd,ctypes.byref(origin))
            def hit(x,y):
                # GPUI caches the window-local hover hit test. Match its existing native probe.
                local_x,local_y=round(x-origin.x),round(y-origin.y)
                u.SendMessageW(self.hwnd,0x200,0,(local_y<<16)|(local_x&0xffff))
                time.sleep(.05)
                return u.SendMessageW(self.hwnd,0x84,0,(int(y)<<16)|(int(x)&0xffff))
            checks=[]
            for n in buttons:
                x,y,w,h=n['rect']
                checks.append({'name':n['name'],'hit':hit(x+w/2,y+h/2),'expected':1})
            command = next(n for n in buttons if n['name']=='Commands')
            left = [n for n in buttons if n['name'].startswith(('Outline','Notebook','History','Checkpoints','Views'))]
            end = max(n['rect'][0]+n['rect'][2] for n in left)
            checks.append({'name':'blank drag region','hit':hit((end+command['rect'][0])/2,command['rect'][1]+command['rect'][3]/2),'expected':2})
            cy=origin.y+26*self.dpi/96
            for name,offset,expected in [('minimize',85,8),('maximize',51,9),('close',17,20)]:
                checks.append({'name':name,'hit':hit(origin.x+client.right-offset*self.dpi/96,cy),'expected':expected})
            results.append({'logical_size':[width,height],'checks':checks})
        self.manifest['caption_hit_tests']=results
        failures=[c for r in results for c in r['checks'] if c['hit']!=c['expected']]
        self.manifest['journeys']['native-caption-hit-tests']={'status':'failed' if failures else 'passed','failures':failures}
        print(json.dumps(results),flush=True)

    def close(self):
        (self.out / 'manifest.json').write_text(json.dumps(self.manifest, indent=2), encoding='utf-8')
        if self.hwnd:
            u.PostMessageW(self.hwnd, 0x10, 0, 0)
        if alive(self.project):
            rpc(self.project, {'command': 'stop'})
        if self.app:
            try:
                self.app.wait(timeout=8)
            except subprocess.TimeoutExpired:
                self.app.terminate()

if __name__ == '__main__':
    p = argparse.ArgumentParser()
    p.add_argument('--exe', type=Path, required=True)
    p.add_argument('--output', type=Path, required=True)
    p.add_argument('--theme', choices=['light', 'dark', 'contrast'], default='light')
    p.add_argument('--scale', type=float, default=1)
    p.add_argument('--font', type=float, default=14)
    p.add_argument('--comfortable', action='store_true')
    p.add_argument('--minimal', action='store_true')
    p.add_argument('--mixed', action='store_true')
    p.add_argument('--timing-test', action='store_true')
    p.add_argument('--narrow-test', action='store_true')
    p.add_argument('--caption-test', action='store_true')
    args = p.parse_args()
    session = Session(args.exe, args.output, theme=args.theme, scale=args.scale, comfortable=args.comfortable, font=args.font, minimal=args.minimal, mixed=args.mixed)
    try:
        session.launch()
        if args.caption_test:
            session.caption_suite()
        elif args.narrow_test:
            session.narrow_suite()
        elif args.timing_test:
            session.timing_suite()
        else:
            session.suite()
    except Exception as error:
        session.manifest['suite_error'] = repr(error)
        raise
    finally:
        session.close()
