# Resident input helper for ck3_keys: starts once, loads the input code once, then serves one request per
# line on stdin for as long as the server keeps it open. Starting PowerShell and compiling the input code
# costs about a second, which is most of what a key press used to take.
# Request:  <scancodes>|<text base64>|<screenshot path base64>|<wait ms>|<log name>|<pattern base64>|<view>
# <view> is "x0,y0,x1,y1,scale" for the screenshot (a part of the window, shrunk), or empty for all of it.
# (all ASCII; base64 is UTF-8). With a pattern, <wait ms> is the longest wait for a line of that log, written
# after the keys went in, to match the regular expression; the reply comes as soon as it shows up.
# Reply:    OK|<message base64>   or   ERR|<message base64>
# The scancodes field "?console" sends nothing: it replies "console open" or "console closed", read from the
# window's picture. "?click:X:Y" or "?click:X:Y:right" clicks the mouse at X, Y given as fractions (0 to 1) of
# the window's width and height, and nothing else is sent.
# The helper ends when stdin closes.
param([string]$Logs = "")
$ErrorActionPreference = "Stop"
. (Join-Path $PSScriptRoot "ck3-input.ps1")
$in = New-Object System.IO.StreamReader([Console]::OpenStandardInput(), [Text.Encoding]::ASCII)

function Reply([string]$kind, [string]$message) {
    [Console]::Out.WriteLine($kind + "|" + [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($message)))
    [Console]::Out.Flush()
}
function Decode([string]$b64) {
    if ($b64) { [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($b64)) } else { "" }
}

# The first line of the log written after $offset that matches, waiting up to $timeoutMs for it. The game
# buffers its logs, so a line can appear a moment after the event it records.
function Wait-ForLog([string]$path, [long]$offset, [regex]$re, [int]$timeoutMs) {
    $sw = [Diagnostics.Stopwatch]::StartNew()
    do {
        if (Test-Path $path) {
            $fs = [IO.File]::Open($path, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::ReadWrite)
            try {
                if ($fs.Length -lt $offset) { $offset = 0 }
                $fs.Seek($offset, [IO.SeekOrigin]::Begin) | Out-Null
                $new = (New-Object IO.StreamReader($fs, [Text.Encoding]::UTF8)).ReadToEnd()
            } finally { $fs.Dispose() }
            foreach ($l in $new.Split("`n")) { if ($re.IsMatch($l)) { return $l.Trim() } }
        }
        Start-Sleep -Milliseconds 100
    } while ($sw.ElapsedMilliseconds -lt $timeoutMs)
    return $null
}

