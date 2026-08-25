import type { PrecipIntensity, WeatherKind } from '../services/weather'

/** Visual scene inside the glass orb */
export type OrbScene = 'sunny' | 'cloudy' | 'overcast' | 'rain' | 'snow' | 'storm'

export type OrbTheme = {
  glow: string
  waterA: string
  waterB: string
  accent: string
  scene: OrbScene
  /** Particle counts / speed hints for precip scenes */
  dropCount: number
  dropFast: boolean
  flakeCount: number
}

const baseThemes: Record<Exclude<WeatherKind, 'loading' | 'error'>, Omit<OrbTheme, 'dropCount' | 'dropFast' | 'flakeCount'>> = {
  clear: {
    glow: 'rgba(255,190,90,.45)',
    waterA: '#2aa8a2',
    waterB: '#3ec6c0',
    accent: '#ffc46b',
    scene: 'sunny',
  },
  cloudy: {
    glow: 'rgba(160,195,240,.35)',
    waterA: '#3d7ba3',
    waterB: '#5a9cc4',
    accent: '#a8c6ec',
    scene: 'cloudy',
  },
  drizzle: {
    glow: 'rgba(90,150,230,.32)',
    waterA: '#2c5d92',
    waterB: '#3f78b4',
    accent: '#7fb4f0',
    scene: 'rain',
  },
  rain: {
    glow: 'rgba(90,150,230,.4)',
    waterA: '#2c5d92',
    waterB: '#3f78b4',
    accent: '#7fb4f0',
    scene: 'rain',
  },
  snow: {
    glow: 'rgba(200,230,255,.42)',
    waterA: '#7fb8d8',
    waterB: '#a5d4ec',
    accent: '#dceefc',
    scene: 'snow',
  },
  storm: {
    glow: 'rgba(150,120,240,.42)',
    waterA: '#2e3560',
    waterB: '#414a82',
    accent: '#b9a6f5',
    scene: 'storm',
  },
  fog: {
    glow: 'rgba(190,200,215,.28)',
    waterA: '#4c6a80',
    waterB: '#68859a',
    accent: '#c3ccd8',
    scene: 'overcast',
  },
}

function precipParticles(
  kind: WeatherKind,
  intensity: PrecipIntensity | null,
): Pick<OrbTheme, 'dropCount' | 'dropFast' | 'flakeCount'> {
  const level: PrecipIntensity = intensity ?? 'moderate'

  // Keep counts modest — WebView2 GPU/compositor memory scales with animated layers
  if (kind === 'drizzle') {
    if (level === 'light') return { dropCount: 4, dropFast: false, flakeCount: 0 }
    if (level === 'heavy') return { dropCount: 8, dropFast: false, flakeCount: 0 }
    return { dropCount: 6, dropFast: false, flakeCount: 0 }
  }

  if (kind === 'rain') {
    if (level === 'light') return { dropCount: 6, dropFast: false, flakeCount: 0 }
    if (level === 'heavy') return { dropCount: 14, dropFast: true, flakeCount: 0 }
    return { dropCount: 10, dropFast: false, flakeCount: 0 }
  }

  if (kind === 'storm') {
    if (level === 'light') return { dropCount: 8, dropFast: true, flakeCount: 0 }
    if (level === 'heavy') return { dropCount: 16, dropFast: true, flakeCount: 0 }
    return { dropCount: 12, dropFast: true, flakeCount: 0 }
  }

  if (kind === 'snow') {
    if (level === 'light') return { dropCount: 0, dropFast: false, flakeCount: 6 }
    if (level === 'heavy') return { dropCount: 0, dropFast: false, flakeCount: 14 }
    return { dropCount: 0, dropFast: false, flakeCount: 10 }
  }

  return { dropCount: 0, dropFast: false, flakeCount: 0 }
}

export const loadingTheme: OrbTheme = {
  glow: 'rgba(148,163,184,.28)',
  waterA: '#475569',
  waterB: '#64748b',
  accent: '#94a3b8',
  scene: 'overcast',
  dropCount: 0,
  dropFast: false,
  flakeCount: 0,
}

export const errorTheme: OrbTheme = {
  glow: 'rgba(168,162,158,.25)',
  waterA: '#57534e',
  waterB: '#78716c',
  accent: '#a8a29e',
  scene: 'overcast',
  dropCount: 0,
  dropFast: false,
  flakeCount: 0,
}

