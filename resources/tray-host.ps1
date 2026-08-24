param(
  [Parameter(Mandatory = $true)][string]$IconPath,
  [Parameter(Mandatory = $true)][string]$CmdPath,
  [Parameter(Mandatory = $true)][string]$StatePath,
  [Parameter(Mandatory = $true)][string]$PidPath,
  [Parameter(Mandatory = $true)][string]$ExitPath,
  [Parameter(Mandatory = $false)][int]$ParentPid = 0
)

# Keep this file ASCII-only. Chinese labels come from tray-state.json (UTF-8).
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing

Add-Type -TypeDefinition @"
using System;
using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Text;

public static class WeatherBallWin {
  const int GWL_EXSTYLE = -20;
  const int WS_EX_APPWINDOW = 0x00040000;
  const int WS_EX_TOOLWINDOW = 0x00000080;
  const int WS_EX_TRANSPARENT = 0x00000020;
  const int WS_EX_LAYERED = 0x00080000;
  const int SW_HIDE = 0;
  const int SW_SHOWNA = 8;
  const int VK_LBUTTON = 0x01;

  [DllImport("user32.dll")]
  static extern bool EnumWindows(EnumWindowsProc lpEnumFunc, IntPtr lParam);

  [DllImport("user32.dll")]
  static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint lpdwProcessId);

  [DllImport("user32.dll")]
  static extern bool IsWindowVisible(IntPtr hWnd);

  [DllImport("user32.dll")]
  static extern bool IsWindow(IntPtr hWnd);

  [DllImport("user32.dll")]
  static extern IntPtr GetParent(IntPtr hWnd);

  [DllImport("user32.dll", EntryPoint = "GetWindowLongPtrW")]
  static extern IntPtr GetWindowLongPtr64(IntPtr hWnd, int nIndex);

  [DllImport("user32.dll", EntryPoint = "SetWindowLongPtrW")]
  static extern IntPtr SetWindowLongPtr64(IntPtr hWnd, int nIndex, IntPtr dwNewLong);

  [DllImport("user32.dll", EntryPoint = "GetWindowLongW")]
  static extern int GetWindowLong32(IntPtr hWnd, int nIndex);

  [DllImport("user32.dll", EntryPoint = "SetWindowLongW")]
  static extern int SetWindowLong32(IntPtr hWnd, int nIndex, int dwNewLong);

  [DllImport("user32.dll")]
  static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);

  [DllImport("user32.dll")]
  static extern bool GetWindowRect(IntPtr hWnd, out RECT lpRect);

  [DllImport("user32.dll")]
  static extern bool GetCursorPos(out POINT lpPoint);

  [DllImport("user32.dll")]
  static extern short GetAsyncKeyState(int vKey);

  [DllImport("user32.dll")]
  static extern int SetWindowRgn(IntPtr hWnd, IntPtr hRgn, bool bRedraw);

  [DllImport("user32.dll")]
  static extern IntPtr GetForegroundWindow();

  [DllImport("user32.dll")]
  static extern IntPtr MonitorFromWindow(IntPtr hwnd, uint dwFlags);

  [DllImport("user32.dll", CharSet = CharSet.Auto)]
  static extern bool GetMonitorInfo(IntPtr hMonitor, ref MONITORINFO lpmi);

  [DllImport("user32.dll", CharSet = CharSet.Unicode)]
  static extern int GetClassName(IntPtr hWnd, StringBuilder lpClassName, int nMaxCount);

  [DllImport("user32.dll", CharSet = CharSet.Unicode)]
  static extern IntPtr FindWindow(string lpClassName, string lpWindowName);

  const uint MONITOR_DEFAULTTONEAREST = 2;

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

  [StructLayout(LayoutKind.Sequential)]
  struct RECT { public int Left, Top, Right, Bottom; }

  [StructLayout(LayoutKind.Sequential)]
  struct POINT { public int X, Y; }

  [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Auto)]
  struct MONITORINFO {
    public int cbSize;
    public RECT rcMonitor;
    public RECT rcWork;
    public uint dwFlags;
  }

  static bool _regionCleared;
  static bool _clickThrough;

  static IntPtr GetExStyle(IntPtr hWnd) {
    if (IntPtr.Size == 8) return GetWindowLongPtr64(hWnd, GWL_EXSTYLE);
    return new IntPtr(GetWindowLong32(hWnd, GWL_EXSTYLE));
  }

  static void SetExStyle(IntPtr hWnd, IntPtr style) {
    if (IntPtr.Size == 8) SetWindowLongPtr64(hWnd, GWL_EXSTYLE, style);
    else SetWindowLong32(hWnd, GWL_EXSTYLE, style.ToInt32());
  }

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

  static void HideFromTaskbar(IntPtr hWnd) {
    if (hWnd == IntPtr.Zero || !IsWindow(hWnd)) return;
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

  public static void HideProcessFromTaskbar(int pid) {
    if (pid <= 0) return;
    EnumWindows((hWnd, lParam) => {
      uint wpid;
      GetWindowThreadProcessId(hWnd, out wpid);
      if ((int)wpid != pid) return true;
      if (GetParent(hWnd) != IntPtr.Zero) return true;
      if (!IsWindowVisible(hWnd)) return true;
      HideFromTaskbar(hWnd);
      return true;
    }, IntPtr.Zero);
  }

  public static bool ProcessAlive(int pid) {
    if (pid <= 0) return true;
    try {
      Process p = Process.GetProcessById(pid);
      return p != null && !p.HasExited;
    } catch {
      return false;
    }
  }

  public static bool TrayReady() {
    return FindWindow("Shell_TrayWnd", null) != IntPtr.Zero;
  }

  static void SetClickThrough(IntPtr hWnd, bool enable) {
    if (hWnd == IntPtr.Zero) return;
    long style = GetExStyle(hWnd).ToInt64();
    long next = style | WS_EX_LAYERED;
    if (enable) next |= WS_EX_TRANSPARENT;
    else next &= ~WS_EX_TRANSPARENT;
    if (next != style) SetExStyle(hWnd, new IntPtr(next));
    _clickThrough = enable;
  }

  // Pass clicks through empty chrome; orb (or full detail window) still receives input.
  // Does not clip painting (unlike SetWindowRgn).
  public static void UpdateClickThrough(int pid) {
    if (pid <= 0) return;
    IntPtr hwnd = FindTopWindow(pid);
    if (hwnd == IntPtr.Zero) return;

    if (!_regionCleared) {
      SetWindowRgn(hwnd, IntPtr.Zero, true);
      _regionCleared = true;
    }

    RECT wr;
    if (!GetWindowRect(hwnd, out wr)) return;
    int width = wr.Right - wr.Left;
    int height = wr.Bottom - wr.Top;
    if (width <= 0 || height <= 0) return;

    POINT pt;
    GetCursorPos(out pt);

    bool leftDown = (GetAsyncKeyState(VK_LBUTTON) & 0x8000) != 0;
    // Keep capture while dragging so the window does not go click-through mid-drag
    if (leftDown && !_clickThrough) {
      SetClickThrough(hwnd, false);
      return;
    }

    bool wantReceive;
    if (height > 280) {
      wantReceive = true;
    } else {
      // Compact layout: ball 100px, padBottom 14 (see WeatherBall.vue)
      const int ball = 100;
      const int padBottom = 14;
      double cx = wr.Left + width * 0.5;
      double cy = wr.Bottom - padBottom - ball * 0.5;
      double r = 54.0;
      double dx = pt.X - cx;
      double dy = pt.Y - cy;
      wantReceive = (dx * dx + dy * dy) <= (r * r);
    }

    SetClickThrough(hwnd, !wantReceive);
  }

  /// <summary>
  /// True when another app's foreground window covers ~entire monitor
  /// (fullscreen video / games). Maximized desktop apps use the work area and won't match.
  /// </summary>
  public static bool IsForeignFullscreen(int selfPid) {
    IntPtr fg = GetForegroundWindow();
    if (fg == IntPtr.Zero) return false;

    uint pid;
    GetWindowThreadProcessId(fg, out pid);
    if (selfPid > 0 && (int)pid == selfPid) return false;

    StringBuilder sb = new StringBuilder(96);
    GetClassName(fg, sb, sb.Capacity);
    string cls = sb.ToString();
    if (cls == "Progman" || cls == "WorkerW" || cls == "Shell_TrayWnd" ||
        cls == "XamlExplorerHostIslandWindow" || cls == "NotifyIconOverflowWindow") {
      return false;
    }

    RECT wr;
    if (!GetWindowRect(fg, out wr)) return false;

    IntPtr mon = MonitorFromWindow(fg, MONITOR_DEFAULTTONEAREST);
    if (mon == IntPtr.Zero) return false;

    MONITORINFO mi = new MONITORINFO();
    mi.cbSize = Marshal.SizeOf(typeof(MONITORINFO));
    if (!GetMonitorInfo(mon, ref mi)) return false;

    RECT mr = mi.rcMonitor;
    int mw = mr.Right - mr.Left;
    int mh = mr.Bottom - mr.Top;
    if (mw < 320 || mh < 240) return false;

    int ww = wr.Right - wr.Left;
    int wh = wr.Bottom - wr.Top;
    if (ww <= 0 || wh <= 0) return false;

    // Fullscreen apps cover the monitor; maximized windows usually stop at the taskbar (rcWork)
    if ((long)ww * wh < (long)(mw * mh * 0.97)) return false;
    if (wr.Left > mr.Left + 8 || wr.Top > mr.Top + 8) return false;
    if (wr.Right < mr.Right - 8 || wr.Bottom < mr.Bottom - 8) return false;
    return true;
  }
}
"@

