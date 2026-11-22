<script setup lang="ts">
import { computed, reactive, ref, watch } from "vue";
import { api } from "../api";
import { t } from "../i18n";
import { cacheHit, compact, dateTime, money, percent, rate, seconds } from "../format";
import type { Detail, Request } from "../types";
import ContextBar from "./ContextBar.vue";

const props = defineProps<{ id: string; tick: number }>();
const emit = defineEmits<{ back: [] }>();

const detail = ref<Detail | null>(null);
const error = ref("");
const loading = ref(false);
// Kept by step index so a refresh does not collapse open rows
const open = reactive(new Set<number>());

let generation = 0;
async function load() {
  const current = ++generation;
  loading.value = true;
  try {
    const result = await api.session(props.id);
    if (current !== generation) return;
    if ("error" in result) {
      error.value = result.error === "not_found" ? t("notFound") : t("unreadable");
      detail.value = null;
      if (result.error === "not_found") emit("back");
    } else {
      detail.value = result;
      error.value = "";
    }
  } catch (e) {
    if (current !== generation) return;
    detail.value = null;
    error.value = t("loadFailed", { error: String((e as Error).message || e) });
  } finally {
    if (current === generation) loading.value = false;
  }
}
watch(() => props.id, () => open.clear());
watch([() => props.id, () => props.tick], load, { immediate: true });

function toggle(r: Request) {
  if (open.has(r.stepIndex)) open.delete(r.stepIndex);
  else open.add(r.stepIndex);
}

const stats = computed(() => {
  const d = detail.value;
  if (!d) return [];
  return [
    { label: t("estimatedCost"), value: money(d.totals.cost) },
    { label: t("requests"), value: compact(d.totals.steps) },
    { label: t("cacheHit"), value: percent(cacheHit(d.totals)) },
    { label: t("avgSpeed"), value: `${rate(d.speed.avgTokPerSec)} tok/s` },
    { label: t("avgTtft"), value: seconds(d.ttft.avgSec) },
  ];
});
</script>