Reply "OK" "ready"
while ($null -ne ($line = $in.ReadLine())) {
    if (-not $line.Trim()) { continue }
    try {
        $clock = [Diagnostics.Stopwatch]::StartNew()
        $f = $line.Split("|")
        if ($f.Count -ne 7) { throw "bad request: expected 7 fields, got $($f.Count)" }
        $view = $f[6]
        $sc = $f[0]; $text = Decode $f[1]; $shot = Decode $f[2]; $wait = [int]$f[3]
        $logName = $f[4]; $pattern = Decode $f[5]
        $re = $null; $logPath = ""; $offset = 0
        if ($pattern) {
            if ($logName -notmatch '^[a-z_]+$' -or -not $Logs) { throw "cannot watch the log '$logName'" }
            try { $re = New-Object Text.RegularExpressions.Regex($pattern, [Text.RegularExpressions.RegexOptions]::IgnoreCase) }
            catch { throw "wait_for is not a valid regular expression: $($_.Exception.Message)" }
            $logPath = Join-Path $Logs "$logName.log"
            if (Test-Path $logPath) { $offset = (Get-Item $logPath).Length }
        }
        $procs = @(Get-Process ck3 -ErrorAction SilentlyContinue | Where-Object { $_.MainWindowHandle -ne 0 })
        if ($procs.Count -eq 0) { throw "no CK3 window found; start the game first (for example ck3_run with keep_open)" }
        if ($procs.Count -gt 1) { throw "more than one CK3 window is open; close the extra ones" }
        $p = $procs[0]
        if ($sc -eq "?console") {
            # Focus is taken every time, even when the game seems to have it: it costs nothing, and a game that
            # lost the keyboard without Windows noticing does not take keys until it is given focus again.
            [CK3Input]::Focus($p.MainWindowHandle) | Out-Null; Start-Sleep -Milliseconds 150
            Reply "OK" $(if (Test-ConsoleOpen $p) { "console open" } else { "console closed" })
            continue
        }
        if ($sc -match '^\?click:') {
            $c = $sc.Split(":")
            $fx = [double]::Parse($c[1], [Globalization.CultureInfo]::InvariantCulture)
            $fy = [double]::Parse($c[2], [Globalization.CultureInfo]::InvariantCulture)
            if ($fx -lt 0 -or $fx -gt 1 -or $fy -lt 0 -or $fy -gt 1) { throw "a click point is a fraction of the window, 0 to 1: $fx, $fy" }
            $right = $c.Count -gt 3 -and $c[3] -eq "right"
            if ([CK3Input]::IsIconic($p.MainWindowHandle)) { throw "the CK3 window is minimized; cannot click in it" }
            [CK3Input]::Focus($p.MainWindowHandle) | Out-Null; Start-Sleep -Milliseconds 300
            $r = New-Object CK3Input+RECT
            if (-not [CK3Input]::GetWindowRect($p.MainWindowHandle, [ref]$r)) { throw "could not read the CK3 window position" }
            $w = $r.Right - $r.Left; $h = $r.Bottom - $r.Top
            $px = $r.Left + [int][Math]::Round($fx * $w); $py = $r.Top + [int][Math]::Round($fy * $h)
            [CK3Input]::Click($px, $py, $right)
            $seen = $null
            if ($re) { $seen = Wait-ForLog $logPath $offset $re $wait }
            elseif ($wait -gt 0) { Start-Sleep -Milliseconds $wait }
            if ($shot) { Save-WindowShot $p $shot; Apply-ShotView $shot $view }
            if ($re -and $null -eq $seen) { throw "clicked, but no line matching '$pattern' reached $logName.log within $wait ms" }
            $found = if ($re) { "; $logName.log: $(if ($seen.Length -gt 160) { $seen.Substring(0, 160) + '...' } else { $seen })" } else { "" }
            Reply "OK" "$(if ($right) { 'right-' } else { '' })clicked at $([Math]::Round($fx, 3)), $([Math]::Round($fy, 3)) of the window ($($px - $r.Left), $($py - $r.Top) px in a $w x $h window)$found"
            continue
        }
        $codes = @($sc.Split(",") | ForEach-Object { $_.Trim() } | Where-Object { $_ })
        foreach ($k in $codes) {
            if ($k -notmatch '^S?[0-9A-Fa-f]{1,4}$') { throw "not a hex scancode: $k" }
        }
        # Refuse before sending anything, so a bad character does not leave half a command typed.
        if ($text) {
            $bad = Test-Typeable $text "unicode"
            if ($bad) { throw "cannot type '$(-join $bad)': $text" }
        }
        # Only wait when focus had to be taken back; a game that already has it can take keys at once.
        # A request with no keys only wants the screenshot, which is taken without the focus.
        $focused = [CK3Input]::ForegroundPid() -eq $p.Id -or ($codes.Count -eq 0 -and -not $text)
        if (-not $focused) { [CK3Input]::Focus($p.MainWindowHandle) | Out-Null; Start-Sleep -Milliseconds 400 }
        $tFocus = $clock.Elapsed.TotalSeconds
        foreach ($k in $codes) {
            $shift = $k.StartsWith("S"); if ($shift) { $k = $k.Substring(1) }
            Send-Tap $p ([Convert]::ToUInt16($k, 16)) $shift
        }
        if ($text) { Send-Text $p $text "unicode" }
        $tKeys = $clock.Elapsed.TotalSeconds
        $seen = $null
        if ($re) { $seen = Wait-ForLog $logPath $offset $re $wait }
        elseif ($wait -gt 0) { Start-Sleep -Milliseconds $wait }
        if ($shot) { Save-WindowShot $p $shot; Apply-ShotView $shot $view }
        $total = $clock.Elapsed.TotalSeconds
        if ($re -and $null -eq $seen) { throw "keys sent, but no line matching '$pattern' reached $logName.log within $wait ms" }
        $found = if ($re) { "; $logName.log: $(if ($seen.Length -gt 160) { $seen.Substring(0, 160) + '...' } else { $seen })" } else { "" }
        Reply "OK" ("sent $($codes.Count) keys$(if ($text) { " and text: $text" }) to pid $($p.Id) (helper: focus $([Math]::Round($tFocus, 2)) s, keys $([Math]::Round($tKeys - $tFocus, 2)) s, total $([Math]::Round($total, 2)) s)$found")
    } catch {
        Reply "ERR" $_.Exception.Message
    }
}
