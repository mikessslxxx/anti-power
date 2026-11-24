<template>
  <section class="sv-panel" aria-labelledby="sv-title">
    <header class="sv-header">
      <h1 id="sv-title" class="sv-title">{{ $t('sessionViewer.title') }}</h1>
      <div class="sv-header-actions">
        <div class="sv-segmented" role="radiogroup" :aria-label="$t('sessionViewer.filterLabel')">
          <label
            v-for="option in providerOptions"
            :key="option.value"
            class="sv-seg"
            :class="{ active: filterProvider === option.value, disabled: deletionPending }"
            :title="option.title"
          >
            <input v-model="filterProvider" class="sv-sr-only" type="radio" :value="option.value" :disabled="deletionPending" />
            <span>{{ option.label }}</span>
            <span v-if="option.version" class="sv-seg-version">{{ option.version }}</span>
          </label>
        </div>
        <div class="sv-search">
          <svg viewBox="0 0 16 16" aria-hidden="true"><circle cx="7" cy="7" r="4.25" /><path d="m10.25 10.25 3 3" /></svg>
          <input
            v-model="searchQuery"
            type="search"
            :placeholder="$t('sessionViewer.searchPlaceholder')"
            :aria-label="$t('sessionViewer.searchLabel')"
            :disabled="deletionPending"
          />
        </div>
        <label class="sv-chip" :title="$t('sessionViewer.hideIndexOnlyHint')">
          <input v-model="hideIndexOnly" type="checkbox" :disabled="deletionPending" />
          <span>{{ $t('sessionViewer.hideIndexOnly') }}</span>
        </label>
        <button class="sv-btn" :disabled="actionsBusy" @click="scanSessions">
          <svg :class="{ spinning: isScanning }" viewBox="0 0 16 16" aria-hidden="true"><path d="M13.25 8A5.25 5.25 0 1 1 11.7 4.3" /><path d="M13.25 2.5v2.75H10.5" /></svg>
          <span>{{ $t('sessionViewer.refresh') }}</span>
        </button>
      </div>
    </header>

    <div class="sv-options">
      <span class="sv-options-label">{{ $t('sessionViewer.cleanOptions') }}</span>
      <label class="sv-chip" :title="$t('sessionViewer.optionAppCacheHint')">
        <input v-model="cleanOptions.appCache" type="checkbox" :disabled="actionsBusy" />
        <span>{{ $t('sessionViewer.optionAppCache') }}</span>
      </label>
      <label class="sv-chip" :title="$t('sessionViewer.optionCcSwitchHint')">
        <input v-model="cleanOptions.ccSwitch" type="checkbox" :disabled="actionsBusy" />
        <span>{{ $t('sessionViewer.optionCcSwitch') }}</span>
      </label>
      <label class="sv-chip" :title="$t('sessionViewer.optionUsageMonitorHint')">
        <input v-model="cleanOptions.usageMonitor" type="checkbox" :disabled="actionsBusy" />
        <span>{{ $t('sessionViewer.optionUsageMonitor') }}</span>
      </label>
      <button
        v-if="magpieStoreAvailable"
        class="sv-btn sv-options-action"
        :disabled="actionsBusy"
        @click="cleanMagpieProbes"
      >
        <span>{{ $t('sessionViewer.cleanMagpieProbes') }}</span>
      </button>
    </div>

    <div v-if="scanError" class="sv-feedback sv-error" role="alert">
      {{ $t('sessionViewer.scanFailed', { error: scanError }) }}
    </div>
    <div v-if="ccSwitchVerificationPending" class="sv-feedback sv-notice" role="status">
      {{ $t('sessionViewer.ccSwitchVerificationPending') }}
    </div>
    <div v-if="usageMonitorCleanupPending" class="sv-feedback sv-notice" role="status">
      {{ $t('sessionViewer.usageMonitorCleanupPending') }}
    </div>
    <div v-if="scanWarnings.length" class="sv-feedback sv-error" role="alert">
      <div>{{ $t('sessionViewer.recoveryPending') }}</div>
      <ul>
        <li v-for="warning in scanWarnings" :key="warning">{{ warning }}</li>
      </ul>
    </div>
    <div v-if="confirmationError" class="sv-feedback sv-error" role="alert">
      {{ $t('sessionViewer.confirmFailed', { error: confirmationError }) }}
    </div>
    <div
      v-if="magpieFeedback"
      class="sv-feedback"
      :class="magpieFeedback.error ? 'sv-error' : 'sv-notice'"
      :role="magpieFeedback.error ? 'alert' : 'status'"
    >
      {{ magpieFeedback.text }}
    </div>
    <div
      v-if="deleteResult"
      class="sv-feedback"
      :class="deleteResult.failures.length || deleteResult.warnings.length ? 'sv-error' : 'sv-success'"
    >
      <div role="status">
        {{ $t('sessionViewer.deleteResult', { deleted: deleteResult.deleted, failed: deleteResult.failures.length }) }}
        <div v-if="deleteResult.orphanIndexesDeleted">
          {{ $t('sessionViewer.orphanIndexesDeleted', { count: deleteResult.orphanIndexesDeleted }) }}
        </div>
      </div>
      <ul v-if="deleteResult.failures.length">
        <li v-for="failure in deleteResult.failures" :key="failure.key" :title="failure.sourcePath">
          {{ $t('sessionViewer.deleteFailed', { name: failure.name, error: failure.error }) }}
        </li>
      </ul>
      <template v-if="deleteResult.warnings.length">
        <div>{{ $t('sessionViewer.deleteWarnings') }}</div>
        <ul>
          <li v-for="warning in deleteResult.warnings" :key="warning">{{ warning }}</li>
        </ul>
      </template>
    </div>

    <div class="sv-body">
      <div class="sv-card sv-list-pane" :aria-busy="isScanning">
        <div class="sv-list-meta">
          <span class="sv-count">{{ $t('sessionViewer.sessionCount', { count: filteredSessions.length }) }}</span>
          <label v-if="filteredSessions.length" class="sv-select-all">
            <input type="checkbox" :checked="isAllChecked" :disabled="actionsBusy" @change="toggleAllChecked" />
            <span>{{ $t('sessionViewer.selectAll') }}</span>
          </label>
        </div>
        <div v-if="isScanning" class="sv-empty" role="status">
          <span class="sv-spinner" aria-hidden="true"></span>
          {{ $t('sessionViewer.scanning') }}
        </div>
        <div v-else-if="!filteredSessions.length" class="sv-empty">
          <template v-if="!scanError || sessions.length">
            {{ $t(sessions.length ? 'sessionViewer.noMatches' : 'sessionViewer.noSessions') }}
          </template>
        </div>
        <ul v-else class="sv-list">
          <li
            v-for="session in filteredSessions"
            :key="sessionKey(session)"
            class="sv-item"
            :class="{ active: selectedKey === sessionKey(session), checked: checkedSessions.has(sessionKey(session)) }"
          >
            <input
              type="checkbox"
              class="sv-item-checkbox"
              :aria-label="$t('sessionViewer.selectSession', { name: sessionName(session) })"
              :checked="checkedSessions.has(sessionKey(session))"
              :disabled="actionsBusy"
              @change="toggleCheck(sessionKey(session))"
            />
            <button
              class="sv-item-open"
              :aria-current="selectedKey === sessionKey(session) ? 'true' : undefined"
              :title="session.sourcePath"
              :disabled="deletionPending"
              @click="selectSession(session)"
            >
              <span class="sv-item-title" :title="sessionName(session)">{{ sessionName(session) }}</span>
              <span class="sv-item-sub">
                <span
                  :class="['sv-provider-badge', 'sv-provider-' + session.providerId]"
                  :title="$t('sessionViewer.providers.' + session.providerId)"
                >
                  {{ $t('sessionViewer.providersShort.' + session.providerId) }}
                </span>
                <span v-if="session.lastActiveAt">{{ formatRelativeTime(session.lastActiveAt) }}</span>
                <span v-if="session.projectDir" class="sv-item-project">{{ pathBasename(session.projectDir) }}</span>
                <span v-if="session.indexOnly" class="sv-index-only">{{ $t('sessionViewer.indexOnly') }}</span>
              </span>
            </button>
          </li>
        </ul>
        <Transition name="sv-slide">
          <div v-if="checkedCount" class="sv-batch-bar">
            <span class="sv-batch-count">{{ $t('sessionViewer.selectedCount', { count: checkedCount }) }}</span>
            <button class="sv-danger-btn solid" :disabled="actionsBusy" @click="batchDelete">
              {{ $t(isDeleting ? 'sessionViewer.deleting' : 'sessionViewer.batchDelete') }}
            </button>
          </div>
        </Transition>
      </div>

      <div class="sv-card sv-msg-pane" :aria-busy="isLoadingMessages">
        <div v-if="!selectedSession" class="sv-empty-msg">
          <svg class="sv-empty-icon" viewBox="0 0 24 24" aria-hidden="true"><path d="M4.5 6.5A2.5 2.5 0 0 1 7 4h10a2.5 2.5 0 0 1 2.5 2.5v7A2.5 2.5 0 0 1 17 16h-6l-4.5 3.5V16H7a2.5 2.5 0 0 1-2.5-2.5z" /><path d="M8.5 8.75h7M8.5 11.75h4.5" /></svg>
          {{ $t('sessionViewer.selectToView') }}
        </div>
        <template v-else>
          <div class="sv-msg-header">
            <div class="sv-msg-title-row">
              <h2 class="sv-msg-session-title">{{ sessionName(selectedSession) }}</h2>
              <button class="sv-danger-btn" :disabled="actionsBusy" @click="deleteSelectedSession">
                <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M2.75 4.25h10.5M6.25 4.25V2.75h3.5v1.5M4.25 4.25l.6 8.6c.06.8.72 1.4 1.5 1.4h3.3c.78 0 1.44-.6 1.5-1.4l.6-8.6" /></svg>
                {{ $t(isDeleting ? 'sessionViewer.deleting' : 'sessionViewer.deleteSession') }}
              </button>
            </div>
            <div class="sv-msg-meta">
              <span :class="['sv-provider-badge', 'sv-provider-' + selectedSession.providerId]">
                {{ $t('sessionViewer.providers.' + selectedSession.providerId) }}
              </span>
              <button
                v-if="selectedSession.projectDir"
                class="sv-path-chip"
                :title="selectedSession.projectDir"
                @click="reveal(selectedSession.projectDir)"
              >
                <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M1.75 4.25c0-.83.67-1.5 1.5-1.5h3l1.5 1.75h5c.83 0 1.5.67 1.5 1.5v5.75c0 .83-.67 1.5-1.5 1.5h-9.5c-.83 0-1.5-.67-1.5-1.5z" /></svg>
                <span class="sv-sr-only">{{ $t('sessionViewer.projectDir') }}:</span>
                {{ pathBasename(selectedSession.projectDir) }}
              </button>
              <span v-if="selectedSession.lastActiveAt" class="sv-meta-item">
                {{ new Date(selectedSession.lastActiveAt).toLocaleString() }}
              </span>
              <span v-if="messages.length" class="sv-meta-item">
                {{ $t('sessionViewer.messageCount', { count: messages.length }) }}
              </span>
            </div>
            <div class="sv-msg-ids">
              <span class="sv-session-id">{{ $t('sessionViewer.sessionId', { id: selectedSession.sessionId }) }}</span>
              <span class="sv-source-path" :title="selectedSession.sourcePath">
                {{ $t('sessionViewer.sourcePath', { path: selectedSession.sourcePath }) }}
              </span>
              <button
                class="sv-icon-btn"
                :title="$t('sessionViewer.revealFile')"
                :aria-label="$t('sessionViewer.revealFile')"
                @click="reveal(selectedSession.sourcePath)"
              >
                <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M9.5 2.75h3.75V6.5M13.25 2.75 8 8M11.75 9.5v2.75c0 .55-.45 1-1 1h-7c-.55 0-1-.45-1-1v-7c0-.55.45-1 1-1H6.5" /></svg>
              </button>
            </div>
          </div>
          <div v-if="isLoadingMessages" class="sv-empty-msg" role="status">
            <span class="sv-spinner" aria-hidden="true"></span>
            {{ $t('sessionViewer.loadingMessages') }}
          </div>
          <div v-else-if="messageError" class="sv-empty-msg sv-error-text" role="alert">
            {{ $t('sessionViewer.loadFailed', { error: messageError }) }}
          </div>
          <div v-else-if="!messages.length" class="sv-empty-msg" role="status">
            {{ $t(selectedSession.indexOnly ? 'sessionViewer.indexOnlyHint' : 'sessionViewer.noMessages') }}
          </div>
          <SessionMessages
            v-else
            :key="sessionKey(selectedSession)"
            :messages="messages"
            :provider-id="selectedSession.providerId"
          />
        </template>
      </div>
    </div>
  </section>
