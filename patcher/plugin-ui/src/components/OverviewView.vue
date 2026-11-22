<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { api } from "../api";
import { t } from "../i18n";
import { cacheHit, compact, daysAgo, money, percent } from "../format";
import type { Summary } from "../types";
import DailyCostChart from "./DailyCostChart.vue";

const props = defineProps<{ tick: number }>();
const emit = defineEmits<{ open: [id: string] }>();

type Range = "today" | "last7" | "last30" | "all" | "custom";
const range = ref<Range>("last7");
const customFrom = ref(daysAgo(29));
const customTo = ref(daysAgo(0));
const summary = ref<Summary | null>(null);
const error = ref("");
const loading = ref(false);

function bounds(): [string?, string?] {
  switch (range.value) {
    case "today": return [daysAgo(0), daysAgo(0)];
    case "last7": return [daysAgo(6), daysAgo(0)];
    case "last30": return [daysAgo(29), daysAgo(0)];
    case "custom": return [customFrom.value, customTo.value];
    default: return [];
  }
}

let generation = 0;
async function load() {
  const current = ++generation;
  loading.value = true;
  try {
    const next = await api.summary(...bounds());
    if (current !== generation) return;
    summary.value = next;
    error.value = "";
  } catch (e) {
    if (current !== generation) return;
    summary.value = null;
    error.value = String((e as Error).message || e);
  } finally {
    if (current === generation) loading.value = false;
  }
}
watch([range, customFrom, customTo, () => props.tick], load, { immediate: true });

const tiles = computed(() => {
  const s = summary.value?.total;
  if (!s) return [];
  return [
    { label: t("estimatedCost"), value: money(s.cost) },
    { label: t("requests"), value: compact(s.steps) },
    { label: t("newInput"), value: compact(s.inputTokens) },
    { label: t("cacheRead"), value: compact(s.cacheReadTokens), sub: `${t("cacheHit")} ${percent(cacheHit(s))}` },
    { label: t("output"), value: compact(s.outputTokens), sub: `${t("thinking")} ${compact(s.thinkingTokens)}` },
  ];
});
const presets: Range[] = ["today", "last7", "last30", "all", "custom"];
</script>

