import { DISTRICTS } from './districts'

export type CityOption = {
  name: string
  latitude: number
  longitude: number
  /** Optional region hint, e.g. 广东 / parent city for 区县 */
  region?: string
}

/** Capitals + common prefecture cities (incl. all Guangxi prefectures). */
export const COMMON_CITIES: CityOption[] = [
  { name: '北京', latitude: 39.9042, longitude: 116.4074 },
  { name: '天津', latitude: 39.3434, longitude: 117.3616 },
  { name: '上海', latitude: 31.2304, longitude: 121.4737 },
  { name: '重庆', latitude: 29.563, longitude: 106.5516 },
  { name: '石家庄', latitude: 38.0428, longitude: 114.5149, region: '河北' },
  { name: '太原', latitude: 37.8706, longitude: 112.5489, region: '山西' },
  { name: '呼和浩特', latitude: 40.8426, longitude: 111.7492, region: '内蒙古' },
  { name: '沈阳', latitude: 41.8057, longitude: 123.4315, region: '辽宁' },
  { name: '大连', latitude: 38.914, longitude: 121.6147, region: '辽宁' },
  { name: '长春', latitude: 43.8171, longitude: 125.3235, region: '吉林' },
  { name: '哈尔滨', latitude: 45.8038, longitude: 126.535, region: '黑龙江' },
  { name: '南京', latitude: 32.0603, longitude: 118.7969, region: '江苏' },
  { name: '苏州', latitude: 31.2989, longitude: 120.5853, region: '江苏' },
  { name: '无锡', latitude: 31.4912, longitude: 120.3119, region: '江苏' },
  { name: '常州', latitude: 31.8107, longitude: 119.9741, region: '江苏' },
  { name: '南通', latitude: 32.0162, longitude: 120.8946, region: '江苏' },
  { name: '扬州', latitude: 32.3932, longitude: 119.4129, region: '江苏' },
  { name: '徐州', latitude: 34.2058, longitude: 117.2841, region: '江苏' },
  { name: '杭州', latitude: 30.2741, longitude: 120.1551, region: '浙江' },
  { name: '宁波', latitude: 29.8683, longitude: 121.544, region: '浙江' },
  { name: '温州', latitude: 28.0006, longitude: 120.6994, region: '浙江' },
  { name: '嘉兴', latitude: 30.7539, longitude: 120.7585, region: '浙江' },
  { name: '金华', latitude: 29.0788, longitude: 119.6474, region: '浙江' },
  { name: '绍兴', latitude: 30.0023, longitude: 120.5819, region: '浙江' },
  { name: '合肥', latitude: 31.8206, longitude: 117.2272, region: '安徽' },
  { name: '福州', latitude: 26.0745, longitude: 119.2965, region: '福建' },
  { name: '厦门', latitude: 24.4798, longitude: 118.0894, region: '福建' },
  { name: '泉州', latitude: 24.874, longitude: 118.6757, region: '福建' },
  { name: '南昌', latitude: 28.682, longitude: 115.8581, region: '江西' },
  { name: '济南', latitude: 36.6512, longitude: 117.1201, region: '山东' },
  { name: '青岛', latitude: 36.0671, longitude: 120.3826, region: '山东' },
  { name: '烟台', latitude: 37.4638, longitude: 121.4479, region: '山东' },
  { name: '潍坊', latitude: 36.7069, longitude: 119.1619, region: '山东' },
  { name: '郑州', latitude: 34.7466, longitude: 113.6254, region: '河南' },
  { name: '洛阳', latitude: 34.6197, longitude: 112.454, region: '河南' },
  { name: '武汉', latitude: 30.5928, longitude: 114.3055, region: '湖北' },
  { name: '长沙', latitude: 28.2282, longitude: 112.9388, region: '湖南' },
  { name: '广州', latitude: 23.1291, longitude: 113.2644, region: '广东' },
  { name: '深圳', latitude: 22.5431, longitude: 114.0579, region: '广东' },
  { name: '佛山', latitude: 23.0215, longitude: 113.1214, region: '广东' },
  { name: '东莞', latitude: 23.0207, longitude: 113.7518, region: '广东' },
  { name: '珠海', latitude: 22.271, longitude: 113.5767, region: '广东' },
  { name: '中山', latitude: 22.5159, longitude: 113.3928, region: '广东' },
  { name: '惠州', latitude: 23.1115, longitude: 114.4152, region: '广东' },
  { name: '汕头', latitude: 23.3541, longitude: 116.682, region: '广东' },
  { name: '南宁', latitude: 22.817, longitude: 108.3669, region: '广西' },
  { name: '柳州', latitude: 24.3264, longitude: 109.4281, region: '广西' },
  { name: '桂林', latitude: 25.2736, longitude: 110.29, region: '广西' },
  { name: '梧州', latitude: 23.4769, longitude: 111.297, region: '广西' },
  { name: '北海', latitude: 21.4733, longitude: 109.12, region: '广西' },
  { name: '防城港', latitude: 21.6867, longitude: 108.3547, region: '广西' },
  { name: '钦州', latitude: 21.95, longitude: 108.6194, region: '广西' },
  { name: '贵港', latitude: 23.0936, longitude: 109.6096, region: '广西' },
  { name: '玉林', latitude: 22.6293, longitude: 110.1545, region: '广西' },
  { name: '百色', latitude: 23.9023, longitude: 106.6186, region: '广西' },
  { name: '贺州', latitude: 24.4141, longitude: 111.552, region: '广西' },
  { name: '河池', latitude: 24.6929, longitude: 108.085, region: '广西' },
  { name: '来宾', latitude: 23.7338, longitude: 109.2292, region: '广西' },
  { name: '崇左', latitude: 22.4167, longitude: 107.3649, region: '广西' },
  { name: '海口', latitude: 20.044, longitude: 110.199, region: '海南' },
  { name: '三亚', latitude: 18.2528, longitude: 109.5117, region: '海南' },
  { name: '成都', latitude: 30.5728, longitude: 104.0668, region: '四川' },
  { name: '绵阳', latitude: 31.4678, longitude: 104.6791, region: '四川' },
  { name: '贵阳', latitude: 26.647, longitude: 106.6302, region: '贵州' },
  { name: '昆明', latitude: 25.0389, longitude: 102.7183, region: '云南' },
  { name: '大理', latitude: 25.6065, longitude: 100.2676, region: '云南' },
  { name: '丽江', latitude: 26.855, longitude: 100.227, region: '云南' },
  { name: '拉萨', latitude: 29.652, longitude: 91.1721, region: '西藏' },
  { name: '西安', latitude: 34.3416, longitude: 108.9398, region: '陕西' },
  { name: '兰州', latitude: 36.0611, longitude: 103.8343, region: '甘肃' },
  { name: '西宁', latitude: 36.6171, longitude: 101.7782, region: '青海' },
  { name: '银川', latitude: 38.4872, longitude: 106.2309, region: '宁夏' },
  { name: '乌鲁木齐', latitude: 43.8256, longitude: 87.6168, region: '新疆' },
  { name: '香港', latitude: 22.3193, longitude: 114.1694 },
  { name: '澳门', latitude: 22.1987, longitude: 113.5439 },
  { name: '台北', latitude: 25.033, longitude: 121.5654 },
]

