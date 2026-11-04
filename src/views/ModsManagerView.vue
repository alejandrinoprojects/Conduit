<script setup lang="ts">
import { ref, computed } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import {
  Package,
  RefreshCw,
  Search,
  Trash2,
  ArrowRightLeft,
  Filter,
  Check,
  AlertTriangle,
  FolderOpen,
  X,
  Compass,
  Download,
  Calendar,
  Layers,
  Sparkles
} from 'lucide-vue-next';
import { Button, Input, Badge, StatusDot, Modal } from '../components/common';
import { useServerState } from '../composables/useServerState';
import type { InstalledMod, ModVersionOption } from '../composables/useConduitIPC';

export interface ModsManagerViewProps {
  instanceDir: string;
  installedMods: InstalledMod[];
  isModsLoading?: boolean;
  loader?: string;
  gameVersion?: string;
  realmName?: string;
}

const props = withDefaults(defineProps<ModsManagerViewProps>(), {
  instanceDir: '',
  installedMods: () => [],
  isModsLoading: false,
  loader: 'NeoForge',
  gameVersion: '1.21.1',
  realmName: 'Active Realm',
});

const emit = defineEmits<{
  (e: 'refresh-mods'): void;
  (e: 'toggle-mod', mod: InstalledMod): void;
  (e: 'delete-mod', fileName: string): void;
  (e: 'open-folder'): void;
  (e: 'open-discover'): void;
}>();

const { toastSuccess, toastError, toastInfo } = useServerState();

// Search and Filter State
const searchQuery = ref<string>('');
const statusFilter = ref<'all' | 'enabled' | 'disabled'>('all');
const envFilter = ref<'all' | 'Both' | 'ClientOnly' | 'ServerOnly'>('all');

// Switch Version Modal State
const switchingMod = ref<boolean>(false);
const selectedModForSwitch = ref<InstalledMod | null>(null);
const availableModVersions = ref<ModVersionOption[]>([]);
const isVersionsLoading = ref<boolean>(false);
const isSwitching = ref<boolean>(false);

// Delete Confirmation Modal State
const showDeleteConfirmModal = ref<boolean>(false);
const modPendingDelete = ref<InstalledMod | null>(null);
const isDeleting = ref<boolean>(false);

// Filtered Mods Computed List
const filteredMods = computed<InstalledMod[]>(() => {
  let list = props.installedMods || [];

  // Filter by enabled/disabled status
  if (statusFilter.value === 'enabled') {
    list = list.filter((m) => m.is_enabled);
  } else if (statusFilter.value === 'disabled') {
    list = list.filter((m) => !m.is_enabled);
  }

  // Filter by environment
  if (envFilter.value !== 'all') {
    list = list.filter((m) => m.environment === envFilter.value);
  }

  // Search query filter
  const q = searchQuery.value.trim().toLowerCase();
  if (!q) return list;

  return list.filter((m) => {
    const nameMatch = (m.mod_name && m.mod_name.toLowerCase().includes(q)) || false;
    const fileMatch = m.file_name.toLowerCase().includes(q);
    const idMatch = (m.mod_id && m.mod_id.toLowerCase().includes(q)) || false;
    return nameMatch || fileMatch || idMatch;
  });
});

// Enabled vs Disabled Counts
const enabledCount = computed(() => (props.installedMods || []).filter((m) => m.is_enabled).length);
const disabledCount = computed(() => (props.installedMods || []).filter((m) => !m.is_enabled).length);

// Status Toggle Handler
async function handleToggleMod(mod: InstalledMod) {
  try {
    await invoke('toggle_mod', {
      instanceDir: props.instanceDir,
      fileName: mod.file_name,
      enable: !mod.is_enabled,
    });
    toastSuccess(
      mod.is_enabled ? 'Mod Disabled' : 'Mod Enabled',
      `${mod.mod_name || mod.file_name} is now ${!mod.is_enabled ? 'enabled' : 'disabled'}.`
    );
    emit('toggle-mod', mod);
    emit('refresh-mods');
  } catch (err: any) {
    const msg = err?.message || String(err);
    toastError('Toggle Failed', `Failed to toggle mod: ${msg}`);
  }
}