</template>

<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import type { SessionTarget } from '../navigation';
import { ask } from '@tauri-apps/plugin-dialog';
import { useI18n } from 'vue-i18n';
import SessionMessages from './SessionMessages.vue';

const { t } = useI18n();

/**
 * Metadata for a local session before its messages are loaded
 */
interface SessionMeta {
  providerId: string;
  sessionId: string;
  title: string | null;
  summary: string | null;
  projectDir: string | null;
  lastActiveAt: number | null;
  sourcePath: string;
  indexOnly: boolean;
}

/**
 * Message in a local session
 */
interface SessionMessage {
  role: string;
  content: string;
  ts: number | null;
}

interface DeleteResult {
  deleted: number;
  orphanIndexesDeleted: number;
  failures: { key: string; name: string; sourcePath: string; error: string }[];
  warnings: string[];
}

interface CleanOptions {
  ccSwitch: boolean;
  usageMonitor: boolean;
  appCache: boolean;
}

const CLEAN_OPTIONS_KEY = 'anti-power-clean-options';
const HIDE_INDEX_ONLY_KEY = 'anti-power-hide-index-only';

function loadCleanOptions(): CleanOptions {
  const options: CleanOptions = { ccSwitch: false, usageMonitor: false, appCache: false };
  try {
    const saved = JSON.parse(localStorage.getItem(CLEAN_OPTIONS_KEY) ?? '{}');
    options.ccSwitch = saved?.ccSwitch === true;
    options.usageMonitor = saved?.usageMonitor === true;
    options.appCache = saved?.appCache === true;
  } catch {
    // Keep defaults
  }
  return options;
}

