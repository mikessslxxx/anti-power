<script setup lang="ts">
// Daily cost columns: one series, so the card title names it and no legend is drawn
import { computed, ref } from "vue";
import { t } from "../i18n";
import { compact, localDay, money } from "../format";
import type { Summary } from "../types";

const props = defineProps<{ days: Summary["byDay"] }>();

const showTable = ref(false);
const hover = ref<number | null>(null);

const WIDTH = 640;
const HEIGHT = 180;
const PAD = { top: 12, right: 8, bottom: 22, left: 44 };
const MAX_DAYS = 92;

// Fill missing days between the first and last so gaps read as zero
const series = computed(() => {
  const known = props.days.filter((d) => /^\d{4}-\d{2}-\d{2}$/.test(d.day));
  if (!known.length) return [];
  const byDay = new Map(known.map((d) => [d.day, d]));
  const out: { day: string; cost: number; steps: number }[] = [];
  const start = new Date(`${known[0].day}T00:00:00`);
  const end = new Date(`${known[known.length - 1].day}T00:00:00`);
  const earliest = new Date(end);
  earliest.setDate(end.getDate() - MAX_DAYS + 1);
  if (start < earliest) start.setTime(earliest.getTime());
  for (const d = start; d <= end; d.setDate(d.getDate() + 1)) {
    const day = localDay(d);
    const row = byDay.get(day);
    out.push({ day, cost: row?.cost ?? 0, steps: row?.steps ?? 0 });
  }
  return out;
});

// Round the axis up to 1, 2, or 5 times a power of ten
const niceMax = computed(() => {
  const max = Math.max(...series.value.map((d) => d.cost), 0);
  if (max <= 0) return 1;
  const power = 10 ** Math.floor(Math.log10(max));
  return ([1, 2, 5, 10].find((m) => m * power >= max) ?? 10) * power;
});

const plotW = WIDTH - PAD.left - PAD.right;
const plotH = HEIGHT - PAD.top - PAD.bottom;
const band = computed(() => plotW / Math.max(series.value.length, 1));
const barW = computed(() => Math.max(2, Math.min(24, band.value - 2)));
const y = (v: number) => PAD.top + plotH - (v / niceMax.value) * plotH;
const ticks = computed(() => [0, niceMax.value / 2, niceMax.value]);

// Column with a 4px rounded top and a square base on the baseline
function column(i: number, cost: number): string {
  const x = PAD.left + i * band.value + (band.value - barW.value) / 2;
  const base = PAD.top + plotH;
  const h = base - y(cost);
  if (h <= 0) return "";
  const r = Math.min(4, barW.value / 2, h);
  const w = barW.value;
  const top = base - h;
  return `M${x},${base}V${top + r}Q${x},${top} ${x + r},${top}H${x + w - r}Q${x + w},${top} ${x + w},${top + r}V${base}Z`;
}

// Label a handful of days so they never collide
const labelEvery = computed(() => Math.max(1, Math.ceil(series.value.length / 8)));

const tip = computed(() => {
  if (hover.value === null) return null;
  const d = series.value[hover.value];
  if (!d) return null;
  const x = PAD.left + (hover.value + 0.5) * band.value;
  return { ...d, left: `${(x / WIDTH) * 100}%`, top: `${(y(d.cost) / HEIGHT) * 100}%` };
});
</script>

<template>
  <div class="card">
    <h3 class="card-title">
      <span>{{ t("dailyCost") }}</span>
      <button v-if="series.length" @click="showTable = !showTable">
        {{ showTable ? t("showChart") : t("showTable") }}
      </button>
    </h3>
    <p v-if="!days.length" class="muted">{{ t("noData") }}</p>
    <table v-else-if="showTable || !series.length">
      <thead>
        <tr><th>{{ t("day") }}</th><th class="num">{{ t("requests") }}</th><th class="num">{{ t("cost") }}</th></tr>
      </thead>
      <tbody>
        <tr v-for="d in days" :key="d.day">
          <td>{{ d.day }}</td><td class="num">{{ compact(d.steps) }}</td><td class="num">{{ money(d.cost) }}</td>
        </tr>
      </tbody>
    </table>
    <div v-else class="chart" @pointerleave="hover = null">
      <svg :viewBox="`0 0 ${WIDTH} ${HEIGHT}`" role="img" :aria-label="t('dailyCost')">
        <g v-for="v in ticks" :key="v">
          <line :x1="PAD.left" :x2="WIDTH - PAD.right" :y1="y(v)" :y2="y(v)"
            :class="v === 0 ? 'baseline' : 'grid'" />
          <text :x="PAD.left - 6" :y="y(v)" class="axis" text-anchor="end" dominant-baseline="middle">
            {{ money(v) }}
          </text>
        </g>
        <g v-for="(d, i) in series" :key="d.day">
          <path :d="column(i, d.cost)" class="bar" :class="{ dim: hover !== null && hover !== i }" />
          <!-- The whole band is the hit target, larger than the column -->
          <rect :x="PAD.left + i * band" :y="PAD.top" :width="band" :height="plotH" class="hit"
            tabindex="0" :aria-label="`${d.day} ${money(d.cost)}`"
            @pointerenter="hover = i" @focus="hover = i" @blur="hover = null" />
          <text v-if="i % labelEvery === 0" :x="PAD.left + (i + 0.5) * band" :y="HEIGHT - 6"
            class="axis" text-anchor="middle">{{ d.day.slice(5) }}</text>
        </g>
      </svg>
      <div v-if="tip" class="tip" :style="{ left: tip.left, top: tip.top }">
        <strong>{{ money(tip.cost) }}</strong>
        <span class="secondary">{{ tip.day }} · {{ compact(tip.steps) }} {{ t("requests") }}</span>
      </div>
    </div>
  </div>
</template>

<style scoped>
.chart { position: relative; }
svg { display: block; width: 100%; height: auto; overflow: visible; }
.grid { stroke: var(--apu-grid); stroke-width: 1; }
.baseline { stroke: var(--apu-baseline); stroke-width: 1; }
.axis { fill: var(--apu-muted); font-size: 10px; font-variant-numeric: tabular-nums; }
.bar { fill: var(--apu-s1); transition: opacity 0.1s; }
.bar.dim { opacity: 0.55; }
.hit { fill: transparent; outline: none; }
.tip {
  position: absolute;
  transform: translate(-50%, calc(-100% - 8px));
  pointer-events: none;
  display: flex;
  flex-direction: column;
  padding: 6px 8px;
  border-radius: 6px;
  background: var(--apu-surface);
  border: 1px solid var(--apu-line);
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.12);
  white-space: nowrap;
  font-size: 12px;
}
</style>