<template>
  <div :class="{ loading }">
    <div class="filters">
      <div class="presets" role="radiogroup">
        <button v-for="p in presets" :key="p" role="radio" :aria-checked="range === p"
          :class="{ on: range === p }" @click="range = p">{{ t(p) }}</button>
      </div>
      <template v-if="range === 'custom'">
        <label>{{ t("from") }} <input v-model="customFrom" type="date" /></label>
        <label>{{ t("to") }} <input v-model="customTo" type="date" /></label>
      </template>
    </div>

    <div v-if="error" class="notice critical" role="alert">{{ t("loadFailed", { error }) }}</div>

    <template v-if="summary">
      <div class="tiles">
        <div v-for="tile in tiles" :key="tile.label" class="card tile">
          <div class="secondary">{{ tile.label }}</div>
          <div class="value">{{ tile.value }}</div>
          <div v-if="tile.sub" class="muted sub">{{ tile.sub }}</div>
        </div>
      </div>
      <p v-if="summary.total.unpricedSteps" class="muted note">
        {{ t("unpricedNote", { n: summary.total.unpricedSteps }) }}
      </p>

      <DailyCostChart :days="summary.byDay" />

      <div class="grid2">
        <div class="card">
          <h3 class="card-title">{{ t("byModel") }}</h3>
          <table>
            <thead><tr><th>{{ t("model") }}</th><th class="num">{{ t("requests") }}</th><th class="num">{{ t("cacheHit") }}</th><th class="num">{{ t("cost") }}</th></tr></thead>
            <tbody>
              <tr v-for="m in summary.byModel" :key="m.model">
                <td class="ellipsis">{{ m.label }}</td>
                <td class="num">{{ compact(m.steps) }}</td>
                <td class="num">{{ percent(cacheHit(m)) }}</td>
                <td class="num">{{ m.unpricedSteps === m.steps ? t("unpriced") : money(m.cost) }}</td>
              </tr>
              <tr v-if="!summary.byModel.length"><td colspan="4" class="muted">{{ t("noData") }}</td></tr>
            </tbody>
          </table>
        </div>
        <div class="card">
          <h3 class="card-title">{{ t("byClient") }}</h3>
          <table>
            <thead><tr><th>{{ t("source") }}</th><th class="num">{{ t("requests") }}</th><th class="num">{{ t("cost") }}</th></tr></thead>
            <tbody>
              <tr v-for="c in summary.byClient" :key="c.client">
                <td>{{ t(c.client) }}</td>
                <td class="num">{{ compact(c.steps) }}</td>
                <td class="num">{{ money(c.cost) }}</td>
              </tr>
              <tr v-if="!summary.byClient.length"><td colspan="3" class="muted">{{ t("noData") }}</td></tr>
            </tbody>
          </table>
        </div>
      </div>

      <div class="card">
        <h3 class="card-title">{{ t("bySession") }}</h3>
        <table>
          <thead>
            <tr>
              <th>{{ t("session") }}</th><th class="num">{{ t("requests") }}</th>
              <th class="num">{{ t("newInput") }}</th><th class="num">{{ t("cacheRead") }}</th>
              <th class="num">{{ t("output") }}</th><th class="num">{{ t("cost") }}</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="s in summary.sessions" :key="s.id" class="clickable" tabindex="0"
              @click="emit('open', s.id)" @keydown.enter="emit('open', s.id)">
              <td class="ellipsis title">{{ s.title || s.id }}</td>
              <td class="num">{{ compact(s.totals.steps) }}</td>
              <td class="num">{{ compact(s.totals.inputTokens) }}</td>
              <td class="num">{{ compact(s.totals.cacheReadTokens) }}</td>
              <td class="num">{{ compact(s.totals.outputTokens) }}</td>
              <td class="num">{{ money(s.totals.cost) }}</td>
            </tr>
            <tr v-if="summary.deleted.steps" class="deleted" :title="t('deletedHint')">
              <td>{{ t("deletedSessions") }}</td>
              <td class="num">{{ compact(summary.deleted.steps) }}</td>
              <td class="num">{{ compact(summary.deleted.inputTokens) }}</td>
              <td class="num">{{ compact(summary.deleted.cacheReadTokens) }}</td>
              <td class="num">{{ compact(summary.deleted.outputTokens) }}</td>
              <td class="num">{{ money(summary.deleted.cost) }}</td>
            </tr>
            <tr v-if="!summary.sessions.length && !summary.deleted.steps">
              <td colspan="6" class="muted">{{ t("noData") }}</td>
            </tr>
          </tbody>
        </table>
      </div>
    </template>
  </div>
</template>

<style scoped>
.filters { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; margin-bottom: 10px; }
.presets { display: flex; gap: 2px; padding: 2px; border-radius: 8px; background: var(--apu-wash); }
.presets button { border: none; background: transparent; }
.presets button.on { background: var(--apu-surface); box-shadow: 0 0 0 1px var(--apu-line); font-weight: 600; }
.tiles { display: grid; grid-template-columns: repeat(auto-fit, minmax(130px, 1fr)); gap: 8px; margin-bottom: 8px; }
.tile .value { font-size: 20px; font-weight: 600; }
.tile .sub { font-size: 12px; }
.note { margin: 0 0 8px; font-size: 12px; }
.card + .card, .grid2, .grid2 + .card, .tiles + .card, .note + .card { margin-top: 10px; }
.grid2 { display: grid; grid-template-columns: repeat(auto-fit, minmax(280px, 1fr)); gap: 10px; }
.grid2 .card, .tiles .card { margin-top: 0; }
.title { max-width: 360px; }
tr.deleted td { color: var(--apu-text-2); font-style: italic; }
</style>
