<template>
  <div v-if="show" class="modal-overlay" @click.self="$emit('close')">
    <div class="modal" role="dialog" aria-labelledby="plugin-title">
      <div class="modal-header">
        <h2 id="plugin-title" class="modal-title">{{ $t('pluginModal.title') }}</h2>
        <button class="close-btn" :aria-label="$t('sessionViewer.cancel')" @click="$emit('close')">×</button>
      </div>

      <div class="modal-body">
        <p class="plugin-desc">{{ $t('pluginModal.desc') }}</p>

        <dl v-if="status" class="plugin-facts">
          <dt>{{ $t('pluginModal.installed') }}</dt>
          <dd>{{ status.installedVersion ? `v${status.installedVersion}` : $t('pluginModal.notInstalled') }}</dd>
          <dt>{{ $t('pluginModal.bundled') }}</dt>
          <dd>{{ status.bundledVersion ? `v${status.bundledVersion}` : '—' }}</dd>
          <dt>{{ $t('pluginModal.catalog') }}</dt>
          <dd>{{ catalogDate || '—' }}</dd>
          <dt>{{ $t('pluginModal.location') }}</dt>
          <dd class="plugin-path" :title="status.path">{{ status.path }}</dd>
        </dl>

        <p v-if="status?.foreign" class="plugin-note plugin-error" role="alert">{{ $t('pluginModal.foreign') }}</p>
        <p v-else-if="status && !status.bundledVersion" class="plugin-note plugin-error" role="alert">
          {{ $t('pluginModal.missingBundle') }}
        </p>
        <p v-if="statusError" class="plugin-note plugin-error" role="alert">{{ statusError }}</p>
        <p v-if="message" class="plugin-note" :class="{ 'plugin-error': messageIsError }" role="status">{{ message }}</p>
        <p v-if="status?.installedVersion" class="plugin-note">
          <span v-if="status.antigravityRunning">{{ $t('pluginModal.restart') }} </span>
          {{ $t('pluginModal.enable') }}
        </p>

        <div class="plugin-actions">
          <button class="primary-btn" :disabled="busy || !canInstall" @click="install">{{ installLabel }}</button>
          <template v-if="status?.installedVersion">
            <button class="about-btn" :disabled="busy" @click="updateCatalog">
              {{ $t('pluginModal.updateCatalog') }}
            </button>
            <button class="about-btn" :disabled="busy" @click="uninstall">
              {{ $t('pluginModal.uninstall') }}
            </button>
          </template>
        </div>
      </div>

      <div class="modal-footer">
        <p>{{ $t('pluginModal.footer') }}</p>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { ask } from '@tauri-apps/plugin-dialog';
import { useI18n } from 'vue-i18n';

interface PluginStatus {
  path: string;
  bundledVersion: string | null;
  installedVersion: string | null;
  foreign: boolean;
  bundledCatalog: string | null;
  installedCatalog: string | null;
  downloadedCatalog: string | null;
  antigravityRunning: boolean;
}

// Prices are maintained upstream by antigravity-usage-monitor
const CATALOG_URL =
  'https://raw.githubusercontent.com/yuhuan-dev/antigravity-usage-monitor/main/pricing-catalog.json';

const props = defineProps<{ show: boolean }>();
defineEmits(['close']);
const { t } = useI18n();

const status = ref<PluginStatus | null>(null);
const statusError = ref('');
const loading = ref(false);
const changing = ref(false);
const busy = computed(() => loading.value || changing.value);
const message = ref('');
const messageIsError = ref(false);
let statusRequest = 0;

const canInstall = computed(() => Boolean(status.value?.bundledVersion && !status.value.foreign));
const installLabel = computed(() => {
  const s = status.value;
  if (!s?.installedVersion) return t('pluginModal.install');
  if (s.installedVersion !== s.bundledVersion) {
    return t('pluginModal.upgrade', { version: s.bundledVersion ?? '' });
  }
  return t('pluginModal.reinstall');
});
// The plugin uses whichever catalog was updated last
const catalogDate = computed(() => {
  const s = status.value;
  const dates = [s?.installedVersion ? s.installedCatalog : s?.bundledCatalog, s?.downloadedCatalog]
    .map((date) => Date.parse(date ?? ''))
    .filter(Number.isFinite);
  return dates.length ? new Date(Math.max(...dates)).toLocaleDateString() : '';
});

function report(text: string, isError = false) {
  message.value = text;
  messageIsError.value = isError;
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

async function run(action: () => Promise<PluginStatus | undefined>, done: string) {
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
    report(done);
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

function install() {
  return run(() => invoke<PluginStatus>('install_plugin'), t('pluginModal.installed_done'));
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

function updateCatalog() {
  return run(async () => {
    const res = await fetch(CATALOG_URL, { cache: 'no-store' });
    if (!res.ok) throw new Error(t('pluginModal.catalogHttp', { status: res.status }));
    return invoke<PluginStatus>('update_plugin_catalog', { catalog: await res.text() });
  }, t('pluginModal.catalogUpdated'));
}
</script>

<style scoped>
.modal-overlay {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.7);
  backdrop-filter: blur(8px) saturate(150%);
  -webkit-backdrop-filter: blur(8px) saturate(150%);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 100;
}

.modal {
  background: var(--ag-surface);
  border: 1px solid var(--ag-border);
  border-radius: var(--radius-xl);
  width: 440px;
  max-width: 92%;
  box-shadow: var(--ag-shadow-xl);
  overflow: hidden;
}

.modal-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 16px 20px;
  border-bottom: 1px solid var(--ag-border);
}

.modal-title {
  font-size: 15px;
  font-weight: 600;
  margin: 0;
  color: var(--ag-text-strong);
}

.close-btn {
  background: none;
  border: none;
  color: var(--ag-text-tertiary);
  font-size: 20px;
  cursor: pointer;
  width: 30px;
  height: 30px;
  border-radius: var(--radius-sm);
}

.close-btn:hover {
  color: var(--ag-text);
  background: rgba(255, 255, 255, 0.08);
}

.modal-body {
  padding: 18px 20px 20px;
}

.plugin-desc {
  font-size: 12px;
  color: var(--ag-text-secondary);
  line-height: 1.6;
  margin: 0 0 14px;
}

.plugin-facts {
  display: grid;
  grid-template-columns: auto 1fr;
  gap: 6px 14px;
  margin: 0 0 14px;
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
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-family: ui-monospace, monospace;
  font-size: 11px;
}

.plugin-note {
  font-size: 12px;
  color: var(--ag-text-secondary);
  margin: 0 0 12px;
  line-height: 1.5;
}

.plugin-error {
  color: var(--ag-error);
}

.plugin-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}

.about-btn {
  padding: 9px 16px;
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
  padding: 9px 18px;
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

.modal-footer {
  padding: 12px 20px;
  border-top: 1px solid var(--ag-border);
}

.modal-footer p {
  font-size: 10px;
  color: var(--ag-text-muted);
  margin: 0;
}
</style>
