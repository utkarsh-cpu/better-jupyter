"""Final captures; see native_review.py for requirements and session setup."""
import json
import os
import signal
import subprocess
import time
from pathlib import Path
from native_review import REPO, OUT, SESSION, apple, keys, code, route, resize, click, capture

state=json.loads(SESSION.read_text())
root=Path(state['root'])

def launch(project, theme='dark', scale=1, python=None):
    state=json.loads(SESSION.read_text())
    os.kill(state['pid'],signal.SIGTERM)
    profile=root/'profile/kernelledger/desktop'
    (profile/'appearance.txt').write_text(theme)
    (profile/'workbench.json').write_text(json.dumps({'schema':2,'ui_scale':scale}))
    p=subprocess.Popen([str(REPO/'desktop/target/debug/kernelledger-desktop'),*([str(project)] if project else [])],cwd=REPO,env={**os.environ,'XDG_STATE_HOME':str(root/'profile'),'KL_RUNTIME_PYTHON':python or str(REPO/'.venv/bin/python')},stdout=(root/'final-app.log').open('a'),stderr=subprocess.STDOUT,start_new_session=True)
    state['pid']=p.pid;SESSION.write_text(json.dumps(state))
    time.sleep(3)
    apple('tell application "System Events" to tell process "kernelledger-desktop" to set frontmost to true')
    resize(1280,800)

launch(root/'Precision notebook')
capture('final-notebook-dark')
keys('3');route('Create checkpoint');time.sleep(.8);capture('final-checkpoint-created')
# Escape does not alter a completed operation; close palette if no command matched.
code(53);keys('1');keys('n','command down, shift down');keys('s');code(53)
code(117,'command down, shift down');capture('final-delete-confirmation');code(53)
route('Debugger');capture('final-debugger-disabled')
keys('1');route('Ask assistant');keys(',');resize(800,600);capture('final-settings-800')
click(350,126);capture('final-provider-settings-800')
keys('1');capture('final-assistant-800')
for theme in ['light','contrast']:
    launch(root/'Precision notebook',theme)
    capture('final-notebook-'+theme)
    keys(',');resize(800,600);capture('final-settings-'+theme)
launch(root/'Precision notebook',scale=2)
keys(',');resize(800,600);capture('final-settings-200-percent')
launch(root/'Precision notebook',python='/usr/bin/false')
capture('final-source-unavailable')
launch(None)
capture('final-welcome')
# A separate genuinely empty notebook tests the distinction from unread source.
from kernelledger.notebook import Notebook
empty=root/'Empty notebook'
Notebook(empty).init()
launch(empty)
capture('final-empty-notebook')
launch(root/'Precision notebook')
keys('o');capture('final-open-dialog');code(53)
keys('p','command down, shift down');capture('final-palette');code(53)
