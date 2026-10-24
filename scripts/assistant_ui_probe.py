"""Windows UI Automation check of an isolated assistant flow, with a fake provider."""

import argparse
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

from kernelledger.notebook import Notebook


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--exe", type=Path, default=Path("desktop/target/debug/kernelledger-desktop.exe")
    )
    args = parser.parse_args()
    if os.name != "nt":
        raise SystemExit("Windows UI Automation only")
    with tempfile.TemporaryDirectory(prefix="kl-assistant-ui-") as directory:
        root = Path(directory)
        notebook = Notebook(root / "notebook")
        notebook.init()
        notebook.add("code", "answer = 42", "first")
        profile = root / "profile/KernelLedger"
        profile.mkdir(parents=True)
        (profile / "assistant.json").write_text(
            json.dumps({"version": 1, "provider": "fake", "model": "fake"})
        )
        process = subprocess.Popen(
            [str(args.exe.resolve()), str(notebook.root)],
            env={
                **os.environ,
                "LOCALAPPDATA": str(root / "profile"),
                "KL_RUNTIME_PYTHON": sys.executable,
            },
            creationflags=0x08000000,
        )
        script = r"""
$ErrorActionPreference='Stop'
Add-Type -AssemblyName UIAutomationClient
$deadline=[DateTime]::UtcNow.AddSeconds(20)
$root=$null
do {
    $p=Get-Process -Id TASK_PROCESS_ID -ErrorAction Stop
    if ($p.MainWindowHandle -ne 0) {$root=[System.Windows.Automation.AutomationElement]::FromHandle($p.MainWindowHandle)}
    if ($null -eq $root) {Start-Sleep -Milliseconds 100}
} while ($null -eq $root -and [DateTime]::UtcNow -lt $deadline)
if ($null -eq $root) {throw 'No owned application window'}
function Find-Named([string] $name) {
    $condition=New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::NameProperty,$name)
    return $root.FindFirst([System.Windows.Automation.TreeScope]::Descendants,$condition)
}
function Invoke-Named([string] $name) {
    $deadline=[DateTime]::UtcNow.AddSeconds(15)
    do {$node=Find-Named $name; if ($null -eq $node) {Start-Sleep -Milliseconds 100}} while ($null -eq $node -and [DateTime]::UtcNow -lt $deadline)
    if ($null -eq $node) {throw "Missing action: $name"}
    $node.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
    Start-Sleep -Milliseconds 200
}
Invoke-Named 'Assistant'
Invoke-Named 'Resume summary'
Invoke-Named 'Approve context and send'
$deadline=[DateTime]::UtcNow.AddSeconds(15)
do {
    $nodes=$root.FindAll([System.Windows.Automation.TreeScope]::Descendants,[System.Windows.Automation.Condition]::TrueCondition)
    $names=@($nodes | ForEach-Object {$_.Current.Name})
    $answer=@($names | Where-Object {$_ -like '*Test provider connected*'})
    if ($answer.Count -eq 0) {Start-Sleep -Milliseconds 100}
} while ($answer.Count -eq 0 -and [DateTime]::UtcNow -lt $deadline)
if ($answer.Count -eq 0) {throw "Fake provider answer missing: $($names -join ', ')"}
[pscustomobject]@{flow='preview -> explicit approval -> fake answer';network='none';result='passed'} | ConvertTo-Json
""".replace("TASK_PROCESS_ID", str(process.pid))
        try:
            result = subprocess.run(
                ["powershell", "-NoProfile", "-Command", script],
                capture_output=True,
                text=True,
                timeout=60,
            )
            if result.returncode:
                raise RuntimeError(result.stderr)
            print(result.stdout.strip())
            assert notebook.source(notebook.cell("first")) == "answer = 42"
            assert not (notebook.root / ".kl/ledger.sqlite").exists()
        finally:
            process.terminate()
            process.wait(timeout=10)


if __name__ == "__main__":
    main()
