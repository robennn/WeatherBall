<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import {
  COMMON_CITIES,
  cityDisplayName,
  searchCities,
  type CityOption,
} from '../services/cities'
import type { HourlyPoint } from '../services/weather'
import HourlyCurve from './HourlyCurve.vue'
import { dayNightLabel } from '../services/appSettings'

const props = defineProps<{
  open: boolean
  city: string
  temperature: number | null
  description: string
  humidity: number | null
  feelsLike: number | null
  windSpeed: number | null
  fetchedAt: number | null
  hourly?: HourlyPoint[]
  loading?: boolean
  /** Manual city selected by user */
  isManualCity?: boolean
  /** Last refresh failed; showing cached data */
  updateFailed?: boolean
  dayNight?: 'auto' | 'day' | 'night'
}>()

const emit = defineEmits<{
  close: []
  refresh: []
  'select-city': [city: CityOption]
  'use-locate': []
  'picker-change': [open: boolean]
  'cycle-day-night': []
}>()

const picking = ref(false)
const query = ref('')
const searching = ref(false)
const results = ref<CityOption[]>(COMMON_CITIES)
let searchTimer: ReturnType<typeof setTimeout> | null = null
let searchSeq = 0

const updatedText = computed(() => {
  if (!props.fetchedAt) {
    return props.updateFailed ? '更新失败' : '更新时间 --:--'
  }
  const d = new Date(props.fetchedAt)
  const pad = (n: number) => String(n).padStart(2, '0')
  const time = `${pad(d.getHours())}:${pad(d.getMinutes())}`
  if (props.updateFailed) return `缓存于 ${time} · 更新失败`
  return `更新时间 ${time}`
})

const windText = computed(() => {
  if (props.windSpeed == null) return '风速 --'
  return `风速 ${Math.round(props.windSpeed)} km/h`
})

const dayNightText = computed(() => dayNightLabel(props.dayNight ?? 'auto'))

const humidityText = computed(() => {
  if (props.humidity == null) return '湿度 --'
  return `湿度 ${props.humidity}%`
})

const feelsLikeText = computed(() => {
  if (props.feelsLike == null) return '体感 --'
  return `体感 ${Math.round(props.feelsLike)}°`
})

watch(
  () => props.open,
  (open) => {
    if (!open) closePicker()
  },
)

watch(picking, (open) => {
  emit('picker-change', open)
})

function openPicker() {
  picking.value = true
  query.value = ''
  results.value = COMMON_CITIES
}

function closePicker() {
  picking.value = false
  query.value = ''
  searching.value = false
  results.value = COMMON_CITIES
  if (searchTimer) {
    clearTimeout(searchTimer)
    searchTimer = null
  }
}

function onQueryInput() {
  if (searchTimer) clearTimeout(searchTimer)
  const q = query.value.trim()
  if (!q) {
    searching.value = false
    results.value = COMMON_CITIES
    return
  }
  searching.value = true
  searchTimer = setTimeout(() => {
    void runSearch(q)
  }, 280)
}

async function runSearch(q: string) {
  const seq = ++searchSeq
  searching.value = true
  try {
    const list = await searchCities(q)
    if (seq !== searchSeq) return
    results.value = list.length ? list : []
  } finally {
    if (seq === searchSeq) searching.value = false
  }
}

function pickCity(city: CityOption) {
  emit('select-city', city)
  closePicker()
}

function onLocate() {
  emit('use-locate')
  closePicker()
}

function onClose() {
  if (picking.value) {
    closePicker()
    return
  }
  emit('close')
}
</script>

