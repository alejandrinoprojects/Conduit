<script setup lang="ts">
import { computed } from 'vue';

export interface InputProps {
  modelValue?: string | number;
  type?: string;
  placeholder?: string;
  disabled?: boolean;
  readonly?: boolean;
  size?: 'sm' | 'md' | 'lg';
  error?: string | boolean;
  id?: string;
  name?: string;
  autofocus?: boolean;
  spellcheck?: boolean;
  autocomplete?: string;
}

const props = withDefaults(defineProps<InputProps>(), {
  modelValue: '',
  type: 'text',
  placeholder: '',
  disabled: false,
  readonly: false,
  size: 'md',
  error: false,
  spellcheck: false,
  autocomplete: 'off',
});

const emit = defineEmits<{
  (e: 'update:modelValue', value: string): void;
  (e: 'change', event: Event): void;
  (e: 'focus', event: FocusEvent): void;
  (e: 'blur', event: FocusEvent): void;
  (e: 'keydown', event: KeyboardEvent): void;
}>();

const sizeClasses = computed(() => {
  switch (props.size) {
    case 'sm':
      return 'h-7 text-xs px-2.5 rounded';
    case 'lg':
      return 'h-10 text-sm px-3.5 rounded-md';
    case 'md':
    default:
      return 'h-8.5 text-xs px-3 rounded-md';
  }
});

function handleInput(event: Event) {
  const target = event.target as HTMLInputElement;
  emit('update:modelValue', target.value);
}
</script>

<template>
  <div class="relative flex flex-col w-full">
    <div
      :class="[
        'group relative flex items-center w-full transition-all duration-150',
        'bg-card border rounded-md',
        error
          ? 'border-status-error ring-1 ring-status-error/40'
          : 'border-border focus-within:border-cyan-accent focus-within:ring-1 focus-within:ring-cyan-accent',
        disabled ? 'opacity-50 cursor-not-allowed bg-sidebar' : 'hover:border-border/80'
      ]"
    >
      <!-- Prefix Slot -->
      <div v-if="$slots.prefix" class="pl-2.5 pr-1 flex items-center text-text-muted shrink-0 pointer-events-none">
        <slot name="prefix" />
      </div>

      <!-- Native Input -->
      <input
        :id="id"
        :name="name"
        :type="type"
        :value="modelValue"
        :placeholder="placeholder"
        :disabled="disabled"
        :readonly="readonly"
        :autofocus="autofocus"
        :spellcheck="spellcheck"
        :autocomplete="autocomplete"
        :class="[
          'w-full bg-transparent text-text-primary placeholder:text-text-muted placeholder:select-none focus:outline-none',
          sizeClasses,
          $slots.prefix ? 'pl-1' : '',
          $slots.suffix ? 'pr-1' : '',
          disabled ? 'cursor-not-allowed' : ''
        ]"
        @input="handleInput"
        @change="emit('change', $event)"
        @focus="emit('focus', $event)"
        @blur="emit('blur', $event)"
        @keydown="emit('keydown', $event)"
      />

      <!-- Suffix Slot -->
      <div v-if="$slots.suffix" class="pr-2.5 pl-1 flex items-center text-text-muted shrink-0">
        <slot name="suffix" />
      </div>
    </div>

    <!-- Error message text if error is a string -->
    <p v-if="typeof error === 'string' && error" class="mt-1 text-[11px] text-status-error leading-tight">
      {{ error }}
    </p>
  </div>
</template>
