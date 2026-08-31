# WeatherBall Native

当前打包版本：**v0.3.5**（[GitHub Releases](https://github.com/robennn/WeatherBall/releases/tag/v0.3.5)）。

无 WebView 的透明悬浮球：`eframe` + `egui`，内存远低于 Neutralino / WebView2 版。

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

重新编译前请先结束正在运行的 `weatherball-native.exe`，否则 exe 会被锁住。

## 能力

- 透明、无边框、置顶；不出现在任务栏
- Open-Meteo 实时天气（IP 定位 + 区县名；定位失败则用上次位置或上海）
- 球体随天气变化：晴 / 多云 / 阴 / 毛毛雨 / 雨 / 雪 / 雷暴
- 悬停提示温度、描述、城市；点击展开详情（小时气温）
- 约 5 分钟自动刷新；详情里可手动刷新（不会叠跑请求）
- 拖拽移动，位置写入 `%APPDATA%\WeatherBall\settings.json`
- 系统托盘：显示/隐藏、开机自启、退出
- 多显示器、每显示器 DPI；单实例

空闲时会降帧；下雨/下雪、拖动、悬停时保持约 30 帧。

## 资源

云贴图在 `assets/`（编译期 `include_bytes!` 打进 exe）：

- `cloud_a.png` / `cloud_b.png` — 手绘风云朵（透明底）
- `cloud_mist.png` — 淡雾条

替换这些 PNG 后重新 `cargo build --release` 即可换皮。

## 发布

```powershell
.\build.ps1
# 产物: target\release\weatherball-native.exe
```

只拷这个 exe 即可运行；设置和开机项写在当前用户下，不依赖安装程序。
