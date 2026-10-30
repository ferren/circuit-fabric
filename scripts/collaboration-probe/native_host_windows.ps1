# Export dialogs may not be returned by MainWindowHandle/GetLastActivePopup.
# PID and window titles are not native project/document identities.
if (!('CFNativeWindows' -as [type])) {
  Add-Type -TypeDefinition @'
using System;
using System.Text;
using System.Collections.Generic;
using System.Runtime.InteropServices;
public class CFNativeWindows {
  public class Window { public long Handle; public uint ProcessId; public string Title; }
  public delegate bool Callback(IntPtr h,IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumWindows(Callback c,IntPtr l);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h,out uint p);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr h,StringBuilder t,int n);
  public static Window[] List(uint processId) {
    var windows = new List<Window>();
    EnumWindows((h,l) => {
      uint pid; GetWindowThreadProcessId(h,out pid);
      if (pid == processId && IsWindowVisible(h)) {
        var title = new StringBuilder(2048); GetWindowText(h,title,title.Capacity);
        if (title.Length > 0) windows.Add(new Window { Handle=h.ToInt64(), ProcessId=pid, Title=title.ToString() });
      }
      return true;
    },IntPtr.Zero);
    return windows.ToArray();
  }
}
'@
}
function Get-CFNativeWindows {
  Get-Process lceda-pro | ForEach-Object { [CFNativeWindows]::List($_.Id) }
}
