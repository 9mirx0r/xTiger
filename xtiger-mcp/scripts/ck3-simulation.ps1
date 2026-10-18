# Wait for CK3's own completion marker. Elapsed wall time never proves that years passed.
function Wait-CK3Simulation($Process, [string]$Log, [int]$Years, [int]$TimeoutSeconds) {
    $timer = [Diagnostics.Stopwatch]::StartNew()
    $nextProgress = 0
    $started = $false
    $completion = 'Game ran ' + $Years + ' years in [0-9.]+ minutes\.'
    while ($true) {
        if ($Process.HasExited) { throw "game exited during observer simulation (code $($Process.ExitCode))" }
        $lines = if (Test-Path -LiteralPath $Log) { @(Get-Content -LiteralPath $Log -Tail 20) } else { @() }
        foreach ($line in $lines) {
            if ($line -match 'Started timed performance test at in-game date') { $started = $true }
            if ($started -and $line -match $completion) { return "simulation completed: $Years years; $($line.Trim())" }
        }
        if ($timer.Elapsed.TotalSeconds -ge $TimeoutSeconds) {
            throw "simulation timeout after $TimeoutSeconds s; $Years years were not confirmed by the game"
        }
        if ($timer.Elapsed.TotalSeconds -ge $nextProgress) {
            Write-Host "simulation: waiting for $Years years ($([int]$timer.Elapsed.TotalSeconds) s elapsed; started: $started)"
            $nextProgress = $timer.Elapsed.TotalSeconds + 15
        }
        Start-Sleep -Milliseconds 500
    }
}
