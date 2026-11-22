<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { api } from "../api";
import { t } from "../i18n";
import { cacheHit, compact, dateTime, money, percent } from "../format";
import type { SessionSummary } from "../types";

const props = defineProps<{ tick: number }>();
const emit = defineEmits<{ open: [id: string] }>();

const sessions = ref<SessionSummary[]>([]);
const query = ref("");
const error = ref("");
const loading = ref(false);

let generation = 0;
async function load() {
  const current = ++generation;
  loading.value = true;
  try {
    const result = await api.sessions();
    if (current !== generation) return;
    sessions.value = result.sessions;
    error.value = "";
  } catch (e) {
    if (current !== generation) return;
    sessions.value = [];
    error.value = String((e as Error).message || e);
  } finally {
    if (current === generation) loading.value = false;
  }
}
watch(() => props.tick, load, { immediate: true });

// Nested agent runs follow their parent; a run whose parent is gone stands on its own
const rows = computed(() => {
  const q = query.value.trim().toLowerCase();
  const match = (s: SessionSummary) =>
    !q || s.id.includes(q) || s.title.toLowerCase().includes(q);
  const ids = new Set(sessions.value.map((s) => s.id));
  const children = new Map<string, SessionSummary[]>();
  const roots: SessionSummary[] = [];
  for (const s of sessions.value) {
    if (s.parentId && ids.has(s.parentId)) {
      const siblings = children.get(s.parentId);
      if (siblings) siblings.push(s);
      else children.set(s.parentId, [s]);
    } else {
      roots.push(s);
    }
  }
  const out: { session: SessionSummary; depth: number }[] = [];
  const seen = new Set<string>();
  const walk = (s: SessionSummary, depth: number) => {
    if (seen.has(s.id)) return;
    seen.add(s.id);
    const kids = children.get(s.id) ?? [];
    if (match(s) || kids.some(match)) out.push({ session: s, depth });
    for (const kid of kids) walk(kid, depth + 1);
  };
  roots.forEach((s) => walk(s, 0));
  // A damaged parent link must not hide an entire cycle
  for (const s of sessions.value) walk(s, 0);
  return out;
});
</script>

<template>
  <div :class="{ loading }">
    <input v-model="query" type="search" class="search" :placeholder="t('search')" :aria-label="t('search')" />
    <div v-if="error" class="notice critical" role="alert">{{ t("loadFailed", { error }) }}</div>
    <div class="card">
      <table>
        <thead>
          <tr>
            <th>{{ t("session") }}</th><th>{{ t("lastActive") }}</th>
            <th class="num">{{ t("requests") }}</th><th class="num">{{ t("cacheHit") }}</th>
            <th class="num">{{ t("output") }}</th><th class="num">{{ t("cost") }}</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="{ session: s, depth } in rows" :key="s.id" class="clickable" tabindex="0"
            @click="emit('open', s.id)" @keydown.enter="emit('open', s.id)">
            <td class="name" :style="{ paddingLeft: `${6 + depth * 16}px` }">
              <div class="ellipsis">{{ s.title || s.id }}</div>
              <div class="muted meta ellipsis">{{ t(s.client) }} · {{ s.models.join(", ") }}</div>
            </td>
            <td class="secondary">{{ dateTime(s.lastActiveMs) }}</td>
            <td class="num">{{ compact(s.totals.steps) }}</td>
            <td class="num">{{ percent(cacheHit(s.totals)) }}</td>
            <td class="num">{{ compact(s.totals.outputTokens) }}</td>
            <td class="num">{{ money(s.totals.cost) }}{{ s.totals.unpricedSteps ? " *" : "" }}</td>
          </tr>
          <tr v-if="!rows.length"><td colspan="6" class="muted">{{ t("noSessions") }}</td></tr>
        </tbody>
      </table>
    </div>
  </div>
</template>

<style scoped>
.search { width: 100%; max-width: 360px; margin-bottom: 10px; }
.name { max-width: 420px; }
.meta { font-size: 12px; }
</style>
