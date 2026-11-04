<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue';
import {
  Plus,
  FolderGit2,
  RefreshCw,
  Play,
  Square,
  Zap,
  ChevronDown,
  ChevronRight,
  Search,
  Folder,
  Terminal,
  Settings,
  Layers,
  ExternalLink,
  Clock,
  Users,
  Server,
  Sparkles,
  Trash2
} from 'lucide-vue-next';
import { Button, Input, Badge, StatusDot, Tabs, type TabItem } from '../components/common';
import type { WorldSaveInfo, ServerStatusType } from '../composables';

export interface RuntimeGroup {
  id: string;
  name: string;
  loader: 'NeoForge' | 'Vanilla' | 'Fabric' | 'Paper' | 'Quilt' | string;
  version: string;
  description: string;
  is_client: boolean;
  is_server: boolean;
  diskSizeMb: number;
}

export interface LibraryViewProps {
  runtimes: RuntimeGroup[];
  worlds: WorldSaveInfo[];
  jumpInRealms?: WorldSaveInfo[];
  isLoading?: boolean;
  serverStatus: ServerStatusType;
  selectedWorldName: string;
  instanceDir?: string;
}

const props = withDefaults(defineProps<LibraryViewProps>(), {
  jumpInRealms: () => [],
  isLoading: false,
  instanceDir: './instance'
});

const emit = defineEmits<{
  (e: 'select-realm', realm: WorldSaveInfo): void;
  (e: 'start-server-and-join', realm: WorldSaveInfo): void;
  (e: 'start-server', realm: WorldSaveInfo): void;
  (e: 'stop-server', realm?: WorldSaveInfo): void;
  (e: 'play-singleplayer', realm: WorldSaveInfo): void;
  (e: 'open-folder', realm?: WorldSaveInfo): void;
  (e: 'create-instance'): void;
  (e: 'create-realm', runtime?: RuntimeGroup): void;
  (e: 'add-mods', runtime: RuntimeGroup): void;
  (e: 'open-mods-folder', runtime: RuntimeGroup): void;
  (e: 'delete-instance', runtime: RuntimeGroup): void;
  (e: 'delete-realm', realm: WorldSaveInfo): void;
  (e: 'import-saves'): void;
  (e: 'refresh'): void;
}>();

// Search & Filter State
const searchQuery = ref<string>('');
const filterTab = ref<string>('all');
const openDropdownId = ref<string | null>(null);

// Collapsed Instances State (Instances can be collapsed/expanded via dropdown chevron)
const collapsedInstances = ref<Record<string, boolean>>({});

function toggleInstanceCollapse(instanceId: string) {
  collapsedInstances.value[instanceId] = !collapsedInstances.value[instanceId];
}

function isInstanceExpanded(instanceId: string): boolean {
  return !collapsedInstances.value[instanceId];
}

const filterTabs: TabItem[] = [
  { id: 'all', label: 'All Realms' },
  { id: 'servers', label: 'Dedicated Servers' },
  { id: 'singleplayer', label: 'Singleplayer' }
];

function toggleSplitMenu(realmFolder: string, event: MouseEvent) {
  event.stopPropagation();
  if (openDropdownId.value === realmFolder) {
    openDropdownId.value = null;
  } else {
    openDropdownId.value = realmFolder;
  }
}

function closeDropdowns() {
  openDropdownId.value = null;
}

onMounted(() => {
  window.addEventListener('click', closeDropdowns);
});

onUnmounted(() => {
  window.removeEventListener('click', closeDropdowns);
});

function getLoaderLogo(loader?: string): string {
  if (!loader) return '/loaders/vanilla.svg';
  const l = loader.toLowerCase();
  if (l.includes('neoforge')) return '/loaders/neoforge.png';
  if (l.includes('fabric')) return '/loaders/fabric.png';
  if (l.includes('quilt')) return '/loaders/quilt.png';
  if (l.includes('paper')) return '/loaders/paper.png';
  if (l.includes('purpur')) return '/loaders/purpur.png';
  if (l.includes('forge')) return '/loaders/forge.png';
  return '/loaders/vanilla.svg';
}

