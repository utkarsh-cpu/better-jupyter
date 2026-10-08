import json, os, subprocess, sys, time
from pathlib import Path
from kernelledger.notebook import Notebook
root=Path('artifacts/uiux-fresh-2026-10-08').resolve()
nb=Notebook(root/'notebook'); nb.init()
nb.add('markdown','# Sales exploration\nA fresh notebook for reviewing the editing and execution experience.','intro')
nb.add('code','sales = [120, 180, 95, 240]\ntotal = sum(sales)\nprint(f"Total sales: {total}")\ntotal','sales')
nb.add('code','1 / 0','error')
profile=root/'profile'; (profile/'KernelLedger/Desktop').mkdir(parents=True,exist_ok=True)
(profile/'KernelLedger/Desktop/appearance.txt').write_text('light')
p=subprocess.Popen([str(Path('desktop/target/debug/kernelledger-desktop.exe').resolve()),str(root/'notebook')],env={**os.environ,'LOCALAPPDATA':str(profile),'KL_RUNTIME_PYTHON':sys.executable})
(root/'process.json').write_text(json.dumps({'pid':p.pid,'notebook':str(root/'notebook')}))
print(p.pid)