function Write-Cmd([string]$cmd) {
  $dir = Split-Path -Parent $CmdPath
  if ($dir -and -not (Test-Path -LiteralPath $dir)) {
    New-Item -ItemType Directory -Path $dir -Force | Out-Null
  }
  # Do not clobber a pending command
  if (Test-Path -LiteralPath $CmdPath) { return }
  [System.IO.File]::WriteAllText($CmdPath, $cmd.Trim(), [System.Text.UTF8Encoding]::new($false))
}

function Read-State {
  $state = @{
    visible     = $true
    alwaysOnTop = $true
    openAtLogin = $false
    tip         = 'WeatherBall'
    hideText    = 'Hide'
    showText    = 'Show'
    refreshText = 'Refresh'
    topOnText   = 'Always on top'
    topOffText  = 'Always on top'
    autoOnText  = 'Open at login'
    autoOffText = 'Open at login'
    quitText    = 'Quit'
  }
  try {
    if (Test-Path -LiteralPath $StatePath) {
      $bytes = [System.IO.File]::ReadAllBytes($StatePath)
      $raw = [System.Text.Encoding]::UTF8.GetString($bytes)
      $j = $raw | ConvertFrom-Json
      if ($null -ne $j.visible) { $state.visible = [bool]$j.visible }
      if ($null -ne $j.alwaysOnTop) { $state.alwaysOnTop = [bool]$j.alwaysOnTop }
      if ($null -ne $j.openAtLogin) { $state.openAtLogin = [bool]$j.openAtLogin }
      if ($j.tip) { $state.tip = [string]$j.tip }
      if ($j.hideText) { $state.hideText = [string]$j.hideText }
      if ($j.showText) { $state.showText = [string]$j.showText }
      if ($j.refreshText) { $state.refreshText = [string]$j.refreshText }
      if ($j.topOnText) { $state.topOnText = [string]$j.topOnText }
      if ($j.topOffText) { $state.topOffText = [string]$j.topOffText }
      if ($j.autoOnText) { $state.autoOnText = [string]$j.autoOnText }
      if ($j.autoOffText) { $state.autoOffText = [string]$j.autoOffText }
      if ($j.quitText) { $state.quitText = [string]$j.quitText }
    }
  } catch { }
  return $state
}

