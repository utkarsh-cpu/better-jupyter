import ctypes, json, os, subprocess, sys, tempfile, time
from ctypes import wintypes
from pathlib import Path
sys.path.insert(0, "src")
os.environ["KL_EXPERIMENTAL_INTERACTIVITY"] = "1"
from kernelledger.cli import ensure_supervisor, rpc
from kernelledger.notebook import Notebook
u = ctypes.windll.user32
u.SetProcessDPIAware()
root = Path(tempfile.mkdtemp(prefix="kl-dbg2-"))
project = root / "notebook"
profile = root / "profile" / "KernelLedger" / "Desktop"
profile.mkdir(parents=True)
(profile / "appearance.txt").write_text("light")
(profile / "workbench.json").write_text(json.dumps({"schema": 2, "ui_scale": 1, "comfortable": False}))
n = Notebook(project); n.init()
n.add("markdown", "# Release notebook", "intro")
n.add("code", "print('Hello, KernelLedger')\nanswer = 42", "hello")
n.add("code", "1 / 0", "error")
print("cells:", len(n.read()["cells"]))
ensure_supervisor(project, sys.executable)
for c in ("hello", "error"):
    print(rpc(project, {"command": "run", "cell_id": c}).get("status"))
p = subprocess.Popen([str(Path("desktop/target/debug/kernelledger-desktop.exe").resolve()), str(project)],
    env={**os.environ, "LOCALAPPDATA": str(root / "profile"), "KL_RUNTIME_PYTHON": sys.executable}, creationflags=0x08000000)
time.sleep(5)
found = []
@ctypes.WINFUNCTYPE(ctypes.c_bool, wintypes.HWND, wintypes.LPARAM)
def visit(hwnd, _):
    pid = wintypes.DWORD(); u.GetWindowThreadProcessId(hwnd, ctypes.byref(pid))
    if pid.value == p.pid and u.IsWindowVisible(hwnd): found.append(hwnd)
    return True
u.EnumWindows(visit, 0)
ps = """
Add-Type -AssemblyName UIAutomationClient
$root = [System.Windows.Automation.AutomationElement]::FromHandle([IntPtr]%d)
$nodes = $root.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
$nodes | ForEach-Object { $_.Current.Name }
""" % found[0]
r = subprocess.run(["powershell", "-NoProfile", "-Command", ps], capture_output=True, text=True, encoding="utf-8")
print(r.stdout[:2500])
u.PostMessageW(found[0], 0x0010, 0, 0)
p.wait(timeout=10)
rpc(project, {"command": "stop"})