// Open Switch Version Modal
async function openSwitchVersion(mod: InstalledMod) {
  selectedModForSwitch.value = mod;
  switchingMod.value = true;
  availableModVersions.value = [];
  isVersionsLoading.value = true;

  try {
    const targetProjId = mod.mod_id || mod.file_name.replace(/\.jar(\.disabled)?$/, '').split('-')[0].toLowerCase();
    const versions = await invoke<ModVersionOption[]>('get_mod_versions', {
      projectId: targetProjId,
      loader: props.loader ? props.loader.toLowerCase() : 'neoforge',
      gameVersion: props.gameVersion || '1.21.1',
    });
    availableModVersions.value = Array.isArray(versions) ? versions : [];
  } catch (err: any) {
    console.warn('Failed to load mod versions:', err);
    availableModVersions.value = [];
  } finally {
    isVersionsLoading.value = false;
  }
}

// Execute Version Switch
async function executeSwitchVersion(ver: ModVersionOption) {
  if (!selectedModForSwitch.value) return;
  isSwitching.value = true;

  try {
    await invoke('switch_mod_version', {
      instanceDir: props.instanceDir,
      oldFileName: selectedModForSwitch.value.file_name,
      downloadUrl: ver.download_url,
      newFileName: ver.filename,
    });

    toastSuccess('Version Switched', `Updated to ${ver.version_number} (${ver.filename})`);
    switchingMod.value = false;
    selectedModForSwitch.value = null;
    emit('refresh-mods');
  } catch (err: any) {
    const msg = err?.message || String(err);
    toastError('Switch Failed', `Failed to switch version: ${msg}`);
  } finally {
    isSwitching.value = false;
  }
}

// Prompt Delete Confirmation Modal
function promptDeleteMod(mod: InstalledMod) {
  modPendingDelete.value = mod;
  showDeleteConfirmModal.value = true;
}

// Confirm Delete Mod Execution
async function confirmDeleteMod() {
  if (!modPendingDelete.value) return;
  isDeleting.value = true;

  try {
    await invoke('delete_mod', {
      instanceDir: props.instanceDir,
      fileName: modPendingDelete.value.file_name,
    });

    toastSuccess('Mod Deleted', `Removed ${modPendingDelete.value.file_name}`);
    showDeleteConfirmModal.value = false;
    emit('delete-mod', modPendingDelete.value.file_name);
    modPendingDelete.value = null;
    emit('refresh-mods');
  } catch (err: any) {
    const msg = err?.message || String(err);
    toastError('Delete Failed', `Failed to delete mod: ${msg}`);
  } finally {
    isDeleting.value = false;
  }
}

function formatBytes(bytes: number): string {
  if (!bytes || bytes === 0) return '0 MB';
  const mb = bytes / (1024 * 1024);
  if (mb < 0.1) return '< 0.1 MB';
  return mb.toFixed(2) + ' MB';
}
</script>

