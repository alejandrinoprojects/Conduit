<script setup lang="ts">
import { computed } from 'vue';
import { Loader2 } from 'lucide-vue-next';

export interface ButtonProps {
  variant?: 'primary' | 'secondary' | 'accent' | 'ghost' | 'danger';
  size?: 'sm' | 'md' | 'lg' | 'icon';
  disabled?: boolean;
  loading?: boolean;
  type?: 'button' | 'submit' | 'reset';
  fullWidth?: boolean;
}

const props = withDefaults(defineProps<ButtonProps>(), {
  variant: 'secondary',
  size: 'md',
  disabled: false,
  loading: false,
  type: 'button',
  fullWidth: false,
});

const emit = defineEmits<{
  (e: 'click', event: MouseEvent): void;
}>();

const variantClasses = computed(() => {
  switch (props.variant) {
    case 'primary':
      // Orange brand/primary CTA: #F39A36, text in dark canvas #11151A for high contrast
      return 'bg-orange-accent text-[#11151A] hover:bg-orange-hover font-semibold shadow-sm focus-visible:ring-2 focus-visible:ring-orange-accent/50';
    case 'accent':
      // Cyan interactive action: #45C5D9
      return 'bg-cyan-accent text-[#11151A] hover:bg-cyan-highlight font-semibold shadow-sm focus-visible:ring-2 focus-visible:ring-cyan-accent/50';
    case 'danger':
      // Muted red error/destructive: #E05252
      return 'bg-status-error text-white hover:bg-[#c94343] font-medium shadow-sm focus-visible:ring-2 focus-visible:ring-status-error/50';
    case 'ghost':
      // Transparent ghost: subtle hover
      return 'bg-transparent text-text-secondary hover:text-text-primary hover:bg-elevated/60 font-medium focus-visible:ring-1 focus-visible:ring-cyan-accent/50';
    case 'secondary':
    default:
      // Dark graphite elevated: #27323D surface with border #303B47
      return 'bg-elevated hover:bg-elevated-hover text-text-primary border border-border hover:border-border/80 font-medium shadow-sm focus-visible:ring-1 focus-visible:ring-cyan-accent/50';
  }
});

const sizeClasses = computed(() => {
  switch (props.size) {
    case 'sm':
      return 'h-7 px-2.5 text-xs gap-1.5 rounded';
    case 'lg':
      return 'h-10 px-4 text-sm gap-2 rounded-md';
    case 'icon':
      return 'h-8 w-8 p-0 justify-center rounded-md';
    case 'md':
    default:
      return 'h-8.5 px-3.5 text-xs gap-2 rounded-md';
  }
});

function handleClick(e: MouseEvent) {
  if (props.disabled || props.loading) {
    e.preventDefault();
    return;
  }
  emit('click', e);
}
</script>

<template>
  <button
    :type="type"
    :disabled="disabled || loading"
    :class="[
      'inline-flex items-center justify-center transition-all duration-150 select-none outline-none shrink-0',
      variantClasses,
      sizeClasses,
      fullWidth ? 'w-full' : '',
      (disabled || loading) ? 'opacity-50 cursor-not-allowed pointer-events-none' : 'cursor-pointer active:scale-[0.98]'
    ]"
    @click="handleClick"
  >
    <Loader2 v-if="loading" class="w-3.5 h-3.5 animate-spin shrink-0" :class="{ 'mr-1': size !== 'icon' }" />
    <span v-else-if="$slots.prefix" class="inline-flex items-center shrink-0">
      <slot name="prefix" />
    </span>

    <span v-if="$slots.default && size !== 'icon'" class="truncate">
      <slot />
    </span>
    <slot v-else-if="size === 'icon' && !loading" />

    <span v-if="$slots.suffix && !loading && size !== 'icon'" class="inline-flex items-center shrink-0">
      <slot name="suffix" />
    </span>
  </button>
</template>
