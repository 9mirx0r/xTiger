# Sends keys or text to a running CK3 and optionally takes a screenshot of its window.
# Usage: powershell -NoProfile -File ck3-keys.ps1 -Sc "29,1C" [-Text "effect add_gold = 100"] [-Shot out.png]
# Scancodes are hex and comma-separated. Prefix one with S to hold Shift (S35 types an underscore on US layouts).
# -Text is typed as Unicode characters after the scancodes, so it does not depend on the keyboard layout.
param([string]$Sc = "", [string]$Text = "", [string]$Shot = "", [string]$View = "")
$ErrorActionPreference = "Stop"
# Where the time goes, reported on the last line: PowerShell's own start, loading the input helper,
# finding the window, the fixed waits around the keys, and the typing itself.
$clock = [Diagnostics.Stopwatch]::StartNew()
$phases = [ordered]@{ "powershell start" = [Math]::Round(((Get-Date) - (Get-Process -Id $PID).StartTime).TotalSeconds, 1) }
function Mark([string]$name) { $phases[$name] = [Math]::Round($clock.Elapsed.TotalSeconds, 1); $clock.Restart() }
. (Join-Path $PSScriptRoot "ck3-input.ps1")
Mark "input helper"
$procs = @(Get-Process ck3 -ErrorAction SilentlyContinue | Where-Object { $_.MainWindowHandle -ne 0 })
if ($procs.Count -eq 0) { throw "no CK3 window found; start the game first (for example ck3_run with keep_open)" }
if ($procs.Count -gt 1) { throw "more than one CK3 window is open; close the extra ones" }
$p = $procs[0]
Mark "find window"
$codes = @($Sc.Split(",") | ForEach-Object { $_.Trim() } | Where-Object { $_ })
foreach ($k in $codes) {
    if ($k -notmatch '^S?[0-9A-Fa-f]{1,4}$') { throw "not a hex scancode: $k" }
}
if ($codes.Count -gt 0 -or $Text) { [CK3Input]::SetForegroundWindow($p.MainWindowHandle) | Out-Null; Start-Sleep 1 }
Mark "focus + wait"
foreach ($k in $codes) {
    $shift = $k.StartsWith("S"); if ($shift) { $k = $k.Substring(1) }
    Send-Tap $p ([Convert]::ToUInt16($k, 16)) $shift
}
Mark "scancodes"
if ($Text) { Send-Text $p $Text "unicode" }
Mark "text ($($Text.Length) chars)"
Start-Sleep 1
Mark "final wait"
if ($Shot) { Save-WindowShot $p $Shot; Apply-ShotView $Shot $View; Mark "screenshot" }
Write-Host "sent $($codes.Count) keys$(if ($Text) { " and text: $Text" }) to pid $($p.Id)"
Write-Host ("timing (s): " + (($phases.GetEnumerator() | ForEach-Object { "$($_.Key) $($_.Value)" }) -join " | "))
