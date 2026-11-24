<script setup lang="ts">
import { useId } from 'vue';

defineProps<{ title: string }>();
defineEmits<{ close: [] }>();
const titleId = useId();
</script>

<template>
  <div class="modal-overlay" @click.self="$emit('close')">
    <div class="modal" role="dialog" aria-modal="true" :aria-labelledby="titleId">
      <div class="modal-header">
        <h2 :id="titleId" class="modal-title">{{ title }}</h2>
        <button class="close-btn" :aria-label="$t('titleBar.close')" @click="$emit('close')">×</button>
      </div>
      <div class="modal-body"><slot /></div>
      <div class="modal-footer"><slot name="footer" /></div>
    </div>
  </div>
</template>

<style scoped>
.modal-overlay {
  position: fixed;
  inset: 0;
  background: rgba(8, 12, 24, 0.55);
  backdrop-filter: blur(8px) saturate(150%);
  -webkit-backdrop-filter: blur(8px) saturate(150%);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 100;
  animation: fadeIn 0.2s ease-out;
}

.modal {
  background: var(--ag-surface);
  border: 1px solid var(--ag-border);
  border-radius: var(--radius-2xl);
  display: flex;
  flex-direction: column;
  width: min(360px, calc(100% - 24px));
  height: min(396px, calc(100% - 24px));
  box-shadow: var(--ag-shadow-xl);
  animation: slideUp 0.35s cubic-bezier(0.16, 1, 0.3, 1);
  position: relative;
  overflow: hidden;
}

.modal::before {
  content: '';
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  height: 1px;
  background: linear-gradient(90deg, transparent, var(--ag-border-hover), transparent);
  pointer-events: none;
}

.modal-header {
  flex-shrink: 0;
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
  padding: 4px;
  line-height: 1;
  border-radius: var(--radius-sm);
  transition: all var(--transition-fast);
  width: 30px;
  height: 30px;
  display: flex;
  align-items: center;
  justify-content: center;
}

.close-btn:hover {
  color: var(--ag-text);
  background: var(--ag-hover);
}

.modal-body {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: 12px 20px;
}

.modal-body :deep(.modal-product-name) {
  margin: 0 0 4px;
  font-size: 18px;
  font-weight: 700;
  letter-spacing: -0.01em;
  color: var(--ag-text-strong);
}

.modal-footer {
  flex-shrink: 0;
  padding: 12px 20px;
  border-top: 1px solid var(--ag-border);
}

@media (prefers-reduced-motion: reduce) {
  .modal, .modal-overlay { animation: none; }
}
</style>
