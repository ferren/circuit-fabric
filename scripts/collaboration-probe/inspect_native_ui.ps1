param([Parameter(Mandatory=$true)][string]$OutPath, [int]$ClickX=-1, [int]$ClickY=-1, [long]$WindowHandle=0)
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'native_host_windows.ps1')
Add-Type -AssemblyName System.Windows.Forms
Add-Type -TypeDefinition @'
using System; using System.Runtime.InteropServices;
public class CFInspectUi {
 [StructLayout(LayoutKind.Sequential)] public struct R {public int L,T,Right,Bottom;}
 [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
 [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h,int n);
 [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h,out R r);
 [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
 [DllImport("user32.dll")] public static extern bool SetCursorPos(int x,int y);
 [DllImport("user32.dll")] public static extern void mouse_event(uint f,uint x,uint y,uint d,UIntPtr e);
}
'@
$windows=@(Get-CFNativeWindows | Where-Object { $_.Title -match 'V4\.' -and (!$WindowHandle -or $_.Handle -eq $WindowHandle) })
if ($windows.Count -ne 1) { throw 'Select exactly one inspected EDA window' }
$handle=[IntPtr]$windows[0].Handle
[System.Windows.Forms.SendKeys]::SendWait('%')
[CFInspectUi]::ShowWindow($handle,9) | Out-Null
[CFInspectUi]::SetForegroundWindow($handle) | Out-Null
Start-Sleep -Milliseconds 250
if ([CFInspectUi]::GetForegroundWindow() -ne $handle) { throw 'EDA focus unavailable' }
$bounds=New-Object CFInspectUi+R
[CFInspectUi]::GetWindowRect($handle,[ref]$bounds) | Out-Null
if ($ClickX -ge 0) {
 if ($ClickX -ge ($bounds.Right-$bounds.L) -or $ClickY -lt 0 -or $ClickY -ge ($bounds.Bottom-$bounds.T)) { throw 'Click outside inspected window' }
 [CFInspectUi]::SetCursorPos(($bounds.L+$ClickX),($bounds.T+$ClickY)) | Out-Null
 [CFInspectUi]::mouse_event(2,0,0,0,[UIntPtr]::Zero)
 [CFInspectUi]::mouse_event(4,0,0,0,[UIntPtr]::Zero)
 Start-Sleep -Milliseconds 400
}
& (Join-Path $PSScriptRoot '../jlc-acceptance/capture.ps1') -OutPath $OutPath -Left $bounds.L -Top $bounds.T -Width ($bounds.Right-$bounds.L) -Height ($bounds.Bottom-$bounds.T)
Get-CFNativeWindows | ConvertTo-Json -Compress
