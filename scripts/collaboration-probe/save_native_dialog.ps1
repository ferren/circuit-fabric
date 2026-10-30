param([Parameter(Mandatory=$true)][string]$Destination, [long]$WindowHandle=0)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'native_host_windows.ps1')
Add-Type -AssemblyName System.Windows.Forms
Add-Type -TypeDefinition @'
using System;
using System.Text;
using System.Runtime.InteropServices;
public class CFNativeSaveUi {
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern IntPtr GetLastActivePopup(IntPtr h);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h,out uint p);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr h,StringBuilder t,int n);
}
'@
$targetPath = [IO.Path]::GetFullPath($Destination)
$workspacePrefix = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..')) + [IO.Path]::DirectorySeparatorChar
if (!$targetPath.StartsWith($workspacePrefix,[StringComparison]::OrdinalIgnoreCase)) { throw 'Destination must be inside this workspace' }
if (Test-Path -LiteralPath $targetPath) { throw 'Refusing to overwrite an existing export' }
if (!(Test-Path -LiteralPath ([IO.Path]::GetDirectoryName($targetPath)))) { throw 'Destination directory missing' }
$exports = @(Get-CFNativeWindows | Where-Object { $_.Title.StartsWith('blob:https://client/') -and (!$WindowHandle -or $_.Handle -eq $WindowHandle) })
if ($exports.Count -ne 1) { throw "Expected one EDA export dialog; found $($exports.Count)" }
$popup = [IntPtr]$exports[0].Handle
$caption = New-Object Text.StringBuilder 1024
[CFNativeSaveUi]::GetWindowText($popup,$caption,$caption.Capacity) | Out-Null
if (!$caption.ToString().StartsWith('blob:https://client/')) { throw "Unexpected export dialog: $caption" }
[System.Windows.Forms.SendKeys]::SendWait('%')
[CFNativeSaveUi]::SetForegroundWindow($popup) | Out-Null
Start-Sleep -Milliseconds 200
$foregroundPid = [uint32]0
[CFNativeSaveUi]::GetWindowThreadProcessId([CFNativeSaveUi]::GetForegroundWindow(),[ref]$foregroundPid) | Out-Null
if ($foregroundPid -ne $exports[0].ProcessId -or [CFNativeSaveUi]::GetForegroundWindow() -ne $popup) { throw 'EDA export dialog focus not acquired' }
$priorClipboard = [System.Windows.Forms.Clipboard]::GetDataObject()
try {
  [System.Windows.Forms.Clipboard]::SetText($targetPath)
  [System.Windows.Forms.SendKeys]::SendWait('%n')
  [System.Windows.Forms.SendKeys]::SendWait('^a')
  [System.Windows.Forms.SendKeys]::SendWait('^v')
  [System.Windows.Forms.SendKeys]::SendWait('{ENTER}')
  Start-Sleep -Milliseconds 900
} finally {
  if ($priorClipboard) { [System.Windows.Forms.Clipboard]::SetDataObject($priorClipboard,$true) }
}
if (!(Test-Path -LiteralPath $targetPath)) { throw 'Export not observed; inspect the dialog before retrying' }
Get-Item -LiteralPath $targetPath | Select-Object FullName,Length
