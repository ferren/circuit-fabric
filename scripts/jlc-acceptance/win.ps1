param(
    [Parameter(Mandatory=$true)][string]$Action,
    [string]$Title = "嘉立创EDA",
    [int]$X = 0,
    [int]$Y = 0,
    [string]$Text = "",
    [string]$Key = ""
)
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing

$src = @"
using System;
using System.Runtime.InteropServices;
public class WinApi {
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] public static extern void mouse_event(uint dwFlags, uint dx, uint dy, uint dwData, int dwExtraInfo);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT rect);
    public struct RECT { public int Left; public int Top; public int Right; public int Bottom; }
}
"@
Add-Type -TypeDefinition $src -ReferencedAssemblies System.Windows.Forms,System.Drawing

function Find-MainWindow([string]$nameOrTitle) {
    $procs = Get-Process -ErrorAction SilentlyContinue | Where-Object {
        $_.MainWindowHandle -ne 0 -and (
            $_.ProcessName -eq $nameOrTitle -or $_.MainWindowTitle -like "*$nameOrTitle*"
        )
    }
    return $procs
}

function Minimize-Others([string]$nameOrTitle) {
    Get-Process -ErrorAction SilentlyContinue | Where-Object {
        $_.MainWindowHandle -ne 0 -and $_.ProcessName -ne $nameOrTitle -and
        $_.ProcessName -notin @("explorer", "TextInputHost", "KimiCU", "kimi-cu")
    } | ForEach-Object {
        [WinApi]::ShowWindow($_.MainWindowHandle, 6) | Out-Null  # SW_MINIMIZE
    }
}

switch ($Action) {
    "focus" {
        Minimize-Others $Title
        $p = Find-MainWindow $Title | Select-Object -First 1
        if (-not $p) { Write-Error "window not found: $Title"; exit 1 }
        [WinApi]::ShowWindow($p.MainWindowHandle, 9) | Out-Null  # SW_RESTORE
        [WinApi]::ShowWindow($p.MainWindowHandle, 3) | Out-Null  # SW_MAXIMIZE
        [WinApi]::SetForegroundWindow($p.MainWindowHandle) | Out-Null
        Start-Sleep -Milliseconds 600
        $r = New-Object WinApi+RECT
        [WinApi]::GetWindowRect($p.MainWindowHandle, [ref]$r) | Out-Null
        Write-Output ("RECT {0},{1} {2}x{3}" -f $r.Left, $r.Top, ($r.Right-$r.Left), ($r.Bottom-$r.Top))
    }
    "rect" {
        $p = Find-MainWindow $Title | Select-Object -First 1
        if (-not $p) { Write-Error "window not found: $Title"; exit 1 }
        $r = New-Object WinApi+RECT
        [WinApi]::GetWindowRect($p.MainWindowHandle, [ref]$r) | Out-Null
        Write-Output ("RECT {0},{1} {2}x{3}" -f $r.Left, $r.Top, ($r.Right-$r.Left), ($r.Bottom-$r.Top))
    }
    "click" {
        [WinApi]::SetCursorPos($X, $Y) | Out-Null
        Start-Sleep -Milliseconds 120
        [WinApi]::mouse_event(0x0002, 0, 0, 0, 0)  # LEFTDOWN
        Start-Sleep -Milliseconds 60
        [WinApi]::mouse_event(0x0004, 0, 0, 0, 0)  # LEFTUP
        Start-Sleep -Milliseconds 300
        Write-Output "clicked $X,$Y"
    }
    "wheel" {
        # Text = wheel delta (positive up, negative down, 120 per notch); X,Y = position
        [WinApi]::SetCursorPos($X, $Y) | Out-Null
        Start-Sleep -Milliseconds 100
        $delta = [uint32][bitconverter]::ToUInt32([bitconverter]::GetBytes([int]$Text), 0)
        [WinApi]::mouse_event(0x0800, 0, 0, $delta, 0)  # MOUSEEVENTF_WHEEL
        Start-Sleep -Milliseconds 400
        Write-Output "wheel $Text at $X,$Y"
    }
    "type" {
        [System.Windows.Forms.Clipboard]::SetText($Text)
        Start-Sleep -Milliseconds 200
        [System.Windows.Forms.SendKeys]::SendWait("^v")
        Start-Sleep -Milliseconds 300
        Write-Output "typed $($Text.Length) chars"
    }
    "key" {
        [System.Windows.Forms.SendKeys]::SendWait($Key)
        Start-Sleep -Milliseconds 300
        Write-Output "key $Key"
    }
    "close" {
        $p = Find-MainWindow $Title | Select-Object -First 1
        if (-not $p) { Write-Error "window not found: $Title"; exit 1 }
        $p.CloseMainWindow() | Out-Null
        Write-Output "close sent"
    }
}
