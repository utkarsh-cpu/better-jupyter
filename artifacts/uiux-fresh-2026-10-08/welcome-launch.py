import json,os,subprocess,sys
from pathlib import Path
root=Path('artifacts/uiux-fresh-2026-10-08/welcome').resolve(); root.mkdir(exist_ok=True)
profile=root/'profile/KernelLedger/Desktop'; profile.mkdir(parents=True,exist_ok=True); (profile/'appearance.txt').write_text('light')
p=subprocess.Popen([str(Path('desktop/target/debug/kernelledger-desktop.exe').resolve())],env={**os.environ,'LOCALAPPDATA':str(root/'profile'),'KL_RUNTIME_PYTHON':sys.executable})
(root/'process.json').write_text(json.dumps({'pid':p.pid}))
