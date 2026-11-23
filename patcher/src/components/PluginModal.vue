<template>
  <AppModal v-if="show" :title="$t('pluginModal.title')" @close="$emit('close')">
    <header class="plugin-heading">
      <h3 class="modal-product-name">Antigravity Usage &amp; Speed Monitor</h3>
      <i18n-t keypath="pluginModal.source" scope="global" tag="p">
        <template #github><a :href="releaseUrl" @click.prevent="openReleasePage">GitHub</a></template>
      </i18n-t>
    </header>
    <p class="plugin-desc">{{ $t('pluginModal.desc') }}</p>

    <dl class="plugin-facts">
      <dt>{{ $t('pluginModal.installed') }}</dt>
      <dd>{{ status ? (status.installedVersion ? `v${status.installedVersion}` : $t('pluginModal.notInstalled')) : '—' }}</dd>
      <dt>{{ $t('pluginModal.location') }}</dt>
      <dd class="plugin-path" :title="status?.path">{{ status?.path ?? '—' }}</dd>
    </dl>

    <div aria-live="polite" class="plugin-feedback">
      <p v-if="status?.foreign" class="plugin-note plugin-error">{{ $t('pluginModal.foreign') }}</p>
      <p v-if="statusError" class="plugin-note plugin-error">{{ statusError }}</p>
      <p v-if="message" class="plugin-note" :class="{ 'plugin-error': messageIsError }">{{ message }}</p>
      <p v-if="status?.installedVersion && !messageIsError" class="plugin-note">{{ $t('pluginModal.enable') }}</p>
    </div>

    <template #footer>
      <div class="plugin-actions">
        <button v-if="showLauncher" class="about-btn" :disabled="!canLaunch" @click="launch">
          {{ $t('pluginModal.launch') }}
        </button>
        <button v-if="!status?.installedVersion" class="primary-btn" :disabled="busy || !status || status.foreign" @click="install">
          {{ $t('pluginModal.install') }}
        </button>
        <button v-else class="about-btn" :disabled="busy" @click="uninstall">
          {{ $t('pluginModal.uninstall') }}
        </button>
      </div>
    </template>
  </AppModal>
</template>

<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import AppModal from './AppModal.vue';
import { invoke } from '@tauri-apps/api/core';
import { ask } from '@tauri-apps/plugin-dialog';
import { useI18n } from 'vue-i18n';

interface PluginStatus {
  path: string;
  installedVersion: string | null;
  macosLauncher: boolean;
  foreign: boolean;
  savedFolder: string | null;
}

const props = defineProps<{ show: boolean }>();
defineEmits(['close']);
const { t } = useI18n();
const releaseUrl = 'https://github.com/yuhuan-dev/antigravity-usage-monitor/releases/latest';

const status = ref<PluginStatus | null>(null);
const statusError = ref('');
const loading = ref(false);
const changing = ref(false);
const busy = computed(() => loading.value || changing.value);
const showLauncher = computed(() => status.value?.macosLauncher ?? navigator.platform.toLowerCase().includes('mac'));
const canLaunch = computed(() => !busy.value && !!status.value?.macosLauncher && !!status.value.installedVersion && !status.value.foreign);
const message = ref('');
const messageIsError = ref(false);
let statusRequest = 0;

function report(text: string, isError = false) {
  message.value = text;
  messageIsError.value = isError;
}

async function openReleasePage() {
  try {
    const { openUrl } = await import('@tauri-apps/plugin-opener');
    await openUrl(releaseUrl);
  } catch (error) {
    report(String(error), true);
  }
}

async function refreshStatus() {
  const request = ++statusRequest;
  loading.value = true;
  status.value = null;
  statusError.value = '';
  try {
    const current = await invoke<PluginStatus>('plugin_status');
    if (request === statusRequest) status.value = current;
  } catch (error) {
    if (request === statusRequest) statusError.value = String(error);
  } finally {
    if (request === statusRequest) loading.value = false;
  }
}

async function run(action: () => Promise<PluginStatus | undefined>, done = '') {
  if (busy.value) return;
  changing.value = true;
  report('');
  try {
    const current = await action();
    if (!current) return;
    // A pending status read may have started before the operation changed the installation
    statusRequest++;
    loading.value = false;
    statusError.value = '';
    status.value = current;
    report(current.savedFolder ? t('pluginModal.savedFolder', { path: current.savedFolder }) : done);
  } catch (error) {
    report(String(error), true);
    await refreshStatus();
  } finally {
    changing.value = false;
  }
}

watch(
  () => props.show,
  (show) => {
    if (!show) return;
    report('');
    void refreshStatus();
  },
  { immediate: true }
);

async function launch() {
  if (!canLaunch.value) return;
  changing.value = true;
  try {
    const started = await invoke<boolean>('launch_antigravity');
    report(t(started ? 'pluginModal.launched' : 'pluginModal.quitFirst'));
  } catch (error) {
    report(String(error), true);
    await refreshStatus();
  } finally {
    changing.value = false;
  }
}

function install() {
  return run(() => invoke<PluginStatus>('install_plugin'));
}

function uninstall() {
  return run(async () => {
    const confirmed = await ask(t('pluginModal.uninstallConfirm'), {
      title: t('pluginModal.title'),
      kind: 'warning',
    });
    if (confirmed) return invoke<PluginStatus>('uninstall_plugin');
  }, t('pluginModal.uninstalled'));
}

</script>

<style scoped>
.plugin-heading {
  margin: 2px 0 14px;
  text-align: center;
}

.plugin-heading p {
  margin: 0;
  font-size: 12px;
  color: var(--ag-text-secondary);
}

.plugin-heading a {
  color: var(--ag-accent);
  text-underline-offset: 2px;
}

.plugin-desc {
  font-size: 12px;
  color: var(--ag-text-secondary);
  line-height: 1.6;
  margin: 0 0 10px;
}

.plugin-facts {
  display: grid;
  grid-template-columns: auto 1fr;
  gap: 6px 14px;
  margin: 0 0 10px;
  font-size: 12px;
}

.plugin-facts dt {
  color: var(--ag-text-tertiary);
}

.plugin-facts dd {
  margin: 0;
  color: var(--ag-text);
  min-width: 0;
}

.plugin-path {
  overflow-wrap: anywhere;
  font-family: ui-monospace, monospace;
  font-size: 11px;
}

.plugin-note {
  overflow-wrap: anywhere;
  font-size: 12px;
  color: var(--ag-text-secondary);
  margin: 0;
  line-height: 1.5;
}

.plugin-feedback .plugin-note + .plugin-note { margin-top: 4px; }

.plugin-error {
  color: var(--ag-error);
}


.plugin-actions {
  display: flex;
  gap: 8px;
}

.plugin-actions button { min-width: 70px; }

.about-btn {
  padding: 9px 12px;
  background: var(--ag-surface-2);
  border: 1px solid var(--ag-border);
  border-radius: var(--radius-md);
  color: var(--ag-text-secondary);
  font-size: 12px;
  font-weight: 500;
  cursor: pointer;
}

.about-btn:hover:not(:disabled) {
  background: var(--ag-surface-3);
  color: var(--ag-text);
}

.primary-btn {
  padding: 9px 12px;
  background: var(--ag-accent-gradient);
  border: none;
  border-radius: var(--radius-md);
  color: white;
  font-size: 12px;
  font-weight: 600;
  cursor: pointer;
}

.primary-btn:hover:not(:disabled) {
  filter: brightness(1.1);
}

.about-btn:disabled,
.primary-btn:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}

</style>
