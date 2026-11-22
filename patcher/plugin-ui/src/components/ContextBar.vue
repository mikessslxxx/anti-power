<script setup lang="ts">
// Context window of one request: a thin segmented bar over the budget, with a legend that carries the numbers
import { computed } from "vue";
import { t, type Key } from "../i18n";
import { compact, percent } from "../format";

const props = defineProps<{
  breakdown?: Record<string, number> | null;
  used?: number;
  max?: number;
}>();

// Ten upstream categories folded into six slots, always in this order
const GROUPS: { key: Key; slot: number; fields: string[] }[] = [
  { key: "ctxUser", slot: 1, fields: ["userInputTokens"] },
  { key: "ctxAssistant", slot: 2, fields: ["assistantTokens"] },
  { key: "ctxTools", slot: 3, fields: ["toolResultsTokens", "subagentTokens"] },
  { key: "ctxCheckpoint", slot: 4, fields: ["checkpointTokens"] },
  { key: "ctxDefinitions", slot: 5, fields: ["systemToolsTokens", "mcpToolsTokens", "skillsTokens"] },
  { key: "ctxSystem", slot: 6, fields: ["systemPromptTokens", "userRulesTokens"] },
];

const parts = computed(() =>
  GROUPS.map((g) => ({
    ...g,
    tokens: g.fields.reduce((sum, f) => sum + (Number(props.breakdown?.[f]) || 0), 0),
  })).filter((p) => p.tokens > 0)
);
const sum = computed(() => parts.value.reduce((s, p) => s + p.tokens, 0));
const usedTokens = computed(() => props.used || sum.value);
const budget = computed(() => {
  const max = props.max;
  return max !== undefined && Number.isFinite(max) && max > 0 ? max : undefined;
});
</script>

<template>
  <div v-if="usedTokens || budget" class="ctx">
    <div class="head">
      <span class="secondary">{{ t("context") }}</span>
      <span class="num">
        {{ usedTokens ? compact(usedTokens) : "—" }}
        <template v-if="budget"> / {{ compact(budget) }}</template>
        <span v-if="usedTokens && budget" class="muted">{{ ` · ${percent(usedTokens / budget)}` }}</span>
      </span>
    </div>
    <div v-if="parts.length && budget" class="bar" role="img"
      :aria-label="parts.map((p) => `${t(p.key)} ${compact(p.tokens)}`).join(', ')">
      <span v-for="p in parts" :key="p.key" class="part" tabindex="0"
        :style="{ width: `${(p.tokens / budget) * 100}%`, background: `var(--apu-s${p.slot})` }"
        :title="`${t(p.key)}: ${compact(p.tokens)} (${percent(p.tokens / sum)})`"
        :aria-label="`${t(p.key)}: ${compact(p.tokens)}`" />
    </div>
    <ul v-if="parts.length" class="legend">
      <li v-for="p in parts" :key="p.key">
        <span class="swatch" :style="{ background: `var(--apu-s${p.slot})` }" />
        <span class="secondary">{{ t(p.key) }}</span>
        <span class="num">{{ compact(p.tokens) }}</span>
      </li>
    </ul>
  </div>
</template>

<style scoped>
.ctx { margin: 6px 0 10px; }
.head { display: flex; flex-wrap: wrap; gap: 3px 8px; justify-content: space-between; font-size: 12px; margin-bottom: 4px; }
.bar {
  display: flex;
  gap: 0;
  height: 8px;
  border-radius: 4px;
  overflow: hidden;
  background: var(--apu-wash);
}
.part { display: block; min-width: 0; border-radius: 4px; outline-offset: 2px; }
.part:hover { filter: brightness(1.12); }
.legend { list-style: none; display: flex; flex-wrap: wrap; gap: 4px 14px; padding: 0; margin: 6px 0 0; font-size: 12px; }
.legend li { display: flex; align-items: center; gap: 4px; }
</style>
