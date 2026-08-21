# Hide WeatherBall windows from the Windows taskbar (by parent PID preferred).
param(
  [Parameter(Mandatory = $false)][string]$Title = '',
  [Parameter(Mandatory = $false)][string]$ProcessName = '',
  [Parameter(Mandatory = $false)][int]$ParentPid = 0
)

Add-Type -TypeDefinition @"
using System;
using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Text;

public static class TaskbarHide {
  const int GWL_EXSTYLE = -20;
  const int WS_EX_APPWINDOW = 0x00040000;
  const int WS_EX_TOOLWINDOW = 0x00000080;
  const int SW_HIDE = 0;
  const int SW_SHOWNA = 8;

  [DllImport("user32.dll")]
  static extern bool EnumWindows(EnumWindowsProc lpEnumFunc, IntPtr lParam);

  [DllImport("user32.dll")]
  static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint lpdwProcessId);

  [DllImport("user32.dll", CharSet = CharSet.Unicode)]
  static extern int GetWindowText(IntPtr hWnd, StringBuilder lpString, int nMaxCount);

  [DllImport("user32.dll")]
  static extern bool IsWindowVisible(IntPtr hWnd);

  [DllImport("user32.dll")]
  static extern IntPtr GetParent(IntPtr hWnd);

  [DllImport("user32.dll", EntryPoint = "GetWindowLongPtrW", CharSet = CharSet.Unicode)]
  static extern IntPtr GetWindowLongPtr64(IntPtr hWnd, int nIndex);

  [DllImport("user32.dll", EntryPoint = "SetWindowLongPtrW", CharSet = CharSet.Unicode)]
  static extern IntPtr SetWindowLongPtr64(IntPtr hWnd, int nIndex, IntPtr dwNewLong);

  [DllImport("user32.dll", EntryPoint = "GetWindowLongW", CharSet = CharSet.Unicode)]
  static extern int GetWindowLong32(IntPtr hWnd, int nIndex);

  [DllImport("user32.dll", EntryPoint = "SetWindowLongW", CharSet = CharSet.Unicode)]
  static extern int SetWindowLong32(IntPtr hWnd, int nIndex, int dwNewLong);

  [DllImport("user32.dll")]
  static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);

  [ComImport, Guid("56FDF342-FD6D-11d0-958A-006097C9A090")]
  [InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
  interface ITaskbarList {
    void HrInit();
    void AddTab(IntPtr hwnd);
    void DeleteTab(IntPtr hwnd);
    void ActivateTab(IntPtr hwnd);
    void SetActiveAlt(IntPtr hwnd);
  }

  [ComImport, Guid("56FDF344-FD6D-11d0-958A-006097C9A090")]
  class CTaskbarList { }

  delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);

  static IntPtr GetExStyle(IntPtr hWnd) {
    if (IntPtr.Size == 8) return GetWindowLongPtr64(hWnd, GWL_EXSTYLE);
    return new IntPtr(GetWindowLong32(hWnd, GWL_EXSTYLE));
  }

  static void SetExStyle(IntPtr hWnd, IntPtr style) {
    if (IntPtr.Size == 8) SetWindowLongPtr64(hWnd, GWL_EXSTYLE, style);
    else SetWindowLong32(hWnd, GWL_EXSTYLE, style.ToInt32());
  }

  static void ApplyOne(IntPtr hWnd) {
    long style = GetExStyle(hWnd).ToInt64();
    long next = (style | WS_EX_TOOLWINDOW) & ~WS_EX_APPWINDOW;
    if (next != style) {
      SetExStyle(hWnd, new IntPtr(next));
      ShowWindow(hWnd, SW_HIDE);
      ShowWindow(hWnd, SW_SHOWNA);
    }
    try {
      ITaskbarList list = (ITaskbarList)new CTaskbarList();
      list.HrInit();
      list.DeleteTab(hWnd);
    } catch { }
  }

  public static int Apply(int parentPid, string title, string processName) {
    int applied = 0;
    EnumWindows((hWnd, lParam) => {
      if (GetParent(hWnd) != IntPtr.Zero) return true;
      if (!IsWindowVisible(hWnd)) return true;

      uint pid;
      GetWindowThreadProcessId(hWnd, out pid);
      if (pid == 0) return true;

      bool match = false;
      if (parentPid > 0 && (int)pid == parentPid) match = true;

      if (!match && !string.IsNullOrEmpty(processName)) {
        try {
          var p = Process.GetProcessById((int)pid);
          if (string.Equals(p.ProcessName, processName, StringComparison.OrdinalIgnoreCase)) {
            match = true;
          }
        } catch { }
      }

      if (!match && !string.IsNullOrEmpty(title)) {
        var sb = new StringBuilder(512);
        GetWindowText(hWnd, sb, sb.Capacity);
        if (sb.ToString() == title) match = true;
      }

      if (match) {
        ApplyOne(hWnd);
        applied++;
      }
      return true;
    }, IntPtr.Zero);
    return applied;
  }
}
"@

[TaskbarHide]::Apply($ParentPid, $Title, $ProcessName) | Out-Null