export function getOrbTheme(
  kind: WeatherKind,
  isDay: boolean,
  intensity: PrecipIntensity | null = null,
  temperature: number | null = null,
): OrbTheme {
  if (kind === 'loading') return loadingTheme
  if (kind === 'error') return errorTheme

  const base = baseThemes[kind]
  const particles = precipParticles(kind, intensity)
  let theme: OrbTheme = { ...base, ...particles }

  if (!isDay) {
    theme = { ...theme, ...nightPalette(kind) }
  }

  return {
    ...theme,
    waterA: tintWaterHex(theme.waterA, kind, isDay, temperature),
    waterB: tintWaterHex(theme.waterB, kind, isDay, temperature),
  }
}

function clamp01(n: number) {
  return Math.min(1, Math.max(0, n))
}

function parseHex(hex: string): [number, number, number] {
  const h = hex.replace('#', '')
  return [
    parseInt(h.slice(0, 2), 16),
    parseInt(h.slice(2, 4), 16),
    parseInt(h.slice(4, 6), 16),
  ]
}

function toHex(r: number, g: number, b: number) {
  const h = (n: number) =>
    Math.round(Math.min(255, Math.max(0, n)))
      .toString(16)
      .padStart(2, '0')
  return `#${h(r)}${h(g)}${h(b)}`
}

function mixHex(a: string, b: string, t: number) {
  const [ar, ag, ab] = parseHex(a)
  const [br, bg, bb] = parseHex(b)
  const k = clamp01(t)
  return toHex(ar + (br - ar) * k, ag + (bg - ag) * k, ab + (bb - ab) * k)
}

function tintWaterHex(
  hex: string,
  kind: Exclude<WeatherKind, 'loading' | 'error'>,
  isDay: boolean,
  temperature: number | null,
): string {
  if (temperature == null || Number.isNaN(temperature)) return hex
  const signed = Math.max(-1, Math.min(1, (temperature - 18) / 18))
  if (Math.abs(signed) < 0.03) return hex

  let coldS = 0.26
  let warmS = 0.28
  if (kind === 'clear') {
    coldS = 0.52
    warmS = 0.7
  } else if (kind === 'cloudy') {
    coldS = 0.3
    warmS = 0.36
  } else if (kind === 'fog') {
    coldS = 0.26
    warmS = 0.28
  } else if (kind === 'drizzle' || kind === 'rain') {
    coldS = 0.2
    warmS = 0.14
  } else if (kind === 'snow') {
    coldS = 0.18
    warmS = 0.08
  } else if (kind === 'storm') {
    coldS = 0.16
    warmS = 0.1
  }

  let strength = signed > 0 ? warmS : coldS
  if (!isDay) strength *= signed > 0 ? 0.72 : 0.85
  const warm = isDay ? '#d0822a' : '#9a622c'
  const cold = isDay ? '#2ac8d8' : '#3a90b8'
  const target = signed > 0 ? warm : cold
  return mixHex(hex, target, Math.abs(signed) * strength)
}

function nightPalette(
  kind: Exclude<WeatherKind, 'loading' | 'error'>,
): Pick<OrbTheme, 'glow' | 'waterA' | 'waterB' | 'accent'> {
  switch (kind) {
    case 'clear':
      return {
        glow: 'rgba(170,195,255,0)',
        waterA: '#1a3a5c',
        waterB: '#24507a',
        accent: '#c5d4ff',
      }
    case 'cloudy':
      return {
        glow: 'rgba(110,145,195,.28)',
        waterA: '#1c3548',
        waterB: '#2a4a62',
        accent: '#9bb4d4',
      }
    case 'fog':
      return {
        glow: 'rgba(90,110,145,.22)',
        waterA: '#1a2c38',
        waterB: '#243848',
        accent: '#8a9aaa',
      }
    case 'drizzle':
      return {
        glow: 'rgba(80,130,210,.28)',
        waterA: '#12243e',
        waterB: '#1c3860',
        accent: '#8fb0e8',
      }
    case 'rain':
      return {
        glow: 'rgba(80,130,210,.34)',
        waterA: '#12243e',
        waterB: '#1c3860',
        accent: '#8fb0e8',
      }
    case 'snow':
      return {
        glow: 'rgba(150,180,230,.36)',
        waterA: '#2a4860',
        waterB: '#3a6080',
        accent: '#c5d8ec',
      }
    case 'storm':
      return {
        glow: 'rgba(130,110,220,.38)',
        waterA: '#12102a',
        waterB: '#221e48',
        accent: '#c4b4f5',
      }
  }
}
