<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import WeatherBall from './components/WeatherBall.vue'
import WeatherDetail from './components/WeatherDetail.vue'
import { useWeather } from './composables/useWeather'
import type { PrecipIntensity, WeatherKind } from './services/weather'
import type { CityOption } from './services/cities'
import { getOrbTheme } from './themes/weatherThemes'
import { getDesktopApi } from './platform'
import {
  applyDayNightPref,
  cycleDayNightPref,
  loadDayNightPref,
  saveDayNightPref,
  type DayNightPref,
} from './services/appSettings'

const COMPACT = { w: 160, h: 204 }
const EXPANDED = { w: 160, h: 520 }
const CITY_PICKER = { w: 160, h: 520 }

type PreviewItem = {
  kind: WeatherKind
  intensity: PrecipIntensity | null
  label: string
}

const PREVIEW_ITEMS: PreviewItem[] = [
  { kind: 'clear', intensity: null, label: '晴' },
  { kind: 'cloudy', intensity: null, label: '多云' },
  { kind: 'fog', intensity: null, label: '阴' },
  { kind: 'drizzle', intensity: 'light', label: '小毛毛雨' },
  { kind: 'rain', intensity: 'light', label: '小雨' },
  { kind: 'rain', intensity: 'moderate', label: '中雨' },
  { kind: 'rain', intensity: 'heavy', label: '大雨' },
  { kind: 'snow', intensity: 'light', label: '小雪' },
  { kind: 'snow', intensity: 'heavy', label: '大雪' },
  { kind: 'storm', intensity: 'moderate', label: '雷阵雨' },
  { kind: 'storm', intensity: 'heavy', label: '强雷暴' },
]

const { weather, loading, error, coords, refresh, setCity, useAutoLocation } = useWeather()

const dayNight = ref<DayNightPref>(loadDayNightPref())

/** Temporarily always on so scene cycling is easy to try */
const showPreview = true

/** null = live weather; otherwise index into PREVIEW_ITEMS */
const previewIndex = ref<number | null>(null)
const detailOpen = ref(false)
const cityPicking = ref(false)
/** Pause CSS animations when OS hides the window (tray / fullscreen) */
const animPaused = ref(false)

function syncAnimPause() {
  animPaused.value = document.hidden
}

onMounted(() => {
  document.addEventListener('visibilitychange', syncAnimPause)
  syncAnimPause()
})

onUnmounted(() => {
  document.removeEventListener('visibilitychange', syncAnimPause)
})

const liveKind = computed<WeatherKind>(() => {
  if (weather.value) return weather.value.kind
  if (loading.value) return 'loading'
  if (error.value) return 'error'
  return 'loading'
})

const kind = computed<WeatherKind>(() => {
  if (previewIndex.value === null) return liveKind.value
  return PREVIEW_ITEMS[previewIndex.value].kind
})

const intensity = computed<PrecipIntensity | null>(() => {
  if (previewIndex.value === null) return weather.value?.intensity ?? null
  return PREVIEW_ITEMS[previewIndex.value].intensity
})

const description = computed(() => {
  if (previewIndex.value === null) return weather.value?.description ?? ''
  return PREVIEW_ITEMS[previewIndex.value].label
})

const previewButtonLabel = computed(() => {
  if (previewIndex.value === null) return '试样式'
  const i = previewIndex.value
  return `${PREVIEW_ITEMS[i].label} ${i + 1}/${PREVIEW_ITEMS.length}`
})

const locationHint = computed(() => {
  if (previewIndex.value !== null) return '预览模式 · 再点切换 · 长按恢复'
  if (detailOpen.value) return null
  if (error.value && weather.value) return '更新失败 · 显示缓存天气'
  if (coords.value?.source === 'manual') {
    return `已选 ${coords.value.label ?? '城市'} · 点击查看详情`
  }
  if (!coords.value) return '点击球体查看详情'
  if (coords.value.source === 'fallback') {
    return `定位失败，使用${coords.value.label ?? '默认城市'} · 点击切换城市`
  }
  if (coords.value.source === 'ip') {
    return '网络粗定位（市级）· 点「自动定位」可精确到区'
  }
  if (coords.value.source === 'cache') {
    return '已用上次定位 · 点击城市可切换'
  }
  return '点击球体查看详情'
})

const cityName = computed(() => weather.value?.city ?? coords.value?.label ?? '定位中…')
const isManualCity = computed(() => coords.value?.source === 'manual')

const visualIsDay = computed(() => {
  if (previewIndex.value !== null) return true
  return applyDayNightPref(dayNight.value, weather.value?.isDay ?? true)
})

const accentStyle = computed(() => ({
  '--accent': getOrbTheme(
    kind.value,
    visualIsDay.value,
    intensity.value,
    weather.value?.temperature ?? null,
  ).accent,
}))

watch(
  [detailOpen, cityPicking],
  ([open, picking]) => {
    if (!open) {
      void getDesktopApi().setSize(COMPACT.w, COMPACT.h)
      return
    }
    const size = picking ? CITY_PICKER : EXPANDED
    void getDesktopApi().setSize(size.w, size.h)
  },
  { immediate: true },
)

