import type { WeatherSnapshot } from './weather'

const CACHE_KEY = 'weatherball.weatherCache'
const MAX_AGE_MS = 7 * 24 * 60 * 60 * 1000

function isValidSnapshot(data: unknown): data is WeatherSnapshot {
  if (!data || typeof data !== 'object') return false
  const w = data as Partial<WeatherSnapshot>
  return (
    typeof w.temperature === 'number' &&
    typeof w.weatherCode === 'number' &&
    typeof w.kind === 'string' &&
    typeof w.description === 'string' &&
    typeof w.city === 'string' &&
    typeof w.latitude === 'number' &&
    typeof w.longitude === 'number' &&
    typeof w.fetchedAt === 'number' &&
    Array.isArray(w.hourly)
  )
}

export function loadWeatherCache(): WeatherSnapshot | null {
  try {
    const raw = localStorage.getItem(CACHE_KEY)
    if (!raw) return null
    const data = JSON.parse(raw) as unknown
    if (!isValidSnapshot(data)) return null
    if (Date.now() - data.fetchedAt > MAX_AGE_MS) return null
    return {
      ...data,
      intensity: data.intensity ?? null,
      humidity: data.humidity ?? null,
      feelsLike: data.feelsLike ?? null,
      hourly: data.hourly ?? [],
    }
  } catch {
    return null
  }
}

export function saveWeatherCache(snapshot: WeatherSnapshot): void {
  try {
    localStorage.setItem(CACHE_KEY, JSON.stringify(snapshot))
  } catch {
    /* quota / private mode */
  }
}