const PROVINCES = new Set([
  '河北', '山西', '辽宁', '吉林', '黑龙江', '江苏', '浙江', '安徽', '福建', '江西',
  '山东', '河南', '湖北', '湖南', '广东', '海南', '四川', '贵州', '云南', '陕西',
  '甘肃', '青海', '台湾', '内蒙古', '广西', '西藏', '宁夏', '新疆',
  '北京', '天津', '上海', '重庆', '香港', '澳门',
])

type GeoHit = {
  name: string
  latitude: number
  longitude: number
  admin1?: string
  admin2?: string
  country_code?: string
  feature_code?: string
  population?: number
}

type OpenMeteoGeoResponse = {
  results?: GeoHit[]
}

function normalizeAdmin(s: string): string {
  return s
    .replace(/\u00a0/g, ' ')
    .replace(/特别行政区|维吾尔自治区|壮族自治区|回族自治区|自治区|省|市/g, '')
    .trim()
}

function isCjk(s: string): boolean {
  return /[\u4e00-\u9fff]/.test(s)
}

function keepHit(q: string, h: GeoHit): boolean {
  const qn = normalizeAdmin(q)
  const name = normalizeAdmin(h.name)
  const admin1 = normalizeAdmin(h.admin1 ?? '')
  const admin2 = normalizeAdmin(h.admin2 ?? '')
  const country = h.country_code ?? ''
  const feat = h.feature_code ?? ''
  const pop = h.population ?? 0

  if (isCjk(q) && country && !['CN', 'HK', 'MO', 'TW'].includes(country)) return false
  if (!name.includes(qn) && !admin1.includes(qn) && !admin2.includes(qn) && !qn.includes(name)) {
    return false
  }
  if (PROVINCES.has(name) && admin1 && admin1 !== name && !admin1.includes(name) && !name.includes(admin1)) {
    return false
  }
  if (feat === 'PPL' && pop < 80_000 && !isAdminName(h.name)) return false
  if ((feat.startsWith('PPLX') || feat === 'PPLQ') && !isAdminName(h.name)) return false
  return true
}

function isAdminName(name: string): boolean {
  return /(?:区|县|旗|自治县|自治旗|新区)$/.test(name)
}

export function filterCommonCities(query: string): CityOption[] {
  const q = normalizeAdmin(query.trim())
  if (!q) return COMMON_CITIES
  return COMMON_CITIES.filter(
    (c) =>
      normalizeAdmin(c.name).includes(q) ||
      (c.region != null && (normalizeAdmin(c.region).includes(q) || q.includes(normalizeAdmin(c.region)))),
  )
}

