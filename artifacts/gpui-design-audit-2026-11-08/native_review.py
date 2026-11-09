"""Disposable macOS native review driver. Requires Accessibility/Screen Recording.
Captures need human review; successful commands are not usability assertions.
The Swift helper used for capture is saved alongside these artifacts.
"""
import json
import os
import signal
import sys
import subprocess
import time
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
OUT = Path(__file__).resolve().parent
SESSION = Path('/tmp/kl-design-audit-session.json')

def apple(*lines):
    return subprocess.check_output(['osascript', *sum((['-e', line] for line in lines), [])], text=True).strip()

def keys(text, modifiers='command down'):
    apple(f'tell application "System Events" to keystroke {json.dumps(text)} using {{{modifiers}}}')
    time.sleep(.15)

def code(number, modifiers=''):
    apple(f'tell application "System Events" to key code {number}' + (f' using {{{modifiers}}}' if modifiers else ''))
    time.sleep(.15)

def route(name):
    code(53)
    keys('p', 'command down, shift down')
    apple(f'tell application "System Events" to keystroke {json.dumps(name)}')
    time.sleep(.15)
    code(36)
    time.sleep(.5)

def resize(width, height):
    apple('tell application "System Events" to tell process "kernelledger-desktop" to set position of window 1 to {0, 30}', f'tell application "System Events" to tell process "kernelledger-desktop" to set size of window 1 to {{{width}, {height}}}')
    time.sleep(.4)
    actual=apple('tell application "System Events" to tell process "kernelledger-desktop" to get {position, size} of window 1')
    return {'requested_logical':[width,height], 'actual_position_size':actual}

def click(x, y):
    # Coordinates are logical window offsets; native window is at (0,30).
    subprocess.run(['/tmp/kl-click', str(x), str(y+30)],check=True)
    time.sleep(.4)

def capture(name):
    time.sleep(.6)
    state=json.loads(SESSION.read_text())
    subprocess.run(['/tmp/kl-design-capture', str(state['pid']), str(OUT/(name+'.png'))],check=True)

def relaunch():
    state=json.loads(SESSION.read_text())
    os.kill(state['pid'], signal.SIGTERM)
    root=Path(state['root'])
    p=subprocess.Popen([str(REPO/'desktop/target/debug/kernelledger-desktop'),str(root/'Precision notebook')],cwd=REPO,env={**os.environ,'XDG_STATE_HOME':str(root/'profile'),'KL_RUNTIME_PYTHON':str(REPO/'.venv/bin/python')},stdout=(root/'app-after.log').open('w'),stderr=subprocess.STDOUT,start_new_session=True)
    state['pid']=p.pid;SESSION.write_text(json.dumps(state))
    time.sleep(3)
    apple('tell application "System Events" to tell process "kernelledger-desktop" to set frontmost to true')

if __name__ == '__main__':
    if '--reuse' not in sys.argv:
        relaunch()
    apple('tell application "System Events" to tell process "kernelledger-desktop" to set frontmost to true')
    metadata=[resize(1280,800)]
    capture('after-notebook-dark')
    keys('2');capture('after-history-cached')
    keys('1');code(36,'control down, shift down');time.sleep(1)
    keys('2');capture('after-history-refreshed')
    keys('3');capture('after-checkpoints-empty')
    route('Create checkpoint');time.sleep(1);capture('after-checkpoint-saved')
    route('Ask assistant');capture('after-assistant')
    keys(',');metadata.append(resize(800,600));capture('after-settings-800')
    click(360,126);capture('after-provider-settings-800')
    keys('1');capture('after-assistant-restored-800')
    keys(',');metadata.append(resize(1280,800))
    # Full-width Settings uses its left navigation at this size.
    capture('after-settings-wide')
    for name in ['Diagnostics','All runtimes','Help','Variables','Console']:
        route(name);capture('after-'+name.lower().replace(' ','-'))
    keys('1');keys('e','command down, shift down');capture('after-export-menu');code(53)
    keys('p','command down, shift down');capture('after-palette');code(53)
    keys('1');keys('n','command down, shift down');time.sleep(.4)
    keys('s');code(53);code(117,'command down, shift down');capture('after-delete-confirmation');code(53)
    for width,height in [(800,600),(1024,768),(1280,800),(1440,900),(1920,1080)]:
        metadata.append(resize(width,height));capture(f'after-notebook-{width}x{height}')
    resize(1280,800)
    (OUT/'native-window-metadata.json').write_text(json.dumps(metadata,indent=2))
