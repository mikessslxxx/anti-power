<script setup lang="ts">
import { computed } from "vue";
import { percent, rate, seconds } from "../format";
import { t } from "../i18n";
import type { SessionMetrics } from "../types";
import ContextBar from "./ContextBar.vue";

const props = defineProps<{ metrics: SessionMetrics; label: string }>();
const fraction = computed(() => {
  const { used, limit } = props.metrics.context;
  return limit && used !== null ? Math.min(1, used / limit) : null;
});
</script>

<template>
  <details class="strip">
    <summary>
      <span class="scope">{{ label }}</span>
      <span :title="t('latestSpeed')" :aria-label="t('latestSpeed') + ' ' + rate(metrics.speed)" class="speed">
        {{ rate(metrics.speed) }} t/s
      </span>
      <span :title="t('latestTtft')" :aria-label="t('latestTtft') + ' ' + seconds(metrics.ttft)">{{ seconds(metrics.ttft) }}</span>
      <span :title="t('contextUsage')" :aria-label="t('contextUsage') + ' ' + percent(fraction)" class="context">
        <i aria-hidden="true" :style="{ '--fill': (fraction ?? 0) * 100 + '%' }" />
        {{ percent(fraction) }}
      </span>
    </summary>
    <ContextBar :breakdown="metrics.context.breakdown" :used="metrics.context.used ?? undefined"
      :max="metrics.context.limit ?? undefined" />
    <p v-if="metrics.context.used === null && !metrics.context.limit" class="muted">{{ t('contextUsage') }} —</p>
  </details>
</template>

<style scoped>
.strip { margin-bottom: 12px; border-block: 1px solid var(--apu-line); }
.strip summary { display: flex; align-items: center; flex-wrap: wrap; gap: 4px 0; padding: 7px 0;
  cursor: pointer; font-variant-numeric: tabular-nums; }
.strip summary > span { white-space: nowrap; }
.scope { margin-right: auto; padding-right: 8px; color: var(--apu-text-2); }
.speed { color: var(--apu-accent); font-weight: 600; }
.speed ~ span { border-inline-start: 1px solid var(--apu-line); padding-inline-start: 9px; margin-inline-start: 9px; }
.context { display: inline-flex; align-items: center; gap: 5px; font-weight: 600; }
.context i { width: 13px; height: 13px; border-radius: 50%;
  background: conic-gradient(var(--apu-accent) var(--fill), var(--apu-line) 0);
  mask: radial-gradient(circle, transparent 40%, black 45%); }
</style>
