$ErrorActionPreference = 'Stop'
$outDir = $PSScriptRoot
$state = Get-Content -LiteralPath (Join-Path $outDir 'session.json') -Raw | ConvertFrom-Json
$samples = @()
foreach ($sampleNumber in 1..3) {
    $allProcesses = @(Get-CimInstance Win32_Process)
    $groups = @{}
    $groups[[int]$state.kl_ui] = 'KernelLedger'
    $groups[[int]$state.kl_supervisor] = 'KernelLedger'
    $groups[[int]$state.marimo] = 'marimo Python'
    $groups[62068] = 'marimo tab'
    do {
        $added = $false
        foreach ($item in $allProcesses) {
            if ($groups.ContainsKey([int]$item.ParentProcessId) -and -not $groups.ContainsKey([int]$item.ProcessId)) {
                $groups[[int]$item.ProcessId] = $groups[[int]$item.ParentProcessId]
                $added = $true
            }
        }
    } while ($added)
    $counters = @(Get-CimInstance Win32_PerfFormattedData_PerfProc_Process)
    $rows = @(foreach ($item in $allProcesses) {
        if ($groups.ContainsKey([int]$item.ProcessId)) {
            $counter = $counters | Where-Object { $_.IDProcess -eq $item.ProcessId } | Select-Object -First 1
            if ($null -eq $counter) { throw "Missing memory counters for $($item.ProcessId)" }
            [pscustomobject]@{
                pid=$item.ProcessId; parent=$item.ParentProcessId; group=$groups[[int]$item.ProcessId]
                name=$item.Name; command=$item.CommandLine
                workingSet=[long]$counter.WorkingSet; privateWorkingSet=[long]$counter.WorkingSetPrivate
                privateBytes=[long]$counter.PrivateBytes
            }
        }
    })
    $sample = [pscustomobject]@{sample=$sampleNumber; time=(Get-Date -Format o); processes=$rows}
    $samples += $sample
    $rows | Group-Object group | ForEach-Object {
        [pscustomobject]@{
            Sample=$sampleNumber; Group=$_.Name; Count=$_.Count
            WorkingSetMiB=[math]::Round(($_.Group | Measure-Object workingSet -Sum).Sum / 1MB,2)
            PrivateRAMMiB=[math]::Round(($_.Group | Measure-Object privateWorkingSet -Sum).Sum / 1MB,2)
            CommitMiB=[math]::Round(($_.Group | Measure-Object privateBytes -Sum).Sum / 1MB,2)
        }
    } | Format-Table -AutoSize
    if ($sampleNumber -lt 3) { Start-Sleep -Seconds 10 }
}
$samples | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $outDir 'samples.json')
