<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch, type PropType } from 'vue'
import type { PrecipIntensity, WeatherKind } from '../services/weather'
import { getOrbTheme, type OrbScene } from '../themes/weatherThemes'
import WeatherTooltip from './WeatherTooltip.vue'
import { getDesktopApi } from '../platform'

const BALL = 100
const FALL = 112
/** Fade-out before rebuild + fade-in after (keeps weather swaps feeling alive) */
const FX_MS = 300

const props = defineProps({
  kind: { type: String as PropType<WeatherKind>, required: true },
  isDay: { type: Boolean, default: true },
  intensity: { type: String as PropType<PrecipIntensity | null>, default: null },
  temperature: { type: Number as PropType<number | null>, default: null },
  description: { type: String, default: '' },
  loading: { type: Boolean, default: false },
  errorMessage: { type: String as PropType<string | null>, default: null },
  locationHint: { type: String as PropType<string | null>, default: null },
})

const emit = defineEmits<{ refresh: []; 'toggle-detail': [] }>()

const hovered = ref(false)
const theme = computed(() => getOrbTheme(props.kind, props.isDay, props.intensity))
const scene = computed(() => theme.value.scene)
/** When true, particle / sun / bolt layers fade out before rebuild */
const fxOut = ref(false)

const motes = ref<Array<Record<string, string>>>([])
const rainDrops = ref<Array<Record<string, string>>>([])
const stormDrops = ref<Array<Record<string, string>>>([])
const flakes = ref<Array<Record<string, string>>>([])

let dragging = false
let dragOffsetX = 0
let dragOffsetY = 0
let pointerStartX = 0
let pointerStartY = 0
let didDrag = false
const DRAG_THRESHOLD = 5

let fxReady = false
let fxGen = 0

function rand(min: number, max: number) {
  return min + Math.random() * (max - min)
}

function sleep(ms: number) {
  return new Promise<void>((resolve) => {
    setTimeout(resolve, ms)
  })
}

function buildParticles() {
  const t = theme.value

  motes.value = Array.from({ length: 5 }, () => {
    const size = rand(2, 4.5)
    const dur = rand(4, 8)
    return {
      left: `${rand(15, 85)}%`,
      top: `${rand(35, 75)}%`,
      width: `${size}px`,
      height: `${size}px`,
      animationDuration: `${dur}s`,
      animationDelay: `${-rand(0, dur)}s`,
    }
  })

  const drops = Array.from({ length: t.dropCount }, () => makeDrop(t.dropFast))
  if (t.scene === 'storm') {
    rainDrops.value = []
    stormDrops.value = drops
  } else {
    rainDrops.value = drops
    stormDrops.value = []
  }

  flakes.value = Array.from({ length: t.flakeCount }, () => {
    const size = rand(1.8, 3.6)
    const dur =
      props.intensity === 'heavy'
        ? rand(2.4, 4.5)
        : props.intensity === 'light'
          ? rand(4.5, 7.5)
          : rand(3.2, 6.5)
    return {
      left: `${rand(4, 94)}%`,
      width: `${size}px`,
      height: `${size}px`,
      opacity: rand(0.5, 1).toFixed(2),
      '--sx': `${rand(-14, 14).toFixed(0)}px`,
      animationDuration: `${dur}s`,
      animationDelay: `${-rand(0, dur)}s`,
    }
  })
}

async function transitionParticles() {
  if (!fxReady) {
    buildParticles()
    return
  }

  const gen = ++fxGen
  fxOut.value = true
  await sleep(FX_MS)
  if (gen !== fxGen) return

  buildParticles()
  await nextTick()
  if (gen !== fxGen) return

  await new Promise<void>((resolve) => {
    requestAnimationFrame(() => resolve())
  })
  if (gen !== fxGen) return
  fxOut.value = false
}

