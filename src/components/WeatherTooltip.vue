<script setup lang="ts">
defineProps<{
  visible: boolean
  temperature: number | null
  description: string
  errorMessage?: string | null
  locationHint?: string | null
  loading?: boolean
}>()
</script>

<template>
  <div class="tooltip" :class="{ show: visible }" aria-live="polite">
    <template v-if="loading && temperature === null">
      <div class="line muted">获取中…</div>
    </template>
    <template v-else-if="errorMessage && temperature === null">
      <div class="line warn">{{ errorMessage }}</div>
      <div v-if="locationHint" class="line muted">{{ locationHint }}</div>
    </template>
    <template v-else>
      <div class="temp">
        <span v-if="temperature !== null">{{ Math.round(temperature) }}°</span>
        <span v-else>--</span>
      </div>
      <div class="line">{{ description || '—' }}</div>
      <div v-if="locationHint" class="line muted">{{ locationHint }}</div>
      <div v-if="errorMessage" class="line warn">{{ errorMessage }}</div>
    </template>
  </div>
</template>

<style scoped>
.tooltip {
  position: absolute;
  top: 6px;
  left: 50%;
  transform: translate(-50%, 6px);
  min-width: 72px;
  max-width: 128px;
  padding: 8px 12px;
  border-radius: 14px;
  background: rgba(12, 18, 32, 0.9);
  border: 1px solid rgba(255, 255, 255, 0.18);
  color: #f4f7fb;
  font-family: 'Microsoft YaHei UI', 'Microsoft YaHei', 'PingFang SC', 'Segoe UI', sans-serif;
  text-align: center;
  opacity: 0;
  pointer-events: none;
  transition:
    opacity 0.18s ease,
    transform 0.18s ease;
  -webkit-app-region: no-drag;
  backdrop-filter: none;
  -webkit-backdrop-filter: none;
  background: rgba(12, 18, 32, 0.94);
  box-shadow:
    0 0 0 1px rgba(0, 0, 0, 0.3),
    0 10px 28px rgba(0, 0, 0, 0.45);
  z-index: 5;
}

.tooltip.show {
  opacity: 1;
  transform: translate(-50%, 0);
}

.temp {
  font-size: 20px;
  font-weight: 200;
  letter-spacing: -0.02em;
  line-height: 1.1;
  font-variant-numeric: tabular-nums;
}

.line {
  margin-top: 2px;
  font-size: 10px;
  opacity: 0.92;
}

.muted {
  opacity: 0.65;
  font-size: 9px;
}

.warn {
  color: #fecaca;
  font-size: 9px;
}
</style>
