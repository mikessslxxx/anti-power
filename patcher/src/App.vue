<script setup lang="ts">
import { onMounted, onUnmounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { parseNavigation } from "./navigation";
import { version } from "../package.json";
import TitleBar from "./components/TitleBar.vue";
import AboutModal from "./components/AboutModal.vue";
import PluginModal from "./components/PluginModal.vue";
import SessionViewer from "./components/SessionViewer.vue";
import FlowBackground from "./components/FlowBackground.vue";

const GITHUB_URL = "https://github.com/mikessslxxx/anti-power";
const showAbout = ref(false);
const showPlugin = ref(false);
const viewer = ref<InstanceType<typeof SessionViewer> | null>(null);
let unlisten: UnlistenFn | undefined;
let disposed = false;

function navigate(urls: string[]) {
  for (const url of urls) {
    const target = parseNavigation(url);
    if (!target) continue;
    showAbout.value = false;
    showPlugin.value = target.kind === "plugins";
    if (target.kind === "session") viewer.value?.openSession(target);
  }
}

onMounted(async () => {
  let received = false;
  unlisten = await listen<string[]>("deep-link://new-url", ({ payload }) => {
    received = true;
    navigate(payload);
  });
  if (disposed) return unlisten();
  const initial = await invoke<string[] | null>("plugin:deep-link|get_current");
  if (!disposed && !received && initial) navigate(initial);
});
onUnmounted(() => {
  disposed = true;
  unlisten?.();
});
</script>

<template>
  <div class="app-wrapper">
    <FlowBackground />
    <TitleBar :title="$t('app.title')" @openAbout="showAbout = true" @openPlugin="showPlugin = true" />

    <main class="app-container">
      <SessionViewer ref="viewer" />
    </main>

    <AboutModal
      :show="showAbout"
      :version="version"
      :githubUrl="GITHUB_URL"
      @close="showAbout = false"
    />
    <PluginModal :show="showPlugin" @close="showPlugin = false" />
  </div>
</template>

<style scoped>
.app-wrapper {
  position: relative;
  height: 100vh;
  min-width: 0;
  display: flex;
  flex-direction: column;
  background-color: var(--ag-bg);
  color: var(--ag-text);
  overflow: hidden;
}

.app-container {
  position: relative;
  z-index: 1;
  flex: 1;
  min-height: 0;
  display: grid;
  grid-template-rows: minmax(0, 1fr);
  width: min(1200px, 100%);
  margin: 0 auto;
  padding: 16px 20px 20px;
}
</style>
