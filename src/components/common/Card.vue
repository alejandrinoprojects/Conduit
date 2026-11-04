<script setup lang="ts">
export interface CardProps {
  hoverable?: boolean;
  selected?: boolean;
  elevated?: boolean;
  compact?: boolean;
  padded?: boolean;
  title?: string;
  subtitle?: string;
}

withDefaults(defineProps<CardProps>(), {
  hoverable: false,
  selected: false,
  elevated: false,
  compact: false,
  padded: true,
});
</script>

<template>
  <div
    :class="[
      'rounded-lg border transition-all duration-150 relative flex flex-col',
      elevated ? 'bg-elevated' : 'bg-card',
      selected
        ? 'border-cyan-accent ring-1 ring-cyan-accent/50'
        : 'border-border',
      hoverable
        ? 'hover:border-border/90 hover:bg-card-hover cursor-pointer'
        : '',
    ]"
  >
    <!-- Card Header -->
    <div
      v-if="$slots.header || title || $slots.actions"
      :class="[
        'flex items-center justify-between border-b border-border/60',
        compact ? 'px-3 py-2' : 'px-4 py-3'
      ]"
    >
      <div v-if="$slots.header" class="w-full">
        <slot name="header" />
      </div>
      <div v-else class="flex flex-col min-w-0">
        <h3 v-if="title" class="text-xs font-semibold text-text-primary tracking-wide uppercase truncate">
          {{ title }}
        </h3>
        <p v-if="subtitle" class="text-[11px] text-text-secondary truncate mt-0.5">
          {{ subtitle }}
        </p>
      </div>

      <div v-if="$slots.actions" class="flex items-center gap-2 shrink-0 ml-3">
        <slot name="actions" />
      </div>
    </div>

    <!-- Card Body -->
    <div
      :class="[
        'flex-1 min-w-0',
        padded ? (compact ? 'p-3' : 'p-4') : ''
      ]"
    >
      <slot />
    </div>

    <!-- Card Footer -->
    <div
      v-if="$slots.footer"
      :class="[
        'border-t border-border/60 bg-canvas/30 rounded-b-lg',
        compact ? 'px-3 py-2' : 'px-4 py-2.5'
      ]"
    >
      <slot name="footer" />
    </div>
  </div>
</template>
