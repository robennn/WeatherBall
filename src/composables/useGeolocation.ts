import { ref } from 'vue'
import { reverseGeocode } from '../services/geocode'

export const DEFAULT_COORDS = {
  latitude: 31.23,
  longitude: 121.47,
  label: '上海',
}

export type GeoResult = {
  latitude: number
  longitude: number
  source: 'geolocation' | 'ip' | 'fallback' | 'manual' | 'cache'
  label?: string
}

const LAST_GEO_KEY = 'weatherball.lastGeo'
const GEO_DENIED_KEY = 'weatherball.geoDenied'
/** Prefer cached GPS coords up to this age before falling back to IP */
const PRECISE_CACHE_MAX_MS = 7 * 24 * 60 * 60 * 1000

type StoredGeo = {
  latitude: number
  longitude: number
  label?: string
  /** true when from browser GPS */
  precise?: boolean
  at?: number
}

function geoErrorMessage(err: unknown): string {
  if (err && typeof err === 'object' && 'code' in err) {
    const code = (err as GeolocationPositionError).code
    if (code === 1) return '定位权限被拒绝（可在系统设置中开启位置服务）'
    if (code === 2) return '暂时无法获取位置'
    if (code === 3) return '定位超时'
  }
  if (err instanceof Error && err.message) return err.message
  return '定位失败'
}

function isPermissionDenied(err: unknown): boolean {
  return (
    !!err &&
    typeof err === 'object' &&
    'code' in err &&
    (err as GeolocationPositionError).code === 1
  )
}

function sleep(ms: number) {
  return new Promise<void>((resolve) => {
    window.setTimeout(resolve, ms)
  })
}

export function loadLastGeo(): GeoResult | null {
  try {
    const raw = localStorage.getItem(LAST_GEO_KEY)
    if (!raw) return null
    const data = JSON.parse(raw) as Partial<StoredGeo>
    if (typeof data.latitude !== 'number' || typeof data.longitude !== 'number') return null
    if (!Number.isFinite(data.latitude) || !Number.isFinite(data.longitude)) return null
    return {
      latitude: data.latitude,
      longitude: data.longitude,
      source: 'cache',
      label: typeof data.label === 'string' ? data.label : undefined,
    }
  } catch {
    return null
  }
}

function loadStoredGeo(): StoredGeo | null {
  try {
    const raw = localStorage.getItem(LAST_GEO_KEY)
    if (!raw) return null
    const data = JSON.parse(raw) as Partial<StoredGeo>
    if (typeof data.latitude !== 'number' || typeof data.longitude !== 'number') return null
    if (!Number.isFinite(data.latitude) || !Number.isFinite(data.longitude)) return null
    return {
      latitude: data.latitude,
      longitude: data.longitude,
      label: typeof data.label === 'string' ? data.label : undefined,
      precise: data.precise === true,
      at: typeof data.at === 'number' ? data.at : undefined,
    }
  } catch {
    return null
  }
}

export function saveLastGeo(geo: GeoResult): void {
  if (geo.source === 'fallback') return
  try {
    const prev = loadStoredGeo()
    const precise = geo.source === 'geolocation' || (geo.source === 'cache' && prev?.precise === true)
    localStorage.setItem(
      LAST_GEO_KEY,
      JSON.stringify({
        latitude: geo.latitude,
        longitude: geo.longitude,
        label: geo.label,
        precise: geo.source === 'geolocation' ? true : precise && geo.source !== 'ip',
        at: Date.now(),
      } satisfies StoredGeo),
    )
  } catch {
    /* ignore */
  }
}

function wasGeoDenied(): boolean {
  try {
    return localStorage.getItem(GEO_DENIED_KEY) === '1'
  } catch {
    return false
  }
}

function markGeoDenied(): void {
  try {
    localStorage.setItem(GEO_DENIED_KEY, '1')
  } catch {
    /* ignore */
  }
}

function clearGeoDenied(): void {
  try {
    localStorage.removeItem(GEO_DENIED_KEY)
  } catch {
    /* ignore */
  }
}

/** `granted` = can call GPS without a new OS prompt; otherwise skip on auto path */
async function getGeoPermissionState(): Promise<PermissionState | 'unknown'> {
  try {
    if (!navigator.permissions?.query) return 'unknown'
    const status = await navigator.permissions.query({
      name: 'geolocation' as PermissionName,
    })
    return status.state
  } catch {
    return 'unknown'
  }
}

async function withDistrictLabel(geo: GeoResult): Promise<GeoResult> {
  try {
    const label = await reverseGeocode(geo.latitude, geo.longitude)
    if (label && label !== '未知位置') {
      return { ...geo, label }
    }
  } catch {
    /* keep original */
  }
  return geo
}

async function locateByBrowser(timeoutMs = 10000): Promise<GeoResult> {
  const pos = await new Promise<GeolocationPosition>((resolve, reject) => {
    if (!navigator.geolocation) {
      reject(new Error('当前环境不支持浏览器定位'))
      return
    }
    navigator.geolocation.getCurrentPosition(resolve, reject, {
      enableHighAccuracy: true,
      timeout: timeoutMs,
      maximumAge: 10 * 60 * 1000,
    })
  })

  return withDistrictLabel({
    latitude: pos.coords.latitude,
    longitude: pos.coords.longitude,
    source: 'geolocation',
  })
}

