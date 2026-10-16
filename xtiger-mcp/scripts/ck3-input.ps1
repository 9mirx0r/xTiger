# Shared Win32 input and capture helpers for ck3-run.ps1 and ck3-keys.ps1. Dot-source this file.
# CK3 ignores virtual-key input. It reads hardware scancodes (SendInput + KEYEVENTF_SCANCODE) for keys,
# and text typed into the console also arrives as Unicode characters (KEYEVENTF_UNICODE), which do not
# depend on the keyboard layout.
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System; using System.Runtime.InteropServices;
public class CK3Input {
  [StructLayout(LayoutKind.Sequential)] public struct KI { public ushort vk, sc; public uint fl, t; public IntPtr ex; }
  [StructLayout(LayoutKind.Explicit, Size=40)] public struct IN { [FieldOffset(0)] public uint type; [FieldOffset(8)] public KI ki; }
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
  [DllImport("user32.dll", SetLastError=true)] static extern uint SendInput(uint n, IN[] i, int sz);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] static extern bool AttachThreadInput(uint a, uint b, bool attach);
  [DllImport("user32.dll")] static extern bool BringWindowToTop(IntPtr h);
  [DllImport("user32.dll")] static extern bool ShowWindow(IntPtr h, int cmd);
  [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr h);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr dc, uint flags);
  [DllImport("user32.dll")] static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extra);
  // A click at a point of the screen: the button goes down and up there. 2/4 are the left button, 8/16 the right.
  public static void Click(int x, int y, bool right) {
    SetCursorPos(x, y); System.Threading.Thread.Sleep(150);
    mouse_event(right ? 8u : 2u, 0, 0, 0, UIntPtr.Zero); System.Threading.Thread.Sleep(80);
    mouse_event(right ? 16u : 4u, 0, 0, 0, UIntPtr.Zero);
  }
  [DllImport("kernel32.dll")] static extern uint GetCurrentThreadId();
  // Windows refuses SetForegroundWindow from a background process. Sharing the input queue of the
  // current foreground thread lifts that restriction, which is how focus is taken back after another
  // window (a notification, the Claude app) has grabbed it.
  public static bool Focus(IntPtr h) {
    if (IsIconic(h)) ShowWindow(h, 9);
    uint dummy; uint fg = GetWindowThreadProcessId(GetForegroundWindow(), out dummy);
    uint me = GetCurrentThreadId(); bool att = fg != 0 && fg != me && AttachThreadInput(me, fg, true);
    BringWindowToTop(h); bool ok = SetForegroundWindow(h);
    if (att) AttachThreadInput(me, fg, false);
    return ok;
  }
  // Background runs keep the game out of the way: minimized, and without activating it, so the window
  // the user is working in keeps the focus. 7 is SW_SHOWMINNOACTIVE.
  public static void KeepMinimized(IntPtr h) { if (h != IntPtr.Zero && !IsIconic(h)) ShowWindow(h, 7); }
  // SendInput returns how many events it injected; 0 means Windows refused them (input blocked by
  // another thread, a locked session or the secure desktop; UIPI blocking can also fail it, though
  // the error code does not say so). Say so instead of losing the key.
  static void Send(ushort sc, uint fl) {
    if (IntPtr.Size != 8) throw new InvalidOperationException("the input helper needs 64-bit PowerShell");
    var i = new IN[1]; i[0].type = 1; i[0].ki.sc = sc; i[0].ki.fl = fl;
    if (SendInput(1, i, Marshal.SizeOf(typeof(IN))) != 1) {
      int e = Marshal.GetLastWin32Error();
      throw new System.ComponentModel.Win32Exception(e, "SendInput did not deliver the key (Windows error " + e + ": " + new System.ComponentModel.Win32Exception(e).Message + ")");
    }
  }
  public static void Key(ushort sc, bool up) { Send(sc, 8u | (up ? 2u : 0u)); }
  public static void Char(char c, bool up) { Send((ushort)c, 4u | (up ? 2u : 0u)); }
  public static uint ForegroundPid() { uint pid; GetWindowThreadProcessId(GetForegroundWindow(), out pid); return pid; }
}
"@
# Without this, window coordinates are scaled on high-DPI screens and captures are cropped.
[CK3Input]::SetProcessDPIAware() | Out-Null

