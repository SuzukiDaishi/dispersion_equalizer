# screenshot.ps1 - Build, launch standalone, capture screenshot, kill process
# Usage: .\scripts\screenshot.ps1 [--no-build]

param([switch]$NoBuild)

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot

Set-Location $Root

# --- Build ---
if (-not $NoBuild) {
    Write-Host "Building standalone binary (demo feature)..."
    cargo build --bin dispersion_equalizer_standalone --features demo
    if ($LASTEXITCODE -ne 0) {
        Write-Error "cargo build failed"
        exit 1
    }
    Write-Host "Build succeeded."
}

# --- Kill existing instances ---
Get-Process -Name "dispersion_equalizer_standalone" -ErrorAction SilentlyContinue | ForEach-Object {
    Write-Host "Killing existing process $($_.Id)..."
    $_ | Stop-Process -Force
}
Start-Sleep -Milliseconds 500

# --- Launch ---
$BinPath = Join-Path $Root "target\debug\dispersion_equalizer_standalone.exe"
if (-not (Test-Path $BinPath)) {
    Write-Error "Binary not found: $BinPath  (run without --no-build first)"
    exit 1
}
Write-Host "Launching $BinPath ..."
Start-Process -FilePath $BinPath

Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
using System.Text;
using System.Collections.Generic;
public class Win32Util {
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);
    [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr hWnd, IntPtr hWndInsertAfter, int X, int Y, int cx, int cy, uint uFlags);
    [DllImport("user32.dll")] public static extern void keybd_event(byte bVk, byte bScan, uint dwFlags, IntPtr extra);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT lpRect);
    [DllImport("user32.dll")] public static extern int GetWindowText(IntPtr hWnd, StringBuilder s, int n);
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int X, int Y);
    [DllImport("user32.dll")] public static extern void mouse_event(uint f, int dx, int dy, uint d, IntPtr e);
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr hWnd, uint msg, IntPtr wParam, IntPtr lParam);
    [StructLayout(LayoutKind.Sequential)]
    public struct RECT { public int Left, Top, Right, Bottom; }
    public delegate bool EnumWinProc(IntPtr hWnd, IntPtr lParam);
    [DllImport("user32.dll")] public static extern bool EnumWindows(EnumWinProc proc, IntPtr lParam);
    public static IntPtr FindByTitle(string needle) {
        IntPtr found = IntPtr.Zero;
        EnumWindows((h, l) => {
            if (!IsWindowVisible(h)) return true;
            var sb = new StringBuilder(256);
            GetWindowText(h, sb, 256);
            RECT r; GetWindowRect(h, out r);
            int w = r.Right - r.Left; int hh = r.Bottom - r.Top;
            if (w > 100 && hh > 100 && sb.ToString().Contains(needle)) { found = h; return false; }
            return true;
        }, IntPtr.Zero);
        return found;
    }
    public static void ForceForeground(IntPtr hWnd) {
        keybd_event(0x12, 0, 0, IntPtr.Zero);
        keybd_event(0x12, 0, 0x0002, IntPtr.Zero);
        ShowWindow(hWnd, 9);
        SetForegroundWindow(hWnd);
        SetWindowPos(hWnd, new IntPtr(-1), 0, 0, 0, 0, 0x0003);
        SetWindowPos(hWnd, new IntPtr(-2), 0, 0, 0, 0, 0x0003);
    }
    public static void Click(int x, int y) {
        SetCursorPos(x, y);
        mouse_event(0x0002, 0, 0, 0, IntPtr.Zero);
        mouse_event(0x0004, 0, 0, 0, IntPtr.Zero);
    }
    // Send mouse click directly to window via message — works regardless of focus/z-order
    public static void PostClick(IntPtr hWnd, int clientX, int clientY) {
        IntPtr lp = new IntPtr((clientY << 16) | (clientX & 0xFFFF));
        PostMessage(hWnd, 0x0201, new IntPtr(1), lp); // WM_LBUTTONDOWN
        PostMessage(hWnd, 0x0202, new IntPtr(0), lp); // WM_LBUTTONUP
    }
}
"@

