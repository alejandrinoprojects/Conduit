<script setup lang="ts">
import { computed } from 'vue';

export interface StatusDotProps {
  status: 'online' | 'stopped' | 'starting' | 'error' | 'warning' | 'idle' | string;
  size?: 'xs' | 'sm' | 'md' | 'lg';
  pulse?: boolean;
  label?: string;
}

const props = withDefaults(defineProps<StatusDotProps>(), {
  size: 'md',
  pulse: false,
});

const normalizedStatus = computed(() => {
  const s = (props.status || '').toLowerCase();
  if (s === 'online' || s === 'running' || s === 'playing' || s === 'active' || s === 'connected') return 'online';
  if (s === 'starting' || s === 'loading' || s === 'restarting' || s === 'provisioning') return 'starting';
  if (s === 'error' || s === 'failed') return 'error';
  if (s === 'warning' || s === 'warn') return 'warning';
  if (s === 'idle') return 'idle';
  return 'stopped';
});

const dotColorClass = computed(() => {
  switch (normalizedStatus.value) {
    case 'online':
      return 'bg-status-success';
    case 'starting':
      return 'bg-orange-accent';
    case 'error':
      return 'bg-status-error';
    case 'warning':
      return 'bg-status-warning';
    case 'idle':
    case 'stopped':
    default:
      return 'bg-text-secondary';
  }
});

const sizeClasses = computed(() => {
  switch (props.size) {
    case 'xs':
      return 'w-[3px] h-[3px]';
    case 'sm':
      return 'w-1.5 h-1.5';
    case 'lg':
      return 'w-2.5 h-2.5';
    case 'md':
    default:
      return 'w-2 h-2';
  }
});

const shouldPulse = computed(() => {
  if (props.pulse) return true;
  return normalizedStatus.value === 'starting';
});
</script>

<template>
  <span class="inline-flex items-center gap-1.5 select-none shrink-0">
    <span class="relative flex items-center justify-center shrink-0">
      <span
        v-if="shouldPulse"
        :class="[
          'absolute inline-flex rounded-full opacity-75 animate-ping',
          sizeClasses,
          dotColorClass,
        ]"
      />
      <span
        :class="[
          'relative inline-flex rounded-full',
          sizeClasses,
          dotColorClass,
        ]"
      />
    </span>
    <span v-if="label" class="text-xs text-text-secondary font-medium">
      {{ label }}
    </span>
  </span>
</template>
