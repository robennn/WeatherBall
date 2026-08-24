import type { DesktopApi, OpenAtLoginResult, WindowBounds } from './types'
import {
  clampWindowPos,
  loadOpenAtLoginPref,
  loadWindowPos,
  saveOpenAtLoginPref,
  saveWindowPos,
} from '../services/appSettings'

const AUTOSTART_TIMEOUT_MS = 8000
const NO_EXE_HINT = '未找到可启动的程序，请用 release/weatherball 里的 exe 运行后再开启自启'

const browserApi: DesktopApi = {
  kind: 'browser',
  onRefreshWeather() {
    return () => undefined
  },
  async getAlwaysOnTop() {
    return false
  },
  async setAlwaysOnTop() {
    return false
  },
  async getBounds(): Promise<WindowBounds> {
    return {
      x: window.screenX,
      y: window.screenY,
      width: window.outerWidth,
      height: window.outerHeight,
    }
  },
  setPosition() {},
  setSize() {},
  async getOpenAtLogin() {
    return loadOpenAtLoginPref()
  },
  async setOpenAtLogin(value) {
    saveOpenAtLoginPref(value)
    return { enabled: value }
  },
}

let alwaysOnTop = true
let openAtLogin = false
let windowVisible = true
let cached: DesktopApi | null = null
let savePosTimer: ReturnType<typeof setTimeout> | null = null

let scheduleSavePosition = (x: number, y: number) => {
  if (savePosTimer) clearTimeout(savePosTimer)
  savePosTimer = setTimeout(() => {
    saveWindowPos(clampWindowPos(x, y))
  }, 200)
}

