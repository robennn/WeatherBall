# WeatherBall Native (Rust 原型)

无 WebView 的透明悬浮球最小原型：`eframe` + `egui`，目标内存远低于 Neutralino 版。

## 环境

1. 安装 [Rust](https://rustup.rs/)（或 `winget install Rustlang.Rustup`）
2. **Windows**：需要 MSVC 链接器（Visual Studio Build Tools 的「C++ 生成工具」）
   ```powershell
   winget install Microsoft.VisualStudio.2022.BuildTools --override "--wait --passive --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
   ```
3. 首次编译会下载依赖，需联网

## 运行

```powershell
cd native
.\build.ps1          # 或: cargo run --release（需已配置 MSVC 环境）
.\target\release\weatherball-native.exe
```

`build.ps1` 会自动调用 `vcvars64.bat`，无需手动开「Developer Command Prompt」。

## 当前能力

- 透明、无边框、置顶小窗
- Open-Meteo 实时天气（IP 定位，失败则上海）
- 球体视觉随天气变化（晴/多云/阴/雨强/雪强/雷暴）
- 鼠标悬停显示温度、天气描述、城市
- 「刷新天气」按钮；约 20 分钟自动刷新
- 拖拽移动、系统托盘（退出）

## 资源

云贴图在 `assets/`（编译期 `include_bytes!` 打进 exe）：

- `cloud_a.png` / `cloud_b.png` — 手绘风云朵（透明底）
- `cloud_mist.png` — 淡雾条

替换这些 PNG 后重新 `cargo build --release` 即可换皮。

## 发布

```powershell
cargo build --release
# 产物: target\release\weatherball-native.exe
```