# Scancodes for scancode typing. Letters, digits, space, '.' and '-' sit on the same keys on most layouts;
# other symbols move between layouts, which is why Unicode typing is the default.
$script:ScanMap = @{ 'a'=0x1E;'b'=0x30;'c'=0x2E;'d'=0x20;'e'=0x12;'f'=0x21;'g'=0x22;'h'=0x23;'i'=0x17;'j'=0x24;'k'=0x25;
  'l'=0x26;'m'=0x32;'n'=0x31;'o'=0x18;'p'=0x19;'q'=0x10;'r'=0x13;'s'=0x1F;'t'=0x14;'u'=0x16;'v'=0x2F;'w'=0x11;'x'=0x2D;
  'y'=0x15;'z'=0x2C;'1'=2;'2'=3;'3'=4;'4'=5;'5'=6;'6'=7;'7'=8;'8'=9;'9'=10;'0'=11;' '=0x39;'.'=0x34;'-'=0x35 }

function Test-Typeable([string]$text, [string]$mode) {
    # Returns the characters that cannot be typed in this mode, so callers can refuse before sending anything.
    $bad = @()
    foreach ($ch in $text.ToCharArray()) {
        if ($mode -eq "unicode") { if ([char]::IsControl($ch)) { $bad += $ch } }
        elseif ($ch -cne '_' -and -not $script:ScanMap.ContainsKey(([string]$ch).ToLower())) { $bad += $ch }
    }
    return $bad
}

function Assert-Focus([System.Diagnostics.Process]$proc) {
    # Input goes to whatever window has focus, so never type into anything but the game.
    if ([CK3Input]::ForegroundPid() -eq $proc.Id) { return }
    # Another window may have grabbed focus for a moment. Take it back a few times before giving up.
    for ($try = 0; $try -lt 4; $try++) {
        [CK3Input]::Focus($proc.MainWindowHandle) | Out-Null
        Start-Sleep -Milliseconds 500
        if ([CK3Input]::ForegroundPid() -eq $proc.Id) { return }
    }
    throw "CK3 (pid $($proc.Id)) is not the foreground window; stopped sending input. Another window took focus."
}

function Send-Tap([System.Diagnostics.Process]$proc, [int]$code, [bool]$shift = $false) {
    Assert-Focus $proc
    if ($shift) { [CK3Input]::Key(0x2A, $false) }
    [CK3Input]::Key($code, $false); Start-Sleep -Milliseconds 40; [CK3Input]::Key($code, $true)
    if ($shift) { [CK3Input]::Key(0x2A, $true) }
    Start-Sleep -Milliseconds 60
}

function Send-Text([System.Diagnostics.Process]$proc, [string]$text, [string]$mode) {
    $bad = Test-Typeable $text $mode
    if ($bad) { throw "cannot type '$(-join $bad)' in $mode mode: $text" }
    foreach ($ch in $text.ToCharArray()) {
        if ($mode -eq "unicode") {
            Assert-Focus $proc
            [CK3Input]::Char($ch, $false); [CK3Input]::Char($ch, $true); Start-Sleep -Milliseconds 30
        } elseif ($ch -ceq '_') { Send-Tap $proc 0x35 $true }
        else { Send-Tap $proc $script:ScanMap[([string]$ch).ToLower()] ([char]::IsUpper($ch)) }
    }
}

function Save-WindowShotNoFocus([System.Diagnostics.Process]$proc, [string]$path) {
    # Asks the window to paint itself (PrintWindow, PW_RENDERFULLCONTENT = 2), so the picture is right even
    # when another window covers the game, and the focus stays where the user has it. Returns $false when
    # the window is minimized or the picture comes back black, and the caller falls back to the screen copy.
    $hwnd = $proc.MainWindowHandle
    if ($hwnd -eq [IntPtr]::Zero -or [CK3Input]::IsIconic($hwnd)) { return $false }
    $r = New-Object CK3Input+RECT
    if (-not [CK3Input]::GetWindowRect($hwnd, [ref]$r)) { return $false }
    $w = $r.Right - $r.Left; $h = $r.Bottom - $r.Top
    if ($w -lt 200 -or $h -lt 200) { return $false }
    $b = New-Object System.Drawing.Bitmap($w, $h)
    try {
        $g = [System.Drawing.Graphics]::FromImage($b)
        $dc = $g.GetHdc()
        try { $ok = [CK3Input]::PrintWindow($hwnd, $dc, 2) } finally { $g.ReleaseHdc($dc); $g.Dispose() }
        if (-not $ok) { return $false }
        $lit = 0
        for ($x = 10; $x -lt $w; $x += [Math]::Max(1, [int]($w / 32))) {
            for ($y = 10; $y -lt $h; $y += [Math]::Max(1, [int]($h / 18))) {
                $c = $b.GetPixel($x, $y)
                if ($c.R + $c.G + $c.B -gt 12) { $lit++ }
            }
        }
        if ($lit -lt 8) { return $false }
        $b.Save($path)
        return $true
    } finally { $b.Dispose() }
}

