<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from "vue";
import { api } from "./api";
import { t } from "./i18n";
import type { Status } from "./types";
import OverviewView from "./components/OverviewView.vue";
import SessionsView from "./components/SessionsView.vue";
import SessionDetail from "./components/SessionDetail.vue";

type Tab = "overview" | "sessions";
const tab = ref<Tab>("overview");
const openSession = ref<string | null>(null);
const status = ref<Status | null>(null);
const statusError = ref("");
// Bumped to make every view refetch
const tick = ref(0);

async function loadStatus() {
  try {
    status.value = await api.status();
    statusError.value = "";
  } catch (error) {
    statusError.value = String((error as Error).message || error);
  }
}

function refresh() {
  tick.value++;
  void loadStatus();
}

// The host sets theme variables on the root element; pick light or dark from the background
function applyTheme() {
  const probe = getComputedStyle(document.body).backgroundColor;
  const [r, g, b] = (probe.match(/\d+(\.\d+)?/g) || ["255", "255", "255"]).map(Number);
  const luminance = (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255;
  document.documentElement.dataset.theme = luminance < 0.5 ? "dark" : "light";
}

let timer = 0;
let observer: MutationObserver | null = null;
onMounted(() => {
  applyTheme();
  observer = new MutationObserver(applyTheme);
  observer.observe(document.documentElement, { attributes: true, attributeFilter: ["style"] });
  void loadStatus();
  timer = window.setInterval(() => {
    if (document.visibilityState === "visible") refresh();
  }, 10_000);
});
onBeforeUnmount(() => {
  window.clearInterval(timer);
  observer?.disconnect();
});

function show(next: Tab) {
  tab.value = next;
  openSession.value = null;
}
</script>

<template>
  <div class="shell">
    <header class="bar">
      <nav class="tabs" role="tablist">
        <button
          v-for="name in (['overview', 'sessions'] as const)"
          :key="name"
          role="tab"
          :aria-selected="tab === name && !openSession"
          :class="{ active: tab === name && !openSession }"
          @click="show(name)"
        >
          {{ t(name) }}
        </button>
      </nav>
      <span class="muted catalog" v-if="status?.catalog.updatedAt">
        {{ t("catalog", { version: status.catalog.updatedAt.slice(0, 10) }) }}
      </span>
      <button @click="refresh">{{ t("refresh") }}</button>
    </header>

    <div v-if="status && !status.sqlite" class="notice critical" role="alert">{{ t("noSqlite") }}</div>
    <div v-if="status?.error" class="notice critical" role="alert">{{ status.error }}</div>
    <div v-if="status?.partial" class="notice" role="status">{{ t("partialScan", status.partial) }}</div>
    <div v-if="status?.rebuilt" class="notice" role="status">{{ t("rebuiltLedger") }}</div>
    <div v-if="statusError" class="notice critical" role="alert">
      {{ t("loadFailed", { error: statusError }) }}
    </div>

    <SessionDetail v-if="openSession" :id="openSession" :tick="tick" @back="openSession = null" />
    <OverviewView v-else-if="tab === 'overview'" :tick="tick" @open="openSession = $event" />
    <SessionsView v-else :tick="tick" @open="openSession = $event" />
  </div>
</template>

<style scoped>
.shell { padding: 10px 12px 24px; max-width: 1200px; margin: 0 auto; }
.bar { display: flex; align-items: center; gap: 8px; margin-bottom: 10px; }
.tabs { display: flex; gap: 2px; padding: 2px; border-radius: 8px; background: var(--apu-wash); }
.tabs button { border: none; background: transparent; }
.tabs button.active { background: var(--apu-surface); box-shadow: 0 0 0 1px var(--apu-line); font-weight: 600; }
.catalog { margin-left: auto; font-size: 12px; }
</style>
