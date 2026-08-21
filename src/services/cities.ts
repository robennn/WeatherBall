export type CityOption = {
  name: string
  latitude: number
  longitude: number
  /** Optional region hint, e.g. 广东 */
  region?: string
}

/** Curated mainland / nearby cities for one-tap pick */
export const COMMON_CITIES: CityOption[] = [
  { name: '北京', latitude: 39.9042, longitude: 116.4074 },
  { name: '上海', latitude: 31.2304, longitude: 121.4737 },
  { name: '广州', latitude: 23.1291, longitude: 113.2644, region: '广东' },
  { name: '深圳', latitude: 22.5431, longitude: 114.0579, region: '广东' },
  { name: '杭州', latitude: 30.2741, longitude: 120.1551, region: '浙江' },
  { name: '南京', latitude: 32.0603, longitude: 118.7969, region: '江苏' },
  { name: '苏州', latitude: 31.2989, longitude: 120.5853, region: '江苏' },
  { name: '成都', latitude: 30.5728, longitude: 104.0668, region: '四川' },
  { name: '重庆', latitude: 29.563, longitude: 106.5516 },
  { name: '武汉', latitude: 30.5928, longitude: 114.3055, region: '湖北' },
  { name: '西安', latitude: 34.3416, longitude: 108.9398, region: '陕西' },
  { name: '天津', latitude: 39.3434, longitude: 117.3616 },
  { name: '长沙', latitude: 28.2282, longitude: 112.9388, region: '湖南' },
  { name: '郑州', latitude: 34.7466, longitude: 113.6254, region: '河南' },
  { name: '青岛', latitude: 36.0671, longitude: 120.3826, region: '山东' },
  { name: '厦门', latitude: 24.4798, longitude: 118.0894, region: '福建' },
  { name: '昆明', latitude: 25.0389, longitude: 102.7183, region: '云南' },
  { name: '大连', latitude: 38.914, longitude: 121.6147, region: '辽宁' },
  { name: '合肥', latitude: 31.8206, longitude: 117.2272, region: '安徽' },
  { name: '福州', latitude: 26.0745, longitude: 119.2965, region: '福建' },
  { name: '香港', latitude: 22.3193, longitude: 114.1694 },
  { name: '台北', latitude: 25.033, longitude: 121.5654 },
]

type OpenMeteoGeoResponse = {
  results?: Array<{
    name: string
    latitude: number
    longitude: number
    admin1?: string
    country_code?: string
  }>
}

export function filterCommonCities(query: string): CityOption[] {
  const q = query.trim().toLowerCase()
  if (!q) return COMMON_CITIES
  return COMMON_CITIES.filter(
    (c) =>
      c.name.toLowerCase().includes(q) ||
      (c.region?.toLowerCase().includes(q) ?? false),
  )
}

/** Search via Open-Meteo geocoding (no API key) */
export async function searchCities(query: string): Promise<CityOption[]> {
  const q = query.trim()
  if (q.length < 1) return []

  const local = filterCommonCities(q)
  const params = new URLSearchParams({
    name: q,
    count: '8',
    language: 'zh',
    format: 'json',
  })

  try {
    const res = await fetch(
      `https://geocoding-api.open-meteo.com/v1/search?${params}`,
      { signal: AbortSignal.timeout(8000) },
    )
    if (!res.ok) return local

    const data = (await res.json()) as OpenMeteoGeoResponse
    const remote: CityOption[] = (data.results ?? []).map((r) => ({
      name: r.name,
      latitude: r.latitude,
      longitude: r.longitude,
      region: r.admin1,
    }))

    // Prefer local curated hits first, then unique remote
    const seen = new Set(local.map((c) => `${c.name}|${c.latitude.toFixed(2)}`))
    const merged = [...local]
    for (const c of remote) {
      const key = `${c.name}|${c.latitude.toFixed(2)}`
      if (seen.has(key)) continue
      seen.add(key)
      merged.push(c)
    }
    return merged.slice(0, 12)
  } catch {
    return local
  }
}

export function cityDisplayName(city: CityOption): string {
  return city.region ? `${city.name} · ${city.region}` : city.name
}
