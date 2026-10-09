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
# Put dlc_load.json back, and drop the temporary mod files, however the script ends. This is defined
# before anything can throw: a trap covers the whole script, so an early error would otherwise call
# a function that does not exist yet and hide the real message.
$modFiles = @(); $tempMods = @(); $dlcLoadBefore = $null; $dlcLoad = ""; $dlcBackup = ""; $dlcChanged = $false
$noOriginal = "xtiger:no-original-file"
function Restore-DlcLoad {
    $tempMods | ForEach-Object { Remove-Item $_ -ErrorAction SilentlyContinue }
    if (-not $dlcChanged) { return }
    if ($null -ne $dlcLoadBefore) { [IO.File]::WriteAllText($dlcLoad, $dlcLoadBefore) } else { Remove-Item $dlcLoad -ErrorAction SilentlyContinue }
    Remove-Item $dlcBackup -ErrorAction SilentlyContinue
    $script:dlcChanged = $false
}
trap { Restore-DlcLoad; break }
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
# that folder. The copy has a name of its own and a marker line, so a file of the user's is never
# overwritten and only our own leftovers are ever removed.
# If this script is killed before it can put the file back, the original is kept in
# dlc_load.json.xtiger-backup (line 1: the list written for the run, the rest: the original), and
# the next run restores it (see recover_dlc_load in game.rs).
$tempMarker = "# xtiger-temp-mod"
$modFiles = @($Mod | ForEach-Object { $_ -split ';' } | Where-Object { $_ })
$modDirs = @(); $missing = @(); $dlcLoad = Join-Path $UserDir "dlc_load.json"
$dlcBackup = Join-Path $UserDir "dlc_load.json.xtiger-backup"
if ($modFiles) {
    # A backup that is still there holds the user's own list from a run that was killed; writing a
    # new one would lose it. The MCP server restores it before a run, so this is only a safeguard.
    if (Test-Path $dlcBackup) { throw "$dlcBackup is left over from a run that was stopped: its lines after the first are your own dlc_load.json. Put them back or delete the file, then try again." }
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
            $name = "xtiger-run-$runId-$i.mod"
            $target = Join-Path $userMods $name
            if (Test-Path $target) { throw "$target already exists; not overwriting it." }
            $body = "$tempMarker`n" + ($text -replace '(?m)^\s*path\s*=.*$', '').TrimEnd() + "`npath=`"$($modDirs[-1].Replace('\', '/'))`"`n"
            Set-Content $target $body -Encoding utf8
            $tempMods += $target
            $enabled += "mod/$name"
        }
    }
    $disabled = @()
    if (Test-Path $dlcLoad) {
        $dlcLoadBefore = Get-Content $dlcLoad -Raw -Encoding utf8
        try { $disabled = @(($dlcLoadBefore | ConvertFrom-Json).disabled_dlcs) } catch { }
    }
    $json = '{"enabled_mods":[' + (($enabled | ForEach-Object { '"' + $_ + '"' }) -join ',') + '],"disabled_dlcs":[' + (($disabled | Where-Object { $_ } | ForEach-Object { '"' + $_ + '"' }) -join ',') + ']}'
    [IO.File]::WriteAllText($dlcBackup, $json + "`n" + $(if ($null -ne $dlcLoadBefore) { $dlcLoadBefore } else { $noOriginal }))
    $dlcChanged = $true
    [IO.File]::WriteAllText($dlcLoad, $json)
}

# Move the previous logs aside so the report only covers this run, and keep them for later comparison.
$archive = Join-Path $logs "xtiger-archive\$runId"
foreach ($f in "error.log","game.log","exceptions.log","debug.log") {
    $src = Join-Path $logs $f
    if (Test-Path $src) { New-Item -ItemType Directory -Force $archive | Out-Null; Move-Item $src $archive }
}
Get-ChildItem (Join-Path $logs "xtiger-archive") -Directory -ErrorAction SilentlyContinue |
    Sort-Object Name -Descending | Select-Object -Skip $KeepArchives | Remove-Item -Recurse -Force

