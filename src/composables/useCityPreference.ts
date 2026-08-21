import type { GeoResult } from './useGeolocation'

const STORAGE_KEY = 'weatherball.manualCity'

export type ManualCity = {
  latitude: number
  longitude: number
  label: string
}

export function loadManualCity(): ManualCity | null {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (!raw) return null
    const data = JSON.parse(raw) as Partial<ManualCity>
    if (
      typeof data.latitude !== 'number' ||
      typeof data.longitude !== 'number' ||
      typeof data.label !== 'string' ||
      !data.label.trim()
    ) {
      return null
    }
    return {
      latitude: data.latitude,
      longitude: data.longitude,
      label: data.label.trim(),
    }
  } catch {
    return null
  }
}

export function saveManualCity(city: ManualCity): void {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(city))
}

export function clearManualCity(): void {
  localStorage.removeItem(STORAGE_KEY)
}

export function toManualGeo(city: ManualCity): GeoResult {
  return {
    latitude: city.latitude,
    longitude: city.longitude,
    source: 'manual',
    label: city.label,
  }
}
