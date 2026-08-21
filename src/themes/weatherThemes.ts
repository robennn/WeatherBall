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

  if (kind === 'drizzle') {
    if (level === 'light') return { dropCount: 6, dropFast: false, flakeCount: 0 }
    if (level === 'heavy') return { dropCount: 12, dropFast: false, flakeCount: 0 }
    return { dropCount: 9, dropFast: false, flakeCount: 0 }
  }

  if (kind === 'rain') {
    if (level === 'light') return { dropCount: 10, dropFast: false, flakeCount: 0 }
    if (level === 'heavy') return { dropCount: 26, dropFast: true, flakeCount: 0 }
    return { dropCount: 16, dropFast: false, flakeCount: 0 }
  }

  if (kind === 'storm') {
    if (level === 'light') return { dropCount: 14, dropFast: true, flakeCount: 0 }
    if (level === 'heavy') return { dropCount: 28, dropFast: true, flakeCount: 0 }
    return { dropCount: 20, dropFast: true, flakeCount: 0 }
  }

  if (kind === 'snow') {
    if (level === 'light') return { dropCount: 0, dropFast: false, flakeCount: 10 }
    if (level === 'heavy') return { dropCount: 0, dropFast: false, flakeCount: 28 }
    return { dropCount: 0, dropFast: false, flakeCount: 18 }
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
): OrbTheme {
  if (kind === 'loading') return loadingTheme
  if (kind === 'error') return errorTheme

  const base = baseThemes[kind]
  const particles = precipParticles(kind, intensity)
  const theme: OrbTheme = { ...base, ...particles }

  if (isDay) return theme

  return {
    ...theme,
    glow: theme.glow.replace(/[\d.]+\)$/, (m) => `${(parseFloat(m) * 0.65).toFixed(2)})`),
  }
}