function Stop-Self {
  try { $timer.Stop() } catch { }
  try {
    $notify.Visible = $false
    $notify.Dispose()
  } catch { }
  try { if ($icon) { $icon.Dispose() } } catch { }
  try { if ($bitmap) { $bitmap.Dispose() } } catch { }
  Remove-Item -LiteralPath $PidPath -Force -ErrorAction SilentlyContinue
  Remove-Item -LiteralPath $ExitPath -Force -ErrorAction SilentlyContinue
  [System.Windows.Forms.Application]::Exit()
}

$mutex = New-Object System.Threading.Mutex($false, 'Local\WeatherBallTrayHost')
$hasMutex = $false
for ($i = 0; $i -lt 20; $i++) {
  try {
    if ($mutex.WaitOne(0)) {
      $hasMutex = $true
      break
    }
  } catch { }
  try {
    $dir = Split-Path -Parent $ExitPath
    if ($dir -and -not (Test-Path -LiteralPath $dir)) {
      New-Item -ItemType Directory -Path $dir -Force | Out-Null
    }
    [System.IO.File]::WriteAllText($ExitPath, '1', [System.Text.UTF8Encoding]::new($false))
  } catch { }
  try {
    if (Test-Path -LiteralPath $PidPath) {
      $old = [int]([System.IO.File]::ReadAllText($PidPath).Trim())
      if ($old -gt 0 -and $old -ne $PID) {
        Stop-Process -Id $old -Force -ErrorAction SilentlyContinue
      }
    }
  } catch { }
  Start-Sleep -Milliseconds 150
}
if (-not $hasMutex) {
  exit 0
}

