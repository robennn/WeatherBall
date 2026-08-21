# Limit the WeatherBall HWND hit-test region so transparent padding does not steal clicks.
param(
  [Parameter(Mandatory = $true)][int]$ParentPid,
  [Parameter(Mandatory = $false)][ValidateSet('orb', 'full')][string]$Mode = 'orb',
  [Parameter(Mandatory = $false)][int]$Width = 160,
  [Parameter(Mandatory = $false)][int]$Height = 204
)

Add-Type -TypeDefinition @"
using System;
using System.Diagnostics;
using System.Runtime.InteropServices;

public static class WindowHitRegion {
  [DllImport("user32.dll")]
  static extern bool EnumWindows(EnumWindowsProc lpEnumFunc, IntPtr lParam);

  [DllImport("user32.dll")]
  static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint lpdwProcessId);

  [DllImport("user32.dll")]
  static extern bool IsWindowVisible(IntPtr hWnd);

  [DllImport("user32.dll")]
  static extern IntPtr GetParent(IntPtr hWnd);

  [DllImport("user32.dll")]
  static extern int SetWindowRgn(IntPtr hWnd, IntPtr hRgn, bool bRedraw);

  [DllImport("gdi32.dll")]
  static extern IntPtr CreateEllipticRgn(int x1, int y1, int x2, int y2);

  [DllImport("gdi32.dll")]
  static extern IntPtr CreateRoundRectRgn(int x1, int y1, int x2, int y2, int w, int h);

  [DllImport("gdi32.dll")]
  static extern bool DeleteObject(IntPtr hObject);

  delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);

  static IntPtr FindTopWindow(int pid) {
    IntPtr found = IntPtr.Zero;
    EnumWindows((hWnd, lParam) => {
      if (!IsWindowVisible(hWnd)) return true;
      if (GetParent(hWnd) != IntPtr.Zero) return true;
      uint wpid;
      GetWindowThreadProcessId(hWnd, out wpid);
      if ((int)wpid == pid) {
        found = hWnd;
        return false;
      }
      return true;
    }, IntPtr.Zero);
    return found;
  }

  public static bool Apply(int pid, string mode, int width, int height) {
    IntPtr hwnd = FindTopWindow(pid);
    if (hwnd == IntPtr.Zero) return false;

    if (string.Equals(mode, "full", StringComparison.OrdinalIgnoreCase)) {
      // NULL region = entire window is hittable again
      SetWindowRgn(hwnd, IntPtr.Zero, true);
      return true;
    }

    // Orb sits at the bottom of the compact window (flex-end + padding).
    // Keep a rounded hit target around the visible ball only.
    int ball = 100;
    int padBottom = 14;
    int cx = width / 2;
    int cy = height - padBottom - ball / 2;
    int r = 58; // ball radius + soft glow margin
    int x1 = Math.Max(0, cx - r);
    int y1 = Math.Max(0, cy - r);
    int x2 = Math.Min(width, cx + r);
    int y2 = Math.Min(height, cy + r);

    IntPtr rgn = CreateEllipticRgn(x1, y1, x2, y2);
    if (rgn == IntPtr.Zero) {
      rgn = CreateRoundRectRgn(x1, y1, x2, y2, r, r);
    }
    if (rgn == IntPtr.Zero) return false;

    // SetWindowRgn takes ownership of the region
    int ok = SetWindowRgn(hwnd, rgn, true);
    return ok != 0;
  }
}
"@

[WindowHitRegion]::Apply($ParentPid, $Mode, $Width, $Height) | Out-Null