function resourceCandidates(name: string): string[] {
  const base =
    typeof window.NL_PATH === 'string' && window.NL_PATH ? window.NL_PATH : '.'
  const norm = base.replace(/\//g, '\\').replace(/\\$/, '')
  return [
    `${norm}\\resources\\${name}`,
    `${norm}\\..\\resources\\${name}`,
  ]
}

async function resolveResourceFile(
  neu: typeof import('@neutralinojs/lib'),
  name: string,
): Promise<string | null> {
  for (const full of resourceCandidates(name)) {
    try {
      await neu.filesystem.getStats(full)
      return full
    } catch {
      /* try next */
    }
  }
  return null
}

function withTimeout<T>(promise: Promise<T>, ms: number, label: string): Promise<T> {
  return new Promise<T>((resolve, reject) => {
    const timer = window.setTimeout(() => {
      reject(new Error(`${label}超时`))
    }, ms)
    promise.then(
      (v) => {
        window.clearTimeout(timer)
        resolve(v)
      },
      (err) => {
        window.clearTimeout(timer)
        reject(err)
      },
    )
  })
}

async function resolveNeutralinoExe(
  neu: typeof import('@neutralinojs/lib'),
): Promise<{ exe: string; workDir: string } | null> {
  const normalize = (p: string) => p.replace(/\//g, '\\').replace(/\\+$/, '')

  // 1) Most reliable: path of the running process
  const pid = Number(window.NL_PID)
  if (Number.isFinite(pid) && pid > 0) {
    try {
      const result = await neu.os.execCommand(
        `powershell -NoProfile -Command "try { (Get-Process -Id ${pid}).Path } catch { '' }"`,
      )
      const exe = (result.stdOut || '').trim().replace(/^"|"$/g, '')
      if (exe.toLowerCase().endsWith('.exe')) {
        try {
          await neu.filesystem.getStats(exe)
          const workDir = normalize(exe.replace(/\\[^\\]+$/, ''))
          return { exe: normalize(exe), workDir }
        } catch {
          /* fall through */
        }
      }
    } catch {
      /* fall through */
    }
  }

  const base =
    typeof window.NL_PATH === 'string' && window.NL_PATH
      ? normalize(window.NL_PATH)
      : ''
  if (!base) return null

  const packaged = [
    'weatherball-win_x64.exe',
    'weatherball-win_arm64.exe',
    'weatherball-win_x86.exe',
    'weatherball.exe',
  ]
  const devBins = ['neutralino-win_x64.exe', 'neutralino-win_arm64.exe']
  const dirs = [
    base,
    `${base}\\release\\weatherball`,
    `${base}\\dist\\weatherball`,
    `${base}\\bin`,
  ]

  for (const dir of dirs) {
    for (const name of [...packaged, ...devBins]) {
      const full = `${dir}\\${name}`
      try {
        await neu.filesystem.getStats(full)
        return { exe: full, workDir: dir }
      } catch {
        /* try next */
      }
    }
  }
  return null
}

async function runAutostartScript(
  neu: typeof import('@neutralinojs/lib'),
  action: 'enable' | 'disable' | 'status',
  exe?: string,
  workDir?: string,
): Promise<boolean> {
  const script = await resolveResourceFile(neu, 'set-autostart.ps1')
  if (!script) throw new Error('找不到开机自启脚本')

  const dq = (s: string) => `"${s.replace(/"/g, '')}"`
  let cmd =
    `powershell -NoProfile -WindowStyle Hidden -ExecutionPolicy Bypass -File ${dq(script)} -Action ${action}`
  if (action === 'enable' && exe) {
    cmd += ` -ExePath ${dq(exe)} -WorkDir ${dq(workDir ?? '')}`
  }

  const result = await withTimeout(
    neu.os.execCommand(cmd),
    AUTOSTART_TIMEOUT_MS,
    '开机自启设置',
  )
  const out = (result.stdOut || '').trim()
  const err = (result.stdErr || '').trim()
  if (action === 'status') return out.startsWith('1')
  if (action === 'disable') {
    if (result.exitCode != null && result.exitCode !== 0) {
      throw new Error(err || `关闭开机自启失败 (${result.exitCode})`)
    }
    return true
  }
  if (!out.startsWith('1')) {
    throw new Error(err || out || `写入启动项失败 (${result.exitCode ?? '?'})`)
  }
  return true
}

async function applyOpenAtLogin(
  neu: typeof import('@neutralinojs/lib'),
  value: boolean,
  syncTray: () => Promise<void>,
): Promise<OpenAtLoginResult> {
  if (value === openAtLogin) return { enabled: openAtLogin }

  try {
    if (value) {
      const resolved = await resolveNeutralinoExe(neu)
      if (!resolved) {
        return { enabled: false, message: NO_EXE_HINT }
      }
      const ok = await runAutostartScript(
        neu,
        'enable',
        resolved.exe,
        resolved.workDir,
      )
      openAtLogin = ok
      if (!ok) {
        saveOpenAtLoginPref(false)
        await syncTray()
        return { enabled: false, message: '写入启动项失败，请重试' }
      }
    } else {
      await runAutostartScript(neu, 'disable')
      openAtLogin = false
    }
    saveOpenAtLoginPref(openAtLogin)
    await syncTray()
    return { enabled: openAtLogin }
  } catch (e) {
    const message = e instanceof Error ? e.message : '设置开机自启失败'
    saveOpenAtLoginPref(openAtLogin)
    await syncTray()
    return { enabled: openAtLogin, message }
  }
}

async function createNeutralinoApi(): Promise<DesktopApi> {
  const neu = await import('@neutralinojs/lib')
  neu.init()

  await Promise.race([
    new Promise<void>((resolve) => {
      void neu.events.on('ready', () => resolve())
    }),
    neu.window.getTitle().then(() => undefined),
    new Promise<void>((resolve) => window.setTimeout(resolve, 300)),
  ])

  const dataDir = `${(window.NL_PATH || '.').replace(/\\/g, '/')}/.tmp`
  const posFilePath = `${dataDir}/window-pos.json`

  const persistPosition = async (x: number, y: number) => {
    const clamped = clampWindowPos(x, y)
    saveWindowPos(clamped)
    try {
      await neu.filesystem.createDirectory(dataDir)
    } catch {
      /* exists */
    }
    try {
      await neu.filesystem.writeFile(
        posFilePath,
        JSON.stringify({ x: clamped.x, y: clamped.y }),
      )
    } catch {
      /* ignore */
    }
  }

  const loadPersistedPosition = async (): Promise<{ x: number; y: number } | null> => {
    try {
      const raw = await neu.filesystem.readFile(posFilePath)
      const data = JSON.parse(String(raw || '')) as { x?: number; y?: number }
      if (typeof data.x === 'number' && typeof data.y === 'number') {
        return clampWindowPos(data.x, data.y)
      }
    } catch {
      /* missing */
    }
    const saved = loadWindowPos()
    return saved ? clampWindowPos(saved.x, saved.y) : null
  }

  scheduleSavePosition = (x: number, y: number) => {
    if (savePosTimer) clearTimeout(savePosTimer)
    savePosTimer = setTimeout(() => {
      void persistPosition(x, y)
    }, 200)
  }

  // Process name = exe filename without extension (packaged or neu run)
  let procName = ''
  try {
    const exeInfo = await resolveNeutralinoExe(neu)
    if (exeInfo) {
      const base = exeInfo.exe.split(/[/\\]/).pop() || ''
      procName = base.replace(/\.exe$/i, '')
    }
  } catch {
    /* ignore */
  }

  const parentPid = Number(window.NL_PID)
  const parentPidArg =
    Number.isFinite(parentPid) && parentPid > 0 ? ` -ParentPid ${parentPid}` : ''

  const hideFromTaskbar = async () => {
    const winAny = neu.window as typeof neu.window & {
      setSkipTaskbar?: (skip: boolean) => Promise<void>
    }
    try {
      if (typeof winAny.setSkipTaskbar === 'function') {
        await winAny.setSkipTaskbar(true)
      }
    } catch {
      /* ignore */
    }

    try {
      const script = await resolveResourceFile(neu, 'hide-from-taskbar.ps1')
      if (!script) return
      const title = (await neu.window.getTitle()) || '天气球'
      await neu.os.execCommand(
        `powershell -NoProfile -WindowStyle Hidden -ExecutionPolicy Bypass -File "${script}" -Title "${title.replace(/"/g, '')}"` +
          (procName ? ` -ProcessName "${procName}"` : '') +
          parentPidArg,
        { background: true },
      )
    } catch {
      /* ignore helper failures */
    }
  }

  // Keep compact size even if Neutralino restored a previous expanded state
  try {
    await neu.window.setSize({
      width: 160,
      height: 204,
      resizable: false,
    })
  } catch {
    /* ignore */
  }

  const savedPos = await loadPersistedPosition()
  if (savedPos) {
    try {
      await neu.window.move(savedPos.x, savedPos.y)
    } catch {
      /* ignore */
    }
  }

  await hideFromTaskbar()
  window.setTimeout(() => {
    void hideFromTaskbar()
  }, 600)
  window.setTimeout(() => {
    void hideFromTaskbar()
  }, 2000)

  try {
    openAtLogin = await runAutostartScript(neu, 'status')
    saveOpenAtLoginPref(openAtLogin)
  } catch {
    openAtLogin = loadOpenAtLoginPref()
  }

  const refreshHandlers = new Set<() => void>()
  const isWindows = window.NL_OS === 'Windows'
  const tmpDir = dataDir
  const trayCmdPath = `${tmpDir}/tray-cmd.txt`
  const trayStatePath = `${tmpDir}/tray-state.json`
  const trayPidPath = `${tmpDir}/tray-host.pid`
  const trayExitPath = `${tmpDir}/tray-exit.flag`
  let trayPollTimer: ReturnType<typeof setInterval> | null = null
  let handlingTrayCmd = false

  const notifySoft = (title: string, content: string) => {
    void neu.os.showNotification(title, content).catch(() => {
      /* never block UI with modal dialogs */
    })
  }

  const writeTrayState = async () => {
    try {
      await neu.filesystem.createDirectory(tmpDir)
    } catch {
      /* exists */
    }
    const payload = JSON.stringify({
      visible: windowVisible,
      alwaysOnTop,
      openAtLogin,
      tip: '天气球',
      hideText: '隐藏天气球',
      showText: '显示天气球',
      refreshText: '刷新天气',
      topOnText: '✓ 始终置顶',
      topOffText: '始终置顶',
      autoOnText: '✓ 开机自启',
      autoOffText: '开机自启',
      quitText: '退出',
    })
    await neu.filesystem.writeFile(trayStatePath, payload)
  }

  const stopWinTrayHost = async () => {
    try {
      await neu.filesystem.writeFile(trayExitPath, '1')
    } catch {
      /* ignore */
    }
    try {
      const pidRaw = await neu.filesystem.readFile(trayPidPath)
      const pid = Number(String(pidRaw).trim())
      if (Number.isFinite(pid) && pid > 0) {
        await neu.os.execCommand(`taskkill /PID ${pid} /F /T`, { background: true })
      }
    } catch {
      /* ignore */
    }
  }

  const startWinTrayHost = async () => {
    const icon = await resolveResourceFile(neu, 'tray.png')
    const script = await resolveResourceFile(neu, 'tray-host.ps1')
    if (!icon || !script) {
      console.warn('[weatherball] tray resources missing; copy resources/ next to the exe')
      return
    }

    // Always tear down any previous host first (avoids stacked tray icons)
    await stopWinTrayHost()
    await new Promise<void>((r) => window.setTimeout(r, 250))

    await writeTrayState()
    try {
      await neu.filesystem.remove(trayExitPath)
    } catch {
      /* ignore */
    }
    try {
      await neu.filesystem.remove(trayCmdPath)
    } catch {
      /* ignore */
    }

    const cmdPathWin = trayCmdPath.replace(/\//g, '\\')
    const statePathWin = trayStatePath.replace(/\//g, '\\')
    const pidPathWin = trayPidPath.replace(/\//g, '\\')
    const exitPathWin = trayExitPath.replace(/\//g, '\\')

    await neu.os.execCommand(
      `powershell -STA -NoProfile -WindowStyle Hidden -ExecutionPolicy Bypass -File "${script}" -IconPath "${icon}" -CmdPath "${cmdPathWin}" -StatePath "${statePathWin}" -PidPath "${pidPathWin}" -ExitPath "${exitPathWin}"${parentPidArg}`,
      { background: true },
    )
  }

  const handleTrayId = async (id: string) => {
    if (id === 'TOGGLE_VIS') {
      if (windowVisible) await hideWindow()
      else await showWindow()
    } else if (id === 'FS_HIDE') {
      // Keep windowVisible=true so leaving fullscreen can restore automatically
      if (await neu.window.isVisible()) {
        await neu.window.hide()
      }
    } else if (id === 'FS_SHOW') {
      if (windowVisible && !(await neu.window.isVisible())) {
        await neu.window.show()
        await hideFromTaskbar()
      }
    } else if (id === 'REFRESH') {
      await showWindow()
      refreshHandlers.forEach((cb) => cb())
    } else if (id === 'TOP') {
      alwaysOnTop = !alwaysOnTop
      await neu.window.setAlwaysOnTop(alwaysOnTop)
      await syncTray()
    } else if (id === 'AUTOSTART') {
      const result = await applyOpenAtLogin(neu, !openAtLogin, syncTray)
      if (result.message) notifySoft('开机自启', result.message)
    } else if (id === 'QUIT') {
      try {
        const pos = await neu.window.getPosition()
        if (pos.x != null && pos.y != null) {
          await persistPosition(pos.x, pos.y)
        }
      } catch {
        /* ignore */
      }
      if (isWindows) await stopWinTrayHost()
      void neu.app.exit(0)
    }
  }

  const pollWinTrayCmd = async () => {
    if (handlingTrayCmd) return
    handlingTrayCmd = true
    try {
      const raw = await neu.filesystem.readFile(trayCmdPath)
      const id = String(raw || '').trim()
      try {
        await neu.filesystem.remove(trayCmdPath)
      } catch {
        /* ignore */
      }
      if (id) await handleTrayId(id)
    } catch {
      /* no command yet */
    } finally {
      handlingTrayCmd = false
    }
  }

  const syncTray = async () => {
    if (isWindows) {
      await writeTrayState()
      return
    }
    await neu.os.setTray({
      icon: '/resources/tray.png',
      menuItems: [
        {
          id: 'TOGGLE_VIS',
          text: windowVisible ? '隐藏天气球' : '显示天气球',
        },
        { id: 'REFRESH', text: '刷新天气' },
        { id: 'SEP_OPT', text: '-' },
        { id: 'TOP', text: alwaysOnTop ? '✓ 始终置顶' : '始终置顶' },
        {
          id: 'AUTOSTART',
          text: openAtLogin ? '✓ 开机自启' : '开机自启',
        },
        { id: 'SEP_QUIT', text: '-' },
        { id: 'QUIT', text: '退出' },
      ],
    })
  }

  const showWindow = async () => {
    windowVisible = true
    if (!(await neu.window.isVisible())) await neu.window.show()
    await neu.window.focus()
    await hideFromTaskbar()
    await syncTray()
  }

  const hideWindow = async () => {
    await neu.window.hide()
    windowVisible = false
    await syncTray()
  }

  if (isWindows) {
    await startWinTrayHost()
    // Autostart / cold boot: tray host may race Explorer — retry a few times
    for (const ms of [2500, 7000, 15000]) {
      window.setTimeout(() => {
        void (async () => {
          try {
            const pidRaw = await neu.filesystem.readFile(trayPidPath)
            const pid = Number(String(pidRaw).trim())
            if (Number.isFinite(pid) && pid > 0) {
              // Host claims to be alive; still nudge a restart if shell was late
              return
            }
          } catch {
            /* pid file missing */
          }
          await startWinTrayHost()
        })()
      }, ms)
    }
    // Always one deferred restart after shell settle (recreates NotifyIcon if needed)
    window.setTimeout(() => {
      void startWinTrayHost()
    }, 12000)
    trayPollTimer = window.setInterval(() => {
      void pollWinTrayCmd()
    }, 350)
  } else {
    await syncTray()
    await neu.events.on('trayMenuItemClicked', (evt) => {
      const id = (evt.detail as { id?: string } | undefined)?.id
      if (!id) return
      void handleTrayId(id)
    })
  }

  await neu.events.on('windowClose', () => {
    void (async () => {
      try {
        const pos = await neu.window.getPosition()
        if (pos.x != null && pos.y != null) {
          await persistPosition(pos.x, pos.y)
        }
      } catch {
        /* ignore */
      }
      if (trayPollTimer) window.clearInterval(trayPollTimer)
      if (isWindows) await stopWinTrayHost()
      void neu.app.exit(0)
    })()
  })

  return {
    kind: 'neutralino',
    onRefreshWeather(cb) {
      refreshHandlers.add(cb)
      return () => {
        refreshHandlers.delete(cb)
      }
    },
    async getAlwaysOnTop() {
      return alwaysOnTop
    },
    async setAlwaysOnTop(value) {
      alwaysOnTop = value
      await neu.window.setAlwaysOnTop(value)
      await syncTray()
      return alwaysOnTop
    },
    async getBounds() {
      const [pos, size] = await Promise.all([
        neu.window.getPosition(),
        neu.window.getSize(),
      ])
      return {
        x: pos.x ?? 0,
        y: pos.y ?? 0,
        width: size.width ?? 160,
        height: size.height ?? 232,
      }
    },
    async setPosition(x, y) {
      await neu.window.move(Math.round(x), Math.round(y))
      scheduleSavePosition(x, y)
    },
    async setSize(width, height) {
      await neu.window.setSize({
        width: Math.round(width),
        height: Math.round(height),
        resizable: false,
      })
    },
    async getOpenAtLogin() {
      return openAtLogin
    },
    async setOpenAtLogin(value) {
      return applyOpenAtLogin(neu, value, syncTray)
    },
  }
}

async function restoreWindowPosition(api: DesktopApi) {
  if (api.kind === 'browser') return
  // Neutralino path already restores during createNeutralinoApi
  const saved = loadWindowPos()
  if (!saved) return
  const clamped = clampWindowPos(saved.x, saved.y)
  await api.setPosition(clamped.x, clamped.y)
}

/** Call once at app startup before using getDesktopApi(). */
export async function initDesktopApi(): Promise<DesktopApi> {
  if (cached) return cached
  if (typeof window.NL_TOKEN !== 'undefined') {
    cached = await createNeutralinoApi()
    await restoreWindowPosition(cached)
    return cached
  }
  cached = browserApi
  return cached
}

export function getDesktopApi(): DesktopApi {
  return cached ?? browserApi
}

export type { DesktopApi, OpenAtLoginResult, WindowBounds }
