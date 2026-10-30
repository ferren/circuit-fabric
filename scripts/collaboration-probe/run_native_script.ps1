param([Parameter(Mandatory=$true)][string]$Source, [string]$WindowTitle, [switch]$DismissError)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'native_host_windows.ps1')
Add-Type -AssemblyName System.Windows.Forms
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public class CFNativeScriptUi {
  [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left,Top,Right,Bottom; }
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h,int n);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h,out Rect r);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h,out uint p);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x,int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f,uint x,uint y,uint d,UIntPtr e);
}
'@
$projectWindows = @(Get-CFNativeWindows | Where-Object {
  $_.Title -match 'V4\.' -and (!$WindowTitle -or $_.Title -eq $WindowTitle)
})
if ($projectWindows.Count -ne 1) { throw "Expected one matching EDA project window; found $($projectWindows.Count). Specify -WindowTitle." }
$edaWindow = [IntPtr]$projectWindows[0].Handle
[System.Windows.Forms.SendKeys]::SendWait('%')
[CFNativeScriptUi]::ShowWindow($edaWindow,9) | Out-Null
[CFNativeScriptUi]::SetForegroundWindow($edaWindow) | Out-Null
Start-Sleep -Milliseconds 250
$foregroundPid = [uint32]0
[CFNativeScriptUi]::GetWindowThreadProcessId([CFNativeScriptUi]::GetForegroundWindow(),[ref]$foregroundPid) | Out-Null
if ($foregroundPid -ne $projectWindows[0].ProcessId -or [CFNativeScriptUi]::GetForegroundWindow() -ne $edaWindow) { throw 'EDA focus not acquired' }
$bounds = New-Object CFNativeScriptUi+Rect
[CFNativeScriptUi]::GetWindowRect($edaWindow,[ref]$bounds) | Out-Null
$width = $bounds.Right-$bounds.Left
$height = $bounds.Bottom-$bounds.Top
if ($width -lt 1280 -or $width -gt 1300 -or $height -lt 720 -or $height -gt 740) {
  throw "Expected the inspected EDA window (1280x720 client plus borders); actual $width x $height"
}
if ($DismissError) { [System.Windows.Forms.SendKeys]::SendWait('{ESC}'); Start-Sleep -Milliseconds 200 }
$scriptText = [IO.File]::ReadAllText([IO.Path]::GetFullPath($Source))
$priorClipboard = [System.Windows.Forms.Clipboard]::GetDataObject()
try {
  [System.Windows.Forms.Clipboard]::SetText($scriptText)
  [CFNativeScriptUi]::SetCursorPos(($bounds.Left+630),($bounds.Top+320)) | Out-Null
  [CFNativeScriptUi]::mouse_event(2,0,0,0,[UIntPtr]::Zero)
  [CFNativeScriptUi]::mouse_event(4,0,0,0,[UIntPtr]::Zero)
  [System.Windows.Forms.SendKeys]::SendWait('^a')
  [System.Windows.Forms.SendKeys]::SendWait('^v')
  Start-Sleep -Milliseconds 500
  [System.Windows.Forms.SendKeys]::SendWait('{F9}')
  Start-Sleep -Milliseconds 1000
} finally {
  if ($priorClipboard) { [System.Windows.Forms.Clipboard]::SetDataObject($priorClipboard,$true) }
}
