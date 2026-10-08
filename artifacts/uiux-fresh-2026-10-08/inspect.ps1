param([string]$Shot='initial',[string]$Click='',[string]$Keys='',[int]$Index=-1,[int]$Width=0,[int]$Height=0)
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.Windows.Forms
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class AuditNative {
[DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
[DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
[DllImport("user32.dll")] public static extern bool MoveWindow(IntPtr h,int x,int y,int w,int z,bool repaint);
[DllImport("user32.dll")] public static extern bool SetCursorPos(int x,int y);
[DllImport("user32.dll")] public static extern void mouse_event(uint flags,uint x,uint y,uint d,UIntPtr extra);
}
"@
[AuditNative]::SetProcessDPIAware() | Out-Null
$dir = Join-Path $PSScriptRoot '.'
$info = Get-Content (Join-Path $dir 'process.json') | ConvertFrom-Json
$proc = Get-Process -Id $info.pid
$handle = $proc.MainWindowHandle
[AuditNative]::SetForegroundWindow($handle) | Out-Null
Start-Sleep -Milliseconds 250
if ($Width -gt 0) { [AuditNative]::MoveWindow($handle,30,30,$Width,$Height,$true) | Out-Null; Start-Sleep -Milliseconds 400 }
$root=[System.Windows.Automation.AutomationElement]::FromHandle($handle)
if ($Click) {
 $nodes=$root.FindAll([System.Windows.Automation.TreeScope]::Descendants,[System.Windows.Automation.Condition]::TrueCondition)
 $match=@($nodes | Where-Object {$_.Current.Name -ceq $Click})
 if ($Index -ge 0) { $match=@($match[$Index]) }; if ($match.Count -ne 1) {throw "Expected one match for $Click, found $($match.Count)"}
 $rect=$match[0].Current.BoundingRectangle
 [AuditNative]::SetCursorPos([int]($rect.X+$rect.Width/2),[int]($rect.Y+$rect.Height/2)) | Out-Null
 [AuditNative]::mouse_event(2,0,0,0,[UIntPtr]::Zero); [AuditNative]::mouse_event(4,0,0,0,[UIntPtr]::Zero)
 Start-Sleep -Milliseconds 650
}
if ($Keys) {[System.Windows.Forms.SendKeys]::SendWait($Keys); Start-Sleep -Milliseconds 700}
$nodes=$root.FindAll([System.Windows.Automation.TreeScope]::Descendants,[System.Windows.Automation.Condition]::TrueCondition)
$items=@($root)+@($nodes | ForEach-Object {$_})
$tree=@($items | ForEach-Object {[pscustomobject]@{name=$_.Current.Name;role=$_.Current.ControlType.ProgrammaticName;enabled=$_.Current.IsEnabled;focusable=$_.Current.IsKeyboardFocusable;rect=$_.Current.BoundingRectangle.ToString()}})
$tree | ConvertTo-Json -Depth 3 | Set-Content (Join-Path $dir "$Shot.json")
$r=$root.Current.BoundingRectangle
$bmp=New-Object System.Drawing.Bitmap ([int]$r.Width),([int]$r.Height)
$g=[System.Drawing.Graphics]::FromImage($bmp)
$g.CopyFromScreen([int]$r.X,[int]$r.Y,0,0,$bmp.Size)
$bmp.Save((Join-Path $dir "$Shot.png"))
$g.Dispose();$bmp.Dispose()
$tree | ForEach-Object {"$($_.role) | $($_.name) | $($_.rect)"}