function toggleDetail() {
  detailOpen.value = !detailOpen.value
}

function closeDetail() {
  detailOpen.value = false
  cityPicking.value = false
}

function onCycleDayNight() {
  dayNight.value = cycleDayNightPref(dayNight.value)
  saveDayNightPref(dayNight.value)
}

function onRefresh() {
  previewIndex.value = null
  void refresh()
}

async function onSelectCity(city: CityOption) {
  previewIndex.value = null
  await setCity(city)
}

async function onUseLocate() {
  previewIndex.value = null
  await useAutoLocation()
}

function cyclePreview() {
  if (previewIndex.value === null) {
    previewIndex.value = 0
    return
  }
  previewIndex.value = (previewIndex.value + 1) % PREVIEW_ITEMS.length
}

function exitPreview() {
  previewIndex.value = null
}

let longPressTimer: ReturnType<typeof setTimeout> | null = null
let longPressTriggered = false

function onTestPointerDown() {
  longPressTriggered = false
  longPressTimer = setTimeout(() => {
    longPressTriggered = true
    exitPreview()
  }, 550)
}

function onTestPointerUp(e: MouseEvent) {
  if (longPressTimer) {
    clearTimeout(longPressTimer)
    longPressTimer = null
  }
  if (longPressTriggered) {
    e.preventDefault()
    return
  }
  cyclePreview()
}

function onTestPointerLeave() {
  if (longPressTimer) {
    clearTimeout(longPressTimer)
    longPressTimer = null
  }
}
</script>

<template>
  <div
    class="app"
    :class="{ expanded: detailOpen, picking: detailOpen && cityPicking, paused: animPaused }"
    :style="accentStyle"
  >
    <WeatherBall
      :kind="kind"
      :intensity="intensity"
      :is-day="visualIsDay"
      :temperature="weather?.temperature ?? 26"
      :description="description"
      :loading="previewIndex === null && loading && !weather"
      :error-message="previewIndex === null ? error : null"
      :location-hint="locationHint"
      @refresh="onRefresh"
      @toggle-detail="toggleDetail"
    />

    <WeatherDetail
      :open="detailOpen"
      :city="cityName"
      :temperature="weather?.temperature ?? null"
      :description="description"
      :humidity="weather?.humidity ?? null"
      :feels-like="weather?.feelsLike ?? null"
      :wind-speed="weather?.windSpeed ?? null"
      :fetched-at="weather?.fetchedAt ?? null"
      :hourly="weather?.hourly ?? []"
      :loading="loading"
      :is-manual-city="isManualCity"
      :update-failed="!!error && !!weather"
      :day-night="dayNight"
      @close="closeDetail"
      @refresh="onRefresh"
      @select-city="onSelectCity"
      @use-locate="onUseLocate"
      @picker-change="cityPicking = $event"
      @cycle-day-night="onCycleDayNight"
    />

    <button
      v-if="showPreview"
      type="button"
      class="test-btn"
      title="点击切换样式，长按或右键恢复实时天气"
      @pointerdown.left="onTestPointerDown"
      @pointerup.left="onTestPointerUp"
      @pointerleave="onTestPointerLeave"
      @pointercancel="onTestPointerLeave"
      @click.prevent
      @contextmenu.prevent="exitPreview"
    >
      {{ previewButtonLabel }}
    </button>
  </div>
</template>

<style scoped>
.app {
  width: 160px;
  height: 204px;
  position: relative;
  display: flex;
  flex-direction: column;
  align-items: center;
  transition: height 0.28s ease;
  /* Let empty chrome pass clicks to the orb / open panel only */
  pointer-events: none;
}

.app.expanded {
  height: 520px;
  pointer-events: none;
}

.app :deep(.ball) {
  pointer-events: auto;
}

.app.expanded.picking {
  height: 520px;
}

.app.paused :deep(.ball),
.app.paused :deep(.glow),
.app.paused :deep(.glass),
.app.paused :deep(.fx),
.app.paused :deep(.drop),
.app.paused :deep(.flake),
.app.paused :deep(.cloud),
.app.paused :deep(.mist),
.app.paused :deep(.mote),
.app.paused :deep(.sun-core),
.app.paused :deep(.sun-rays),
.app.paused :deep(.bolt),
.app.paused :deep(.sky-flash),
.app.paused :deep(.wave) {
  animation-play-state: paused !important;
}

/* Sit under the orb so it never covers the detail refresh button */
.test-btn {
  position: absolute;
  top: 168px;
  left: 50%;
  transform: translateX(-50%);
  z-index: 20;
  margin: 0;
  padding: 3px 8px;
  border: none;
  border-radius: 999px;
  background: rgba(15, 23, 42, 0.78);
  color: #f8fafc;
  font-size: 10px;
  line-height: 1.2;
  cursor: pointer;
  -webkit-app-region: no-drag;
  white-space: nowrap;
  max-width: 148px;
  overflow: hidden;
  text-overflow: ellipsis;
  pointer-events: auto;
}

.test-btn:hover {
  background: rgba(30, 41, 59, 0.92);
}

.app.expanded .test-btn {
  top: 6px;
  left: auto;
  right: 4px;
  transform: none;
}
</style>
