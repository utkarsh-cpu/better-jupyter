param([string]$Action = 'inspect', [string]$Name, [string]$Keys, [string]$Shot = 'notebook', [int]$Width = 800, [int]$Height = 600)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.Windows.Forms
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class ReviewWindow {
 [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
 [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint x, uint y, uint data, UIntPtr extra);
 [DllImport("user32.dll")] public static extern bool MoveWindow(IntPtr hwnd, int x, int y, int width, int height, bool repaint);
 [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
 [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hwnd);
 [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hwnd, out Rect r);
 [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr hwnd, out Rect r);
 [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
 [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr hwnd, uint message, IntPtr w, IntPtr l);
 public struct Rect { public int Left, Top, Right, Bottom; }
}
'@
[ReviewWindow]::SetProcessDPIAware() | Out-Null
$session = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'session.json') -Raw | ConvertFrom-Json
$hwnd = [IntPtr]$session.hwnd
$root = [System.Windows.Automation.AutomationElement]::FromHandle($hwnd)
if ($root.Current.ProcessId -ne $session.pid) { throw 'Window does not belong to the review application' }
if ($Action -eq 'resize') {
 $scale = $session.dpi / 96
 [ReviewWindow]::MoveWindow($hwnd, 0, 0, [int]($Width * $scale), [int]($Height * $scale), $true) | Out-Null
}
if ($Action -eq 'click') {
 $condition = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::NameProperty, $Name)
 $element = $root.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $condition)
 if ($null -eq $element) { throw "Missing named element: $Name" }
 $b = $element.Current.BoundingRectangle
 [ReviewWindow]::SetForegroundWindow($hwnd) | Out-Null
 if ([ReviewWindow]::GetForegroundWindow() -ne $hwnd) { throw 'Native app does not own foreground' }
 [ReviewWindow]::SetCursorPos([int]($b.X + [Math]::Min(100, $b.Width / 2)), [int]($b.Y + [Math]::Min(14, $b.Height / 2))) | Out-Null
 [ReviewWindow]::mouse_event(2,0,0,0,[UIntPtr]::Zero)
 [ReviewWindow]::mouse_event(4,0,0,0,[UIntPtr]::Zero)
}
if ($Action -eq 'invoke') {
 $condition = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::NameProperty, $Name)
 $element = $root.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $condition)
 if ($null -eq $element) { throw "Missing named element: $Name" }
 $element.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
}
if ($Action -eq 'keys') {
 [ReviewWindow]::SetForegroundWindow($hwnd) | Out-Null
 if ([ReviewWindow]::GetForegroundWindow() -ne $hwnd) { throw 'Native app does not own foreground' }
 [System.Windows.Forms.SendKeys]::SendWait($Keys)
}
if ($Action -eq 'close') {
 [ReviewWindow]::PostMessage($hwnd, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
 exit
}
Start-Sleep -Milliseconds 600
$nodes = $root.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
$items = @($root) + @($nodes | ForEach-Object { $_ })
$tree = @($items | ForEach-Object {
 $b = $_.Current.BoundingRectangle
 [pscustomobject]@{name=$_.Current.Name;role=$_.Current.ControlType.ProgrammaticName;id=$_.Current.AutomationId;enabled=$_.Current.IsEnabled;offscreen=$_.Current.IsOffscreen;bounds=@($b.X,$b.Y,$b.Width,$b.Height)}
})
$tree | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $PSScriptRoot "$Shot.uia.json") -Encoding UTF8
$rect = New-Object ReviewWindow+Rect
$client = New-Object ReviewWindow+Rect
[ReviewWindow]::GetWindowRect($hwnd, [ref]$rect) | Out-Null
[ReviewWindow]::GetClientRect($hwnd, [ref]$client) | Out-Null
$screen = [System.Windows.Forms.SystemInformation]::VirtualScreen
$left = [Math]::Max($rect.Left, $screen.Left)
$top = [Math]::Max($rect.Top, $screen.Top)
$width = [Math]::Min($rect.Right, $screen.Right) - $left
$height = [Math]::Min($rect.Bottom, $screen.Bottom) - $top
[ReviewWindow]::SetForegroundWindow($hwnd) | Out-Null
Start-Sleep -Milliseconds 150
if ([ReviewWindow]::GetForegroundWindow() -ne $hwnd) { throw 'Cannot capture: app does not own foreground' }
$bitmap = New-Object System.Drawing.Bitmap($width, $height)
$graphics = [System.Drawing.Graphics]::FromImage($bitmap)
try {
 $graphics.CopyFromScreen($left, $top, 0, 0, $bitmap.Size)
 if ([ReviewWindow]::GetForegroundWindow() -ne $hwnd) { throw 'Foreground changed during capture' }
 $bitmap.Save((Join-Path $PSScriptRoot "$Shot.png"), [System.Drawing.Imaging.ImageFormat]::Png)
} finally { $graphics.Dispose(); $bitmap.Dispose() }
[pscustomobject]@{shot=$Shot;clientPhysical=@($client.Right,$client.Bottom);dpi=$session.dpi;nodes=$tree.Count} | ConvertTo-Json -Compress
$tree | ForEach-Object { "$($_.role): $($_.name) [enabled=$($_.enabled); offscreen=$($_.offscreen)]" }
