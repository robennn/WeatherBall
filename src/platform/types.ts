export type WindowBounds = {
  x: number
  y: number
  width: number
  height: number
}

export type OpenAtLoginResult = {
  enabled: boolean
  /** Soft error for UI; never use blocking dialogs */
  message?: string
}

export type DesktopApi = {
  kind: 'neutralino' | 'browser'
  onRefreshWeather: (cb: () => void) => () => void
  getAlwaysOnTop: () => Promise<boolean>
  setAlwaysOnTop: (value: boolean) => Promise<boolean>
  getBounds: () => Promise<WindowBounds>
  setPosition: (x: number, y: number) => void | Promise<void>
  setSize: (width: number, height: number) => void | Promise<void>
  getOpenAtLogin: () => Promise<boolean>
  setOpenAtLogin: (value: boolean) => Promise<OpenAtLoginResult>
}