# Wait for the Windows shell tray (critical after logon / autostart)
for ($i = 0; $i -lt 90; $i++) {
  try {
    if (Test-Path -LiteralPath $ExitPath) { exit 0 }
  } catch { }
  try {
    if ([WeatherBallWin]::TrayReady()) { break }
  } catch { }
  Start-Sleep -Seconds 1
}
Start-Sleep -Milliseconds 800

$bitmap = $null
$icon = $null
try {
  $bitmap = [System.Drawing.Bitmap]::FromFile($IconPath)
  $icon = [System.Drawing.Icon]::FromHandle($bitmap.GetHicon())
} catch {
  $icon = [System.Drawing.SystemIcons]::Application
}

$notify = New-Object System.Windows.Forms.NotifyIcon
$notify.Icon = $icon
$notify.Visible = $true

function Refresh-TrayIcon {
  try {
    $notify.Visible = $false
    $notify.Visible = $true
  } catch { }
}

# Recreate tray icon when Explorer restarts (TaskbarCreated)
Add-Type -TypeDefinition @"
using System;
using System.Windows.Forms;
using System.Runtime.InteropServices;

public class TaskbarCreatedWatcher : NativeWindow {
  public Action OnTaskbarCreated;
  readonly uint _msg;
  public TaskbarCreatedWatcher() {
    _msg = RegisterWindowMessage("TaskbarCreated");
    CreateHandle(new CreateParams());
  }
  [DllImport("user32.dll", CharSet = CharSet.Unicode)]
  static extern uint RegisterWindowMessage(string lpString);
  protected override void WndProc(ref Message m) {
    if (m.Msg == (int)_msg && OnTaskbarCreated != null) OnTaskbarCreated();
    base.WndProc(ref m);
  }
}
"@ -ReferencedAssemblies System.Windows.Forms

$script:trayWatcher = $null
try {
  $script:trayWatcher = New-Object TaskbarCreatedWatcher
  $script:trayWatcher.OnTaskbarCreated = [Action]{ Refresh-TrayIcon }
} catch { }

$menu = New-Object System.Windows.Forms.ContextMenuStrip
$itemToggle = New-Object System.Windows.Forms.ToolStripMenuItem
$itemRefresh = New-Object System.Windows.Forms.ToolStripMenuItem
$itemTop = New-Object System.Windows.Forms.ToolStripMenuItem
$itemAutostart = New-Object System.Windows.Forms.ToolStripMenuItem
$itemQuit = New-Object System.Windows.Forms.ToolStripMenuItem

function Sync-Menu {
  $s = Read-State
  $notify.Text = $s.tip
  $itemToggle.Text = $(if ($s.visible) { $s.hideText } else { $s.showText })
  $itemRefresh.Text = $s.refreshText
  $itemTop.Text = $(if ($s.alwaysOnTop) { $s.topOnText } else { $s.topOffText })
  $itemAutostart.Text = $(if ($s.openAtLogin) { $s.autoOnText } else { $s.autoOffText })
  $itemQuit.Text = $s.quitText
}