function Save-WindowShot([System.Diagnostics.Process]$proc, [string]$path) {
    # Captures only the game window, so other windows and monitors stay out of the picture.
    if (Save-WindowShotNoFocus $proc $path) { return }
    Assert-Focus $proc
    $r = New-Object CK3Input+RECT
    if (-not [CK3Input]::GetWindowRect($proc.MainWindowHandle, [ref]$r)) { throw "could not read the CK3 window position" }
    $w = $r.Right - $r.Left; $h = $r.Bottom - $r.Top
    if ($w -le 0 -or $h -le 0) { throw "the CK3 window has no visible area (minimized?)" }
    $b = New-Object System.Drawing.Bitmap($w, $h)
    $g = [System.Drawing.Graphics]::FromImage($b)
    $g.CopyFromScreen($r.Left, $r.Top, 0, 0, $b.Size); $b.Save($path); $g.Dispose(); $b.Dispose()
}

function Apply-ShotView([string]$path, [string]$view) {
    # $view is "x0,y0,x1,y1,scale": the part of the picture to keep, as fractions of its width and height,
    # and how much to shrink it (1 = as it is). Empty leaves the picture alone. A smaller picture costs
    # the reader fewer tokens.
    if (-not $view) { return }
    $v = @($view.Split(",") | ForEach-Object { [double]::Parse($_, [Globalization.CultureInfo]::InvariantCulture) })
    if ($v.Count -ne 5) { throw "bad screenshot view: $view" }
    $src = [System.Drawing.Image]::FromFile($path)
    try {
        $x0 = [int][Math]::Round($v[0] * $src.Width); $y0 = [int][Math]::Round($v[1] * $src.Height)
        $w = [Math]::Max(1, [int][Math]::Round(($v[2] - $v[0]) * $src.Width))
        $h = [Math]::Max(1, [int][Math]::Round(($v[3] - $v[1]) * $src.Height))
        $ow = [Math]::Max(1, [int][Math]::Round($w * $v[4])); $oh = [Math]::Max(1, [int][Math]::Round($h * $v[4]))
        $out = New-Object System.Drawing.Bitmap($ow, $oh)
        try {
            $g = [System.Drawing.Graphics]::FromImage($out)
            $g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
            $g.DrawImage($src, (New-Object System.Drawing.Rectangle(0, 0, $ow, $oh)), $x0, $y0, $w, $h, [System.Drawing.GraphicsUnit]::Pixel)
            $g.Dispose()
            $tmp = "$path.tmp"
            $out.Save($tmp, [System.Drawing.Imaging.ImageFormat]::Png)
        } finally { $out.Dispose() }
    } finally { $src.Dispose() }
    Move-Item -Force $tmp $path
}

function Test-ConsoleOpen([System.Diagnostics.Process]$proc) {
    # The console is a flat grey panel (29,29,29 for the input line, 41,41,41 for the history) over the left
    # side of the window; nothing else in the game paints exactly that grey over a wide area, so counting
    # such pixels tells whether it is open. The window paints itself (PrintWindow), so a covering window
    # does not matter; a minimized one cannot be read.
    $hwnd = $proc.MainWindowHandle
    if ($hwnd -eq [IntPtr]::Zero -or [CK3Input]::IsIconic($hwnd)) { throw "the CK3 window is minimized; cannot see the console" }
    $r = New-Object CK3Input+RECT
    if (-not [CK3Input]::GetWindowRect($hwnd, [ref]$r)) { throw "could not read the CK3 window position" }
    $w = $r.Right - $r.Left; $h = $r.Bottom - $r.Top
    if ($w -lt 200 -or $h -lt 200) { throw "the CK3 window has no visible area" }
    $b = New-Object System.Drawing.Bitmap($w, $h)
    try {
        $g = [System.Drawing.Graphics]::FromImage($b)
        $dc = $g.GetHdc()
        try { $ok = [CK3Input]::PrintWindow($hwnd, $dc, 2) } finally { $g.ReleaseHdc($dc); $g.Dispose() }
        if (-not $ok) { throw "the CK3 window could not be captured" }
        $flat = 0; $seen = 0
        for ($x = [int]($w * 0.015); $x -lt [int]($w * 0.2); $x += 6) {
            for ($y = [int]($h * 0.45); $y -lt [int]($h * 0.7); $y += 6) {
                $c = $b.GetPixel($x, $y); $seen++
                if ($c.R -eq $c.G -and $c.G -eq $c.B -and (($c.R -ge 28 -and $c.R -le 30) -or ($c.R -ge 40 -and $c.R -le 42))) { $flat++ }
            }
        }
        return ($seen -gt 0 -and ($flat / $seen) -gt 0.3)
    } finally { $b.Dispose() }
}