const providerIds = ['antigravity', 'agy'] as const;
type Provider = typeof providerIds[number];
const versions = ref<Partial<Record<Provider, string | null>>>({});
function sourceLabel(provider: Provider) {
  const name = t('sessionViewer.providers.' + provider);
  const version = versions.value[provider];
  return version ? `${name} · ${version}` : name;
}

let versionRequest = 0;
async function refreshVersions() {
  const request = ++versionRequest;
  const next = await invoke<Record<Provider, string | null>>('client_versions').catch(() => ({}));
  if (request === versionRequest) versions.value = next;
}
const providerOptions = computed(() => [
  { value: '', label: t('sessionViewer.filterAll'), version: null, title: t('sessionViewer.filterAll') },
  ...providerIds.map(provider => ({
    value: provider,
    label: t('sessionViewer.providersShort.' + provider),
    version: versions.value[provider] ?? null,
    title: sourceLabel(provider),
  })),
]);

async function reveal(path: string) {
  try {
    const { revealItemInDir } = await import('@tauri-apps/plugin-opener');
    await revealItemInDir(path);
  } catch {
    // The path may no longer exist
  }
}

const sessions = ref<SessionMeta[]>([]);
const messages = ref<SessionMessage[]>([]);
const selectedSession = ref<SessionMeta | null>(null);
const checkedSessions = ref(new Set<string>());
const filterProvider = ref('');
const searchQuery = ref('');
const hideIndexOnly = ref(localStorage.getItem(HIDE_INDEX_ONLY_KEY) === 'true');
const isScanning = ref(false);
const isLoadingMessages = ref(false);
const deletionPending = ref(false);
const isDeleting = ref(false);
const scanError = ref('');
const scanWarnings = ref<string[]>([]);
const magpieStoreAvailable = ref(false);
const magpieFeedback = ref<{ text: string; error: boolean } | null>(null);
const ccSwitchVerificationPending = ref(false);
const usageMonitorCleanupPending = ref(false);
const messageError = ref('');
const confirmationError = ref('');
const deleteResult = ref<DeleteResult | null>(null);
const cleanOptions = ref(loadCleanOptions());
watch(cleanOptions, options => {
  localStorage.setItem(CLEAN_OPTIONS_KEY, JSON.stringify(options));
}, { deep: true });
watch(hideIndexOnly, hide => {
  localStorage.setItem(HIDE_INDEX_ONLY_KEY, String(hide));
});
let detailRequest = 0;