function formatRelativeTime(timestampSec?: number): string {
  if (!timestampSec) return 'Recent';
  const diffSec = Math.floor(Date.now() / 1000) - timestampSec;
  if (diffSec < 60) return 'Just now';
  const diffMin = Math.floor(diffSec / 60);
  if (diffMin < 60) return `${diffMin}m ago`;
  const diffHr = Math.floor(diffMin / 60);
  if (diffHr < 24) return `${diffHr}h ago`;
  const diffDays = Math.floor(diffHr / 24);
  return `${diffDays}d ago`;
}

function isRealmOnline(realm: WorldSaveInfo): boolean {
  return props.serverStatus === 'Online' && props.selectedWorldName === realm.folder_name;
}

function isRealmStarting(realm: WorldSaveInfo): boolean {
  return props.serverStatus === 'Starting' && props.selectedWorldName === realm.folder_name;
}

function getRealmsForRuntime(runtime: RuntimeGroup): WorldSaveInfo[] {
  return props.worlds.filter(w => {
    // 1. Group membership filter
    let belongs = false;
    if (w.runtime_id && w.runtime_id === runtime.id) belongs = true;
    else if (w.runtime_name && w.runtime_name.toLowerCase() === runtime.name.toLowerCase()) belongs = true;
    else {
      const fn = w.folder_name.toLowerCase();
      if (runtime.id === 'lifeore-create-s1') {
        belongs = fn.includes('lifeore');
      } else if (runtime.id === 'neoforge-1.21.1') {
        belongs = (w.loader === 'NeoForge' && !fn.includes('lifeore')) || fn.includes('smp') || fn.includes('new-world');
      } else if (runtime.id === 'vanilla-1.21.1') {
        belongs = w.loader === 'Vanilla' || fn === 'world1';
      } else if (runtime.id === 'fabric-26.2') {
        belongs = w.loader === 'Fabric';
      } else {
        belongs = w.loader?.toLowerCase() === runtime.loader?.toLowerCase();
      }
    }

    if (!belongs) return false;

    // 2. Search query filter
    if (searchQuery.value.trim()) {
      const q = searchQuery.value.toLowerCase();
      const matchName = w.display_name.toLowerCase().includes(q) || w.folder_name.toLowerCase().includes(q);
      const matchLoader = (w.loader || '').toLowerCase().includes(q);
      const matchVersion = (w.version || '').toLowerCase().includes(q);
      if (!matchName && !matchLoader && !matchVersion) return false;
    }

    // 3. Tab filter
    if (filterTab.value === 'servers') {
      return w.is_realm !== false && (w.hosting_mode === 'always_online' || w.hosting_mode === 'on_demand');
    }
    if (filterTab.value === 'singleplayer') {
      return w.hosting_mode !== 'always_online';
    }

    return true;
  });
}
</script>