# --- Wait for the plugin GUI window (find by title containing "Dispersion") ---
Write-Host "Waiting for GUI to open..."
Start-Sleep -Seconds 5

$hwnd = [IntPtr]::Zero
for ($i = 0; $i -lt 10; $i++) {
    $h = [Win32Util]::FindByTitle("Dispersion Equalizer")
    if ($h -ne [IntPtr]::Zero) { $hwnd = $h; break }
    Start-Sleep -Seconds 1
}

if ($hwnd -ne [IntPtr]::Zero) {
    Write-Host "Found window handle: $hwnd"

    # Move window to a clear area to avoid overlap with VS Code
    [Win32Util]::SetWindowPos($hwnd, [IntPtr]::Zero, 300, 100, 0, 0, 0x0001) | Out-Null
    Start-Sleep -Milliseconds 300

    [Win32Util]::ForceForeground($hwnd)
    Start-Sleep -Seconds 2

    $wr = New-Object Win32Util+RECT
    [Win32Util]::GetWindowRect($hwnd, [ref]$wr)
    $winW = $wr.Right - $wr.Left
    $winH = $wr.Bottom - $wr.Top
    Write-Host "Window: $($wr.Left),$($wr.Top) ${winW}x${winH}"

    # Use ClientToScreen to get exact screen coordinates of client-area top-left
    Add-Type -TypeDefinition @"
using System; using System.Runtime.InteropServices;
public class ClientHelper {
    [StructLayout(LayoutKind.Sequential)] public struct PT { public int X, Y; }
    [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr h, ref PT p);
}
"@
    # Demo build pre-loads node 0 as Disperser — no click needed.
    # Extra sleep so egui finishes first render.
    Start-Sleep -Seconds 1
} else {
    Write-Host "WARNING: Could not find 'Dispersion Equalizer' window, capturing full screen"
    Start-Sleep -Seconds 2
}

# --- Screenshot via PrintWindow (captures window content regardless of z-order) ---
Add-Type -TypeDefinition @"
using System;
using System.Drawing;
using System.Runtime.InteropServices;
public class PrintWin {
    [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr hWnd, out RECT r);
    [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr hWnd, ref POINT p);
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr hWnd, IntPtr hDC, uint flags);
    [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
    [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X, Y; }
}
"@

$bmp = $null
$g = $null
if ($hwnd -ne [IntPtr]::Zero) {
    $cr = New-Object PrintWin+RECT
    [PrintWin]::GetClientRect($hwnd, [ref]$cr) | Out-Null
    $cw = $cr.Right - $cr.Left; $ch = $cr.Bottom - $cr.Top
    Write-Host "Client area: ${cw}x${ch}"
    if ($cw -gt 100 -and $ch -gt 100) {
        $bmp = New-Object System.Drawing.Bitmap($cw, $ch)
        $g   = [System.Drawing.Graphics]::FromImage($bmp)
        $hdc = $g.GetHdc()
        [PrintWin]::PrintWindow($hwnd, $hdc, 0x00000001) | Out-Null
        $g.ReleaseHdc($hdc)
    }
}
if ($null -eq $bmp) {
    Write-Host "Falling back to full screen capture"
    $capRect = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
    $bmp = New-Object System.Drawing.Bitmap($capRect.Width, $capRect.Height)
    $g   = [System.Drawing.Graphics]::FromImage($bmp)
    $g.CopyFromScreen((New-Object System.Drawing.Point($capRect.X, $capRect.Y)), [System.Drawing.Point]::Empty, $capRect.Size)
}

$outDir = Join-Path $Root "test_artifacts"
if (-not (Test-Path $outDir)) { New-Item -ItemType Directory -Path $outDir | Out-Null }
$timestamp = Get-Date -Format "yyyyMMdd_HHmmss"
$outPath   = Join-Path $outDir "screenshot_$timestamp.png"
$bmp.Save($outPath, [System.Drawing.Imaging.ImageFormat]::Png)
$g.Dispose()
$bmp.Dispose()

Write-Host "Screenshot saved: $outPath"

# --- Kill ---
Stop-Process -Name "dispersion_equalizer_standalone" -Force -ErrorAction SilentlyContinue
Write-Host "Process killed."

$outPath