function sessionKey(session: SessionMeta): string {
  return JSON.stringify([session.providerId, session.sourcePath]);
}

function sessionName(session: SessionMeta): string {
  return session.title || session.sessionId.slice(0, 12);
}

const selectedKey = computed(() => selectedSession.value ? sessionKey(selectedSession.value) : null);
const checkedCount = computed(() => checkedSessions.value.size);
const actionsBusy = computed(() => isScanning.value || deletionPending.value);

const filteredSessions = computed(() => {
  const query = searchQuery.value.trim().toLowerCase();
  const matchesId = query && sessions.value.some(session => session.sessionId.toLowerCase() === query);
  return sessions.value.filter(session => {
    if (filterProvider.value && session.providerId !== filterProvider.value) return false;
    if (matchesId) return session.sessionId.toLowerCase() === query;
    if (hideIndexOnly.value && session.indexOnly) return false;
    if (!query) return true;
    return [session.title, session.summary, session.projectDir, session.sessionId]
      .some(value => value?.toLowerCase().includes(query));
  });
});

const isAllChecked = computed(() =>
  filteredSessions.value.length > 0 &&
  filteredSessions.value.every(session => checkedSessions.value.has(sessionKey(session))),
);

watch([filterProvider, searchQuery, hideIndexOnly], () => {
  checkedSessions.value.clear();
  if (selectedKey.value && !filteredSessions.value.some(session => sessionKey(session) === selectedKey.value)) {
    void selectSession(null);
  }
}, { flush: 'sync' });

let pendingSession: SessionTarget | null = null;

function openSession(target: SessionTarget) {
  pendingSession = target;
  void scanSessions();
}

defineExpose({ openSession });
onMounted(() => void scanSessions());
onUnmounted(() => {
  detailRequest++;
});

function errorMessage(error: unknown): string {
  if (typeof error === 'string' && error) return error;
  if (error && typeof error === 'object' && 'message' in error &&
      typeof error.message === 'string' && error.message) {
    return error.message;
  }
  return t('app.error.unknown');
}

async function scanSessions() {
  if (isScanning.value) return;
  isScanning.value = true;
  void refreshVersions();
  scanError.value = '';
  try {
    const result = await invoke<{
      sessions: SessionMeta[];
      magpieStoreAvailable: boolean;
      warnings: string[];
      ccSwitchVerificationPending: boolean;
      usageMonitorCleanupPending: boolean;
    }>('scan_sessions');
    sessions.value = result.sessions;
    magpieStoreAvailable.value = result.magpieStoreAvailable;
    scanWarnings.value = result.warnings;
    ccSwitchVerificationPending.value = result.ccSwitchVerificationPending;
    usageMonitorCleanupPending.value = result.usageMonitorCleanupPending;

    const byKey = new Map(filteredSessions.value.map(session => [sessionKey(session), session]));
    for (const key of checkedSessions.value) {
      if (!byKey.has(key)) checkedSessions.value.delete(key);
    }
    const target = pendingSession;
    if (target) {
      pendingSession = null;
      filterProvider.value = target.provider ?? '';
      searchQuery.value = target.id;
      const matches = filteredSessions.value.filter(row => row.sessionId.toLowerCase() === target.id);
      const fullCopies = matches.filter(row => !row.indexOnly);
      const candidates = fullCopies.length ? fullCopies : matches;
      const match = target.profile
        ? matches.find(row => row.sourcePath.split('\\').join('/').endsWith(
            '/.cc-switch/agy/' + target.profile + '/antigravity-cli/conversations/' + row.sessionId + '.db'))
        : candidates.length === 1 ? candidates[0] : undefined;
      void selectSession(match ?? null);
    } else if (selectedKey.value) {
      void selectSession(byKey.get(selectedKey.value) ?? null);
    }
  } catch (error) {
    scanError.value = errorMessage(error);
  } finally {
    isScanning.value = false;
  }
}

async function selectSession(session: SessionMeta | null) {
  // Only the most recent detail request can update the pane
  const request = ++detailRequest;
  selectedSession.value = session;
  messages.value = [];
  messageError.value = '';
  isLoadingMessages.value = session !== null;
  if (!session) return;

  try {
    const loaded = await invoke<SessionMessage[]>('load_session_messages', {
      providerId: session.providerId,
      sourcePath: session.sourcePath,
    });
    if (request === detailRequest) messages.value = loaded;
  } catch (error) {
    if (request === detailRequest) messageError.value = errorMessage(error);
  } finally {
    if (request === detailRequest) isLoadingMessages.value = false;
  }
}

