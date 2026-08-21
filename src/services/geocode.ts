type GeoCodeResponse = {
  locality?: string
  city?: string
  principalSubdivision?: string
  localityInfo?: {
    administrative?: Array<{
      name?: string
      description?: string
      adminLevel?: number
      isoName?: string
    }>
  }
}

/** Prefer district / county (区县) over city when BigDataCloud provides admin levels */
export async function reverseGeocode(latitude: number, longitude: number): Promise<string> {
  try {
    const params = new URLSearchParams({
      latitude: String(latitude),
      longitude: String(longitude),
      localityLanguage: 'zh-Hans',
    })
    const res = await fetch(
      `https://api.bigdatacloud.net/data/reverse-geocode-client?${params}`,
    )
    if (!res.ok) return '未知位置'
    const data = (await res.json()) as GeoCodeResponse

    const admins = data.localityInfo?.administrative ?? []
    // Higher adminLevel = more specific (e.g. 区/县). Skip country-level.
    const ranked = admins
      .filter((a) => a.name && typeof a.adminLevel === 'number' && a.adminLevel >= 5)
      .sort((a, b) => (b.adminLevel ?? 0) - (a.adminLevel ?? 0))

    const district = ranked[0]?.name
    const name = (district || data.locality || data.city || data.principalSubdivision || '未知位置')
      .normalize('NFC')
      .replace(/\u00a0/g, ' ')
      .trim()
    return name || '未知位置'
  } catch {
    return '未知位置'
  }
}
