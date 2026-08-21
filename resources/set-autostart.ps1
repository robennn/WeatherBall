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

# Neutralino resolves resources.neu from --path / exe directory
$runValue = '"' + $exe + '" --path="' + $wd + '"'
Set-ItemProperty -Path $RunKey -Name $Name -Value $runValue

# Startup folder shortcut is more reliable on some Windows setups
try {
  if (-not (Test-Path -LiteralPath $StartupDir)) {
    New-Item -ItemType Directory -Path $StartupDir -Force | Out-Null
  }
  $shell = New-Object -ComObject WScript.Shell
  $sc = $shell.CreateShortcut($LnkPath)
  $sc.TargetPath = $exe
  $sc.WorkingDirectory = $wd
  $sc.Arguments = '--path="' + $wd + '"'
  $sc.WindowStyle = 1
  $sc.Description = 'WeatherBall'
  $sc.Save()
} catch {
  # Run key alone is still OK
}

if (-not (Test-Enabled)) {
  throw 'Failed to write startup entry'
}

Write-Output '1'
exit 0
