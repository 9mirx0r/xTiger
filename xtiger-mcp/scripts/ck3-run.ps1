# One-shot CK3 test driver: launch, load a bookmark, run console commands, take screenshots, write a log report, close.
# Usage: powershell -NoProfile -File ck3-run.ps1 -GameDir <install root> -Cmd "script_docs","event my_event.0001" -Shot out.png [-Keep]
# The console key is scancode 0x29 (the key under Esc) on US and most European layouts.
# Commands are typed as Unicode text by default (-Typing unicode), which works on any keyboard layout.
# -Typing scancode is the fallback; it only types letters, digits, space, '.', '-' and '_'.
param(
    [string[]]$Cmd = @(),
    [string]$CmdFile = "",
    [string]$Shot = "",
    [string]$Bookmark = "bm_867_carolingians",
    [string]$Play = "k_france",
    [int]$LoadTimeout = 300,
    [string]$ShotDir = "",
    [string[]]$Mod = @(),
    [string]$Report = "",
    [ValidateSet("unicode", "scancode")][string]$Typing = "unicode",
    [int]$KeepArchives = 10,
    [string]$GameDir = $env:CK3_GAME_DIR,
    [string]$UserDir = $env:CK3_USER_DIR,
    [switch]$Keep
)
$ErrorActionPreference = "Stop"
if (-not $GameDir) { throw "Pass -GameDir or set CK3_GAME_DIR to the CK3 install folder (the one with binaries\ck3.exe)." }
if (-not $UserDir) { $UserDir = Join-Path ([Environment]::GetFolderPath("MyDocuments")) "Paradox Interactive\Crusader Kings III" }
. (Join-Path $PSScriptRoot "ck3-input.ps1")
# -CmdFile holds one console command per line, which avoids quoting trouble when called from other tools.
if ($CmdFile) { $Cmd += @(Get-Content $CmdFile -Encoding utf8 | Where-Object { $_.Trim() }) }
$exe  = Join-Path $GameDir "binaries\ck3.exe"
$logs = Join-Path $UserDir "logs"
if (-not (Test-Path $exe)) { throw "ck3.exe not found at $exe" }
if (Get-Process ck3 -ErrorAction SilentlyContinue) { throw "CK3 is already running; close it first so the logs belong to this run." }
# Refuse untypeable commands before launching, instead of silently dropping characters.
foreach ($c in $Cmd) {
    $bad = Test-Typeable $c $Typing
    if ($bad) { throw "cannot type '$(-join $bad)' in $Typing mode: $c" }
}
if (-not $Report) { $Report = Join-Path ([IO.Path]::GetTempPath()) "ck3-run-report.txt" }

$runId = Get-Date -Format "yyyyMMdd-HHmmss"
$args2 = @("-debug_mode","-skip_checksum","-skip","-bookmark=$Bookmark","-play=$Play")

