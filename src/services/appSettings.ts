export type SavedWindowPos = {
  x: number
  y: number
}

const POS_KEY = 'weatherball.windowPos'
const AUTOSTART_KEY = 'weatherball.openAtLogin'

export function loadWindowPos(): SavedWindowPos | null {
  try {
    const raw = localStorage.getItem(POS_KEY)
    if (!raw) return null
    const data = JSON.parse(raw) as Partial<SavedWindowPos>
    if (typeof data.x !== 'number' || typeof data.y !== 'number') return null
    if (!Number.isFinite(data.x) || !Number.isFinite(data.y)) return null
    return { x: data.x, y: data.y }
  } catch {
    return null
  }
}

export function saveWindowPos(pos: SavedWindowPos): void {
  localStorage.setItem(
    POS_KEY,
    JSON.stringify({ x: Math.round(pos.x), y: Math.round(pos.y) }),
  )
}

/** Keep the orb at least partially on a screen after monitor changes */
export function clampWindowPos(
  x: number,
  y: number,
  width = 160,
  _height = 204,
): SavedWindowPos {
  const screenExt = window.screen as Screen & {
    availLeft?: number
    availTop?: number
  }
  const availLeft = screenExt.availLeft ?? 0
  const availTop = screenExt.availTop ?? 0
  const availW = window.screen.availWidth || 1280
  const availH = window.screen.availHeight || 720
  const margin = 48
  const minX = availLeft - width + margin
  const maxX = availLeft + availW - margin
  const minY = availTop
  const maxY = availTop + availH - margin
  return {
    x: Math.min(maxX, Math.max(minX, Math.round(x))),
    y: Math.min(maxY, Math.max(minY, Math.round(y))),
  }
}

export function loadOpenAtLoginPref(): boolean {
  try {
    return localStorage.getItem(AUTOSTART_KEY) === '1'
  } catch {
    return false
  }
}

export function saveOpenAtLoginPref(enabled: boolean): void {
  localStorage.setItem(AUTOSTART_KEY, enabled ? '1' : '0')
}
