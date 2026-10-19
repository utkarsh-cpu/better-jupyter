$ErrorActionPreference = 'Stop'
$outDir = $PSScriptRoot
$state = Get-Content -LiteralPath (Join-Path $outDir 'session.json') -Raw | ConvertFrom-Json
$samples = @()
foreach ($sampleNumber in 1..3) {
    $allProcesses = @(Get-CimInstance Win32_Process)
    $roots = @([int]$state.ui, [int]$state.supervisor)
    $include = @{}
    foreach ($id in $roots) { $include[$id] = $true }
    do {
        $added = $false
        foreach ($item in $allProcesses) {
            if ($include.ContainsKey([int]$item.ParentProcessId) -and -not $include.ContainsKey([int]$item.ProcessId)) {
                $include[[int]$item.ProcessId] = $true
                $added = $true
            }
        }
    } while ($added)
    $counters = @(Get-CimInstance Win32_PerfFormattedData_PerfProc_Process)
    $rows = @(foreach ($item in $allProcesses) {
        if ($include.ContainsKey([int]$item.ProcessId)) {
            $counter = $counters | Where-Object { $_.IDProcess -eq $item.ProcessId } | Select-Object -First 1
            if ($null -eq $counter) { throw "Missing memory counters for $($item.ProcessId)" }
            [pscustomobject]@{
                pid = $item.ProcessId
                parent = $item.ParentProcessId
                name = $item.Name
                command = $item.CommandLine
                workingSet = [long]$counter.WorkingSet
                privateWorkingSet = [long]$counter.WorkingSetPrivate
                privateBytes = [long]$counter.PrivateBytes
            }
        }
    })
    $samples += [pscustomobject]@{ sample = $sampleNumber; time = (Get-Date -Format o); processes = $rows }
    [pscustomobject]@{
        Sample = $sampleNumber
        Count = $rows.Count
        Conhost = @($rows | Where-Object { $_.name -eq 'conhost.exe' }).Count
        PrivateRAMMiB = [math]::Round(($rows | Measure-Object privateWorkingSet -Sum).Sum / 1MB, 2)
        WorkingSetMiB = [math]::Round(($rows | Measure-Object workingSet -Sum).Sum / 1MB, 2)
        CommitMiB = [math]::Round(($rows | Measure-Object privateBytes -Sum).Sum / 1MB, 2)
    } | Format-List
    if ($sampleNumber -lt 3) { Start-Sleep -Seconds 10 }
}
$samples | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $outDir 'samples.json')
