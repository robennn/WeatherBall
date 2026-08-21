import { reverseGeocode } from './geocode'

export type WeatherKind =
  | 'clear'
  | 'cloudy'
  | 'drizzle'
  | 'rain'
  | 'snow'
  | 'storm'
  | 'fog'
  | 'loading'
  | 'error'

/** Precipitation strength for visuals (rain / drizzle / storm) */
export type PrecipIntensity = 'light' | 'moderate' | 'heavy'

export type HourlyPoint = {
  /** Local hour label, e.g. "14" */
  hour: string
  temperature: number
  weatherCode: number
}

export type WeatherSnapshot = {
  temperature: number
  /** Apparent / feels-like temperature °C */
  feelsLike: number | null
  weatherCode: number
  isDay: boolean
  precipitation: number
  humidity: number | null
  windSpeed: number
  kind: WeatherKind
  description: string
  intensity: PrecipIntensity | null
  hourly: HourlyPoint[]
  city: string
  latitude: number
  longitude: number
  fetchedAt: number
}

export type WeatherMapping = {
  kind: WeatherKind
  description: string
  intensity: PrecipIntensity | null
}

/** WMO Weather interpretation codes → kind + Chinese label + intensity */
export function mapWeatherCode(code: number): WeatherMapping {
  if (code === 0) return { kind: 'clear', description: '晴朗', intensity: null }
  if (code === 1) return { kind: 'clear', description: '大致晴朗', intensity: null }
  if (code === 2) return { kind: 'cloudy', description: '局部多云', intensity: null }
  if (code === 3) return { kind: 'cloudy', description: '阴天', intensity: null }
  if (code === 45 || code === 48) return { kind: 'fog', description: '雾', intensity: null }

  if (code === 51) return { kind: 'drizzle', description: '小毛毛雨', intensity: 'light' }
  if (code === 53) return { kind: 'drizzle', description: '毛毛雨', intensity: 'moderate' }
  if (code === 55) return { kind: 'drizzle', description: '浓毛毛雨', intensity: 'heavy' }
  if (code === 56) return { kind: 'drizzle', description: '轻冻毛毛雨', intensity: 'light' }
  if (code === 57) return { kind: 'drizzle', description: '冻毛毛雨', intensity: 'moderate' }

  if (code === 61) return { kind: 'rain', description: '小雨', intensity: 'light' }
  if (code === 63) return { kind: 'rain', description: '中雨', intensity: 'moderate' }
  if (code === 65) return { kind: 'rain', description: '大雨', intensity: 'heavy' }
  if (code === 66) return { kind: 'rain', description: '轻冻雨', intensity: 'light' }
  if (code === 67) return { kind: 'rain', description: '冻雨', intensity: 'heavy' }

  if (code === 71) return { kind: 'snow', description: '小雪', intensity: 'light' }
  if (code === 73) return { kind: 'snow', description: '中雪', intensity: 'moderate' }
  if (code === 75) return { kind: 'snow', description: '大雪', intensity: 'heavy' }
  if (code === 77) return { kind: 'snow', description: '米雪', intensity: 'light' }

  if (code === 80) return { kind: 'rain', description: '小阵雨', intensity: 'light' }
  if (code === 81) return { kind: 'rain', description: '阵雨', intensity: 'moderate' }
  if (code === 82) return { kind: 'rain', description: '强阵雨', intensity: 'heavy' }

  if (code === 85) return { kind: 'snow', description: '小阵雪', intensity: 'light' }
  if (code === 86) return { kind: 'snow', description: '强阵雪', intensity: 'heavy' }

  if (code === 95) return { kind: 'storm', description: '雷阵雨', intensity: 'moderate' }
  if (code === 96) return { kind: 'storm', description: '雷暴伴冰雹', intensity: 'heavy' }
  if (code === 99) return { kind: 'storm', description: '强雷暴伴冰雹', intensity: 'heavy' }

  return { kind: 'cloudy', description: '多云', intensity: null }
}

/** Optionally bump intensity using precipitation rate (mm) */
export function refineIntensity(
  kind: WeatherKind,
  intensity: PrecipIntensity | null,
  precipitationMm: number,
): PrecipIntensity | null {
  if (kind !== 'rain' && kind !== 'drizzle' && kind !== 'snow' && kind !== 'storm') {
    return null
  }
  let level: PrecipIntensity = intensity ?? 'moderate'
  if (precipitationMm >= 8) level = 'heavy'
  else if (precipitationMm >= 2.5 && level === 'light') level = 'moderate'
  return level
}

const HOURLY_COUNT = 12

type OpenMeteoResponse = {
  current: {
    temperature_2m: number
    apparent_temperature?: number
    weather_code: number
    is_day: number
    precipitation: number
    relative_humidity_2m: number
    wind_speed_10m: number
    time?: string
  }
  hourly?: {
    time: string[]
    temperature_2m: number[]
    weather_code?: number[]
  }
}

function parseHourly(data: OpenMeteoResponse): HourlyPoint[] {
  const times = data.hourly?.time
  const temps = data.hourly?.temperature_2m
  const codes = data.hourly?.weather_code
  if (!times?.length || !temps?.length) return []

  const now = Date.now()
  let start = times.findIndex((t) => {
    const ms = Date.parse(t)
    return Number.isFinite(ms) && ms >= now - 30 * 60 * 1000
  })
  if (start < 0) start = 0

  const points: HourlyPoint[] = []
  for (let i = start; i < times.length && points.length < HOURLY_COUNT; i++) {
    const temp = temps[i]
    if (temp == null || !Number.isFinite(temp)) continue
    const d = new Date(times[i])
    points.push({
      hour: String(d.getHours()).padStart(2, '0'),
      temperature: temp,
      weatherCode: codes?.[i] ?? data.current.weather_code,
    })
  }
  return points
}

export async function fetchWeather(
  latitude: number,
  longitude: number,
  fallbackCity?: string,
): Promise<WeatherSnapshot> {
  const params = new URLSearchParams({
    latitude: String(latitude),
    longitude: String(longitude),
    current:
      'temperature_2m,apparent_temperature,weather_code,is_day,precipitation,relative_humidity_2m,wind_speed_10m',
    hourly: 'temperature_2m,weather_code',
    forecast_hours: String(HOURLY_COUNT + 2),
    timezone: 'auto',
  })

  const [weatherRes, city] = await Promise.all([
    fetch(`https://api.open-meteo.com/v1/forecast?${params}`),
    fallbackCity
      ? Promise.resolve(fallbackCity)
      : reverseGeocode(latitude, longitude),
  ])

  if (!weatherRes.ok) {
    throw new Error(`天气服务异常 (${weatherRes.status})`)
  }

  const data = (await weatherRes.json()) as OpenMeteoResponse
  const mapped = mapWeatherCode(data.current.weather_code)
  const precip = data.current.precipitation ?? 0
  const intensity = refineIntensity(mapped.kind, mapped.intensity, precip)

  return {
    temperature: data.current.temperature_2m,
    feelsLike:
      data.current.apparent_temperature != null &&
      Number.isFinite(data.current.apparent_temperature)
        ? data.current.apparent_temperature
        : null,
    weatherCode: data.current.weather_code,
    isDay: data.current.is_day === 1,
    precipitation: precip,
    humidity:
      data.current.relative_humidity_2m != null
        ? Math.round(data.current.relative_humidity_2m)
        : null,
    windSpeed: data.current.wind_speed_10m,
    kind: mapped.kind,
    description: mapped.description,
    intensity,
    hourly: parseHourly(data),
    city,
    latitude,
    longitude,
    fetchedAt: Date.now(),
  }
}