function Get-LoggedCount([string]$command) {
    $dl = Join-Path $logs "debug.log"
    if (-not (Test-Path $dl)) { return 0 }
    return @(Select-String -Path $dl -Pattern ('Running console command: ' + [regex]::Escape($command.Trim()) + '\s*$') -ErrorAction SilentlyContinue).Count
}
# One flag per sent command: did the game log it? The n-th send of the same text needs n log lines.
function Get-Confirmed {
    $logged = @{}
    if (Test-Path $debugLog) {
        foreach ($m in (Select-String -Path $debugLog -Pattern 'Running console command: (.*)$')) {
            $key = $m.Matches[0].Groups[1].Value.Trim(); $logged[$key] = 1 + [int]$logged[$key]
        }
    }
    $used = @{}; $flags = @()
    foreach ($c in $sent) {
        $key = $c.Trim(); $used[$key] = 1 + [int]$used[$key]
        $flags += ([int]$logged[$key] -ge $used[$key])
    }
    return ,$flags
}

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
        # Typing stops with an error when another window takes focus. The command was not sent then
        # (Enter is the last key), so it is safe to clear the line and type it again. A command that was
        # typed completely is never repeated, because that would run its effect twice.
        $typed = $false
        # How often the game had logged this command before it was typed, so a repeat of the same
        # command is only confirmed by a new line, not by the earlier one.
        $before = Get-LoggedCount $c
        for ($attempt = 1; $attempt -le 3 -and -not $typed; $attempt++) {
            $entering = $false
            try {
                # The console key can leave its own character in the input line (for example '|' on Spanish
                # layouts), which turns the command into an unknown one. Clear the line before typing.
                for ($i = 0; $i -lt ($c.Length + 3); $i++) { Send-Tap $p 0x0E }
                Send-Text $p $c $Typing; Assert-Focus $p
                # From here on a failure may come after Enter went down, when the command has run.
                $entering = $true; Send-Tap $p 0x1C           # enter
                $typed = $true
            } catch {
                $out += "typing '$c' attempt ${attempt}: $($_.Exception.Message)"
                if ($entering) {
                    # Not typed again: running its effect twice is worse than one unconfirmed command.
                    $typed = $true; Write-Host "enter may not have reached the game for: $c"
                } else {
                    Write-Host "retry $attempt for: $c ($($_.Exception.Message))"
                    Start-Sleep 2
                }
            }
        }
        if (-not $typed) { throw "could not type '$c' after 3 attempts, the game keeps losing focus" }
        # Wait for the game to log the command, so a slow or dropped one is visible at once.
        $seen = $false; $until = (Get-Date).AddSeconds(8)
        do {
            Start-Sleep -Milliseconds 1000
            $seen = (Get-LoggedCount $c) -gt $before
        } while (-not $seen -and (Get-Date) -lt $until)
        if ($ShotDir) { Save-WindowShot $p (Join-Path $ShotDir (($c -replace '[^a-zA-Z0-9]','_') + ".png")) }
        $sent += $c; Write-Host ("sent: $c" + $(if ($seen) { " (confirmed)" } else { " (not logged yet)" }))
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
$deadline = (Get-Date).AddSeconds(15)
do {
    $flags = Get-Confirmed
    if (-not ($flags -contains $false)) { break }
    Start-Sleep 2
} while ((Get-Date) -lt $deadline -and -not $p.HasExited)
$out += "commands sent: $($sent.Count) of $($Cmd.Count), run by the game: $(@($flags | Where-Object { $_ }).Count)"
for ($i = 0; $i -lt $sent.Count; $i++) {
    $out += if ($flags[$i]) { "  ran      > $($sent[$i])" } else { "  unconfirmed > $($sent[$i])  (not in debug.log: mistyped, unknown, or a command the game does not log; check its output)" }
}
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
# A mod the game did not mount means nothing was tested: say so with an exit code of its own.
if ($missing.Count -gt 0) { exit 2 }