# The game ignores mods on its command line: it loads the ones listed in dlc_load.json, which the
# launcher writes. So list the wanted mods there for this run, and put the file back afterwards.
# A .mod file outside the user's mod folder gets a temporary copy in it, since the game only reads
# that folder.
$modFiles = @($Mod | ForEach-Object { $_ -split ';' } | Where-Object { $_ })
$modDirs = @(); $tempMods = @(); $dlcLoad = Join-Path $UserDir "dlc_load.json"; $dlcLoadBefore = $null
if ($modFiles) {
    $userMods = Join-Path $UserDir "mod"
    $enabled = @()
    for ($i = 0; $i -lt $modFiles.Count; $i++) {
        $file = (Resolve-Path $modFiles[$i]).Path
        $text = Get-Content $file -Raw -Encoding utf8
        $dir = if ($text -match '(?m)^\s*path\s*=\s*"([^"]+)"') { $Matches[1] } else { Split-Path $file }
        if (-not [IO.Path]::IsPathRooted($dir)) { $dir = Join-Path $UserDir $dir }
        $modDirs += [IO.Path]::GetFullPath($dir)
        if ((Split-Path $file) -eq $userMods) {
            $enabled += "mod/" + (Split-Path $file -Leaf)
        } else {
            $name = "xtiger-run-$i.mod"
            $body = ($text -replace '(?m)^\s*path\s*=.*$', '').TrimEnd() + "`npath=`"$($modDirs[-1].Replace('\', '/'))`"`n"
            Set-Content (Join-Path $userMods $name) $body -Encoding utf8
            $tempMods += Join-Path $userMods $name
            $enabled += "mod/$name"
        }
    }
    $disabled = @()
    if (Test-Path $dlcLoad) {
        $dlcLoadBefore = Get-Content $dlcLoad -Raw -Encoding utf8
        try { $disabled = @(($dlcLoadBefore | ConvertFrom-Json).disabled_dlcs) } catch { }
    }
    $json = '{"enabled_mods":[' + (($enabled | ForEach-Object { '"' + $_ + '"' }) -join ',') + '],"disabled_dlcs":[' + (($disabled | Where-Object { $_ } | ForEach-Object { '"' + $_ + '"' }) -join ',') + ']}'
    [IO.File]::WriteAllText($dlcLoad, $json)
}
function Restore-DlcLoad {
    if (-not $modFiles) { return }
    if ($null -ne $dlcLoadBefore) { [IO.File]::WriteAllText($dlcLoad, $dlcLoadBefore) } else { Remove-Item $dlcLoad -ErrorAction SilentlyContinue }
    $tempMods | ForEach-Object { Remove-Item $_ -ErrorAction SilentlyContinue }
}
# Put dlc_load.json back even if the game cannot be started.
trap { Restore-DlcLoad; break }

# Move the previous logs aside so the report only covers this run, and keep them for later comparison.
$archive = Join-Path $logs "xtiger-archive\$runId"
foreach ($f in "error.log","game.log","exceptions.log","debug.log") {
    $src = Join-Path $logs $f
    if (Test-Path $src) { New-Item -ItemType Directory -Force $archive | Out-Null; Move-Item $src $archive }
}
Get-ChildItem (Join-Path $logs "xtiger-archive") -Directory -ErrorAction SilentlyContinue |
    Sort-Object Name -Descending | Select-Object -Skip $KeepArchives | Remove-Item -Recurse -Force

$out = @("=== run $runId ===", "game: $exe", "args: $($args2 -join ' ')", "typing: $Typing")
$sent = @(); $failure = $null
$p = Start-Process $exe -ArgumentList $args2 -WorkingDirectory (Split-Path $exe) -PassThru
Write-Host "launched pid $($p.Id)"
try {
    # The game writes this line to game.log once the bookmark has been turned into a running game.
    $ready = 'Completed generating game state from bookmark'
    $t0 = Get-Date
    while ($true) {
        Start-Sleep 3
        if ($p.HasExited) { throw "game exited early (code $($p.ExitCode)), check $UserDir\crashes" }
        $p.Refresh()
        if ($p.MainWindowHandle -ne 0 -and (Select-String -Path (Join-Path $logs "game.log") -Pattern $ready -SimpleMatch -Quiet -ErrorAction SilentlyContinue)) { break }
        if (((Get-Date) - $t0).TotalSeconds -gt $LoadTimeout) { throw "load timeout after $LoadTimeout s (no '$ready' in game.log)" }
    }
    $out += "loaded after $([int]((Get-Date) - $t0).TotalSeconds) s"
    # The loading screen stays up for a moment after the game state is ready.
    Start-Sleep 12
    [CK3Input]::SetForegroundWindow($p.MainWindowHandle) | Out-Null; Start-Sleep 1
    # Open the console once. The game can still be streaming assets, and keys typed before the
    # console takes text are lost (a run once lost the first three letters of a command).
    if ($Cmd) { Send-Tap $p 0x29; Start-Sleep -Milliseconds 2000 }
    foreach ($c in $Cmd) {
        # The console key can leave its own character in the input line (for example '|' on Spanish
        # layouts), which turns the command into an unknown one. Clear the line before typing.
        for ($i = 0; $i -lt 3; $i++) { Send-Tap $p 0x0E }
        Send-Text $p $c $Typing; Send-Tap $p 0x1C           # enter
        Start-Sleep 3
        if ($ShotDir) { Save-WindowShot $p (Join-Path $ShotDir (($c -replace '[^a-zA-Z0-9]','_') + ".png")) }
        $sent += $c; Write-Host "sent: $c"
    }
    if ($Cmd) { Send-Tap $p 0x29; Start-Sleep -Milliseconds 400 }   # close the console
    if ($Shot) { Save-WindowShot $p $Shot; Write-Host "screenshot $Shot" }
} catch {
    $failure = $_.Exception.Message
    Write-Host "FAILED: $failure"
} finally {
    # The game has read dlc_load.json by now, so the user's own list can go back.
    Restore-DlcLoad
}

# debug.log records every command the console accepted, so a mistyped or unknown command shows up here.
# The game buffers debug.log, so poll for a while before deciding a command did not run.
$debugLog = Join-Path $logs "debug.log"
$ran = @()
$deadline = (Get-Date).AddSeconds(15)
do {
    if (Test-Path $debugLog) {
        $ran = @(Select-String -Path $debugLog -Pattern 'Running console command: (.*)$' | ForEach-Object { $_.Matches[0].Groups[1].Value.Trim() })
    }
    if (@($sent | Where-Object { $ran -notcontains $_.Trim() }).Count -eq 0) { break }
    Start-Sleep 2
} while ((Get-Date) -lt $deadline -and -not $p.HasExited)
$out += "commands sent: $($sent.Count) of $($Cmd.Count), run by the game: $(@($sent | Where-Object { $ran -contains $_.Trim() }).Count)"
$out += $sent | ForEach-Object { if ($ran -contains $_.Trim()) { "  ran      > $_" } else { "  unconfirmed > $_  (not in debug.log: mistyped, unknown, or a command the game does not log; check its output)" } }
if ($failure) { $out += "FAILED: $failure" }

# debug.log lists every folder the game mounted, so it shows whether each mod was really loaded.
if ($modDirs) {
    $mounted = @()
    if (Test-Path $debugLog) {
        $mounted = @(Select-String -Path $debugLog -Pattern 'Mounted Data: (.*)$' | ForEach-Object { [IO.Path]::GetFullPath($_.Matches[0].Groups[1].Value.Trim()).TrimEnd('\', '/') })
    }
    $missing = @($modDirs | Where-Object { $mounted -notcontains $_.TrimEnd('\', '/') })
    $out += "mods loaded: $($modDirs.Count - $missing.Count) of $($modDirs.Count)"
    $out += $missing | ForEach-Object { "  NOT LOADED > $_  (the game did not mount this folder, so this run did not test it)" }
}
if ($p.HasExited) { $out += "game exit code: $($p.ExitCode)" }

# error.log: timestamps dropped, repeats counted. Two vanilla lines that appear in every run are hidden, and counted.
$noise = 'Invalid achievement|Failed to read key reference'
$errLog = Join-Path $logs "error.log"; $gameLog = Join-Path $logs "game.log"
$out += "=== error.log (deduplicated) ==="
if (Test-Path $errLog) {
    $lines = @(Get-Content $errLog)
    $kept = @($lines | Where-Object { $_ -notmatch $noise })
    $out += "$($lines.Count) lines, $($lines.Count - $kept.Count) hidden as known vanilla noise ($noise)"
    $out += ($kept | ForEach-Object { $_ -replace '^\[[0-9:.]+\]','' } | Group-Object | Sort-Object Count -Descending | ForEach-Object { "$($_.Count)x $($_.Name)" })
} else { $out += "(no error.log)" }
$out += "=== game.log lines matching mod|error|fail|missing ==="
if (Test-Path $gameLog) {
    $hits = @(Select-String -Path $gameLog -Pattern 'mod|error|fail|missing')
    $out += ($hits | Select-Object -First 60 | ForEach-Object { $_.Line })
    if ($hits.Count -gt 60) { $out += "... $($hits.Count - 60) more matching lines in $gameLog" }
} else { $out += "(no game.log)" }
if (Test-Path $archive) { $out += "previous logs moved to $archive" }
$out | Set-Content $Report -Encoding utf8; Write-Host "report $Report ($($out.Count) lines)"

if ((-not $Keep -or $failure) -and -not $p.HasExited) { Start-Sleep 2; Stop-Process -Id $p.Id -Force; Write-Host "game closed" }
if ($failure) { exit 1 }