function makeDrop(fast: boolean) {
  const intensity = props.intensity ?? 'moderate'
  let dur: number
  let height: number
  if (fast || intensity === 'heavy') {
    dur = rand(0.45, 0.85)
    height = rand(8, 14)
  } else if (intensity === 'light' || props.kind === 'drizzle') {
    dur = rand(1.1, 1.8)
    height = rand(4, 8)
  } else {
    dur = rand(0.75, 1.3)
    height = rand(6, 11)
  }
  return {
    left: `${rand(4, 94)}%`,
    height: `${height}px`,
    opacity: rand(0.4, 0.95).toFixed(2),
    animationDuration: `${dur}s`,
    animationDelay: `${-rand(0, dur)}s`,
  }
}

function cssVars() {
  const t = theme.value
  return {
    '--glow': t.glow,
    '--water-a': t.waterA,
    '--water-b': t.waterB,
    '--accent': t.accent,
    '--fall': `${FALL}px`,
    '--ball': `${BALL}px`,
  }
}

function isOn(s: OrbScene) {
  return scene.value === s
}

async function onPointerDown(e: PointerEvent) {
  if (e.button !== 0) return
  hovered.value = true
  didDrag = false
  pointerStartX = e.screenX
  pointerStartY = e.screenY
  const target = e.currentTarget as HTMLElement | null
  const api = getDesktopApi()
  const bounds = await api.getBounds()
  dragging = true
  dragOffsetX = e.screenX - bounds.x
  dragOffsetY = e.screenY - bounds.y
  target?.setPointerCapture?.(e.pointerId)
}

function onPointerMove(e: PointerEvent) {
  if (!dragging) return
  const dx = e.screenX - pointerStartX
  const dy = e.screenY - pointerStartY
  if (!didDrag && Math.hypot(dx, dy) >= DRAG_THRESHOLD) {
    didDrag = true
  }
  if (didDrag) {
    void getDesktopApi().setPosition(e.screenX - dragOffsetX, e.screenY - dragOffsetY)
  }
}

function onPointerUp(e: PointerEvent) {
  const wasDrag = didDrag
  dragging = false
  didDrag = false
  try {
    ;(e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId)
  } catch {
    /* already released */
  }
  if (!wasDrag) {
    emit('toggle-detail')
  }
}

function onOrbEnter() {
  hovered.value = true
}

function onOrbLeave() {
  if (!dragging) hovered.value = false
}

function onContextMenu(e: MouseEvent) {
  e.preventDefault()
  emit('refresh')
}

onMounted(() => {
  buildParticles()
  fxReady = true
})

onUnmounted(() => {
  fxGen += 1
})

watch(
  () => [props.kind, props.intensity, props.isDay] as const,
  () => {
    void transitionParticles()
  },
)
</script>

<template>
  <div
    class="ball-root"
    :class="{ loading }"
    :style="cssVars()"
    @contextmenu="onContextMenu"
  >
    <WeatherTooltip
      :visible="hovered"
      :temperature="temperature"
      :description="description"
      :error-message="errorMessage"
      :location-hint="locationHint"
      :loading="loading"
    />

    <div
      class="ball"
      :class="{ dragging }"
      @pointerdown="onPointerDown"
      @pointermove="onPointerMove"
      @pointerup="onPointerUp"
      @pointercancel="onPointerUp"
      @pointerenter="onOrbEnter"
      @pointerleave="onOrbLeave"
    >
      <div class="glow" />
      <div class="glass">
        <div class="interior">
          <!-- 晴 -->
          <div class="layer" :class="{ on: isOn('sunny') }">
            <div class="fx" :class="{ out: fxOut }">
              <div class="sun-rays" />
              <div class="sun-core" />
              <span
                v-for="(m, i) in motes"
                :key="'m' + i"
                class="mote"
                :style="m"
              />
            </div>
          </div>

          <!-- 多云 -->
          <div class="layer" :class="{ on: isOn('cloudy') }">
            <div class="fx" :class="{ out: fxOut }">
              <div class="cloud c1" />
              <div class="cloud c2" />
              <div class="cloud c3" />
            </div>
          </div>

          <!-- 阴 / 雾 -->
          <div class="layer layer-overcast" :class="{ on: isOn('overcast') }">
            <div class="fx" :class="{ out: fxOut }">
              <div class="cloud c1" />
              <div class="cloud c2" />
              <div class="cloud c3" />
              <div class="mist m1" />
              <div class="mist m2" />
            </div>
          </div>

          <!-- 雨 -->
          <div class="layer layer-rain" :class="{ on: isOn('rain') }">
            <div class="cloud c1" />
            <div class="cloud c2" />
            <div class="fx precip" :class="{ out: fxOut }">
              <span
                v-for="(d, i) in rainDrops"
                :key="'r' + i"
                class="drop"
                :style="d"
              />
            </div>
          </div>

          <!-- 雪 -->
          <div class="layer layer-snow" :class="{ on: isOn('snow') }">
            <div class="cloud c1" />
            <div class="cloud c2" />
            <div class="fx precip" :class="{ out: fxOut }">
              <span
                v-for="(f, i) in flakes"
                :key="'f' + i"
                class="flake"
                :style="f"
              />
            </div>
          </div>

          <!-- 雷暴 -->
          <div class="layer layer-storm" :class="{ on: isOn('storm') }">
            <div class="cloud c1" />
            <div class="cloud c2" />
            <div class="fx precip" :class="{ out: fxOut }">
              <span
                v-for="(d, i) in stormDrops"
                :key="'s' + i"
                class="drop"
                :style="d"
              />
              <div class="bolt" />
              <div class="flash" />
            </div>
          </div>

          <div class="wave back" />
          <div class="wave front" />
        </div>
        <div class="highlight" />
        <div class="rim" />
      </div>
    </div>
    <div class="ball-shadow" />
  </div>
