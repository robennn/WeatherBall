# Build weatherball-native with MSVC (vcvars64)
$ErrorActionPreference = "Stop"
$vcvars = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat"
if (-not (Test-Path $vcvars)) {
    $vcvars = "${env:ProgramFiles}\Microsoft Visual Studio\2022\Community\VC\Auxiliary\Build\vcvars64.bat"
}
if (-not (Test-Path $vcvars)) {
    Write-Error "找不到 vcvars64.bat。请安装 Visual Studio Build Tools（含 C++ 工具）。"
}
$nativeDir = $PSScriptRoot
$cargoArgs = if ($args.Count -gt 0) { $args -join " " } else { "build --release" }
$cmd = "call `"$vcvars`" && cd /d `"$nativeDir`" && cargo $cargoArgs"
cmd.exe /c $cmd