/**
 * Format a timestamp with the viewer's localized relative time labels
 * @param ts - Unix timestamp in milliseconds
 * @returns Relative time text, or an empty string when absent
 */
function formatRelativeTime(ts: number | null | undefined): string {
  if (!ts) return '';

  const now = Date.now();
  const diff = now - ts;
  const minutes = Math.floor(diff / 60000);

  if (minutes < 1) return t('sessionViewer.timeAgo.justNow');
  if (minutes < 60) return t('sessionViewer.timeAgo.minutesAgo', { n: minutes });

  const hours = Math.floor(minutes / 60);
  if (hours < 24) return t('sessionViewer.timeAgo.hoursAgo', { n: hours });

  const days = Math.floor(hours / 24);
  if (days < 7) return t('sessionViewer.timeAgo.daysAgo', { n: days });

  const weeks = Math.floor(days / 7);
  if (weeks < 5) return t('sessionViewer.timeAgo.weeksAgo', { n: weeks });

  const months = Math.floor(days / 30);
  return t('sessionViewer.timeAgo.monthsAgo', { n: months });
}

/**
 * Handle both Windows and Unix path separators
 * @param p - Session file or directory path
 * @returns Final path component, or the original path if empty
 */
function pathBasename(p: string): string {
  const parts = p.replace(/\\/g, '/').split('/');
  return parts[parts.length - 1] || p;
}

function toggleCheck(key: string) {
  if (checkedSessions.value.has(key)) {
    checkedSessions.value.delete(key);
  } else {
    checkedSessions.value.add(key);
  }
}

function toggleAllChecked() {
  checkedSessions.value = isAllChecked.value
    ? new Set()
    : new Set(filteredSessions.value.map(sessionKey));
}

function deleteSelectedSession() {
  if (!selectedSession.value) return;
  const session = selectedSession.value;
  const id = session.sessionId.toLowerCase();
  const hasOtherSources = sessions.value.some(other =>
    other.sessionId.toLowerCase() === id &&
    other.sourcePath !== session.sourcePath && !other.indexOnly,
  );
  const message = t('sessionViewer.deleteConfirmMessage', {
    name: sessionName(session),
    path: session.sourcePath,
  });
  const scope = hasOtherSources
    ? [message, t('sessionViewer.deleteOtherSources')].join('\n\n')
    : message;
  void deleteSessions([session], scope);
}

function batchDelete() {
  const selected = filteredSessions.value.filter(session => checkedSessions.value.has(sessionKey(session)));
  if (!selected.length) return;
  void deleteSessions(selected, t('sessionViewer.batchDeleteConfirmMessage', { count: selected.length }));
}

async function confirmDeletion(message: string): Promise<boolean> {
  try {
    return await ask(message, {
      title: t('sessionViewer.deleteConfirmTitle'),
      kind: 'warning',
      okLabel: t('sessionViewer.confirmDelete'),
      cancelLabel: t('sessionViewer.cancel'),
    });
  } catch (error) {
    confirmationError.value = errorMessage(error);
    return false;
  }
}

async function cleanMagpieProbes() {
  if (actionsBusy.value) return;
  deletionPending.value = true;
  confirmationError.value = '';
  magpieFeedback.value = null;
  try {
    const preview = await invoke<{ store: string; ids: string[] } | null>('preview_magpie_probes');
    if (!preview?.ids.length) {
      magpieFeedback.value = { text: t('sessionViewer.noMagpieProbes'), error: false };
      return;
    }
    const { store, ids } = preview;
    const message = t('sessionViewer.confirmMagpieProbes', { count: ids.length, store });
    if (!await confirmDeletion(message)) return;

    isDeleting.value = true;
    deleteResult.value = null;
    const { warnings } = await invoke<{ warnings: string[] }>('delete_magpie_probes', { ids });
    deleteResult.value = { deleted: ids.length, orphanIndexesDeleted: 0, failures: [], warnings };
    await scanSessions();
  } catch (error) {
    magpieFeedback.value = { text: t('sessionViewer.cleanMagpieFailed', { error: errorMessage(error) }), error: true };
  } finally {
    isDeleting.value = false;
    deletionPending.value = false;
  }
}