function filterDistricts(query: string): CityOption[] {
  const q = normalizeAdmin(query.trim())
  if (!q || [...q].length < 2 || PROVINCES.has(q)) return []
  return DISTRICTS.filter((d) => {
    const name = normalizeAdmin(d.name)
    const city = normalizeAdmin(d.city)
    return name.includes(q) || q.includes(name) || city === q
  }).map((d) => ({
    name: d.name,
    latitude: d.latitude,
    longitude: d.longitude,
    region: d.city,
  }))
}

type PhotonFeature = {
  properties: {
    name?: string
    city?: string
    state?: string
    countrycode?: string
    type?: string
    osm_type?: string
    osm_key?: string
    osm_value?: string
  }
  geometry?: { coordinates?: number[] }
}

function keepPhoton(q: string, f: PhotonFeature): boolean {
  const p = f.properties
  const name = p.name ?? ''
  if (!name) return false
  const cc = p.countrycode ?? ''
  if (isCjk(q) && cc && !['CN', 'HK', 'MO', 'TW'].includes(cc)) return false
  const osmKey = p.osm_key ?? ''
  const osmVal = p.osm_value ?? ''
  if (['amenity', 'highway', 'railway', 'office', 'landuse', 'shop', 'tourism', 'leisure'].includes(osmKey)) {
    return false
  }
  if (['village', 'hamlet', 'isolated_dwelling'].includes(osmVal)) return false
  const kind = p.type ?? ''
  const admin =
    ['city', 'district', 'county', 'town', 'state'].includes(kind) ||
    (osmKey === 'place' &&
      ['city', 'district', 'county', 'town', 'suburb', 'municipality', 'city_district'].includes(osmVal)) ||
    isAdminName(name)
  if (!admin) return false
  const qn = normalizeAdmin(q)
  const nn = normalizeAdmin(name)
  const city = normalizeAdmin(p.city ?? '')
  const state = normalizeAdmin(p.state ?? '')
  if (!nn.includes(qn) && !city.includes(qn) && !state.includes(qn) && !qn.includes(nn)) return false
  if (PROVINCES.has(nn) && state && state !== nn && !state.includes(nn) && !nn.includes(state)) return false
  return true
}

function photonToCity(f: PhotonFeature): CityOption | null {
  const coords = f.geometry?.coordinates
  if (!coords || coords.length < 2) return null
  const name = f.properties.name?.trim()
  if (!name) return null
  const regionRaw = f.properties.city || f.properties.state || ''
  const region = regionRaw ? normalizeAdmin(regionRaw) : ''
  return {
    name,
    latitude: coords[1],
    longitude: coords[0],
    region: region && region !== normalizeAdmin(name) ? region : undefined,
  }
}

async function searchPhoton(q: string): Promise<CityOption[]> {
  const res = await fetch(`https://photon.komoot.io/api/?q=${encodeURIComponent(q)}&limit=12`, {
    signal: AbortSignal.timeout(8000),
  })
  if (!res.ok) return []
  const data = (await res.json()) as { features?: PhotonFeature[] }
  return (data.features ?? [])
    .filter((f) => keepPhoton(q, f))
    .map(photonToCity)
    .filter((c): c is CityOption => c != null)
}

async function searchOpenMeteo(q: string): Promise<CityOption[]> {
  const params = new URLSearchParams({
    name: q,
    count: '20',
    language: 'zh',
    format: 'json',
  })
  const res = await fetch(`https://geocoding-api.open-meteo.com/v1/search?${params}`, {
    signal: AbortSignal.timeout(8000),
  })
  if (!res.ok) return []
  const data = (await res.json()) as OpenMeteoGeoResponse
  return (data.results ?? [])
    .filter((r) => keepHit(q, r))
    .map((r) => ({
      name: r.name,
      latitude: r.latitude,
      longitude: r.longitude,
      region: r.admin1 || r.admin2,
    }))
}

/** Local cities/districts first; CJK uses Photon (Open-Meteo misses 区县). */
export async function searchCities(query: string): Promise<CityOption[]> {
  const q = query.trim()
  if (q.length < 1) return []

  const local = [...filterCommonCities(q), ...filterDistricts(q)]
  try {
    const remote = isCjk(q)
      ? await searchPhoton(q).then((hits) => (hits.length ? hits : searchOpenMeteo(q)))
      : await searchOpenMeteo(q)

    const seen = new Set(local.map((c) => `${normalizeAdmin(c.name)}|${c.latitude.toFixed(2)}`))
    const merged = [...local]
    for (const c of remote) {
      const key = `${normalizeAdmin(c.name)}|${c.latitude.toFixed(2)}`
      if (seen.has(key)) continue
      seen.add(key)
      merged.push(c)
    }
    return merged.slice(0, 24)
  } catch {
    return local
  }
}

export function cityDisplayName(city: CityOption): string {
  const name = city.name.trim()
  const region = city.region ? normalizeAdmin(city.region) : ''
  if (!region || region === name || name.includes(region) || region.includes(name)) {
    return name
  }
  return `${name} · ${region}`
}
