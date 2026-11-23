<template>
  <AppModal v-if="show" :title="$t('aboutModal.title')" @close="$emit('close')">
    <div class="about-content">
      <div class="about-logo">
        <img src="../assets/logo.png" alt="logo" class="about-icon" />
      </div>

      <h3 class="modal-product-name">Anti-Power</h3>
      <p class="about-version">{{ $t('app.version', { version }) }}</p>

      <p class="about-desc">
        {{ $t('aboutModal.desc') }}
      </p>

      <div class="about-actions">
        <button
          class="about-btn"
          @click="checkUpdate"
          :disabled="isCheckingUpdate"
        >
          {{ isCheckingUpdate ? $t('aboutModal.checking') : $t('aboutModal.checkUpdate') }}
        </button>
        <button class="about-btn" @click="openGitHub">
          {{ $t('aboutModal.repo') }}
        </button>
      </div>

      <div v-if="updateInfo" ref="updateNotice" class="update-info" role="status">
        <template v-if="updateInfo.hasUpdate">
          <p class="update-available">
            {{ $t('aboutModal.newVersion', { version: updateInfo.latestVersion }) }}
          </p>
          <button class="primary-btn update-btn" @click="openGitHub">
            {{ $t('aboutModal.download') }}
          </button>
        </template>
        <template v-else-if="updateInfo.error">
          <p class="update-error">{{ $t(updateInfo.error) }}</p>
        </template>
        <template v-else>
          <p class="update-latest">{{ $t('aboutModal.latest') }}</p>
        </template>
      </div>
    </div>

    <template #footer>
      <p class="about-copyright">{{ $t('aboutModal.copyright') }}</p>
    </template>
  </AppModal>
</template>

<script setup lang="ts">
import { nextTick, ref } from 'vue';
import AppModal from './AppModal.vue';
import { useI18n } from 'vue-i18n';
const { t } = useI18n();

const props = defineProps<{
  show: boolean;
  version: string;
  githubUrl: string;
}>();

defineEmits(['close']);

const isCheckingUpdate = ref(false);
const updateNotice = ref<HTMLElement | null>(null);
const updateInfo = ref<{ hasUpdate: boolean; latestVersion?: string; error?: string } | null>(null);

/**
 * Compare numeric version components
 * @param a - First version
 * @param b - Second version
 * @returns 1 when a is newer, -1 when b is newer, or 0 when equal
 */
function compareVersions(a: string, b: string): number {
  const partsA = a.split('.').map(Number);
  const partsB = b.split('.').map(Number);
  for (let i = 0; i < Math.max(partsA.length, partsB.length); i++) {
    const numA = partsA[i] || 0;
    const numB = partsB[i] || 0;
    if (numA > numB) return 1;
    if (numA < numB) return -1;
  }
  return 0;
}

/**
 * Check the latest GitHub release against the installed version
 */
async function checkUpdate() {
  isCheckingUpdate.value = true;
  updateInfo.value = null;
  try {
    const controller = new AbortController();
    const timeoutId = setTimeout(() => controller.abort(), 10000);
    
    const res = await fetch(`https://api.github.com/repos/mikessslxxx/anti-power/releases/latest`, {
      signal: controller.signal
    });
    clearTimeout(timeoutId);
    
    if (res.ok) {
      const data = await res.json();
      const latestVersion = data.tag_name?.replace('v', '') || data.name;
      updateInfo.value = {
        hasUpdate: compareVersions(latestVersion, props.version) > 0,
        latestVersion
      };
    } else {
      updateInfo.value = { hasUpdate: false, error: 'aboutModal.checkFailed' };
    }
  } catch (e) {
    console.error(t('aboutModal.error.checkUpdate'), e);
    updateInfo.value = { hasUpdate: false, error: 'aboutModal.networkError' };
  } finally {
    isCheckingUpdate.value = false;
    await nextTick();
    updateNotice.value?.scrollIntoView({ block: 'nearest' });
  }
}

/**
 * Open the repository in the system browser
 */
async function openGitHub() {
  const { openUrl } = await import('@tauri-apps/plugin-opener');
  await openUrl(props.githubUrl);
}
</script>

<style scoped>
.about-content {
  display: flex;
  flex-direction: column;
  justify-content: center;
  min-height: 100%;
  text-align: center;
}

.about-logo {
  width: 80px;
  height: 80px;
  margin: 0 auto 18px;
  border-radius: 18px;
  overflow: hidden;
  box-shadow: var(--ag-shadow-lg), 0 0 0 1px rgba(255, 255, 255, 0.05);
  transition: transform var(--transition-normal);
}

.about-logo:hover {
  transform: scale(1.05);
}

.about-icon {
  width: 100%;
  height: 100%;
  object-fit: cover;
}

.about-version {
  color: var(--ag-text-tertiary);
  font-size: 12px;
  font-weight: 500;
  margin: 0 0 16px;
}

.about-desc {
  font-size: 12px;
  color: var(--ag-text-secondary);
  line-height: 1.6;
  margin: 0 0 18px;
}

.about-actions {
  display: flex;
  gap: 8px;
  justify-content: center;
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
  transition: all var(--transition-fast);
}

.about-btn:hover:not(:disabled) {
  background: var(--ag-surface-3);
  border-color: var(--ag-border-hover);
  color: var(--ag-text);
  transform: translateY(-1px);
}

.about-btn:disabled {
  opacity: 0.35;
  cursor: not-allowed;
}

.update-info {
  margin-top: 20px;
  padding-top: 18px;
  border-top: 1px solid var(--ag-border);
}

.update-available {
  color: var(--ag-success);
  font-size: 13px;
  font-weight: 500;
  margin: 0 0 12px;
}

.update-error {
  color: var(--ag-error);
  font-size: 13px;
}

.update-latest {
  color: var(--ag-text-tertiary);
  font-size: 12px;
  margin: 0;
}

.primary-btn {
  padding: 10px 20px;
  background: var(--ag-accent-gradient);
  border: none;
  border-radius: var(--radius-md);
  color: white;
  font-size: 13px;
  font-weight: 600;
  cursor: pointer;
  transition: all var(--transition-fast);
  position: relative;
}

.primary-btn::before {
  content: '';
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  height: 50%;
  background: linear-gradient(180deg, rgba(255, 255, 255, 0.15), transparent);
  pointer-events: none;
  border-radius: inherit;
}

.primary-btn:hover {
  transform: translateY(-1px);
  filter: brightness(1.1);
  box-shadow: var(--ag-shadow-accent-lg);
}

.about-copyright {
  text-align: center;
  font-size: 10px;
  color: var(--ag-text-muted);
  margin: 0;
  letter-spacing: 0.01em;
}
</style>