async function deleteSessions(targets: SessionMeta[], message: string) {
  if (actionsBusy.value) return;
  deletionPending.value = true;
  confirmationError.value = '';
  magpieFeedback.value = null;
  try {
    const selectedKeys = new Set(targets.map(sessionKey));
    const fullIds = new Set(targets.filter(session => !session.indexOnly)
      .map(session => session.sessionId.toLowerCase()));
    const matchingIndexes = sessions.value.filter(session =>
      session.indexOnly && fullIds.has(session.sessionId.toLowerCase()) &&
      !selectedKeys.has(sessionKey(session)),
    );
    const options = { ...cleanOptions.value };
    let orphanScanError = '';
    const orphanIndexes = await invoke<Pick<SessionMeta, 'providerId' | 'sourcePath' | 'sessionId'>[]>(
      'preview_orphan_indexes', { selectedIds: targets.map(session => session.sessionId) },
    ).catch(error => {
      orphanScanError = errorMessage(error);
      return [];
    });
    const extras = [
      matchingIndexes.length > 0 && t('sessionViewer.confirmMatchingIndexes', { count: matchingIndexes.length }),
      orphanIndexes.length > 0 && t('sessionViewer.confirmOrphanIndexes', { count: orphanIndexes.length }),
      orphanScanError && t('sessionViewer.orphanScanFailed', { error: orphanScanError }),
      options.appCache && t('sessionViewer.confirmAppCache'),
      options.ccSwitch && t('sessionViewer.confirmCcSwitch'),
      options.usageMonitor && t('sessionViewer.confirmUsageMonitor'),
    ].filter(Boolean);
    if (!await confirmDeletion([message, ...extras].join('\n\n'))) return;

    isDeleting.value = true;
    deleteResult.value = null;
    const deleted = new Set<string>();
    const failures: DeleteResult['failures'] = [];
    // The backend reports an error per selected session and warnings for extra index cleanups
    let errors: (string | null)[];
    let warnings: string[] = [];
    let orphanIndexesDeleted = 0;
    try {
      const result = await invoke<{
        errors: (string | null)[];
        warnings: string[];
        orphanIndexesDeleted: number;
        usageMonitorCleanupPending: boolean;
      }>('delete_sessions', {
        sessions: targets.map(({ providerId, sourcePath, sessionId, indexOnly }) => ({
          providerId, sourcePath, sessionId, indexOnly,
        })),
        matchingIndexes: matchingIndexes.map(({ providerId, sourcePath, sessionId }) => ({
          providerId, sourcePath, sessionId,
        })),
        orphanIndexes,
        options,
      });
      ({ errors, warnings, orphanIndexesDeleted } = result);
      usageMonitorCleanupPending.value ||= result.usageMonitorCleanupPending;
    } catch (error) {
      errors = targets.map(() => errorMessage(error));
    }
    targets.forEach((session, index) => {
      const key = sessionKey(session);
      const error = errors[index];
      if (error) {
        failures.push({ key, name: sessionName(session), sourcePath: session.sourcePath, error });
      } else {
        deleted.add(key);
      }
    });

    sessions.value = sessions.value.filter(session => !deleted.has(sessionKey(session)));
    for (const key of deleted) checkedSessions.value.delete(key);
    if (selectedKey.value && deleted.has(selectedKey.value)) void selectSession(null);

    deleteResult.value = { deleted: deleted.size, orphanIndexesDeleted, failures, warnings };
    await scanSessions();
  } finally {
    isDeleting.value = false;
    deletionPending.value = false;
  }
}
</script>

<style scoped>
.sv-panel {
  display: flex;
  flex-direction: column;
  gap: 10px;
  flex: 1;
  min-height: 0;
  min-width: 0;
}

.sv-sr-only {
  position: absolute;
  width: 1px;
  height: 1px;
  overflow: hidden;
  clip-path: inset(50%);
  white-space: nowrap;
}

.sv-panel svg {
  fill: none;
  stroke: currentColor;
  stroke-width: 1.4;
  stroke-linecap: round;
  stroke-linejoin: round;
  flex-shrink: 0;
}

.sv-panel button:disabled,
.sv-panel input:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

/* Toolbar */
.sv-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  flex-wrap: wrap;
  gap: 10px 16px;
  flex-shrink: 0;
}

.sv-title {
  font-size: 18px;
  font-weight: 650;
  letter-spacing: -0.01em;
  color: var(--ag-text-strong);
}

.sv-header-actions {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  flex-wrap: wrap;
  flex: 1;
  gap: 8px;
  min-width: 0;
}

.sv-segmented {
  display: inline-flex;
  padding: 3px;
  gap: 2px;
  border-radius: var(--radius-pill);
  background: var(--ag-surface);
  box-shadow: var(--ag-shadow-card), inset 0 0 0 1px var(--ag-border);
}

.sv-seg {
  position: relative;
  display: inline-flex;
  align-items: baseline;
  gap: 5px;
  height: 26px;
  padding: 0 12px;
  border-radius: var(--radius-pill);
  font-size: 12px;
  line-height: 26px;
  font-weight: 500;
  color: var(--ag-text-tertiary);
  cursor: pointer;
  white-space: nowrap;
  transition: background var(--transition-fast), color var(--transition-fast);
}

.sv-seg:hover:not(.disabled) {
  color: var(--ag-text-strong);
}

.sv-seg.active {
  background: var(--ag-accent-solid);
  color: #fff;
}

.sv-seg.disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.sv-seg:has(input:focus-visible) {
  outline: 2px solid var(--ag-accent);
  outline-offset: 1px;
}

.sv-seg-version {
  font-size: 10.5px;
  opacity: 0.62;
}

.sv-search {
  position: relative;
  display: flex;
  align-items: center;
  width: 190px;
  min-width: 120px;
  flex: 0 1 190px;
}

.sv-search svg {
  position: absolute;
  left: 11px;
  width: 14px;
  height: 14px;
  color: var(--ag-text-muted);
  pointer-events: none;
}

.sv-search input {
  width: 100%;
  height: 32px;
  padding: 0 12px 0 32px;
  border-radius: var(--radius-pill);
  background: var(--ag-surface);
  box-shadow: var(--ag-shadow-card), inset 0 0 0 1px var(--ag-border);
  font-size: 12.5px;
  outline: none;
  transition: box-shadow var(--transition-fast);
}

.sv-search input::placeholder {
  color: var(--ag-text-muted);
}

.sv-search input:focus {
  box-shadow: inset 0 0 0 1px var(--ag-border-active), var(--ag-ring);
}