<template>
  <div
    class="panel"
    :class="{ open, picking }"
    :inert="!open"
    :aria-hidden="!open"
    role="dialog"
    :aria-label="picking ? '选择城市' : '天气详情'"
  >
    <template v-if="!picking">
      <div class="city-row">
        <button type="button" class="city-btn" title="点击切换城市" @click="openPicker">
          <span class="city">{{ city || '定位中…' }}</span>
          <span class="city-edit">切换</span>
        </button>
        <span class="live-dot" />
        <button type="button" class="close" aria-label="关闭" @click="onClose">×</button>
      </div>
      <div class="temp">
        <template v-if="temperature !== null">
          {{ Math.round(temperature) }}<span class="deg">°</span>
        </template>
        <template v-else>--</template>
      </div>
      <div class="desc">{{ description || '—' }}</div>
      <div class="meta">
        <span>{{ humidityText }}</span>
        <span>{{ feelsLikeText }}</span>
        <span>{{ windText }}</span>
      </div>
      <HourlyCurve v-if="hourly?.length" :points="hourly" />
      <div class="time" :class="{ failed: updateFailed }">
        {{ updatedText }}
        <span v-if="isManualCity" class="manual-tag">手动</span>
      </div>
      <button
        type="button"
        class="refresh"
        @click="emit('cycle-day-night')"
      >
        <span>外观 {{ dayNightText }}</span>
      </button>
      <button
        type="button"
        class="refresh"
        :class="{ loading }"
        :disabled="loading"
        @click="emit('refresh')"
      >
        <span class="ico">↻</span>
        <span>{{ loading ? '刷新中…' : '刷新天气' }}</span>
      </button>
    </template>

    <template v-else>
      <div class="picker-head">
        <span class="picker-title">选择城市</span>
        <button type="button" class="close" aria-label="返回" @click="closePicker">×</button>
      </div>
      <input
        class="search"
        type="search"
        enterkeyhint="search"
        placeholder="搜索城市或区县…"
        :value="query"
        @input="query = ($event.target as HTMLInputElement).value; onQueryInput()"
      />
      <div class="city-list" role="listbox" aria-label="城市列表">
        <div v-if="searching" class="list-hint">搜索中…</div>
        <div v-else-if="!results.length" class="list-hint">未找到城市或区县</div>
        <button
          v-for="c in results"
          :key="`${c.name}-${c.latitude}-${c.longitude}`"
          type="button"
          class="city-item"
          role="option"
          @click="pickCity(c)"
        >
          {{ cityDisplayName(c) }}
        </button>
      </div>
      <button type="button" class="locate" :disabled="loading" @click="onLocate">
        {{ loading ? '定位中…' : '使用自动定位' }}
      </button>
    </template>
  </div>
</template>

<style scoped>
.panel {
  width: 152px;
  padding: 0;
  max-height: 0;
  opacity: 0;
  overflow: hidden;
  pointer-events: none !important;
  visibility: hidden;
  position: absolute;
  left: 50%;
  top: 200px;
  transform: translateX(-50%);
  border-radius: 18px;
  background: rgba(12, 18, 32, 0.92);
  border: 1px solid transparent;
  background: rgba(12, 18, 32, 0.94);
  box-shadow: none;
  color: #f4f7fb;
  font-family: 'Microsoft YaHei UI', 'Microsoft YaHei', 'PingFang SC', 'Segoe UI', sans-serif;
  transition:
    max-height 0.28s ease,
    opacity 0.22s ease,
    padding 0.22s ease,
    border-color 0.22s ease,
    box-shadow 0.22s ease;
  -webkit-app-region: no-drag;
  box-sizing: border-box;
  z-index: 4;
}

.panel.open {
  position: relative;
  left: auto;
  top: auto;
  transform: none;
  visibility: visible;
  max-height: 320px;
  opacity: 1;
  pointer-events: auto !important;
  padding: 14px 14px 12px;
  border-color: rgba(255, 255, 255, 0.22);
  box-shadow:
    0 0 0 1px rgba(0, 0, 0, 0.35),
    0 14px 36px rgba(0, 0, 0, 0.55);
  margin-top: 8px;
}

.panel.open.picking {
  max-height: 320px;
}

.city-row {
  display: flex;
  align-items: flex-start;
  gap: 6px;
}

.city-btn {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 2px;
  margin: 0;
  padding: 0;
  border: none;
  background: transparent;
  color: inherit;
  text-align: left;
  cursor: pointer;
}

.city-btn:hover .city {
  text-decoration: underline;
  text-underline-offset: 2px;
}

.city-btn:hover .city-edit {
  opacity: 1;
}

.city {
  font-size: 13px;
  font-weight: 600;
  letter-spacing: 0;
  line-height: 1.35;
  /* Avoid ellipsis clipping the last CJK glyph (e.g. 区) */
  overflow: visible;
  white-space: normal;
  word-break: keep-all;
  overflow-wrap: anywhere;
}

.city-edit {
  font-size: 10px;
  font-weight: 400;
  color: rgba(255, 255, 255, 0.55);
  opacity: 0.85;
}

