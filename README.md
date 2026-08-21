# Weather Ball · 天气球

桌面悬浮天气球：随实时天气变换形态。基于 **Neutralino**（系统 WebView2）+ Vue 3。

## 启动

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

## 脚本

| 命令 | 说明 |
|------|------|
| `npm run dev` | Neutralino 桌面开发 |
| `npm run dev:web` | 仅 Vite 浏览器预览 |
| `npm run build` | 构建前端 + Neutralino 发行包 |
| `npm run neu:update` | 更新 Neutralino 二进制 |

打包产物在 `release/weatherball/`（含 `weatherball-win_*.exe`）。

开发时底部球体下方会显示「试样式」；`npm run build` 正式包默认隐藏。若要在正式包里打开，构建前设置 `VITE_SHOW_PREVIEW=true`。

## 交互

- 拖拽球体移动窗口（位置会记住）
- 点击球体查看详情；点城市名可手动切换城市
- 系统托盘：左键显示/隐藏，右键菜单（刷新、置顶、开机自启、退出）
- 右键球体：刷新天气