Sync-Menu
[void]$menu.Items.Add($itemToggle)
[void]$menu.Items.Add($itemRefresh)
[void]$menu.Items.Add((New-Object System.Windows.Forms.ToolStripSeparator))
[void]$menu.Items.Add($itemTop)
[void]$menu.Items.Add($itemAutostart)
[void]$menu.Items.Add((New-Object System.Windows.Forms.ToolStripSeparator))
[void]$menu.Items.Add($itemQuit)
$notify.ContextMenuStrip = $menu

$itemToggle.add_Click({ Write-Cmd 'TOGGLE_VIS' })
$itemRefresh.add_Click({ Write-Cmd 'REFRESH' })
$itemTop.add_Click({ Write-Cmd 'TOP' })
$itemAutostart.add_Click({ Write-Cmd 'AUTOSTART' })
$itemQuit.add_Click({ Write-Cmd 'QUIT' })

$notify.add_MouseClick({
  param($sender, $e)
  if ($e.Button -eq [System.Windows.Forms.MouseButtons]::Left) {
    Write-Cmd 'TOGGLE_VIS'
  }
})

$menu.add_Opening({ Sync-Menu })

$pidDir = Split-Path -Parent $PidPath
if ($pidDir -and -not (Test-Path -LiteralPath $pidDir)) {
  New-Item -ItemType Directory -Path $pidDir -Force | Out-Null
}
[System.IO.File]::WriteAllText($PidPath, [string]$PID, [System.Text.UTF8Encoding]::new($false))
Remove-Item -LiteralPath $ExitPath -Force -ErrorAction SilentlyContinue

$script:tickCount = 0
$script:fsSuppressed = $false
$timer = New-Object System.Windows.Forms.Timer
# Fast poll for click-through; fullscreen check ~5x/sec; taskbar hide infrequent
$timer.Interval = 40
$timer.add_Tick({
  try {
    if (Test-Path -LiteralPath $ExitPath) {
      Stop-Self
      return
    }
    if ($ParentPid -gt 0 -and -not [WeatherBallWin]::ProcessAlive($ParentPid)) {
      Stop-Self
      return
    }
    if ($ParentPid -gt 0) {
      [WeatherBallWin]::UpdateClickThrough($ParentPid)
      $script:tickCount++
      if (($script:tickCount % 8) -eq 0) {
        $fs = [WeatherBallWin]::IsForeignFullscreen($ParentPid)
        if ($fs -and -not $script:fsSuppressed) {
          Write-Cmd 'FS_HIDE'
          $script:fsSuppressed = $true
        } elseif (-not $fs -and $script:fsSuppressed) {
          $s = Read-State
          if ($s.visible) { Write-Cmd 'FS_SHOW' }
          $script:fsSuppressed = $false
        }
      }
      if (($script:tickCount % 25) -eq 0) {
        [WeatherBallWin]::HideProcessFromTaskbar($ParentPid)
      }
      # Re-assert icon visibility periodically (helps flaky post-logon trays)
      if (($script:tickCount % 75) -eq 0) {
        if (-not $notify.Visible) { $notify.Visible = $true }
      }
    }
  } catch { }
})
$timer.Start()

if ($ParentPid -gt 0) {
  try {
    [WeatherBallWin]::UpdateClickThrough($ParentPid)
    [WeatherBallWin]::HideProcessFromTaskbar($ParentPid)
  } catch { }
}

try {
  [System.Windows.Forms.Application]::Run()
} finally {
  try { $timer.Stop() } catch { }
  try {
    $notify.Visible = $false
    $notify.Dispose()
  } catch { }
  try { if ($icon) { $icon.Dispose() } } catch { }
  try { if ($bitmap) { $bitmap.Dispose() } } catch { }
  Remove-Item -LiteralPath $PidPath -Force -ErrorAction SilentlyContinue
  Remove-Item -LiteralPath $ExitPath -Force -ErrorAction SilentlyContinue
  if ($hasMutex) {
    try { $mutex.ReleaseMutex() } catch { }
  }
  try { $mutex.Dispose() } catch { }
}