.live-dot {
  width: 7px;
  height: 7px;
  margin-top: 5px;
  border-radius: 50%;
  background: var(--accent, #7fb4f0);
  box-shadow: 0 0 8px var(--accent, #7fb4f0);
  animation: dotPulse 2.4s ease-in-out infinite;
  flex-shrink: 0;
}

.close {
  margin-left: 2px;
  border: none;
  background: rgba(255, 255, 255, 0.08);
  color: rgba(255, 255, 255, 0.75);
  font-size: 14px;
  line-height: 1;
  cursor: pointer;
  padding: 2px 6px;
  border-radius: 8px;
  flex-shrink: 0;
}

.close:hover {
  color: #fff;
  background: rgba(255, 255, 255, 0.16);
}

.temp {
  margin-top: 8px;
  font-size: 36px;
  font-weight: 200;
  line-height: 1.05;
  letter-spacing: -0.02em;
  font-variant-numeric: tabular-nums;
  color: #ffffff;
  text-shadow: 0 1px 2px rgba(0, 0, 0, 0.35);
}

.deg {
  font-size: 18px;
  vertical-align: 10px;
  opacity: 0.75;
}

.desc {
  margin-top: 4px;
  font-size: 12px;
  color: rgba(255, 255, 255, 0.9);
}

.meta {
  margin-top: 10px;
  display: flex;
  flex-wrap: wrap;
  gap: 8px 12px;
  font-size: 11px;
  color: rgba(255, 255, 255, 0.72);
}

.time {
  margin-top: 6px;
  font-size: 10px;
  color: rgba(255, 255, 255, 0.5);
  display: flex;
  align-items: center;
  gap: 6px;
}

.time.failed {
  color: #fecaca;
}

.manual-tag {
  padding: 1px 5px;
  border-radius: 4px;
  background: rgba(255, 255, 255, 0.12);
  color: rgba(255, 255, 255, 0.7);
  font-size: 9px;
}

.refresh {
  margin-top: 8px;
  width: 100%;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  padding: 8px 0;
  border: none;
  border-radius: 12px;
  background: rgba(255, 255, 255, 0.16);
  color: #f8fafc;
  font-size: 12px;
  letter-spacing: 0;
  cursor: pointer;
  transition:
    background 0.25s ease,
    transform 0.15s ease;
}

.refresh:hover:not(:disabled) {
  background: rgba(255, 255, 255, 0.24);
}

.refresh:active:not(:disabled) {
  transform: scale(0.97);
}

.refresh.loading {
  opacity: 0.7;
  pointer-events: none;
}

.refresh .ico {
  display: inline-block;
  font-size: 13px;
}

.refresh.loading .ico {
  animation: spin 1s linear infinite;
}

.picker-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  margin-bottom: 8px;
}

.picker-title {
  font-size: 13px;
  font-weight: 600;
}

.search {
  width: 100%;
  box-sizing: border-box;
  margin: 0 0 8px;
  padding: 7px 8px;
  border: 1px solid rgba(255, 255, 255, 0.18);
  border-radius: 10px;
  background: rgba(0, 0, 0, 0.28);
  color: #f8fafc;
  font-size: 12px;
  outline: none;
}

.search::placeholder {
  color: rgba(255, 255, 255, 0.4);
}

.search:focus {
  border-color: rgba(255, 255, 255, 0.4);
}

.city-list {
  max-height: 168px;
  overflow-y: auto;
  overflow-x: hidden;
  display: flex;
  flex-direction: column;
  gap: 2px;
  margin: 0 -4px;
  padding: 0 4px;
  scrollbar-width: thin;
  scrollbar-color: rgba(255, 255, 255, 0.25) transparent;
}

.list-hint {
  padding: 10px 6px;
  font-size: 11px;
  color: rgba(255, 255, 255, 0.45);
  text-align: center;
}

.city-item {
  display: block;
  width: 100%;
  margin: 0;
  padding: 7px 8px;
  border: none;
  border-radius: 8px;
  background: transparent;
  color: rgba(255, 255, 255, 0.92);
  font-size: 12px;
  text-align: left;
  cursor: pointer;
  line-height: 1.3;
}

.city-item:hover {
  background: rgba(255, 255, 255, 0.12);
}

.locate {
  margin-top: 8px;
  width: 100%;
  padding: 7px 0;
  border: none;
  border-radius: 10px;
  background: rgba(255, 255, 255, 0.12);
  color: rgba(255, 255, 255, 0.88);
  font-size: 11px;
  cursor: pointer;
}

.locate:hover:not(:disabled) {
  background: rgba(255, 255, 255, 0.2);
}

.locate:disabled {
  opacity: 0.6;
  cursor: default;
}

@keyframes dotPulse {
  0%,
  100% {
    opacity: 1;
  }
  50% {
    opacity: 0.45;
  }
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}
</style>
