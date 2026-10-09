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
  [DllImport("user32.dll")] static extern bool IsIconic(IntPtr h);
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

function Save-WindowShot([System.Diagnostics.Process]$proc, [string]$path) {
    # Captures only the game window, so other windows and monitors stay out of the picture.
    Assert-Focus $proc
    $r = New-Object CK3Input+RECT
    if (-not [CK3Input]::GetWindowRect($proc.MainWindowHandle, [ref]$r)) { throw "could not read the CK3 window position" }
    $w = $r.Right - $r.Left; $h = $r.Bottom - $r.Top
    if ($w -le 0 -or $h -le 0) { throw "the CK3 window has no visible area (minimized?)" }
    $b = New-Object System.Drawing.Bitmap($w, $h)
    $g = [System.Drawing.Graphics]::FromImage($b)
    $g.CopyFromScreen($r.Left, $r.Top, 0, 0, $b.Size); $b.Save($path); $g.Dispose(); $b.Dispose()
}
