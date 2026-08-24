param(
  [Parameter(Mandatory = $true)]
  [ValidateSet('enable', 'disable', 'status')]
  [string]$Action,

  [string]$ExePath = '',
  [string]$WorkDir = ''
)

$ErrorActionPreference = 'Stop'
$Name = 'WeatherBall'
$RunKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
$StartupDir = [Environment]::GetFolderPath('Startup')
$LnkPath = Join-Path $StartupDir ($Name + '.lnk')

function Test-Enabled {
  $prop = Get-ItemProperty -Path $RunKey -Name $Name -ErrorAction SilentlyContinue
  if ($null -ne $prop -and -not [string]::IsNullOrWhiteSpace([string]$prop.$Name)) {
    return $true
  }
  if (Test-Path -LiteralPath $LnkPath) { return $true }
  return $false
}

if ($Action -eq 'status') {
  if (Test-Enabled) { Write-Output '1' } else { Write-Output '0' }
  exit 0
}

if ($Action -eq 'disable') {
  Remove-ItemProperty -Path $RunKey -Name $Name -ErrorAction SilentlyContinue
  Remove-Item -LiteralPath $LnkPath -Force -ErrorAction SilentlyContinue
  Write-Output '0'
  exit 0
}

if ([string]::IsNullOrWhiteSpace($ExePath)) {
  throw 'ExePath is required for enable'
}

$exe = $ExePath.Trim().Trim('"')
if (-not (Test-Path -LiteralPath $exe)) {
  throw "Executable not found: $exe"
}

$wd = if ([string]::IsNullOrWhiteSpace($WorkDir)) {
  Split-Path -Parent $exe
} else {
  $WorkDir.Trim().Trim('"')
}
if (-not (Test-Path -LiteralPath $wd)) {
  $wd = Split-Path -Parent $exe
}

# Prefer delayed launcher so the shell tray exists before NotifyIcon is created
$launcher = Join-Path $wd 'resources\start-on-login.ps1'
if (-not (Test-Path -LiteralPath $launcher)) {
  # Fallback: start exe directly (may miss tray on cold boot)
  $runValue = '"' + $exe + '" --path="' + $wd + '"'
} else {
  $runValue =
    'powershell.exe -NoProfile -WindowStyle Hidden -ExecutionPolicy Bypass -File "' +
    $launcher +
    '" -ExePath "' + $exe + '" -WorkDir "' + $wd + '"'
}

Set-ItemProperty -Path $RunKey -Name $Name -Value $runValue

# Remove Startup shortcut to avoid launching twice (Run + Startup)
Remove-Item -LiteralPath $LnkPath -Force -ErrorAction SilentlyContinue

if (-not (Test-Enabled)) {
  throw 'Failed to write startup entry'
}

Write-Output '1'
exit 0