.sv-btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 32px;
  padding: 0 14px;
  border-radius: var(--radius-pill);
  background: var(--ag-surface);
  box-shadow: var(--ag-shadow-card), inset 0 0 0 1px var(--ag-border);
  font-size: 12.5px;
  font-weight: 500;
  color: var(--ag-text-strong);
  cursor: pointer;
  transition: background var(--transition-fast);
}

.sv-btn svg {
  width: 14px;
  height: 14px;
}

.sv-btn:hover:not(:disabled) {
  background: var(--ag-surface-2);
}

.spinning {
  animation: spin 0.9s linear infinite;
}

/* Cleanup options */
.sv-options {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 6px;
  flex-shrink: 0;
  font-size: 12px;
  color: var(--ag-text-tertiary);
}

.sv-options-label {
  margin-right: 2px;
}

.sv-options-action {
  margin-inline-start: auto;
}

.sv-options .sv-chip,
.sv-options-action {
  white-space: nowrap;
}

.sv-chip {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 26px;
  padding: 0 10px 0 7px;
  border-radius: var(--radius-pill);
  background: var(--ag-hover);
  color: var(--ag-text-secondary);
  cursor: pointer;
  transition: background var(--transition-fast), color var(--transition-fast);
}

.sv-chip:hover {
  background: var(--ag-accent-subtle);
}

.sv-chip:has(input:checked) {
  background: var(--ag-accent-subtle);
  color: var(--ag-text-strong);
}

.sv-chip input {
  width: 14px;
  height: 14px;
  border-radius: 4px;
}

.sv-chip input:checked::after {
  left: 3.5px;
  top: 0.5px;
}

.sv-header-actions .sv-chip {
  height: 32px;
  font-size: 12px;
  white-space: nowrap;
}

/* Feedback banners */
.sv-feedback {
  flex-shrink: 0;
  max-height: 20%;
  overflow: auto;
  padding: 9px 14px 9px 16px;
  border-radius: var(--radius-md);
  background: var(--ag-success-subtle);
  box-shadow: inset 3px 0 0 var(--ag-success);
  color: var(--ag-text-strong);
  font-size: 12.5px;
  overflow-wrap: anywhere;
  animation: fadeIn 0.2s ease-out;
}

.sv-feedback ul {
  list-style: disc;
  padding-left: 20px;
  margin: 4px 0 0;
}

.sv-notice {
  background: var(--ag-bronze-subtle);
  box-shadow: inset 3px 0 0 var(--ag-bronze);
  color: var(--ag-text-secondary);
}

.sv-error {
  background: var(--ag-error-subtle);
  box-shadow: inset 3px 0 0 var(--ag-error);
}

/* Cards */
.sv-body {
  display: flex;
  gap: 12px;
  flex: 1;
  min-height: 0;
}

.sv-card {
  display: flex;
  flex-direction: column;
  min-height: 0;
  min-width: 0;
  overflow: hidden;
  border-radius: var(--radius-xl);
  background: var(--ag-surface);
  box-shadow: var(--ag-shadow-card), inset 0 0 0 1px var(--ag-border);
}

/* Session list */
.sv-list-pane {
  width: 38%;
  max-width: 330px;
  min-width: 230px;
  flex-shrink: 0;
}

.sv-list-meta {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 4px 12px;
  flex-wrap: wrap;
  min-height: 42px;
  padding: 8px 14px 6px 16px;
  flex-shrink: 0;
}

.sv-count {
  font-size: 12px;
  font-weight: 600;
  color: var(--ag-text-secondary);
}

.sv-select-all {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 11.5px;
  color: var(--ag-text-tertiary);
  cursor: pointer;
}

.sv-empty {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  padding: 36px 16px;
  text-align: center;
  color: var(--ag-text-muted);
  font-size: 13px;
}

.sv-spinner {
  width: 14px;
  height: 14px;
  border-radius: 50%;
  border: 2px solid var(--ag-border-hover);
  border-top-color: var(--ag-accent);
  animation: spin 0.8s linear infinite;
}

.sv-list {
  flex: 1;
  overflow-y: auto;
  padding: 0 8px 8px;
}

.sv-item {
  display: flex;
  align-items: center;
  gap: 10px;
  padding-left: 8px;
  margin-bottom: 2px;
  border-radius: var(--radius-md);
  transition: background var(--transition-fast);
}

.sv-item:hover {
  background: var(--ag-hover);
}

.sv-item.checked {
  background: var(--ag-accent-subtle);
}

.sv-item.active {
  background: var(--ag-accent-subtle);
  box-shadow: inset 0 0 0 1px var(--ag-border-active);
}

.sv-item-open {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 3px;
  padding: 9px 10px 9px 0;
  border-radius: var(--radius-md);
  text-align: left;
  cursor: pointer;
}

.sv-item-open:focus-visible {
  outline-offset: -2px;
}

.sv-item-title {
  font-size: 13px;
  font-weight: 500;
  line-height: 1.45;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  color: var(--ag-text-strong);
}

.sv-item-sub {
  display: flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
  font-size: 11px;
  color: var(--ag-text-tertiary);
  white-space: nowrap;
}

.sv-item-project {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
}

.sv-item-project::before {
  content: '·';
  margin-right: 6px;
  color: var(--ag-text-muted);
}

.sv-index-only {
  padding: 0 6px;
  border-radius: var(--radius-pill);
  background: var(--ag-warning-subtle);
  color: var(--ag-warning);
  font-weight: 500;
}

