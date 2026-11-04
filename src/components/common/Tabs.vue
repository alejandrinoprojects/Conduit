<script setup lang="ts">
import { computed } from 'vue';

export interface TabItem {
  id: string;
  label: string;
  icon?: any;
  count?: number | string;
  disabled?: boolean;
}

export interface TabsProps {
  modelValue: string;
  tabs: TabItem[];
  variant?: 'line' | 'segmented';
  size?: 'sm' | 'md';
  fullWidth?: boolean;
}

const props = withDefaults(defineProps<TabsProps>(), {
  variant: 'line',
  size: 'md',
  fullWidth: false,
});

const emit = defineEmits<{
  (e: 'update:modelValue', id: string): void;
  (e: 'change', id: string): void;
}>();

const sizeClasses = computed(() => {
  if (props.variant === 'segmented') {
    return props.size === 'sm'
      ? 'px-2 py-1 text-xs'
      : 'px-3 py-1.5 text-xs';
  }
  return props.size === 'sm'
    ? 'px-2.5 py-1.5 text-xs'
    : 'px-3.5 py-2 text-xs';
});

function selectTab(tab: TabItem) {
  if (tab.disabled || tab.id === props.modelValue) return;
  emit('update:modelValue', tab.id);
  emit('change', tab.id);
}
</script>

<template>
  <!-- Line Variant -->
  <div
    v-if="variant === 'line'"
    class="flex items-center border-b border-border select-none"
    :class="fullWidth ? 'w-full' : ''"
  >
    <button
      v-for="tab in tabs"
      :key="tab.id"
      type="button"
      :disabled="tab.disabled"
      :class="[
        'inline-flex items-center gap-2 border-b-2 font-medium transition-all duration-150 outline-none focus-visible:ring-1 focus-visible:ring-cyan-accent -mb-px',
        sizeClasses,
        fullWidth ? 'flex-1 justify-center' : '',
        tab.id === modelValue
          ? 'border-cyan-accent text-cyan-accent'
          : 'border-transparent text-text-secondary hover:text-text-primary hover:border-border',
        tab.disabled ? 'opacity-40 cursor-not-allowed' : 'cursor-pointer'
      ]"
      @click="selectTab(tab)"
    >
      <component :is="tab.icon" v-if="tab.icon" class="w-3.5 h-3.5 shrink-0" />
      <span>{{ tab.label }}</span>
      <span
        v-if="tab.count !== undefined"
        :class="[
          'px-1.5 py-0.5 rounded text-[10px] font-mono shrink-0',
          tab.id === modelValue
            ? 'bg-cyan-subtle text-cyan-accent'
            : 'bg-elevated text-text-muted'
        ]"
      >
        {{ tab.count }}
      </span>
    </button>
  </div>

  <!-- Segmented Variant -->
  <div
    v-else
    class="inline-flex items-center p-0.5 bg-sidebar border border-border rounded-md select-none"
    :class="fullWidth ? 'w-full flex' : ''"
  >
    <button
      v-for="tab in tabs"
      :key="tab.id"
      type="button"
      :disabled="tab.disabled"
      :class="[
        'inline-flex items-center gap-1.5 font-medium transition-all duration-150 outline-none focus-visible:ring-1 focus-visible:ring-cyan-accent rounded',
        sizeClasses,
        fullWidth ? 'flex-1 justify-center' : '',
        tab.id === modelValue
          ? 'bg-elevated text-cyan-accent border border-border/80 shadow-sm'
          : 'text-text-secondary hover:text-text-primary border border-transparent',
        tab.disabled ? 'opacity-40 cursor-not-allowed' : 'cursor-pointer'
      ]"
      @click="selectTab(tab)"
    >
      <component :is="tab.icon" v-if="tab.icon" class="w-3.5 h-3.5 shrink-0" />
      <span>{{ tab.label }}</span>
      <span
        v-if="tab.count !== undefined"
        :class="[
          'px-1 py-0.5 rounded text-[10px] font-mono shrink-0',
          tab.id === modelValue
            ? 'bg-cyan-subtle text-cyan-accent'
            : 'bg-canvas text-text-muted'
        ]"
      >
        {{ tab.count }}
      </span>
    </button>
  </div>
</template>