<template>
  <div :class="{ loading }">
    <div class="top">
      <button @click="emit('back')">← {{ t("back") }}</button>
      <h2 class="ellipsis">{{ detail?.title || id }}</h2>
    </div>
    <div v-if="error" class="notice critical" role="alert">{{ error }}</div>

    <template v-if="detail">
      <p class="muted meta">{{ t(detail.client) }} · {{ detail.id }}</p>
      <div class="stats">
        <div v-for="s in stats" :key="s.label" class="stat">
          <span class="secondary">{{ s.label }}</span>
          <strong>{{ s.value }}</strong>
        </div>
      </div>
      <p v-if="detail.totals.unpricedSteps" class="muted note">
        {{ t("unpricedNote", { n: detail.totals.unpricedSteps }) }}
      </p>

      <section v-for="turn in detail.turns" :key="turn.index" class="card turn">
        <div class="turn-head">
          <span class="muted">{{ t("turn", { n: turn.index + 1 }) }} · {{ dateTime(turn.atMs) }}</span>
          <span class="num secondary">{{ money(turn.totals.cost) }}</span>
        </div>
        <div class="prompt">{{ turn.prompt || t("noPrompt") }}</div>

        <table v-if="turn.requests.length">
          <thead>
            <tr>
              <th>{{ t("time") }}</th><th>{{ t("model") }}</th>
              <th class="num">{{ t("newInput") }}</th><th class="num">{{ t("cacheRead") }}</th>
              <th class="num">{{ t("output") }}</th><th class="num">{{ t("ttft") }}</th>
              <th class="num">{{ t("duration") }}</th><th class="num">{{ t("speed") }}</th>
              <th class="num">{{ t("cost") }}</th>
            </tr>
          </thead>
          <tbody>
            <template v-for="r in turn.requests" :key="r.stepIndex">
              <tr class="clickable" tabindex="0" :aria-expanded="open.has(r.stepIndex)"
                @click="toggle(r)" @keydown.enter="toggle(r)">
                <td class="secondary">{{ dateTime(r.createdAtMs) }}</td>
                <td class="ellipsis model">
                  {{ r.model.label }}
                  <span v-if="r.isCompactionStep" class="tag">{{ t("compaction") }}</span>
                </td>
                <td class="num">{{ compact(r.inputTokens) }}</td>
                <td class="num">{{ compact(r.cacheReadTokens) }}</td>
                <td class="num" :title="`${t('thinking')} ${compact(r.thinkingTokens)}`">{{ compact(r.outputTokens) }}</td>
                <td class="num">{{ seconds(r.ttftSec) }}</td>
                <td class="num">{{ seconds(r.totalDurationSec) }}</td>
                <!-- Speeds left out of the average are shown muted -->
                <td class="num" :class="{ muted: !r.speedSampleEligible }" :title="r.speedReason || ''">
                  {{ rate(r.totalTokPerSec) }}
                </td>
                <td class="num">{{ r.cost === null ? t("unpriced") : money(r.cost) }}</td>
              </tr>
              <tr v-if="open.has(r.stepIndex)" class="expanded">
                <td colspan="9">
                  <ContextBar :breakdown="r.contextBreakdown"
                    :used="r.estimatedContextTokens" :max="r.maxContextBudgetTokens" />
                  <p v-if="r.rates" class="secondary small">
                    {{ t("rates") }} · {{ t("newInput") }} ${{ r.rates.input }}
                    · {{ t("cacheRead") }} ${{ r.rates.cache }}
                    · {{ t("output") }} ${{ r.rates.output }}
                  </p>
                  <p v-if="r.cacheHitRate !== null" class="muted small">
                    {{ t("cacheHit") }} {{ percent(r.cacheHitRate) }}
                    <template v-if="r.cacheSavings !== null"> · {{ t("cacheSaved") }} {{ money(r.cacheSavings) }}</template>
                  </p>
                  <details v-if="r.content.thinking">
                    <summary>{{ t("thinking") }}</summary>
                    <pre>{{ r.content.thinking }}</pre>
                  </details>
                  <div v-if="r.content.response">
                    <div class="label secondary">{{ t("response") }}</div>
                    <pre>{{ r.content.response }}</pre>
                  </div>
                  <details v-if="r.content.toolCalls.length">
                    <summary>{{ t("toolCalls", { n: r.content.toolCalls.length }) }}</summary>
                    <div v-for="(call, i) in r.content.toolCalls" :key="i" class="call">
                      <code>{{ call.name }}</code>
                      <pre>{{ call.arguments }}</pre>
                      <p v-if="call.truncated" class="muted small">{{ t("truncated") }}</p>
                    </div>
                  </details>
                </td>
              </tr>
            </template>
          </tbody>
        </table>
        <p v-for="e in turn.errors" :key="e.stepIndex" class="err" role="note">
          <strong>{{ t("error") }}</strong> {{ e.message }}
        </p>
      </section>
    </template>
  </div>
</template>

<style scoped>
.top { display: flex; align-items: center; gap: 10px; }
h2 { font-size: 15px; margin: 0; }
.meta { margin: 4px 0 8px; font-size: 12px; }
.stats { display: flex; flex-wrap: wrap; gap: 6px 18px; margin-bottom: 8px; }
.stat { display: flex; flex-direction: column; }
.stat strong { font-size: 16px; }
.note { margin: 0 0 8px; font-size: 12px; }
.turn { margin-top: 10px; }
.turn-head { display: flex; justify-content: space-between; font-size: 12px; }
.prompt { white-space: pre-wrap; word-break: break-word; margin: 4px 0 8px; font-weight: 500; }
.model { max-width: 200px; }
.tag {
  font-size: 11px;
  padding: 0 5px;
  border-radius: 4px;
  border: 1px solid var(--apu-line);
  color: var(--apu-text-2);
  margin-left: 4px;
}
tr.expanded > td { background: var(--apu-wash); }
pre {
  white-space: pre-wrap;
  word-break: break-word;
  margin: 4px 0 8px;
  font: 12px/1.5 ui-monospace, SFMono-Regular, Menlo, monospace;
  max-height: 360px;
  overflow: auto;
}
details { margin: 4px 0; }
summary { cursor: pointer; color: var(--apu-text-2); }
.label { font-size: 12px; margin-top: 6px; }
.small { font-size: 12px; margin: 2px 0 6px; }
.call code { font-size: 12px; }
.err { margin: 6px 0 0; padding: 6px 8px; border-left: 3px solid var(--apu-critical); background: var(--apu-wash); }
</style>
