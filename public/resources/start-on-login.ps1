# Delayed launcher for Windows logon — waits for the shell tray, then starts WeatherBall.
param(
  [Parameter(Mandatory = $true)][string]$ExePath,
  [Parameter(Mandatory = $true)][string]$WorkDir
)

$ErrorActionPreference = 'Stop'
$exe = $ExePath.Trim().Trim('"')
$wd = $WorkDir.Trim().Trim('"')

Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
public static class ShellWait {
  [DllImport("user32.dll", CharSet = CharSet.Unicode)]
  public static extern IntPtr FindWindow(string lpClassName, string lpWindowName);
  public static bool TrayReady() {
    return FindWindow("Shell_TrayWnd", null) != IntPtr.Zero;
  }
}
"@

# Wait until Explorer tray exists (up to ~90s after logon)
for ($i = 0; $i -lt 90; $i++) {
  try {
    if ([ShellWait]::TrayReady()) { break }
  } catch { }
  Start-Sleep -Seconds 1
}
# Brief settle so NotifyIcon registration is reliable
Start-Sleep -Seconds 3

if (-not (Test-Path -LiteralPath $exe)) { exit 1 }

# Avoid stacking if user already launched manually
$procs = Get-Process -ErrorAction SilentlyContinue | Where-Object {
  $_.ProcessName -like 'weatherball*' -or $_.ProcessName -like 'neutralino*'
}
if ($procs) { exit 0 }

Start-Process -FilePath $exe -WorkingDirectory $wd -ArgumentList ('--path="{0}"' -f $wd)
exit 0
