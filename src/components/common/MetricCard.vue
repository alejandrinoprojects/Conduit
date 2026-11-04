<script setup lang="ts">
import { computed } from 'vue';

export interface MetricCardProps {
  label: string;
  value: string | number;
  unit?: string;
  secondaryValue?: string | number;
  secondaryLabel?: string;
  status?: 'normal' | 'success' | 'warning' | 'error' | 'cyan';
  compact?: boolean;
}

const props = withDefaults(defineProps<MetricCardProps>(), {
  status: 'normal',
  compact: false,
});

const valueColorClass = computed(() => {
  switch (props.status) {
    case 'success':
      return 'text-status-success';
    case 'warning':
      return 'text-status-warning';
    case 'error':
      return 'text-status-error';
    case 'cyan':
      return 'text-cyan-accent';
    case 'normal':
    default:
      return 'text-text-primary';
  }
});
</script>

<template>
  <div
    :class="[
      'bg-card border border-border rounded-lg flex flex-col justify-between transition-colors select-none',
      compact ? 'p-2.5 gap-1.5' : 'p-3.5 gap-2'
    ]"
  >
    <!-- Label + Icon -->
    <div class="flex items-center justify-between text-text-secondary text-[11px] font-medium tracking-wide uppercase">
      <span class="truncate">{{ label }}</span>
      <div v-if="$slots.icon" class="text-text-muted shrink-0 ml-1">
        <slot name="icon" />
      </div>
    </div>

    <!-- Main Value + Unit -->
    <div class="flex items-baseline gap-1.5">
      <span
        :class="[
          'font-semibold font-mono tracking-tight',
          valueColorClass,
          compact ? 'text-lg' : 'text-xl'
        ]"
      >
        {{ value }}
      </span>
      <span v-if="unit" class="text-xs text-text-muted font-normal">
        {{ unit }}
      </span>
    </div>

    <!-- Secondary Sub-value or Custom Slot (e.g., Sparkline) -->
    <div v-if="$slots.default || secondaryValue !== undefined" class="mt-0.5">
      <slot>
        <div v-if="secondaryValue !== undefined" class="flex items-center gap-1 text-[11px] text-text-muted">
          <span v-if="secondaryLabel">{{ secondaryLabel }}:</span>
          <span class="text-text-secondary font-mono">{{ secondaryValue }}</span>
        </div>
      </slot>
    </div>
  </div>
</template>
