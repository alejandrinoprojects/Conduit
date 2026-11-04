<script setup lang="ts">
import { computed } from 'vue';

export interface BadgeProps {
  variant?: 'success' | 'error' | 'warning' | 'cyan' | 'orange' | 'neutral';
  size?: 'sm' | 'md';
  dot?: boolean;
}

const props = withDefaults(defineProps<BadgeProps>(), {
  variant: 'neutral',
  size: 'md',
  dot: false,
});

const variantClasses = computed(() => {
  switch (props.variant) {
    case 'success':
      return 'bg-status-success-subtle text-status-success border-status-success/30';
    case 'error':
      return 'bg-status-error-subtle text-status-error border-status-error/30';
    case 'warning':
      return 'bg-status-warning-subtle text-status-warning border-status-warning/30';
    case 'cyan':
      return 'bg-cyan-subtle text-cyan-accent border-cyan-border';
    case 'orange':
      return 'bg-orange-subtle text-orange-accent border-orange-border';
    case 'neutral':
    default:
      return 'bg-elevated/70 text-text-secondary border-border';
  }
});

const dotColorClasses = computed(() => {
  switch (props.variant) {
    case 'success':
      return 'bg-status-success';
    case 'error':
      return 'bg-status-error';
    case 'warning':
      return 'bg-status-warning';
    case 'cyan':
      return 'bg-cyan-accent';
    case 'orange':
      return 'bg-orange-accent';
    case 'neutral':
    default:
      return 'bg-text-secondary';
  }
});

const sizeClasses = computed(() => {
  switch (props.size) {
    case 'sm':
      return 'px-1.5 py-0.5 text-[10px] gap-1 rounded';
    case 'md':
    default:
      return 'px-2 py-0.5 text-xs gap-1.5 rounded';
  }
});
</script>

<template>
  <span
    :class="[
      'inline-flex items-center font-medium border select-none transition-colors duration-150',
      variantClasses,
      sizeClasses,
    ]"
  >
    <span
      v-if="dot"
      :class="['w-1.5 h-1.5 rounded-full shrink-0', dotColorClasses]"
    />
    <slot name="icon" />
    <slot />
  </span>
</template>
