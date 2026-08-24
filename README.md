# Weather Ball · 天气球

桌面悬浮天气球：随实时天气变换形态（晴 / 多云 / 阴 / 雨 / 雪 / 雷暴）。

有两个实现：

| | **原生版（推荐日常使用）** | Neutralino 版 |
|---|---|---|
| 目录 | `native/` | 仓库根目录 |
| 技术 | Rust + egui，无 WebView | Vue 3 + Neutralino（WebView2） |
| 特点 | 内存更低、无任务栏图标、托盘控制 | 浏览器可预览，样式便于改 CSS |

天气数据来自 [Open-Meteo](https://open-meteo.com/)，位置先用 IP 定位，再逆地理到区县；失败时回退上海。约 20 分钟自动刷新。

---

## 原生版（Windows）

无 WebView 的透明置顶小球，适合长期挂在桌面上。

### 环境

1. [Rust](https://rustup.rs/)（或 `winget install Rustlang.Rustup`）
2. MSVC 链接器：Visual Studio Build Tools 的「使用 C++ 的桌面开发」
   ```powershell
   winget install Microsoft.VisualStudio.2022.BuildTools --override "--wait --passive --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
   ```

### 下载运行

不解源码可直接下载 [**v0.2.0** 压缩包](release/weatherball-native-windows-v0.2.0.zip)，解压后双击 `weatherball-native.exe`。

### 自行编译

```powershell
cd native
.\build.ps1
.\target\release\weatherball-native.exe
```

`build.ps1` 会调用 `vcvars64.bat`，不必手动开 Developer Command Prompt。产物是单个 exe：`native/target/release/weatherball-native.exe`。

更细的说明见 [`native/README.md`](native/README.md)。

### 交互

- 拖拽球体移动（位置写入 `%APPDATA%\WeatherBall\settings.json`）
- 悬停看温度、天气、城市；点击展开详情
- 详情里可刷新天气；点空白处或关闭可收起
- 系统托盘「天气球」：显示/隐藏、开机自启、退出
- 视频或游戏全屏时自动隐藏，退出全屏后恢复
- 不占用任务栏；同一时间只跑一个实例
- 支持多显示器和不同 DPI

球体穿过空白区域时鼠标会穿透到下面的窗口。

---

## Neutralino 版（Vue）

适合改界面、或只在浏览器里看效果。

```bash
npm install
npm run neu:update   # 首次或升级二进制时
npm run dev          # Neutralino 桌面
```

若 `neu update` 因网络失败，可手动把 [Neutralino releases](https://github.com/neutralinojs/neutralinojs/releases) 的 Windows 二进制放到 `bin/`。

打包后请运行 `release/weatherball/` 目录下的 exe（同目录需有 `resources/` 与 `resources.neu`，`npm run build` 会自动复制）。

仅浏览器预览：

```bash
npm run dev:web
```

### 脚本

| 命令 | 说明 |
|------|------|
| `npm run dev` | Neutralino 桌面开发 |
| `npm run dev:web` | 仅 Vite 浏览器预览 |
| `npm run build` | 构建前端 + Neutralino 发行包 |
| `npm run neu:update` | 更新 Neutralino 二进制 |

打包产物在 `release/weatherball/`（含 `weatherball-win_*.exe`）。

开发时底部球体下方会显示「试样式」；`npm run build` 正式包默认隐藏。若要在正式包里打开，构建前设置 `VITE_SHOW_PREVIEW=true`。

### 交互

- 拖拽球体移动窗口（位置会记住）
- 点击球体查看详情；点城市名可手动切换城市
- 系统托盘：左键显示/隐藏，右键菜单（刷新、置顶、开机自启、退出）
- 右键球体：刷新天气
- 窗口被系统隐藏时动画会暂停
