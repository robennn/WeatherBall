<script setup lang="ts">
import { computed } from 'vue'
import type { HourlyPoint } from '../services/weather'

const props = defineProps<{
  points: HourlyPoint[]
}>()

const W = 124
const H = 40
const PAD_X = 2
const PAD_TOP = 6
const PAD_BOTTOM = 4

const chart = computed(() => {
  const pts = props.points
  if (pts.length < 2) return null

  const temps = pts.map((p) => p.temperature)
  let min = Math.min(...temps)
  let max = Math.max(...temps)
  // Keep a little headroom so flat lines still read as a curve
  if (max - min < 1) {
    min -= 0.5
    max += 0.5
  }

  const n = pts.length
  const innerW = W - PAD_X * 2
  const innerH = H - PAD_TOP - PAD_BOTTOM

  const coords = pts.map((p, i) => {
    const x = PAD_X + (n === 1 ? innerW / 2 : (i / (n - 1)) * innerW)
    const t = (p.temperature - min) / (max - min)
    const y = PAD_TOP + innerH - t * innerH
    return { x, y, ...p }
  })

  const line = coords
    .map((c, i) => `${i === 0 ? 'M' : 'L'}${c.x.toFixed(1)},${c.y.toFixed(1)}`)
    .join(' ')
  const area =
    `${line} L${coords[coords.length - 1].x.toFixed(1)},${(H - 1).toFixed(1)}` +
    ` L${coords[0].x.toFixed(1)},${(H - 1).toFixed(1)} Z`

  const start = pts[0]
  const end = pts[pts.length - 1]

  return {
    line,
    area,
    coords,
    low: Math.round(min),
    high: Math.round(max),
    startHour: start.hour,
    endHour: end.hour,
    startTemp: Math.round(start.temperature),
    endTemp: Math.round(end.temperature),
  }
})
</script>

<template>
  <div v-if="chart" class="hourly" aria-label="未来12小时气温走势">
    <div class="head">
      <span class="label">气温走势 · {{ points.length }}小时</span>
      <span class="range">最低 {{ chart.low }}° · 最高 {{ chart.high }}°</span>
    </div>
    <svg
      class="spark"
      :viewBox="`0 0 ${W} ${H}`"
      width="100%"
      height="40"
      role="img"
    >
      <title>未来{{ points.length }}小时气温变化曲线</title>
      <path class="area" :d="chart.area" />
      <path class="line" :d="chart.line" fill="none" />
      <circle
        v-for="(c, i) in chart.coords"
        :key="i"
        class="dot"
        :cx="c.x"
        :cy="c.y"
        r="1.4"
      >
        <title>{{ c.hour }}时 {{ Math.round(c.temperature) }}°</title>
      </circle>
    </svg>
    <div class="hours">
      <span>{{ chart.startHour }}时 {{ chart.startTemp }}°</span>
      <span>{{ chart.endHour }}时 {{ chart.endTemp }}°</span>
    </div>
  </div>
</template>

<style scoped>
.hourly {
  margin-top: 10px;
  width: 100%;
}

.head {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 2px;
  margin-bottom: 4px;
}

.label {
  font-size: 10px;
  color: rgba(255, 255, 255, 0.7);
}

.range {
  font-size: 9px;
  font-variant-numeric: tabular-nums;
  color: rgba(255, 255, 255, 0.48);
}

.spark {
  display: block;
  overflow: visible;
}

.area {
  fill: color-mix(in srgb, var(--accent, #7fb4f0) 28%, transparent);
}

.line {
  stroke: var(--accent, #7fb4f0);
  stroke-width: 1.5;
  stroke-linecap: round;
  stroke-linejoin: round;
}

.dot {
  fill: rgba(255, 255, 255, 0.92);
  stroke: var(--accent, #7fb4f0);
  stroke-width: 0.6;
}

.hours {
  display: flex;
  justify-content: space-between;
  margin-top: 2px;
  font-size: 9px;
  color: rgba(255, 255, 255, 0.5);
  font-variant-numeric: tabular-nums;
}
</style>
