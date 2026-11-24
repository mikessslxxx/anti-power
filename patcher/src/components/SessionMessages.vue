<template>
  <div class="cv">
    <div ref="scroller" class="cv-scroll" @scroll="updateLatest" @click="onLinkClick">
      <article
        v-for="(item, idx) in items"
        :key="idx"
        :class="['cv-msg', 'cv-' + item.role, { 'cv-run': !item.first }]"
        :aria-label="$t('sessionViewer.roles.' + item.role, item.role)"
      >
        <template v-if="item.role === 'user'">
          <div v-if="item.time" class="cv-user-time">{{ item.time }}</div>
          <div class="cv-user-row">
            <div class="cv-bubble selectable" :class="{ 'cv-collapsed': isCollapsed(idx) }">{{ item.content }}</div>
            <button class="cv-copy" :title="copyTitle(idx)" :aria-label="copyTitle(idx)" @click="copy(item.content, idx)">
              <svg v-if="copiedIdx === idx" viewBox="0 0 16 16" aria-hidden="true"><path d="M3.5 8.5l3 3 6-7" /></svg>
              <svg v-else viewBox="0 0 16 16" aria-hidden="true"><rect x="5.25" y="5.25" width="8" height="8" rx="1.75" /><path d="M10.75 5.25V4A1.75 1.75 0 0 0 9 2.25H4A1.75 1.75 0 0 0 2.25 4v5c0 .97.78 1.75 1.75 1.75h1.25" /></svg>
            </button>
          </div>
        </template>

        <template v-else>
          <header v-if="item.first" class="cv-head">
            <span class="cv-avatar" aria-hidden="true">
              <svg viewBox="0 0 16 16"><path d="M8 1.5c.4 3.3 1.9 5.1 5.5 6.5-3.6 1.4-5.1 3.2-5.5 6.5-.4-3.3-1.9-5.1-5.5-6.5C6.1 6.6 7.6 4.8 8 1.5z" /></svg>
            </span>
            <span class="cv-name">{{ $t('sessionViewer.providers.' + providerId) }}</span>
            <span v-if="item.time" class="cv-time">{{ item.time }}</span>
          </header>
          <div class="cv-body">
            <div class="cv-md selectable" :class="{ 'cv-collapsed': isCollapsed(idx) }" v-html="item.html"></div>
            <button class="cv-copy" :title="copyTitle(idx)" :aria-label="copyTitle(idx)" @click="copy(item.content, idx)">
              <svg v-if="copiedIdx === idx" viewBox="0 0 16 16" aria-hidden="true"><path d="M3.5 8.5l3 3 6-7" /></svg>
              <svg v-else viewBox="0 0 16 16" aria-hidden="true"><rect x="5.25" y="5.25" width="8" height="8" rx="1.75" /><path d="M10.75 5.25V4A1.75 1.75 0 0 0 9 2.25H4A1.75 1.75 0 0 0 2.25 4v5c0 .97.78 1.75 1.75 1.75h1.25" /></svg>
            </button>
          </div>
        </template>

        <button v-if="item.long" class="cv-toggle" :aria-expanded="expanded.has(idx)" @click="toggle(idx)">
          {{ expanded.has(idx) ? $t('sessionViewer.showLess') : $t('sessionViewer.showMore') }}
        </button>
      </article>
    </div>

    <Transition name="cv-fade">
      <button v-if="showLatest" class="cv-latest" @click="scrollToLatest">
        <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M8 3v10M3.5 8.5 8 13l4.5-4.5" /></svg>
        {{ $t('sessionViewer.jumpLatest') }}
      </button>
    </Transition>
  </div>
</template>

<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { renderMarkdown } from '../markdown';

const props = defineProps<{
  messages: { role: string; content: string; ts: number | null }[];
  providerId: string;
}>();

const { t, locale } = useI18n();
const scroller = ref<HTMLElement | null>(null);
const expanded = ref(new Set<number>());
const copiedIdx = ref<number | null>(null);
const showLatest = ref(false);
let copyTimer: ReturnType<typeof setTimeout> | undefined;

