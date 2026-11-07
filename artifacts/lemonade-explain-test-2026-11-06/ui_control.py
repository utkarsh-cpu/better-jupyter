"""Operate only the user's launched Sol comparison window; never close it."""
import argparse
import ctypes
from ctypes import wintypes
import json
from pathlib import Path
import subprocess
import time

ROOT = Path(__file__).resolve().parents[2]
OUT = Path(__file__).resolve().parent
SESSION = ROOT / 'artifacts/ui-candidates-2026-11-05/candidate-a/interactive-session.json'
state = json.loads(SESSION.read_text())
u=ctypes.windll.user32
u.GetWindowThreadProcessId.argtypes=[wintypes.HWND,ctypes.POINTER(wintypes.DWORD)]
u.PostMessageW.argtypes=[wintypes.HWND,wintypes.UINT,wintypes.WPARAM,wintypes.LPARAM]
pid=wintypes.DWORD();u.GetWindowThreadProcessId(state['hwnd'],ctypes.byref(pid))
assert pid.value==state['pid'], 'Sol window identity changed'

def ps(code):
    pre=f"""$ErrorActionPreference='Stop'
[Console]::OutputEncoding=New-Object System.Text.UTF8Encoding $false
Add-Type -AssemblyName UIAutomationClient
$root=[System.Windows.Automation.AutomationElement]::FromHandle([IntPtr]{state['hwnd']})
"""
    result=subprocess.run(['powershell','-NoProfile','-Command',pre+code],capture_output=True,text=True,encoding='utf-8',timeout=25)
    if result.returncode:
        raise RuntimeError(result.stderr)
    return result.stdout.strip()

def find(name,last=False):
    label=name.replace("'","''")
    return f"$el=@($root.FindAll([System.Windows.Automation.TreeScope]::Descendants,[System.Windows.Automation.Condition]::TrueCondition)|Where-Object {{$_.Current.Name -eq '{label}'}})|Select-Object -{'Last' if last else 'First'} 1; if($null -eq $el){{throw 'Control not found: {label}'}}; "

def click(name,last=False):
    print(ps(find(name,last)+"$p=$null;if($el.TryGetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern,[ref]$p)){$p.Invoke()}elseif($el.TryGetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern,[ref]$p)){$p.Toggle()}else{throw 'No activation pattern'}"))
    time.sleep(.25)

def set_value(name,value):
    val=value.replace("'","''")
    ps(find(name)+f"$el.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).SetValue('{val}')")
    time.sleep(.15)

def dump(name):
    text=ps("""$nodes=$root.FindAll([System.Windows.Automation.TreeScope]::Descendants,[System.Windows.Automation.Condition]::TrueCondition)
@($nodes | ForEach-Object {
$n=$_;$c=$n.Current;$r=$c.BoundingRectangle
$item=[ordered]@{name=$c.Name;role=$c.ControlType.ProgrammaticName;enabled=$c.IsEnabled;focused=$c.HasKeyboardFocus;rect=@($r.X,$r.Y,$r.Width,$r.Height)}
if($c.ControlType -eq [System.Windows.Automation.ControlType]::Edit -and $c.Name -notmatch 'key|credential|password'){
$p=$null;if($n.TryGetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern,[ref]$p)){$item.value=$p.Current.Value}}
[pscustomobject]$item})|ConvertTo-Json -Depth 5""")
    (OUT/(name+'-uia.json')).write_text(text,encoding='utf-8')
    data=json.loads(text)
    print(json.dumps(data))
    return data

def capture(name):
    cap=ROOT/'artifacts/frontend-bars-polish/capture/target/debug/kl-window-capture.exe'
    subprocess.run([str(cap),str(state['hwnd']),str(OUT/(name+'.png'))],capture_output=True,check=True,timeout=20)
    print(str(OUT/(name+'.png')))

if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('action',choices=['click','set','dump','capture','key']);p.add_argument('name');p.add_argument('value',nargs='?');p.add_argument('--last',action='store_true');a=p.parse_args()
    if a.action=='click':click(a.name,a.last)
    elif a.action=='set':set_value(a.name,a.value)
    elif a.action=='dump':dump(a.name)
    elif a.action=='capture':capture(a.name)
    else:
        vk=int(a.name,0);u.PostMessageW(state['hwnd'],0x100,vk,0);u.PostMessageW(state['hwnd'],0x101,vk,0)