</template>

<style scoped>
.ball-root {
  width: 160px;
  height: 204px;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: flex-end;
  position: relative;
  padding-bottom: 14px;
  box-sizing: border-box;
  background: transparent;
  user-select: none;
  pointer-events: none;
}

.ball {
  position: relative;
  width: var(--ball);
  height: var(--ball);
  cursor: grab;
  touch-action: none;
  z-index: 3;
  flex-shrink: 0;
  pointer-events: auto;
}

.ball.dragging {
  cursor: grabbing;
}

.ball.dragging .glass {
  animation-play-state: paused;
}

.glow {
  position: absolute;
  inset: -18%;
  background: radial-gradient(circle, var(--glow) 0%, transparent 68%);
  /* Avoid heavy blur filters — large GPU memory on WebView2 */
  transition: background 1.2s ease;
  animation: glowPulse 7s ease-in-out infinite;
  pointer-events: none;
  opacity: 0.9;
}

.glass {
  position: absolute;
  inset: 0;
  border-radius: 50%;
  background:
    radial-gradient(
      circle at 32% 28%,
      rgba(255, 255, 255, 0.3),
      rgba(255, 255, 255, 0.07) 42%,
      rgba(160, 200, 255, 0.05) 68%,
      rgba(255, 255, 255, 0.14)
    );
  border: 1px solid rgba(255, 255, 255, 0.22);
  box-shadow:
    inset 0 -10px 18px rgba(255, 255, 255, 0.1),
    inset 0 7px 14px rgba(255, 255, 255, 0.16),
    0 12px 28px rgba(0, 0, 0, 0.4);
  animation: bob 5.5s ease-in-out infinite;
  overflow: hidden;
}

.interior {
  position: absolute;
  inset: 1px;
  border-radius: 50%;
  overflow: hidden;
}

.highlight {
  position: absolute;
  top: 9%;
  left: 13%;
  width: 36%;
  height: 24%;
  background: radial-gradient(ellipse, rgba(255, 255, 255, 0.9), transparent 70%);
  border-radius: 50%;
  transform: rotate(-22deg);
  filter: blur(0.5px);
  pointer-events: none;
}

.rim {
  position: absolute;
  bottom: 6%;
  right: 14%;
  width: 26%;
  height: 12%;
  background: radial-gradient(ellipse, rgba(255, 255, 255, 0.35), transparent 70%);
  border-radius: 50%;
  transform: rotate(18deg);
  filter: blur(1px);
  pointer-events: none;
}

.wave {
  position: absolute;
  left: -55%;
  width: 210%;
  height: 210%;
  border-radius: 43%;
  transition: background 1.2s ease;
  pointer-events: none;
}