type IpProvider = () => Promise<GeoResult>

const IP_PROVIDERS: IpProvider[] = [
  async () => {
    const res = await fetch('https://ipwho.is/', { signal: AbortSignal.timeout(6000) })
    if (!res.ok) throw new Error('ipwho')
    const data = (await res.json()) as {
      success?: boolean
      latitude?: number
      longitude?: number
      city?: string
      region?: string
    }
    if (!data.success || data.latitude == null || data.longitude == null) throw new Error('ipwho')
    return {
      latitude: data.latitude,
      longitude: data.longitude,
      source: 'ip',
      label: data.city || data.region,
    }
  },
  async () => {
    const res = await fetch('https://ipapi.co/json/', { signal: AbortSignal.timeout(6000) })
    if (!res.ok) throw new Error('ipapi')
    const data = (await res.json()) as {
      error?: boolean
      latitude?: number
      longitude?: number
      city?: string
      region?: string
    }
    if (data.error || data.latitude == null || data.longitude == null) throw new Error('ipapi')
    return {
      latitude: data.latitude,
      longitude: data.longitude,
      source: 'ip',
      label: data.city || data.region,
    }
  },
  async () => {
    const res = await fetch('https://get.geojs.io/v1/ip/geo.json', {
      signal: AbortSignal.timeout(6000),
    })
    if (!res.ok) throw new Error('geojs')
    const data = (await res.json()) as {
      latitude?: string | number
      longitude?: string | number
      city?: string
      region?: string
    }
    const lat = Number(data.latitude)
    const lon = Number(data.longitude)
    if (!Number.isFinite(lat) || !Number.isFinite(lon)) throw new Error('geojs')
    return {
      latitude: lat,
      longitude: lon,
      source: 'ip',
      label: data.city || data.region,
    }
  },
]

async function locateByIp(rounds = 2): Promise<GeoResult> {
  let lastErr: unknown
  for (let round = 0; round < rounds; round++) {
    for (const provider of IP_PROVIDERS) {
      try {
        const coarse = await provider()
        // Reverse-geocode IP coords for a better display name when possible
        return await withDistrictLabel(coarse)
      } catch (e) {
        lastErr = e
      }
    }
    if (round < rounds - 1) await sleep(500 * (round + 1))
  }
  throw lastErr instanceof Error ? lastErr : new Error('IP 定位失败')
}

function preciseCacheIfFresh(): GeoResult | null {
  const stored = loadStoredGeo()
  if (!stored?.precise) return null
  const age = Date.now() - (stored.at ?? 0)
  if (age > PRECISE_CACHE_MAX_MS) return null
  return {
    latitude: stored.latitude,
    longitude: stored.longitude,
    source: 'cache',
    label: stored.label,
  }
}

export type LocateOptions = {
  /** Manual button: may prompt once for GPS if not yet decided */
  preferGps?: boolean
}

export function useGeolocation() {
  const coords = ref<GeoResult | null>(null)
  const locating = ref(false)
  const geoError = ref<string | null>(null)

  async function locate(opts?: LocateOptions): Promise<GeoResult> {
    locating.value = true
    geoError.value = null
    const preferGps = opts?.preferGps === true

    try {
      const perm = await getGeoPermissionState()
      const hadPreciseBefore = loadStoredGeo()?.precise === true
      // WebView2 may report "unknown"; if GPS worked before, retry without assuming a new prompt
      const canSilentGps =
        perm === 'granted' || (perm === 'unknown' && hadPreciseBefore && !wasGeoDenied())
      const canPromptGps = preferGps && perm !== 'denied' && !wasGeoDenied()

      // 1) Precise GPS when already allowed (no new dialog) — district-level
      if (canSilentGps || canPromptGps) {
        try {
          const gps = await locateByBrowser(canSilentGps ? 10000 : 12000)
          coords.value = gps
          saveLastGeo(gps)
          clearGeoDenied()
          geoError.value = null
          return gps
        } catch (browserErr) {
          if (isPermissionDenied(browserErr)) {
            markGeoDenied()
            geoError.value = geoErrorMessage(browserErr)
          } else if (preferGps) {
            geoError.value = geoErrorMessage(browserErr)
          }
        }
      }

      // 2) Recent precise cache (keeps 区-level between launches without re-prompt)
      const preciseCached = preciseCacheIfFresh()
      if (preciseCached) {
        const labeled = preciseCached.label
          ? preciseCached
          : await withDistrictLabel(preciseCached)
        coords.value = labeled
        saveLastGeo({ ...labeled, source: 'geolocation' })
        return labeled
      }

      // 3) IP coarse fallback
      try {
        const ipResult = await locateByIp(preferGps ? 3 : 2)
        coords.value = ipResult
        saveLastGeo(ipResult)
        if (!geoError.value) geoError.value = null
        return ipResult
      } catch {
        /* continue */
      }

      const cached = loadLastGeo()
      if (cached) {
        coords.value = cached
        return cached
      }

      const fallback: GeoResult = {
        ...DEFAULT_COORDS,
        source: 'fallback',
        label: DEFAULT_COORDS.label,
      }
      coords.value = fallback
      return fallback
    } finally {
      locating.value = false
    }
  }

  return { coords, locating, geoError, locate }
}
