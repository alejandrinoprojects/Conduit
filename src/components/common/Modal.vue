<script setup lang="ts">
import { computed, watch, onUnmounted } from 'vue';
import { X } from 'lucide-vue-next';

export interface ModalProps {
  modelValue: boolean;
  title?: string;
  description?: string;
  maxWidth?: 'sm' | 'md' | 'lg' | 'xl' | '2xl' | '3xl' | string;
  preventClose?: boolean;
  showClose?: boolean;
}

const props = withDefaults(defineProps<ModalProps>(), {
  title: '',
  description: '',
  maxWidth: 'md',
  preventClose: false,
  showClose: true,
});

const emit = defineEmits<{
  (e: 'update:modelValue', value: boolean): void;
  (e: 'close'): void;
}>();

const maxWidthClass = computed(() => {
  switch (props.maxWidth) {
    case 'sm':
      return 'max-w-sm';
    case 'md':
      return 'max-w-md';
    case 'lg':
      return 'max-w-lg';
    case 'xl':
      return 'max-w-xl';
    case '2xl':
      return 'max-w-2xl';
    case '3xl':
      return 'max-w-3xl';
    default:
      return props.maxWidth.startsWith('max-w-') ? props.maxWidth : 'max-w-md';
  }
});

function handleClose() {
  if (props.preventClose) return;
  emit('update:modelValue', false);
  emit('close');
}

function handleBackdropClick(event: MouseEvent) {
  if (event.target === event.currentTarget) {
    handleClose();
  }
}

function handleKeydown(e: KeyboardEvent) {
  if (e.key === 'Escape' && props.modelValue) {
    handleClose();
  }
}

watch(
  () => props.modelValue,
  (open) => {
    if (open) {
      window.addEventListener('keydown', handleKeydown);
    } else {
      window.removeEventListener('keydown', handleKeydown);
    }
  },
  { immediate: true }
);

onUnmounted(() => {
  window.removeEventListener('keydown', handleKeydown);
});
</script>

<template>
  <Teleport to="body">
    <Transition
      enter-active-class="transition duration-200 ease-out"
      enter-from-class="opacity-0"
      enter-to-class="opacity-100"
      leave-active-class="transition duration-150 ease-in"
      leave-from-class="opacity-100"
      leave-to-class="opacity-0"
    >
      <div
        v-if="modelValue"
        class="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/75 backdrop-blur-[2px] select-none"
        @mousedown="handleBackdropClick"
      >
        <div
          :class="[
            'relative w-full flex flex-col bg-card border border-border rounded-lg shadow-2xl overflow-hidden',
            maxWidthClass,
            'max-h-[90vh] text-text-primary'
          ]"
          @mousedown.stop
        >
          <!-- Header -->
          <div
            v-if="title || $slots.header"
            class="flex items-center justify-between px-5 py-3.5 border-b border-border bg-sidebar shrink-0"
          >
            <div v-if="$slots.header" class="w-full">
              <slot name="header" />
            </div>
            <div v-else class="flex flex-col min-w-0 pr-2">
              <h3 class="text-sm font-semibold text-text-primary truncate">
                {{ title }}
              </h3>
              <p v-if="description" class="text-xs text-text-secondary mt-0.5 truncate">
                {{ description }}
              </p>
            </div>

            <button
              v-if="showClose"
              type="button"
              class="w-7 h-7 flex items-center justify-center rounded text-text-muted hover:text-text-primary hover:bg-elevated transition-colors cursor-pointer shrink-0"
              @click="handleClose"
            >
              <X class="w-4 h-4" />
            </button>
          </div>

          <!-- Body -->
          <div class="p-5 overflow-y-auto flex-1 text-xs text-text-secondary leading-relaxed">
            <slot />
          </div>

          <!-- Footer -->
          <div
            v-if="$slots.footer"
            class="flex items-center justify-end gap-2 px-5 py-3 border-t border-border bg-sidebar/50 shrink-0"
          >
            <slot name="footer" />
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>
