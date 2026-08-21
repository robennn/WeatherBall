import { onMounted, onUnmounted, ref } from 'vue'
import {
  fetchWeather,
  type PrecipIntensity,
  type WeatherKind,
  type WeatherSnapshot,
} from '../services/weather'
import { loadWeatherCache, saveWeatherCache } from '../services/weatherCache'
import type { CityOption } from '../services/cities'
import { getDesktopApi } from '../platform'
import { useGeolocation, type GeoResult } from './useGeolocation'
import {
  clearManualCity,
  loadManualCity,
  saveManualCity,
  toManualGeo,
} from './useCityPreference'

const MINUTE = 60 * 1000
const UPDATE_FAIL = '更新失败'

/** Adaptive poll interval from current conditions */
export function refreshIntervalMs(
  kind: WeatherKind | undefined,
  intensity: PrecipIntensity | null | undefined,
  precipitationMm = 0,
): number {
  if (!kind || kind === 'loading' || kind === 'error') return 10 * MINUTE

  const wet =
    kind === 'rain' ||
    kind === 'drizzle' ||
    kind === 'snow' ||
    kind === 'storm' ||
    precipitationMm > 0

  if (kind === 'storm' || intensity === 'heavy' || precipitationMm >= 5) {
    return 5 * MINUTE
  }
  if (wet && (intensity === 'moderate' || precipitationMm >= 1)) {
    return 8 * MINUTE
  }
  if (wet) {
    return 10 * MINUTE
  }
  if (kind === 'cloudy' || kind === 'fog') {
    return 20 * MINUTE
  }
  return 30 * MINUTE
}

export function useWeather() {
  const { locate, geoError, coords } = useGeolocation()
  const weather = ref<WeatherSnapshot | null>(null)
  const loading = ref(true)
  const error = ref<string | null>(null)
  let timer: ReturnType<typeof setTimeout> | null = null
  let unsubRefresh: (() => void) | null = null
  let disposed = false

  function hydrateFromCache() {
    const cached = loadWeatherCache()
    if (!cached) return false
    weather.value = cached
    if (!coords.value) {
      coords.value = {
        latitude: cached.latitude,
        longitude: cached.longitude,
        source: 'cache',
        label: cached.city,
      }
    }
    return true
  }

  function clearSchedule() {
    if (timer) {
      clearTimeout(timer)
      timer = null
    }
  }

  function scheduleNext() {
    clearSchedule()
    if (disposed) return
    const ms = refreshIntervalMs(
      weather.value?.kind,
      weather.value?.intensity,
      weather.value?.precipitation ?? 0,
    )
    timer = setTimeout(() => {
      void refresh({ quiet: true })
    }, ms)
  }

  function markFail(e: unknown) {
    if (weather.value) {
      error.value = UPDATE_FAIL
      return
    }
    error.value = e instanceof Error ? e.message : '获取天气失败'
  }

  async function fetchFor(geo: GeoResult) {
    // Manual / Shanghai fallback keep their labels; GPS/IP always reverse-geocode for 区-level names
    const next = await fetchWeather(
      geo.latitude,
      geo.longitude,
      geo.source === 'manual' || geo.source === 'fallback' ? geo.label : undefined,
    )
    if (geo.source === 'manual' && geo.label) {
      next.city = geo.label
    } else if (geo.label && geo.source === 'geolocation') {
      // Prefer district label from locate() reverse-geocode
      next.city = geo.label
    }
    weather.value = next
    saveWeatherCache(next)
    error.value = null
  }

  async function refresh(opts?: { quiet?: boolean; forceLocate?: boolean }) {
    if (!opts?.quiet) loading.value = true
    try {
      const saved = loadManualCity()
      if (saved) {
        const geo = toManualGeo(saved)
        coords.value = geo
        await fetchFor(geo)
      } else {
        // Startup / refresh: always re-run network locate unless quiet poll with known coords
        const reuse =
          opts?.quiet &&
          !opts.forceLocate &&
          coords.value &&
          coords.value.source !== 'fallback' &&
          coords.value.source !== 'cache'
        const geo = reuse ? coords.value! : await locate()
        await fetchFor(geo)
      }
    } catch (e) {
      markFail(e)
    } finally {
      loading.value = false
      scheduleNext()
    }
  }

  async function setCity(city: CityOption) {
    loading.value = true
    clearSchedule()
    try {
      const manual = {
        latitude: city.latitude,
        longitude: city.longitude,
        label: city.name,
      }
      saveManualCity(manual)
      const geo = toManualGeo(manual)
      coords.value = geo
      await fetchFor(geo)
    } catch (e) {
      markFail(e)
    } finally {
      loading.value = false
      scheduleNext()
    }
  }

  async function useAutoLocation() {
    clearManualCity()
    coords.value = null
    loading.value = true
    clearSchedule()
    try {
      // Same as startup auto-locate (IP); GPS only if IP fully fails
      const geo = await locate({ preferGps: true })
      await fetchFor(geo)
    } catch (e) {
      markFail(e)
    } finally {
      loading.value = false
      scheduleNext()
    }
  }

  onMounted(() => {
    disposed = false
    const hadCache = hydrateFromCache()
    if (hadCache) loading.value = false
    // Always auto-locate on open (unless user locked a manual city — handled inside refresh)
    void refresh({ quiet: hadCache, forceLocate: true })
    unsubRefresh = getDesktopApi().onRefreshWeather(() => {
      clearSchedule()
      void refresh({ forceLocate: true })
    })
  })

  onUnmounted(() => {
    disposed = true
    clearSchedule()
    unsubRefresh?.()
  })

  return {
    weather,
    loading,
    error,
    geoError,
    coords,
    refresh,
    setCity,
    useAutoLocation,
  }
}