const items = computed(() => {
  const now = new Date();
  const timeOnly = new Intl.DateTimeFormat(locale.value, { hour: '2-digit', minute: '2-digit' });
  const withDate = new Intl.DateTimeFormat(locale.value, { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' });
  return props.messages.map((msg, idx) => {
    const role = msg.role === 'user' ? 'user' : 'assistant';
    const lines = msg.content.split('\n').length;
    // Out-of-range timestamps make an invalid Date, which Intl refuses to format
    const date = msg.ts ? new Date(msg.ts) : null;
    const valid = date !== null && !Number.isNaN(date.getTime());
    return {
      role,
      content: msg.content,
      html: role === 'assistant' ? renderMarkdown(msg.content) : '',
      // Consecutive replies share one header
      first: idx === 0 || props.messages[idx - 1].role !== msg.role,
      time: valid ? (date.toDateString() === now.toDateString() ? timeOnly : withDate).format(date) : '',
      long: role === 'user' ? lines > 14 || msg.content.length > 900 : lines > 36 || msg.content.length > 2600,
    };
  });
});

const isCollapsed = (idx: number) => items.value[idx].long && !expanded.value.has(idx);
const copyTitle = (idx: number) => t(copiedIdx.value === idx ? 'sessionViewer.copied' : 'sessionViewer.copyMessage');

function toggle(idx: number) {
  if (!expanded.value.delete(idx)) expanded.value.add(idx);
  void nextTick(updateLatest);
}

async function copy(text: string, idx: number) {
  try {
    await navigator.clipboard.writeText(text);
    copiedIdx.value = idx;
    clearTimeout(copyTimer);
    copyTimer = setTimeout(() => (copiedIdx.value = null), 1500);
  } catch {
    // Ignore clipboard failures so the viewer stays usable
  }
}

function updateLatest() {
  const el = scroller.value;
  showLatest.value = !!el && el.scrollHeight - el.scrollTop - el.clientHeight > 480;
}

function scrollToLatest() {
  scroller.value?.scrollTo({ top: scroller.value.scrollHeight, behavior: 'smooth' });
}

async function onLinkClick(event: MouseEvent) {
  const link = (event.target as Element).closest?.('a');
  if (!link) return;
  event.preventDefault();
  const href = link.getAttribute('href') ?? '';
  try {
    const opener = await import('@tauri-apps/plugin-opener');
    if (link.dataset.path) await opener.revealItemInDir(link.dataset.path);
    else if (/^(https?:|mailto:)/i.test(href)) await opener.openUrl(href);
  } catch {
    // The file may no longer exist
  }
}

onMounted(() => void nextTick(updateLatest));
onUnmounted(() => clearTimeout(copyTimer));
</script>

<style scoped>
.cv {
  position: relative;
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
}

.cv-scroll {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: 18px 22px 28px;
  display: flex;
  flex-direction: column;
  animation: fadeIn 0.24s ease-out;
}

.cv-msg {
  position: relative;
  margin-top: 22px;
}

.cv-msg:first-child {
  margin-top: 0;
}

.cv-msg.cv-run {
  margin-top: 10px;
}

/* User prompts */
.cv-user {
  display: flex;
  flex-direction: column;
  align-items: flex-end;
  padding-left: 12%;
}

.cv-user-time {
  margin: 0 4px 5px;
  font-size: 11px;
  color: var(--ag-text-muted);
}

.cv-user-row {
  display: flex;
  flex-direction: row-reverse;
  align-items: flex-start;
  gap: 6px;
  max-width: 100%;
}

.cv-bubble {
  min-width: 0;
  padding: 9px 14px;
  border-radius: 16px 16px 4px 16px;
  background: var(--ag-user-bubble);
  color: var(--ag-text-strong);
  font-size: 13px;
  line-height: 1.6;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
}

/* Assistant replies */
.cv-head {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 6px;
  min-width: 0;
}

.cv-avatar {
  display: grid;
  place-items: center;
  width: 24px;
  height: 24px;
  border-radius: 50%;
  background: var(--ag-accent-subtle);
  box-shadow: inset 0 0 0 1px var(--ag-border);
  color: var(--ag-accent);
  flex-shrink: 0;
}

.cv-avatar svg {
  width: 13px;
  height: 13px;
  fill: currentColor;
}

.cv-name {
  font-size: 12.5px;
  font-weight: 600;
  color: var(--ag-text-strong);
  white-space: nowrap;
}

.cv-time {
  font-size: 11px;
  color: var(--ag-text-muted);
  white-space: nowrap;
}

.cv-body {
  position: relative;
  padding-left: 32px;
  padding-right: 28px;
}

.cv-copy {
  display: grid;
  place-items: center;
  width: 26px;
  height: 26px;
  border-radius: var(--radius-sm);
  color: var(--ag-text-muted);
  cursor: pointer;
  opacity: 0;
  transition: opacity var(--transition-fast), background var(--transition-fast), color var(--transition-fast);
  flex-shrink: 0;
}

.cv-body .cv-copy {
  position: absolute;
  top: -2px;
  right: 0;
}

.cv-copy svg {
  width: 14px;
  height: 14px;
  fill: none;
  stroke: currentColor;
  stroke-width: 1.4;
  stroke-linecap: round;
  stroke-linejoin: round;
}

.cv-msg:hover .cv-copy,
.cv-copy:focus-visible {
  opacity: 1;
}

.cv-copy:hover {
  background: var(--ag-hover);
  color: var(--ag-text);
}

.cv-collapsed {
  max-height: 300px;
  overflow: hidden;
  mask-image: linear-gradient(180deg, #000 70%, transparent);
  -webkit-mask-image: linear-gradient(180deg, #000 70%, transparent);
}

.cv-bubble.cv-collapsed {
  max-height: 220px;
}

.cv-toggle {
  margin: 4px 0 0 32px;
  padding: 2px 8px;
  border-radius: var(--radius-pill);
  font-size: 11.5px;
  color: var(--ag-accent);
  cursor: pointer;
}

.cv-user .cv-toggle {
  margin: 4px 36px 0 0;
}

.cv-toggle:hover {
  background: var(--ag-accent-subtle);
}

.cv-latest {
  position: absolute;
  right: 18px;
  bottom: 16px;
  display: inline-flex;
  align-items: center;
  gap: 5px;
  padding: 6px 12px;
  border-radius: var(--radius-pill);
  background: var(--ag-surface-elevated);
  box-shadow: var(--ag-shadow-md), inset 0 0 0 1px var(--ag-border);
  color: var(--ag-text-strong);
  font-size: 12px;
  font-weight: 500;
  cursor: pointer;
}

.cv-latest svg {
  width: 13px;
  height: 13px;
  fill: none;
  stroke: currentColor;
  stroke-width: 1.6;
  stroke-linecap: round;
  stroke-linejoin: round;
}

.cv-latest:hover {
  background: var(--ag-surface-2);
}

.cv-fade-enter-active,
.cv-fade-leave-active {
  transition: opacity var(--transition-fast), transform var(--transition-fast);
}

.cv-fade-enter-from,
.cv-fade-leave-to {
  opacity: 0;
  transform: translateY(6px);
}

/* Markdown */
.cv-md {
  font-size: 13px;
  line-height: 1.7;
  color: var(--ag-text);
  overflow-wrap: anywhere;
}

.cv-md > :deep(:first-child) { margin-top: 0; }
.cv-md > :deep(:last-child) { margin-bottom: 0; }

.cv-md :deep(p),
.cv-md :deep(ul),
.cv-md :deep(ol),
.cv-md :deep(blockquote),
.cv-md :deep(.md-code),
.cv-md :deep(table) {
  margin: 0 0 10px;
}

.cv-md :deep(h1),
.cv-md :deep(h2),
.cv-md :deep(h3),
.cv-md :deep(h4),
.cv-md :deep(h5),
.cv-md :deep(h6) {
  margin: 16px 0 6px;
  font-weight: 650;
  line-height: 1.4;
  color: var(--ag-text-strong);
}

.cv-md :deep(h1) { font-size: 16px; }
.cv-md :deep(h2) { font-size: 15px; }
.cv-md :deep(h3) { font-size: 14px; }
.cv-md :deep(h4),
.cv-md :deep(h5),
.cv-md :deep(h6) { font-size: 13px; }

.cv-md :deep(strong) {
  font-weight: 650;
  color: var(--ag-text-strong);
}

.cv-md :deep(ul),
.cv-md :deep(ol) {
  padding-left: 20px;
}

.cv-md :deep(ul) { list-style: disc; }
.cv-md :deep(ul ul) { list-style: circle; }
.cv-md :deep(ol) { list-style: decimal; }

.cv-md :deep(li) {
  margin: 3px 0;
}

.cv-md :deep(li > ul),
.cv-md :deep(li > ol),
.cv-md :deep(li > p) {
  margin: 3px 0;
}

.cv-md :deep(li:has(> input[type="checkbox"])) {
  list-style: none;
  margin-left: -20px;
}

.cv-md :deep(li > input[type="checkbox"]) {
  display: inline-block;
  width: 14px;
  height: 14px;
  margin-right: 4px;
  vertical-align: -2px;
  border-radius: 4px;
  opacity: 1;
  cursor: default;
}

.cv-md :deep(li > input[type="checkbox"]:checked::after) {
  left: 3.5px;
  top: 0.5px;
}

.cv-md :deep(li::marker) {
  color: var(--ag-text-muted);
}

.cv-md :deep(blockquote) {
  padding: 2px 0 2px 12px;
  border-left: 3px solid var(--ag-bronze-subtle);
  color: var(--ag-text-secondary);
}

.cv-md :deep(hr) {
  margin: 14px 0;
  border-top: 1px solid var(--ag-border);
}

.cv-md :deep(a) {
  color: var(--ag-accent);
  text-decoration: underline;
  text-decoration-color: var(--ag-accent-glow);
  text-underline-offset: 2px;
  cursor: pointer;
}

.cv-md :deep(a:hover) {
  text-decoration-color: currentColor;
}

.cv-md :deep(code) {
  font-family: var(--ag-font-mono);
  font-size: 0.9em;
}

.cv-md :deep(:not(pre) > code) {
  padding: 1px 5px;
  border-radius: 5px;
  background: var(--ag-code-bg);
  box-shadow: inset 0 0 0 1px var(--ag-border);
  color: var(--ag-text-strong);
}

.cv-md :deep(.md-file) {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  max-width: 100%;
  padding: 0 6px;
  border-radius: 6px;
  background: var(--ag-code-bg);
  box-shadow: inset 0 0 0 1px var(--ag-border);
  color: var(--ag-text-strong);
  font-family: var(--ag-font-mono);
  font-size: 0.88em;
  line-height: 1.7;
  text-decoration: none;
  vertical-align: baseline;
}

.cv-md :deep(.md-file:hover) {
  background: var(--ag-accent-subtle);
  box-shadow: inset 0 0 0 1px var(--ag-border-active);
}

.cv-md :deep(.md-file svg) {
  width: 12px;
  height: 12px;
  flex-shrink: 0;
  color: var(--ag-text-tertiary);
}

.cv-md :deep(.md-file code) {
  padding: 0;
  background: none;
  box-shadow: none;
  font-size: inherit;
}

.cv-md :deep(.md-file-line) {
  color: var(--ag-text-muted);
}

.cv-md :deep(.md-image) {
  color: var(--ag-text-tertiary);
  font-style: italic;
}

.cv-md :deep(.md-code) {
  position: relative;
  border-radius: var(--radius-md);
  background: var(--ag-code-bg);
  box-shadow: inset 0 0 0 1px var(--ag-border);
  overflow: hidden;
}

.cv-md :deep(.md-code-lang) {
  display: block;
  padding: 5px 12px 0;
  font-family: var(--ag-font-mono);
  font-size: 10.5px;
  letter-spacing: 0.02em;
  color: var(--ag-bronze);
}

.cv-md :deep(pre) {
  padding: 8px 12px 10px;
  overflow-x: auto;
  font-size: 12px;
  line-height: 1.6;
  color: var(--ag-text);
}

.cv-md :deep(table) {
  display: block;
  max-width: 100%;
  overflow-x: auto;
  border-collapse: collapse;
  font-size: 12.5px;
}

.cv-md :deep(th),
.cv-md :deep(td) {
  padding: 5px 10px;
  border: 1px solid var(--ag-border);
  text-align: left;
}

.cv-md :deep(th) {
  background: var(--ag-surface-2);
  font-weight: 600;
  color: var(--ag-text-strong);
}

@media (max-width: 620px) {
  .cv-scroll { padding: 14px 14px 22px; }
  .cv-body { padding-left: 0; }
  .cv-toggle { margin-left: 0; }
}
</style>