.sv-provider-badge {
  padding: 0 6px;
  border-radius: 5px;
  font-size: 10px;
  font-weight: 650;
  line-height: 17px;
  letter-spacing: 0.02em;
  white-space: nowrap;
  flex-shrink: 0;
}

.sv-provider-antigravity { background: var(--ag-accent-subtle); color: var(--ag-accent); }
.sv-provider-agy { background: var(--ag-bronze-subtle); color: var(--ag-bronze); }

.sv-batch-bar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  flex-wrap: wrap;
  gap: 6px 8px;
  margin: 0 8px 8px;
  padding: 8px 8px 8px 12px;
  border-radius: var(--radius-md);
  background: var(--ag-surface-2);
  flex-shrink: 0;
}

.sv-batch-count {
  font-size: 12px;
  font-weight: 600;
  color: var(--ag-text-strong);
}

.sv-slide-enter-active,
.sv-slide-leave-active {
  transition: opacity var(--transition-fast), transform var(--transition-fast);
}

.sv-slide-enter-from,
.sv-slide-leave-to {
  opacity: 0;
  transform: translateY(6px);
}

/* Buttons */
.sv-danger-btn {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  height: 28px;
  padding: 0 12px;
  border-radius: var(--radius-pill);
  box-shadow: inset 0 0 0 1px var(--ag-error-border);
  color: var(--ag-error);
  font-size: 12px;
  font-weight: 500;
  white-space: nowrap;
  cursor: pointer;
  transition: background var(--transition-fast), color var(--transition-fast);
}

.sv-danger-btn svg {
  width: 13px;
  height: 13px;
}

.sv-danger-btn:hover:not(:disabled) {
  background: var(--ag-error-subtle);
}

.sv-danger-btn.solid {
  background: var(--ag-error-solid);
  box-shadow: none;
  color: #fff;
}

.sv-danger-btn.solid:hover:not(:disabled) {
  background: var(--ag-error-solid);
  filter: brightness(1.1);
}

.sv-icon-btn {
  display: grid;
  place-items: center;
  width: 22px;
  height: 22px;
  border-radius: 6px;
  color: var(--ag-text-muted);
  cursor: pointer;
  flex-shrink: 0;
}

.sv-icon-btn svg {
  width: 13px;
  height: 13px;
}

.sv-icon-btn:hover {
  background: var(--ag-hover);
  color: var(--ag-text-strong);
}

/* Session detail */
.sv-msg-pane {
  flex: 1;
}

.sv-empty-msg {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 10px;
  padding: 16px 24px;
  text-align: center;
  overflow-wrap: anywhere;
  color: var(--ag-text-muted);
  font-size: 13px;
}

.sv-empty-icon {
  width: 36px;
  height: 36px;
  stroke-width: 1.2 !important;
  color: var(--ag-text-muted);
  opacity: 0.7;
}

.sv-error-text {
  color: var(--ag-error);
}

.sv-msg-header {
  display: flex;
  flex-direction: column;
  gap: 7px;
  padding: 14px 18px 12px 22px;
  border-bottom: 1px solid var(--ag-border);
  flex-shrink: 0;
}

.sv-msg-title-row {
  display: flex;
  align-items: flex-start;
  gap: 12px;
}

.sv-msg-session-title {
  flex: 1;
  min-width: 0;
  padding-top: 2px;
  overflow-wrap: anywhere;
  font-size: 15px;
  font-weight: 650;
  line-height: 1.4;
  color: var(--ag-text-strong);
}

.sv-msg-meta {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 6px 12px;
}

.sv-meta-item {
  font-size: 12px;
  color: var(--ag-text-tertiary);
}

.sv-path-chip {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  max-width: 220px;
  height: 20px;
  padding: 0 7px;
  border-radius: 6px;
  background: var(--ag-code-bg);
  box-shadow: inset 0 0 0 1px var(--ag-border);
  font-family: var(--ag-font-mono);
  font-size: 11.5px;
  color: var(--ag-text-secondary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  cursor: pointer;
}

.sv-path-chip svg {
  width: 12px;
  height: 12px;
  color: var(--ag-text-muted);
}

.sv-path-chip:hover {
  background: var(--ag-accent-subtle);
  color: var(--ag-text-strong);
}

.sv-msg-ids {
  display: flex;
  align-items: center;
  gap: 4px 12px;
  min-width: 0;
  font-family: var(--ag-font-mono);
  font-size: 10.5px;
  color: var(--ag-text-muted);
}

.sv-session-id {
  user-select: text;
  white-space: nowrap;
  flex-shrink: 0;
}

.sv-source-path {
  flex: 1;
  min-width: 0;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  user-select: text;
}

@media (max-width: 960px) {
  .sv-search { flex-basis: 150px; }
  .sv-options { column-gap: 4px; }
  .sv-options .sv-chip { gap: 5px; padding-inline: 6px; }
  .sv-options-action { padding-inline: 11px; }
}

@media (max-width: 760px) {
  .sv-header-actions { flex-basis: 100%; }
  .sv-msg-ids { flex-wrap: wrap; }
  .sv-session-id { white-space: normal; overflow-wrap: anywhere; flex-shrink: 1; }
}

@media (max-width: 620px) {
  .sv-search { flex: 1 1 120px; }

  .sv-body {
    flex-direction: column;
    overflow-y: auto;
  }

  .sv-list-pane {
    flex: 1 0 200px;
    width: 100%;
    max-width: none;
  }

  .sv-msg-pane {
    flex: 1 0 260px;
  }
}
</style>