<template>
  <div class="flex-1 flex flex-col h-full overflow-hidden bg-canvas text-text-primary">
    <!-- Header Toolbar -->
    <div class="h-12 border-b border-border px-6 flex items-center justify-between bg-sidebar/70 shrink-0">
      <!-- Title & Metric Badge -->
      <div class="flex items-center space-x-3">
        <div class="flex items-center space-x-2">
          <Package class="w-4 h-4 text-cyan-accent" />
          <h2 class="text-xs font-bold tracking-wider text-text-primary uppercase">Installed Mods</h2>
        </div>

        <Badge variant="cyan" size="sm">
          {{ installedMods.length }} {{ installedMods.length === 1 ? 'mod' : 'mods' }}
        </Badge>

        <div class="hidden sm:flex items-center space-x-2 text-[11px] text-text-muted font-mono pl-1">
          <span>&bull; {{ enabledCount }} active</span>
          <span v-if="disabledCount > 0">&bull; {{ disabledCount }} disabled</span>
        </div>
      </div>

      <!-- Search & Quick Actions -->
      <div class="flex items-center space-x-3">
        <!-- Live Table Filter Input -->
        <div class="relative w-56">
          <Search class="w-3.5 h-3.5 absolute left-2.5 top-1/2 -translate-y-1/2 text-text-muted pointer-events-none" />
          <input
            type="text"
            v-model="searchQuery"
            placeholder="Filter installed mods..."
            class="w-full h-8 pl-8 pr-7 bg-card border border-border rounded text-xs text-text-primary placeholder:text-text-muted focus:outline-none focus:border-cyan-accent focus:ring-1 focus:ring-cyan-accent/40 transition-all font-sans"
          />
          <button
            v-if="searchQuery"
            @click="searchQuery = ''"
            class="absolute right-2 top-1/2 -translate-y-1/2 text-text-muted hover:text-text-primary p-0.5 cursor-pointer transition-colors"
          >
            <X class="w-3 h-3" />
          </button>
        </div>

        <!-- Open Mods Folder Action -->
        <Button
          variant="ghost"
          size="sm"
          @click="$emit('open-folder')"
          title="Open instance mods directory"
        >
          <template #prefix>
            <FolderOpen class="w-3.5 h-3.5" />
          </template>
          <span class="hidden md:inline">Open Folder</span>
        </Button>

        <!-- Refresh CTA -->
        <Button
          variant="secondary"
          size="sm"
          :disabled="isModsLoading"
          @click="$emit('refresh-mods')"
          title="Reload installed mods from disk"
        >
          <template #prefix>
            <RefreshCw class="w-3.5 h-3.5" :class="isModsLoading ? 'animate-spin text-cyan-accent' : ''" />
          </template>
          <span>Refresh</span>
        </Button>
      </div>
    </div>

    <!-- Secondary Filter Pills Bar -->
    <div class="border-b border-border bg-sidebar/30 px-6 py-2 flex items-center justify-between text-xs shrink-0">
      <div class="flex items-center space-x-2">
        <span class="text-[11px] font-medium text-text-muted uppercase tracking-wide mr-1">Status:</span>
        <button
          @click="statusFilter = 'all'"
          class="px-2 py-0.5 rounded text-[11px] transition-colors cursor-pointer border"
          :class="
            statusFilter === 'all'
              ? 'bg-elevated text-cyan-accent border-cyan-accent/50 font-medium'
              : 'bg-card text-text-secondary border-border/60 hover:text-text-primary'
          "
        >
          All ({{ installedMods.length }})
        </button>
        <button
          @click="statusFilter = 'enabled'"
          class="px-2 py-0.5 rounded text-[11px] transition-colors cursor-pointer border"
          :class="
            statusFilter === 'enabled'
              ? 'bg-elevated text-cyan-accent border-cyan-accent/50 font-medium'
              : 'bg-card text-text-secondary border-border/60 hover:text-text-primary'
          "
        >
          Enabled ({{ enabledCount }})
        </button>
        <button
          @click="statusFilter = 'disabled'"
          class="px-2 py-0.5 rounded text-[11px] transition-colors cursor-pointer border"
          :class="
            statusFilter === 'disabled'
              ? 'bg-elevated text-cyan-accent border-cyan-accent/50 font-medium'
              : 'bg-card text-text-secondary border-border/60 hover:text-text-primary'
          "
        >
          Disabled ({{ disabledCount }})
        </button>
      </div>

      <div class="flex items-center space-x-2">
        <span class="text-[11px] font-medium text-text-muted uppercase tracking-wide">Environment:</span>
        <select
          v-model="envFilter"
          class="h-6 px-1.5 bg-card border border-border rounded text-[11px] text-text-primary focus:outline-none focus:border-cyan-accent cursor-pointer"
        >
          <option value="all">All Environments</option>
          <option value="Both">Client & Server</option>
          <option value="ClientOnly">Client Only</option>
          <option value="ServerOnly">Server Only</option>
        </select>
      </div>
    </div>

    <!-- Main Content: Desktop-Dense Mods Table -->
    <div class="flex-1 overflow-y-auto p-6">
      <!-- Empty State: No Mods in instance -->
      <div
        v-if="installedMods.length === 0 && !isModsLoading"
        class="max-w-md mx-auto my-16 p-8 bg-card border border-border rounded-xl text-center space-y-3"
      >
        <div class="w-12 h-12 mx-auto rounded-full bg-elevated border border-border flex items-center justify-center text-text-muted">
          <Package class="w-6 h-6 text-cyan-accent" />
        </div>
        <div>
          <h3 class="text-sm font-semibold text-text-primary">No Mods Installed</h3>
          <p class="text-xs text-text-secondary mt-1 leading-relaxed">
            This realm has no mods installed in its <code class="font-mono text-cyan-accent">mods/</code> folder yet.
          </p>
        </div>
        <div class="pt-2">
          <Button variant="primary" size="sm" @click="$emit('open-discover')">
            <template #prefix>
              <Compass class="w-3.5 h-3.5" />
            </template>
            <span>Browse Modrinth Catalog</span>
          </Button>
        </div>
      </div>

      <!-- Empty State: Filter matched nothing -->
      <div
        v-else-if="filteredMods.length === 0 && !isModsLoading"
        class="max-w-md mx-auto my-16 p-8 bg-card border border-border rounded-xl text-center space-y-3"
      >
        <Search class="w-8 h-8 text-text-muted mx-auto" />
        <h3 class="text-sm font-semibold text-text-primary">No Matches Found</h3>
        <p class="text-xs text-text-secondary">
          No installed mods match your filter query "{{ searchQuery }}".
        </p>
        <div class="pt-2">
          <Button variant="ghost" size="sm" @click="searchQuery = ''; statusFilter = 'all'; envFilter = 'all'">
            Clear Filters
          </Button>
        </div>
      </div>

      <!-- Desktop-Dense Installed Mods Table -->
      <div v-else class="border border-border rounded-lg overflow-hidden bg-card shadow-sm">
        <table class="w-full text-left text-xs border-collapse">
          <thead>
            <tr class="border-b border-border bg-sidebar/60 text-[10px] font-semibold text-text-muted uppercase tracking-wider">
              <th class="py-2.5 px-4">Icon + Name + Author</th>
              <th class="py-2.5 px-4">Version + Loader</th>
              <th class="py-2.5 px-4 text-center w-32">Status Toggle</th>
              <th class="py-2.5 px-4 text-right w-36">Actions</th>
            </tr>
          </thead>
          <tbody class="divide-y divide-border/40">
            <tr
              v-for="mod in filteredMods"
              :key="mod.file_name"
              class="hover:bg-elevated/40 transition-colors group"
            >
              <!-- Column 1: [Icon + Name + Author] -->
              <td class="py-2.5 px-4">
                <div class="flex items-center space-x-3">
                  <!-- Mod Icon -->
                  <img
                    v-if="mod.icon_base64"
                    :src="mod.icon_base64"
                    alt="Mod Icon"
                    class="w-8 h-8 rounded object-cover bg-sidebar border border-border/80 shrink-0"
                  />
                  <div
                    v-else
                    class="w-8 h-8 rounded bg-sidebar border border-border/80 flex items-center justify-center text-cyan-accent shrink-0"
                  >
                    <Package class="w-4 h-4" />
                  </div>

                  <!-- Name and Details -->
                  <div class="min-w-0">
                    <div class="font-medium text-text-primary group-hover:text-cyan-accent transition-colors truncate">
                      {{ mod.mod_name || mod.file_name }}
                    </div>
                    <div class="text-[10px] text-text-muted font-mono truncate mt-0.5">
                      {{ mod.file_name }}
                    </div>
                  </div>
                </div>
              </td>

              <!-- Column 2: [Version + Loader] -->
              <td class="py-2.5 px-4 font-mono text-[11px] text-text-secondary whitespace-nowrap">
                <div class="flex items-center space-x-2">
                  <span class="text-text-primary font-medium">{{ mod.version || 'v1.0.0' }}</span>
                  <Badge variant="neutral" size="sm" class="text-[9px] uppercase">
                    {{ mod.environment }}
                  </Badge>
                </div>
                <div class="text-[10px] text-text-muted mt-0.5">
                  {{ formatBytes(mod.size_bytes) }}
                </div>
              </td>

              <!-- Column 3: [Status Toggle] (Cyan #45C5D9 when enabled, Muted #303B47 when disabled) -->
              <td class="py-2.5 px-4 text-center">
                <button
                  type="button"
                  @click="handleToggleMod(mod)"
                  class="inline-flex items-center space-x-1.5 px-2.5 py-1 rounded text-xs font-semibold cursor-pointer transition-all border shadow-xs"
                  :class="
                    mod.is_enabled
                      ? 'bg-cyan-accent text-canvas border-cyan-accent hover:bg-cyan-accent/90 shadow-cyan-accent/20'
                      : 'bg-[#303B47] text-text-secondary border-border hover:bg-[#384553] hover:text-text-primary'
                  "
                  title="Toggle mod enabled or disabled state"
                >
                  <span
                    class="w-1.5 h-1.5 rounded-full"
                    :class="mod.is_enabled ? 'bg-canvas' : 'bg-text-muted'"
                  />
                  <span>{{ mod.is_enabled ? 'Enabled' : 'Disabled' }}</span>
                </button>
              </td>

              <!-- Column 4: [Actions] ("Switch Version" & "Delete") -->
              <td class="py-2.5 px-4 text-right">
                <div class="inline-flex items-center space-x-1">
                  <!-- Switch Version Action Button -->
                  <Button
                    variant="ghost"
                    size="sm"
                    title="Switch to another compatible Modrinth version"
                    @click="openSwitchVersion(mod)"
                  >
                    <template #prefix>
                      <ArrowRightLeft class="w-3 h-3 text-cyan-accent" />
                    </template>
                    <span>Switch</span>
                  </Button>

                  <!-- Delete Action Button -->
                  <Button
                    variant="ghost"
                    size="icon"
                    title="Delete mod file permanently"
                    @click="promptDeleteMod(mod)"
                  >
                    <Trash2 class="w-3.5 h-3.5 text-status-error hover:scale-110 transition-transform" />
                  </Button>
                </div>
              </td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>

    <!-- RESTORED MODAL: Mod Version Switcher Modal (switchingMod) -->
    <Modal
      v-model="switchingMod"
      :title="'Switch Version: ' + (selectedModForSwitch?.mod_name || selectedModForSwitch?.file_name || '')"
      maxWidth="md"
    >
      <div class="space-y-4">
        <!-- Target Compatibility Bar -->
        <div class="p-3 bg-sidebar border border-border rounded flex items-center justify-between text-xs">
          <div>
            <div class="text-[10px] text-text-muted uppercase tracking-wide">Target Environment</div>
            <div class="font-medium text-text-primary mt-0.5">
              Loader: <span class="text-cyan-accent font-semibold">{{ loader }}</span> &bull; Minecraft: <span class="text-cyan-accent font-semibold">{{ gameVersion }}</span>
            </div>
          </div>
          <div class="text-right">
            <div class="text-[10px] text-text-muted uppercase tracking-wide">Current Version</div>
            <div class="font-mono text-text-secondary mt-0.5">{{ selectedModForSwitch?.version || 'Unknown' }}</div>
          </div>
        </div>

        <!-- Loading State -->
        <div v-if="isVersionsLoading" class="py-8 flex flex-col items-center justify-center space-y-2 text-xs text-text-muted font-mono">
          <RefreshCw class="w-5 h-5 text-cyan-accent animate-spin" />
          <span>Fetching compatible versions from Modrinth...</span>
        </div>

        <!-- Available Versions List -->
        <div
          v-else-if="availableModVersions.length > 0"
          class="max-h-72 overflow-y-auto space-y-2 divide-y divide-border/40 pr-1"
        >
          <div
            v-for="ver in availableModVersions"
            :key="ver.id"
            class="pt-2.5 flex items-center justify-between text-xs hover:bg-elevated/40 p-2.5 rounded transition-colors"
          >
            <!-- Version details -->
            <div class="min-w-0 pr-3">
              <div class="flex items-center space-x-2">
                <span class="font-bold text-text-primary text-xs">{{ ver.version_number }}</span>
                <span v-if="ver.name && ver.name !== ver.version_number" class="text-[11px] text-text-secondary truncate">
                  ({{ ver.name }})
                </span>
                <Badge v-if="ver.version_type" variant="neutral" size="sm" class="text-[9px] uppercase">
                  {{ ver.version_type }}
                </Badge>
              </div>

              <div class="text-[10px] text-text-muted font-mono mt-1 flex items-center space-x-2">
                <span v-if="ver.date_published">
                  Released: {{ ver.date_published.substring(0, 10) }}
                </span>
                <span>&bull;</span>
                <span class="truncate">{{ ver.filename }}</span>
                <span v-if="ver.downloads">&bull; {{ ver.downloads.toLocaleString() }} dl</span>
              </div>
            </div>

            <!-- Install Version CTA: Orange #F39A36 Primary Action -->
            <Button
              variant="primary"
              size="sm"
              :disabled="isSwitching"
              :loading="isSwitching"
              @click="executeSwitchVersion(ver)"
              title="Install this version into instance"
              class="shrink-0"
            >
              <template #prefix>
                <Download class="w-3 h-3" />
              </template>
              <span>Install Version</span>
            </Button>
          </div>
        </div>

        <!-- Empty State: No alternative versions found -->
        <div v-else class="py-8 text-center text-xs text-text-muted font-mono space-y-2">
          <AlertTriangle class="w-6 h-6 text-text-muted mx-auto" />
          <p>No compatible versions found on Modrinth for {{ loader }} {{ gameVersion }}.</p>
        </div>
      </div>

      <template #footer>
        <Button variant="ghost" size="sm" @click="switchingMod = false">
          Cancel
        </Button>
      </template>
    </Modal>

    <!-- Delete Mod Confirmation Modal -->
    <Modal
      v-model="showDeleteConfirmModal"
      title="Delete Mod"
      maxWidth="sm"
    >
      <div class="space-y-3 text-xs">
        <p class="text-text-secondary leading-relaxed">
          Are you sure you want to permanently delete
          <span class="font-bold text-text-primary">{{ modPendingDelete?.mod_name || modPendingDelete?.file_name }}</span>
          from your instance?
        </p>
        <p class="text-[11px] text-status-error/90 font-mono">
          File: {{ modPendingDelete?.file_name }}
        </p>
      </div>

      <template #footer>
        <div class="flex items-center justify-end space-x-2">
          <Button variant="ghost" size="sm" @click="showDeleteConfirmModal = false">
            Cancel
          </Button>
          <Button
            variant="danger"
            size="sm"
            :disabled="isDeleting"
            :loading="isDeleting"
            @click="confirmDeleteMod"
          >
            Delete Mod
          </Button>
        </div>
      </template>
    </Modal>
  </div>
</template>
