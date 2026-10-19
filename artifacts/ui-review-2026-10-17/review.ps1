param([string]$Name, [string]$Shot = 'review')
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient
$reviewSession = Get-Content (Join-Path $PSScriptRoot 'session.json') -Raw | ConvertFrom-Json
$reviewRoot = [System.Windows.Automation.AutomationElement]::FromHandle([IntPtr]$reviewSession.hwnd)
if ($reviewRoot.Current.ProcessId -ne $reviewSession.pid) { throw 'Unexpected process' }
if ($Name) {
    $nameCondition = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::NameProperty, $Name)
    $typeCondition = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ControlTypeProperty, [System.Windows.Automation.ControlType]::Button)
    $condition = New-Object System.Windows.Automation.AndCondition($nameCondition, $typeCondition)
    $element = $reviewRoot.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $condition)
    if ($null -eq $element) { throw "Missing control: $Name" }
    if ($element.GetSupportedPatterns().Id -contains [System.Windows.Automation.TogglePattern]::Pattern.Id) {
        $element.GetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern).Toggle()
    } else {
        $element.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
    }
    Start-Sleep -Milliseconds 700
}
$nodes = $reviewRoot.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
$nodes | ForEach-Object { [pscustomobject]@{name=$_.Current.Name; role=$_.Current.ControlType.ProgrammaticName} } | ConvertTo-Json | Set-Content (Join-Path $PSScriptRoot "$Shot.uia.json") -Encoding UTF8
& (Join-Path $PSScriptRoot '../frontend-bars-polish/capture/target/debug/kl-window-capture.exe') $reviewSession.hwnd (Join-Path $PSScriptRoot "$Shot.png")
$nodes | Where-Object { $_.Current.ControlType.ProgrammaticName -eq 'ControlType.Button' } | ForEach-Object { $_.Current.Name }