<template>
  <div class="flex-1 flex flex-col h-full overflow-hidden bg-canvas select-none">
    <!-- Top Action & Navigation Toolbar -->
    <header class="h-12 border-b border-border px-6 flex items-center justify-between bg-sidebar/50 shrink-0 gap-4">
      <!-- Title & Loader/Scope Tabs -->
      <div class="flex items-center space-x-4 min-w-0">
        <div class="flex items-center gap-2 shrink-0">
          <Layers class="w-4 h-4 text-cyan-accent" />
          <h2 class="text-xs font-bold text-text-primary tracking-wide uppercase">INSTANCES &amp; REALMS</h2>
        </div>

        <div class="hidden sm:block">
          <Tabs
            v-model="filterTab"
            :tabs="filterTabs"
            variant="segmented"
            size="sm"
          />
        </div>
      </div>

      <!-- Center Search -->
      <div class="flex-1 max-w-xs">
        <Input
          v-model="searchQuery"
          placeholder="Filter realms..."
          size="sm"
        >
          <template #prefix>
            <Search class="w-3.5 h-3.5 text-text-muted" />
          </template>
        </Input>
      </div>

      <!-- Action Buttons -->
      <div class="flex items-center space-x-2 shrink-0">
        <Button
          variant="primary"
          size="sm"
          @click="$emit('create-instance')"
        >
          <template #prefix>
            <Plus class="w-3.5 h-3.5" />
          </template>
          <span>Create Instance</span>
        </Button>

        <Button
          variant="secondary"
          size="sm"
          @click="$emit('create-realm')"
        >
          <template #prefix>
            <Plus class="w-3.5 h-3.5" />
          </template>
          <span>Create Realm</span>
        </Button>

        <Button
          variant="secondary"
          size="sm"
          @click="$emit('import-saves')"
        >
          <template #prefix>
            <FolderGit2 class="w-3.5 h-3.5 text-text-muted" />
          </template>
          <span>Import Saves</span>
        </Button>

        <Button
          variant="ghost"
          size="icon"
          title="Refresh Instances"
          :disabled="isLoading"
          @click="$emit('refresh')"
        >
          <RefreshCw class="w-3.5 h-3.5 text-text-secondary" :class="isLoading ? 'animate-spin text-cyan-accent' : ''" />
        </Button>
      </div>
    </header>

    <!-- Main Scrollable Content Area -->
    <main class="flex-1 overflow-y-auto p-6 space-y-6">
      
      <!-- Quick "Jump In" Strip (Recently Played Worlds) -->
      <section v-if="jumpInRealms && jumpInRealms.length > 0" class="space-y-2.5">
        <div class="flex items-center justify-between text-[11px] font-semibold text-text-secondary uppercase tracking-wider">
          <span class="flex items-center gap-1.5 text-cyan-accent">
            <Sparkles class="w-3.5 h-3.5" />
            <span>Jump In — Recently Played</span>
          </span>
          <span class="font-mono text-text-muted text-[10px]">Quick Launch</span>
        </div>

        <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-3">
          <div
            v-for="realm in jumpInRealms"
            :key="'jump-' + realm.folder_name"
            class="bg-card hover:bg-elevated/70 border border-border hover:border-cyan-border rounded-lg p-3 flex items-center justify-between transition-colors group cursor-pointer shadow-sm"
            @click="$emit('select-realm', realm)"
          >
            <div class="flex items-center space-x-3 min-w-0 pr-2">
              <div class="w-8 h-8 rounded bg-elevated border border-border flex items-center justify-center shrink-0 overflow-hidden p-1">
                <img v-if="realm.icon_base64" :src="realm.icon_base64" alt="Icon" class="w-full h-full object-cover rounded-sm" />
                <img v-else :src="getLoaderLogo(realm.loader)" :alt="realm.loader || 'Vanilla'" class="w-full h-full object-contain" />
              </div>
              <div class="min-w-0">
                <div class="font-bold text-xs text-text-primary group-hover:text-cyan-accent transition-colors truncate">
                  {{ realm.display_name }}
                </div>
                <div class="text-[10px] text-text-muted font-mono truncate flex items-center gap-1 mt-0.5">
                  <span class="text-text-secondary">{{ realm.loader || 'Vanilla' }} {{ realm.version }}</span>
                  <span>&bull;</span>
                  <span>{{ formatRelativeTime(realm.last_played) }}</span>
                </div>
              </div>
            </div>

            <!-- Quick Launch Button -->
            <Button
              v-if="isRealmOnline(realm)"
              variant="danger"
              size="sm"
              @click.stop="$emit('stop-server', realm)"
            >
              <template #prefix>
                <Square class="w-3 h-3 fill-current" />
              </template>
              <span>Stop</span>
            </Button>
            <Button
              v-else
              variant="primary"
              size="sm"
              @click.stop="$emit('start-server-and-join', realm)"
            >
              <template #prefix>
                <Zap class="w-3 h-3 fill-current" />
              </template>
              <span>Launch</span>
            </Button>
          </div>
        </div>
      </section>

      <!-- Grouped Instances & Flat Realms Tables -->
      <section class="space-y-6">
        <div
          v-for="runtime in runtimes"
          :key="runtime.id"
          class="bg-card border border-border rounded-lg overflow-hidden shadow-sm"
        >
          <!-- Instance Group Header (Clickable Dropdown Toggle) -->
          <div
            class="px-4 py-3 bg-sidebar/70 border-b border-border flex flex-wrap items-center justify-between gap-3 cursor-pointer hover:bg-sidebar transition-colors select-none"
            @click="toggleInstanceCollapse(runtime.id)"
          >
            <div class="flex items-center space-x-3 min-w-0">
              <button
                type="button"
                class="w-5 h-5 flex items-center justify-center text-text-muted hover:text-text-primary transition-transform duration-200"
                :class="{ 'rotate-0': isInstanceExpanded(runtime.id), '-rotate-90': !isInstanceExpanded(runtime.id) }"
              >
                <ChevronDown class="w-4 h-4" />
              </button>
              <div class="w-7 h-7 rounded bg-elevated border border-border flex items-center justify-center shrink-0 p-1">
                <img :src="getLoaderLogo(runtime.loader)" :alt="runtime.loader" class="w-full h-full object-contain" />
              </div>
              <div class="flex items-baseline space-x-2 truncate">
                <h3 class="text-xs font-bold text-text-primary tracking-wide">
                  {{ runtime.name }}
                </h3>
                <Badge variant="cyan" size="sm">
                  {{ runtime.loader }} {{ runtime.version }}
                </Badge>
                <Badge variant="neutral" size="sm">
                  {{ runtime.diskSizeMb }} MB
                </Badge>
              </div>
            </div>

            <div class="flex items-center space-x-1.5" @click.stop>
              <span class="text-[11px] text-text-muted font-mono mr-2">
                {{ getRealmsForRuntime(runtime).length }} {{ getRealmsForRuntime(runtime).length === 1 ? 'Realm' : 'Realms' }}
              </span>
              <Button
                variant="ghost"
                size="sm"
                title="Browse & Install Mods for this instance"
                @click="$emit('add-mods', runtime)"
              >
                <template #prefix>
                  <Plus class="w-3 h-3 text-cyan-accent" />
                </template>
                <span>Add Mods</span>
              </Button>
              <Button
                variant="ghost"
                size="sm"
                title="Open instance mods folder in Windows Explorer"
                @click="$emit('open-mods-folder', runtime)"
              >
                <template #prefix>
                  <Folder class="w-3 h-3 text-text-secondary" />
                </template>
                <span>Mods Folder</span>
              </Button>
              <Button
                variant="ghost"
                size="sm"
                @click="$emit('create-realm', runtime)"
              >
                <template #prefix>
                  <Plus class="w-3 h-3" />
                </template>
                <span>Add Realm</span>
              </Button>
              <Button
                variant="ghost"
                size="sm"
                title="Delete this instance"
                class="hover:text-status-error hover:bg-status-error/10"
                @click="$emit('delete-instance', runtime)"
              >
                <template #prefix>
                  <Trash2 class="w-3 h-3" />
                </template>
                <span>Delete</span>
              </Button>
            </div>
          </div>

          <!-- Collapsible Realms Table -->
          <div v-show="isInstanceExpanded(runtime.id)" class="overflow-x-auto transition-all">
            <table class="w-full text-left text-xs border-collapse">
              <thead>
                <tr class="border-b border-border/80 bg-sidebar/20 text-[10px] font-semibold text-text-muted uppercase tracking-wider">
                  <th class="py-2.5 px-4 font-medium">Realm / Name</th>
                  <th class="py-2.5 px-4 font-medium">Status</th>
                  <th class="py-2.5 px-4 font-medium">Players</th>
                  <th class="py-2.5 px-4 font-medium">Last Played</th>
                  <th class="py-2.5 px-4 font-medium text-right">Actions</th>
                </tr>
              </thead>
              <tbody class="divide-y divide-border/40">
                <tr
                  v-for="realm in getRealmsForRuntime(runtime)"
                  :key="realm.folder_name"
                  class="hover:bg-elevated/40 transition-colors cursor-pointer group"
                  @click="$emit('select-realm', realm)"
                >
                  <!-- 1. Icon + Name + Version -->
                  <td class="py-3 px-4 min-w-[220px]">
                    <div class="flex items-center space-x-3">
                      <div class="w-8 h-8 rounded bg-elevated border border-border flex items-center justify-center shrink-0 overflow-hidden p-1">
                        <img v-if="realm.icon_base64" :src="realm.icon_base64" alt="Icon" class="w-full h-full object-cover rounded-sm" />
                        <img v-else :src="getLoaderLogo(realm.loader)" class="w-full h-full object-contain" />
                      </div>
                      <div class="min-w-0">
                        <div class="font-semibold text-text-primary group-hover:text-cyan-accent transition-colors truncate">
                          {{ realm.display_name }}
                        </div>
                        <div class="text-[10px] text-text-muted flex items-center gap-1.5 mt-0.5">
                          <span class="font-mono text-text-secondary">{{ realm.loader || 'Vanilla' }} {{ realm.version }}</span>
                          <span>&bull;</span>
                          <span class="capitalize">{{ realm.game_mode || 'Survival' }}</span>
                        </div>
                      </div>
                    </div>
                  </td>

                  <!-- 2. Status Pill with StatusDot -->
                  <td class="py-3 px-4 whitespace-nowrap">
                    <div class="flex items-center gap-2">
                      <Badge
                        v-if="isRealmOnline(realm)"
                        variant="success"
                        dot
                        size="sm"
                      >
                        Online :25565
                      </Badge>
                      <Badge
                        v-else-if="isRealmStarting(realm)"
                        variant="cyan"
                        dot
                        size="sm"
                      >
                        Starting...
                      </Badge>
                      <Badge
                        v-else
                        variant="neutral"
                        dot
                        size="sm"
                      >
                        Offline
                      </Badge>

                      <span
                        v-if="realm.hosting_mode === 'always_online'"
                        class="text-[10px] text-orange-accent/80 font-mono"
                        title="Dedicated server configured to stay online"
                      >
                        Always Online
                      </span>
                    </div>
                  </td>

                  <!-- 3. Player Count -->
                  <td class="py-3 px-4 whitespace-nowrap text-text-secondary font-mono">
                    <div class="flex items-center gap-1.5">
                      <Users class="w-3.5 h-3.5 text-text-muted" />
                      <span>{{ isRealmOnline(realm) ? '0 / 20' : '-' }}</span>
                    </div>
                  </td>

                  <!-- 4. Last Played -->
                  <td class="py-3 px-4 whitespace-nowrap text-text-secondary font-mono">
                    <div class="flex items-center gap-1.5">
                      <Clock class="w-3.5 h-3.5 text-text-muted" />
                      <span>{{ formatRelativeTime(realm.last_played) }}</span>
                    </div>
                  </td>

                  <!-- 5. Primary Action + Split Menu -->
                  <td class="py-3 px-4 whitespace-nowrap text-right" @click.stop>
                    <div class="inline-flex items-center relative">
                      <!-- Primary Action CTA -->
                      <Button
                        v-if="isRealmOnline(realm)"
                        variant="danger"
                        size="sm"
                        class="rounded-r-none border-r border-black/20"
                        @click="$emit('stop-server', realm)"
                      >
                        <template #prefix>
                          <Square class="w-3 h-3 fill-current" />
                        </template>
                        <span>Stop Server</span>
                      </Button>
                      <Button
                        v-else
                        variant="primary"
                        size="sm"
                        class="rounded-r-none border-r border-black/20"
                        @click="$emit('start-server-and-join', realm)"
                      >
                        <template #prefix>
                          <Zap class="w-3.5 h-3.5" />
                        </template>
                        <span>Start &amp; Join</span>
                      </Button>

                      <!-- Split Menu Dropdown Toggle -->
                      <button
                        type="button"
                        :class="[
                          'h-7 px-1.5 rounded-r border border-l-0 text-text-secondary hover:text-text-primary cursor-pointer transition-colors flex items-center justify-center',
                          isRealmOnline(realm) ? 'bg-status-error text-white hover:bg-[#c94343] border-status-error' : 'bg-orange-accent text-[#11151A] hover:bg-orange-hover border-orange-accent'
                        ]"
                        title="More Launch Options"
                        @click="toggleSplitMenu(realm.folder_name, $event)"
                      >
                        <ChevronDown class="w-3.5 h-3.5" />
                      </button>

                      <!-- Dropdown Menu -->
                      <div
                        v-if="openDropdownId === realm.folder_name"
                        class="absolute right-0 top-full mt-1 w-52 bg-elevated border border-border rounded-md shadow-2xl py-1 z-30 text-left text-xs divide-y divide-border/50"
                      >
                        <div class="py-1">
                          <button
                            type="button"
                            class="w-full px-3 py-1.5 text-left text-text-primary hover:bg-card hover:text-cyan-accent flex items-center gap-2 cursor-pointer transition-colors"
                            @click="closeDropdowns(); $emit('start-server', realm)"
                          >
                            <Terminal class="w-3.5 h-3.5 text-text-muted" />
                            <span>Run Server in Background</span>
                          </button>

                          <button
                            type="button"
                            class="w-full px-3 py-1.5 text-left text-text-primary hover:bg-card hover:text-cyan-accent flex items-center gap-2 cursor-pointer transition-colors"
                            @click="closeDropdowns(); $emit('play-singleplayer', realm)"
                          >
                            <Play class="w-3.5 h-3.5 text-text-muted" />
                            <span>Solo Play (Singleplayer)</span>
                          </button>
                        </div>

                        <div class="py-1">
                          <button
                            type="button"
                            class="w-full px-3 py-1.5 text-left text-text-secondary hover:bg-card hover:text-text-primary flex items-center gap-2 cursor-pointer transition-colors"
                            @click="closeDropdowns(); $emit('select-realm', realm)"
                          >
                            <Settings class="w-3.5 h-3.5 text-text-muted" />
                            <span>Configure Realm</span>
                          </button>

                          <button
                            type="button"
                            class="w-full px-3 py-1.5 text-left text-text-secondary hover:bg-card hover:text-text-primary flex items-center gap-2 cursor-pointer transition-colors"
                            @click="closeDropdowns(); $emit('open-folder', realm)"
                          >
                            <Folder class="w-3.5 h-3.5 text-text-muted" />
                            <span>Open Folder</span>
                          </button>

                          <button
                            type="button"
                            class="w-full px-3 py-1.5 text-left text-status-error hover:bg-status-error/10 flex items-center gap-2 cursor-pointer transition-colors"
                            @click="closeDropdowns(); $emit('delete-realm', realm)"
                          >
                            <Trash2 class="w-3.5 h-3.5" />
                            <span>Delete Realm</span>
                          </button>
                        </div>
                      </div>
                    </div>
                  </td>
                </tr>

                <!-- Empty State for this Runtime Group -->
                <tr v-if="getRealmsForRuntime(runtime).length === 0">
                  <td colspan="5" class="py-6 px-4 text-center text-text-muted font-mono text-xs">
                    <p>No realms found in {{ runtime.name }}.</p>
                    <Button
                      variant="ghost"
                      size="sm"
                      class="mt-2 text-cyan-accent"
                      @click="$emit('create-realm', runtime)"
                    >
                      <template #prefix>
                        <Plus class="w-3 h-3" />
                      </template>
                      <span>Create First Realm</span>
                    </Button>
                  </td>
                </tr>
              </tbody>
            </table>
          </div>
        </div>
      </section>

    </main>
  </div>
</template>