.wave.back {
  top: 60%;
  background: var(--water-a);
  opacity: 0.75;
  animation: spin 11s linear infinite;
}

.wave.front {
  top: 63%;
  background: var(--water-b);
  opacity: 0.9;
  animation: spin 8s linear infinite reverse;
}

.layer {
  position: absolute;
  inset: 0;
  opacity: 0;
  transition: opacity 0.36s ease;
  pointer-events: none;
}

.layer.on {
  opacity: 1;
}

.fx {
  position: absolute;
  inset: 0;
  opacity: 1;
  transition: opacity 0.3s ease;
  pointer-events: none;
}

.fx.out {
  opacity: 0;
}

.fx.precip {
  /* Keep clouds visible while only rain/snow/storm particles crossfade */
  z-index: 1;
}

.sun-rays {
  position: absolute;
  top: 6%;
  left: 15%;
  width: 70%;
  height: 70%;
  border-radius: 50%;
  background: repeating-conic-gradient(
    rgba(255, 205, 90, 0.4) 0deg 7deg,
    transparent 7deg 22deg
  );
  -webkit-mask: radial-gradient(circle, transparent 28%, #000 32%, transparent 72%);
  mask: radial-gradient(circle, transparent 28%, #000 32%, transparent 72%);
  animation: spin 26s linear infinite;
}

.sun-core {
  position: absolute;
  top: 20%;
  left: 50%;
  transform: translateX(-50%);
  width: 42%;
  height: 42%;
  border-radius: 50%;
  background: radial-gradient(
    circle at 42% 38%,
    #fff7d6,
    #ffd873 45%,
    #ffab3d 78%,
    rgba(255, 160, 60, 0.4)
  );
  box-shadow:
    0 0 16px rgba(255, 190, 80, 0.75),
    0 0 36px rgba(255, 170, 60, 0.4);
  animation: sunPulse 4.5s ease-in-out infinite;
}

.mote {
  position: absolute;
  border-radius: 50%;
  background: radial-gradient(circle, rgba(255, 225, 140, 0.95), transparent 70%);
  animation: rise ease-in-out infinite;
}

.cloud {
  position: absolute;
  width: 28px;
  height: 10px;
  border-radius: 10px;
  background: var(--cloud-c, #ffffff);
  box-shadow:
    8px -6px 0 -1px var(--cloud-c, #ffffff),
    17px -2px 0 0 var(--cloud-c, #ffffff),
    -7px -3px 0 -2px var(--cloud-c, #ffffff);
  filter: blur(0.4px);
  animation: drift ease-in-out infinite alternate;
}

.layer:not(.layer-overcast):not(.layer-rain):not(.layer-snow):not(.layer-storm) .cloud {
  --cloud-c: #f4f8fd;
}

.layer-overcast .cloud {
  --cloud-c: #aeb8c4;
  opacity: 0.92;
}

.layer-rain .cloud {
  --cloud-c: #8fa0b4;
}

.layer-snow .cloud {
  --cloud-c: #dde6ef;
}

.layer-storm .cloud {
  --cloud-c: #5b5f78;
}

.cloud.c1 {
  top: 18%;
  left: 14%;
  animation-duration: 7s;
}

.cloud.c2 {
  top: 34%;
  left: 42%;
  width: 22px;
  height: 8px;
  animation-duration: 9s;
}

.cloud.c3 {
  top: 52%;
  left: 18%;
  width: 18px;
  height: 7px;
  animation-duration: 11s;
}

.layer-rain .cloud.c1,
.layer-snow .cloud.c1,
.layer-storm .cloud.c1 {
  top: 12%;
  left: 14%;
}

.layer-rain .cloud.c2,
.layer-snow .cloud.c2,
.layer-storm .cloud.c2 {
  top: 20%;
  left: 46%;
}

.mist {
  position: absolute;
  width: 150%;
  height: 12px;
  background: linear-gradient(90deg, transparent, rgba(205, 214, 224, 0.4), transparent);
  filter: none;
  animation: mistMove ease-in-out infinite alternate;
  opacity: 0.45;
}

.mist.m1 {
  top: 62%;
  animation-duration: 8s;
}

.mist.m2 {
  top: 74%;
  animation-duration: 11s;
}

.drop {
  position: absolute;
  top: -16%;
  width: 1.5px;
  border-radius: 2px;
  background: linear-gradient(rgba(255, 255, 255, 0), rgba(190, 222, 255, 0.9));
  animation: fall linear infinite;
}

.flake {
  position: absolute;
  top: -12%;
  border-radius: 50%;
  background: radial-gradient(circle, #ffffff, rgba(255, 255, 255, 0.55));
  animation: snowfall linear infinite;
}

.bolt {
  position: absolute;
  top: 30%;
  left: 44%;
  width: 26%;
  height: 46%;
  background: linear-gradient(#fffbe0, #ffd76a 60%, #ffb43d);
  clip-path: polygon(56% 0, 24% 46%, 46% 46%, 34% 100%, 76% 36%, 52% 36%, 72% 0);
  filter: drop-shadow(0 0 6px rgba(255, 220, 120, 0.9));
  opacity: 0;
  animation: boltFlash 4.2s linear infinite;
}

.flash {
  position: absolute;
  inset: 0;
  background: radial-gradient(circle at 50% 34%, rgba(255, 255, 225, 0.85), transparent 62%);
  opacity: 0;
  animation: skyFlash 4.2s linear infinite;
}

.ball-shadow {
  width: 64%;
  height: 10px;
  margin-top: 2px;
  border-radius: 50%;
  background: radial-gradient(ellipse, rgba(0, 0, 0, 0.45), transparent 70%);
  flex-shrink: 0;
}

.ball-root.loading .glow {
  animation: glowPulse 1.6s ease-in-out infinite;
}

@keyframes glowPulse {
  0%,
  100% {
    opacity: 0.85;
    transform: scale(1);
  }
  50% {
    opacity: 1;
    transform: scale(1.06);
  }
}

@keyframes bob {
  0%,
  100% {
    transform: translateY(0);
  }
  50% {
    transform: translateY(-4px);
  }
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}

@keyframes sunPulse {
  0%,
  100% {
    box-shadow:
      0 0 14px rgba(255, 190, 80, 0.7),
      0 0 32px rgba(255, 170, 60, 0.35);
  }
  50% {
    box-shadow:
      0 0 20px rgba(255, 200, 90, 0.9),
      0 0 44px rgba(255, 175, 65, 0.5);
  }
}

@keyframes rise {
  0% {
    transform: translateY(14px);
    opacity: 0;
  }
  25% {
    opacity: 0.9;
  }
  75% {
    opacity: 0.7;
  }
  100% {
    transform: translateY(-48px);
    opacity: 0;
  }
}

@keyframes drift {
  from {
    transform: translateX(-6px);
  }
  to {
    transform: translateX(6px);
  }
}

@keyframes mistMove {
  from {
    transform: translateX(-16%);
  }
  to {
    transform: translateX(10%);
  }
}

@keyframes fall {
  to {
    transform: translateY(var(--fall, 112px));
  }
}

@keyframes snowfall {
  0% {
    transform: translate3d(0, -8px, 0);
  }
  100% {
    transform: translate3d(var(--sx, 8px), var(--fall, 112px), 0);
  }
}

@keyframes boltFlash {
  0%,
  84%,
  100% {
    opacity: 0;
  }
  86%,
  88% {
    opacity: 1;
  }
  90% {
    opacity: 0.25;
  }
  92% {
    opacity: 0.9;
  }
  95% {
    opacity: 0;
  }
}

@keyframes skyFlash {
  0%,
  84%,
  100% {
    opacity: 0;
  }
  86% {
    opacity: 0.75;
  }
  90% {
    opacity: 0.2;
  }
  92% {
    opacity: 0.55;
  }
  95% {
    opacity: 0;
  }
}

@media (prefers-reduced-motion: reduce) {
  .glow,
  .glass,
  .wave,
  .sun-rays,
  .sun-core,
  .mote,
  .cloud,
  .mist,
  .drop,
  .flake,
  .bolt,
  .flash {
    animation-duration: 0.01s !important;
    animation-iteration-count: 1 !important;
  }
}
</style>
