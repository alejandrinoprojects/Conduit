<script setup lang="ts">
import { ref, computed, watch, onMounted } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import {
  Search,
  Download,
  Package,
  RefreshCw,
  ExternalLink,
  Filter,
  Check,
  Tag,
  WifiOff,
  AlertTriangle,
  Layers,
  Sparkles,
  SlidersHorizontal,
  X,
  Users
} from 'lucide-vue-next';
import { Button, Input, Badge, StatusDot } from '../components/common';
import { useServerState } from '../composables/useServerState';
import type { ModrinthProject, InstalledMod, VersionMetaItem } from '../composables/useConduitIPC';

export interface ModrinthProjectItem extends ModrinthProject {
  follows?: number;
  loaders?: string[];
  date_modified?: string;
}

export interface DiscoverViewProps {
  instanceDir?: string;
  selectedLoader?: string;
  selectedVersion?: string;
  installedMods?: InstalledMod[];
  allVersions?: VersionMetaItem[];
  initialQuery?: string;
}

const props = withDefaults(defineProps<DiscoverViewProps>(), {
  instanceDir: '',
  selectedLoader: 'neoforge',
  selectedVersion: '1.21.1',
  installedMods: () => [],
  allVersions: () => [],
  initialQuery: '',
});

const emit = defineEmits<{
  (e: 'install-project', project: ModrinthProject): void;
  (e: 'refresh-mods'): void;
  (e: 'update:search-query', query: string): void;
}>();

const { toastSuccess, toastError, toastInfo } = useServerState();

// Primary content categories
export type ProjectCategory = 'mod' | 'modpack' | 'resourcepack' | 'shader';
const activeCategory = ref<ProjectCategory>('mod');

const categoryTabs: { id: ProjectCategory; label: string; icon: any }[] = [
  { id: 'mod', label: 'Mods', icon: Package },
  { id: 'modpack', label: 'Modpacks', icon: Layers },
  { id: 'resourcepack', label: 'Resource Packs', icon: Sparkles },
  { id: 'shader', label: 'Shaders', icon: Sparkles },
];

// Search & Filter state
const searchQuery = ref<string>(props.initialQuery || '');
const currentLoader = ref<string>(props.selectedLoader ? props.selectedLoader.toLowerCase() : 'neoforge');
const currentVersion = ref<string>(props.selectedVersion || '1.21.1');
const currentSort = ref<'relevance' | 'downloads' | 'follows' | 'newest' | 'updated'>('relevance');
const activeFacetTag = ref<string>('all');

// Functional facet tags
const facetTags = [
  { id: 'all', label: 'All Tags' },
  { id: 'technology', label: 'Technology' },
  { id: 'magic', label: 'Magic' },
  { id: 'adventure', label: 'Adventure' },
  { id: 'optimization', label: 'Optimization' },
  { id: 'storage', label: 'Storage' },
  { id: 'library', label: 'Library' },
  { id: 'worldgen', label: 'World Gen' },
  { id: 'utility', label: 'Utility' },
  { id: 'decoration', label: 'Decoration' },
];

// Loaders options
const loaderOptions = [
  { id: 'all', label: 'All Loaders' },
  { id: 'neoforge', label: 'NeoForge' },
  { id: 'fabric', label: 'Fabric' },
  { id: 'forge', label: 'Forge' },
  { id: 'quilt', label: 'Quilt' },
];

// Common Minecraft versions fallback if allVersions is empty
const defaultVersions = [
  '1.21.1',
  '1.21',
  '1.20.6',
  '1.20.4',
  '1.20.1',
  '1.19.4',
  '1.19.2',
  '1.18.2',
  '1.16.5',
];

const availableVersions = computed<string[]>(() => {
  if (props.allVersions && props.allVersions.length > 0) {
    const releases = props.allVersions
      .filter((v) => v.release_type === 'release')
      .map((v) => v.id)
      .slice(0, 25);
    if (releases.length > 0) return ['all', ...releases];
  }
  return ['all', ...defaultVersions];
});

// Search results & Network state
const projects = ref<ModrinthProjectItem[]>([]);
const isSearching = ref<boolean>(false);
const isOffline = ref<boolean>(false);
const searchError = ref<string | null>(null);
const installingMap = ref<Record<string, boolean>>({});

// Debounce handle
let debounceTimeout: any = null;

function triggerDebouncedSearch() {
  emit('update:search-query', searchQuery.value);
  if (debounceTimeout) clearTimeout(debounceTimeout);
  debounceTimeout = setTimeout(() => {
    executeSearch();
  }, 320);
}

// Modrinth Search Execution
async function executeSearch() {
  isSearching.value = true;
  isOffline.value = false;
  searchError.value = null;

  try {
    const res: any = await invoke('search_modrinth', {
      query: searchQuery.value.trim(),
      projectType: activeCategory.value,
      loader: currentLoader.value !== 'all' ? currentLoader.value.toLowerCase() : undefined,
      gameVersion: currentVersion.value !== 'all' ? currentVersion.value : undefined,
      category: activeFacetTag.value !== 'all' ? activeFacetTag.value : undefined,
      index: currentSort.value,
      limit: 28,
    });

    const items = Array.isArray(res) ? res : (res?.hits || []);
    projects.value = items.map((h: any) => ({
      project_id: h.project_id || h.id || h.slug,
      title: h.title,
      description: h.description,
      icon_url: h.icon_url,
      downloads: Number(h.downloads || 0),
      client_side: h.client_side || 'optional',
      server_side: h.server_side || 'optional',
      author: h.author || '',
      categories: h.categories || [],
      follows: Number(h.follows || 0),
      date_modified: h.date_modified,
      loaders: h.loaders || [],
    }));
  } catch (err: any) {
    console.warn('Modrinth search failed:', err);
    isOffline.value = true;
    searchError.value = err?.message || 'Failed connecting to Modrinth API catalog.';
    projects.value = [];
  } finally {
    isSearching.value = false;
  }
}

// Primary install CTA handler
async function install_mod_version(project: ModrinthProject) {
  const pId = project.project_id;
  installingMap.value[pId] = true;

  try {
    await invoke('install_modrinth_project', {
      instanceDir: props.instanceDir || '',
      projectId: pId,
      projectType: activeCategory.value,
      loader: currentLoader.value !== 'all' ? currentLoader.value.toLowerCase() : 'neoforge',
      gameVersion: currentVersion.value !== 'all' ? currentVersion.value : '1.21.1',
    });

    toastSuccess('Installation Complete', `Successfully installed ${project.title} to instance.`);
    emit('install-project', project);
    emit('refresh-mods');
  } catch (err: any) {
    const msg = err?.message || String(err);
    toastError('Installation Failed', `Failed to install ${project.title}: ${msg}`);
  } finally {
    installingMap.value[pId] = false;
  }
}

// Helper to determine if mod is already installed in active realm
function isProjectInstalled(project: ModrinthProject): boolean {
  if (!props.installedMods || props.installedMods.length === 0) return false;
  const pId = (project.project_id || '').toLowerCase();
  const title = project.title.toLowerCase();

  return props.installedMods.some((m) => {
    const fileName = m.file_name.toLowerCase();
    const modId = (m.mod_id || '').toLowerCase();
    const modName = (m.mod_name || '').toLowerCase();
    return (
      (pId && (modId === pId || fileName.includes(pId))) ||
      (modName && modName === title) ||
      fileName.includes(title.replace(/\s+/g, '-')) ||
      fileName.includes(title.replace(/\s+/g, '_'))
    );
  });
}

function formatNumber(num: number): string {
  if (!num) return '0';
  if (num >= 1000000) return (num / 1000000).toFixed(1) + 'M';
  if (num >= 1000) return (num / 1000).toFixed(1) + 'K';
  return num.toLocaleString();
}

function handleSelectCategory(cat: ProjectCategory) {
  activeCategory.value = cat;
  executeSearch();
}

function handleSelectFacet(facetId: string) {
  activeFacetTag.value = facetId;
  executeSearch();
}

function handleClearSearch() {
  searchQuery.value = '';
  triggerDebouncedSearch();
}

// React to prop changes
watch(
  () => props.selectedLoader,
  (newVal) => {
    if (newVal) {
      currentLoader.value = newVal.toLowerCase();
      executeSearch();
    }
  }
);

watch(
  () => props.selectedVersion,
  (newVal) => {
    if (newVal) {
      currentVersion.value = newVal;
      executeSearch();
    }
  }
);

onMounted(() => {
  executeSearch();
});
</script>

<template>
  <div class="flex-1 flex flex-col h-full overflow-hidden bg-canvas text-text-primary">
    <!-- Top Action / Category Toolbar -->
    <div class="h-12 border-b border-border px-6 flex items-center justify-between bg-sidebar/70 shrink-0">
      <!-- Title & Category Tabs -->
      <div class="flex items-center space-x-4">
        <div class="flex items-center space-x-2">
          <Compass class="w-4 h-4 text-cyan-accent" />
          <h2 class="text-xs font-bold tracking-wider text-text-primary uppercase">Discover Content</h2>
        </div>

        <div class="h-4 w-px bg-border" />

        <div class="flex items-center space-x-1">
          <button
            v-for="cat in categoryTabs"
            :key="cat.id"
            @click="handleSelectCategory(cat.id)"
            class="px-2.5 py-1 rounded text-xs font-medium transition-colors cursor-pointer flex items-center space-x-1.5"
            :class="
              activeCategory === cat.id
                ? 'bg-cyan-accent text-canvas font-semibold shadow-sm'
                : 'bg-card text-text-secondary hover:text-text-primary hover:bg-elevated border border-border/80'
            "
          >
            <component :is="cat.icon" class="w-3.5 h-3.5" />
            <span>{{ cat.label }}</span>
          </button>
        </div>
      </div>

      <!-- Live Search & Sort Controls -->
      <div class="flex items-center space-x-3">
        <!-- Live debounced search input -->
        <div class="relative w-64">
          <Search class="w-3.5 h-3.5 absolute left-2.5 top-1/2 -translate-y-1/2 text-text-muted pointer-events-none" />
          <input
            type="text"
            v-model="searchQuery"
            @input="triggerDebouncedSearch"
            @keyup.enter="executeSearch"
            placeholder="Search Modrinth catalog..."
            class="w-full h-8 pl-8 pr-7 bg-card border border-border rounded text-xs text-text-primary placeholder:text-text-muted focus:outline-none focus:border-cyan-accent focus:ring-1 focus:ring-cyan-accent/40 transition-all font-sans"
          />
          <button
            v-if="searchQuery"
            @click="handleClearSearch"
            class="absolute right-2 top-1/2 -translate-y-1/2 text-text-muted hover:text-text-primary p-0.5 cursor-pointer transition-colors"
          >
            <X class="w-3 h-3" />
          </button>
        </div>

        <!-- Refresh CTA -->
        <Button
          variant="secondary"
          size="sm"
          :disabled="isSearching"
          @click="executeSearch"
          title="Refresh search"
        >
          <template #prefix>
            <RefreshCw class="w-3.5 h-3.5" :class="isSearching ? 'animate-spin text-cyan-accent' : ''" />
          </template>
        </Button>
      </div>
    </div>

    <!-- Secondary Filter Bar (Loaders, Versions, Sort, Facets) -->
    <div class="border-b border-border bg-sidebar/30 px-6 py-2.5 flex flex-wrap items-center justify-between gap-3 shrink-0">
      <!-- Left: Loader & Version Selectors -->
      <div class="flex items-center space-x-3">
        <!-- Loader Chips -->
        <div class="flex items-center space-x-1 text-xs">
          <span class="text-[11px] font-medium text-text-muted uppercase tracking-wide mr-1.5">Loader:</span>
          <button
            v-for="loader in loaderOptions"
            :key="loader.id"
            @click="currentLoader = loader.id; executeSearch()"
            class="px-2 py-0.5 rounded text-[11px] transition-colors cursor-pointer border"
            :class="
              currentLoader === loader.id
                ? 'bg-elevated text-cyan-accent border-cyan-accent/50 font-medium'
                : 'bg-card text-text-secondary border-border/60 hover:text-text-primary hover:bg-elevated/50'
            "
          >
            {{ loader.label }}
          </button>
        </div>

        <div class="h-3.5 w-px bg-border" />

        <!-- Minecraft Version Dropdown -->
        <div class="flex items-center space-x-1.5 text-xs">
          <span class="text-[11px] font-medium text-text-muted uppercase tracking-wide">Version:</span>
          <select
            v-model="currentVersion"
            @change="executeSearch"
            class="h-7 px-2 bg-card border border-border rounded text-xs text-text-primary focus:outline-none focus:border-cyan-accent cursor-pointer transition-colors"
          >
            <option v-for="ver in availableVersions" :key="ver" :value="ver">
              {{ ver === 'all' ? 'All Versions' : ver }}
            </option>
          </select>
        </div>

        <div class="h-3.5 w-px bg-border" />

        <!-- Sort Option -->
        <div class="flex items-center space-x-1.5 text-xs">
          <span class="text-[11px] font-medium text-text-muted uppercase tracking-wide">Sort:</span>
          <select
            v-model="currentSort"
            @change="executeSearch"
            class="h-7 px-2 bg-card border border-border rounded text-xs text-text-primary focus:outline-none focus:border-cyan-accent cursor-pointer transition-colors"
          >
            <option value="relevance">Relevance</option>
            <option value="downloads">Most Downloads</option>
            <option value="follows">Most Follows</option>
            <option value="newest">Recently Published</option>
            <option value="updated">Recently Updated</option>
          </select>
        </div>
      </div>

      <!-- Right: Category Tag Filter Chips -->
      <div class="flex items-center space-x-1 overflow-x-auto max-w-full pb-0.5">
        <button
          v-for="facet in facetTags"
          :key="facet.id"
          @click="handleSelectFacet(facet.id)"
          class="px-2 py-0.5 rounded text-[10px] whitespace-nowrap transition-colors cursor-pointer border"
          :class="
            activeFacetTag === facet.id
              ? 'bg-cyan-accent/15 text-cyan-accent border-cyan-accent/40 font-semibold'
              : 'bg-card/70 text-text-muted border-border/50 hover:text-text-secondary hover:bg-elevated/40'
          "
        >
          {{ facet.label }}
        </button>
      </div>
    </div>

    <!-- Main Content Area: Project Cards Grid -->
    <div class="flex-1 overflow-y-auto p-6">
      <!-- Loading State -->
      <div v-if="isSearching" class="py-24 flex flex-col items-center justify-center space-y-3">
        <RefreshCw class="w-7 h-7 text-cyan-accent animate-spin" />
        <span class="text-xs text-text-secondary font-mono tracking-wide">Querying Modrinth catalog...</span>
      </div>

      <!-- Offline / Error State -->
      <div
        v-else-if="isOffline"
        class="max-w-md mx-auto my-12 p-6 bg-card border border-border rounded-xl text-center space-y-3 shadow-lg"
      >
        <div class="w-12 h-12 mx-auto rounded-full bg-status-error/15 border border-status-error/30 flex items-center justify-center text-status-error">
          <WifiOff class="w-6 h-6" />
        </div>
        <div>
          <h3 class="text-sm font-bold text-text-primary">Modrinth Catalog Offline</h3>
          <p class="text-xs text-text-secondary mt-1 leading-relaxed">
            {{ searchError || 'Unable to connect to api.modrinth.com. Please verify your internet connection.' }}
          </p>
        </div>
        <div class="pt-2">
          <Button variant="secondary" size="sm" @click="executeSearch">
            <template #prefix>
              <RefreshCw class="w-3.5 h-3.5" />
            </template>
            <span>Retry Connection</span>
          </Button>
        </div>
      </div>

      <!-- Empty Results State -->
      <div
        v-else-if="projects.length === 0"
        class="max-w-md mx-auto my-16 p-8 bg-card border border-border rounded-xl text-center space-y-3"
      >
        <Package class="w-8 h-8 text-text-muted mx-auto" />
        <h3 class="text-sm font-semibold text-text-primary">No Projects Found</h3>
        <p class="text-xs text-text-secondary">
          No matching {{ activeCategory }}s found for your current search criteria. Try modifying your search query or filters.
        </p>
        <div class="pt-2 flex justify-center gap-2">
          <Button variant="ghost" size="sm" @click="handleClearSearch">Clear Filters</Button>
        </div>
      </div>

      <!-- Desktop-Dense Project Cards Grid -->
      <div v-else class="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-3.5">
        <div
          v-for="project in projects"
          :key="project.project_id"
          class="bg-card hover:bg-elevated/70 border border-border hover:border-cyan-accent/40 rounded-xl p-4 flex flex-col justify-between transition-all group shadow-sm"
        >
          <!-- Top: Icon + Info -->
          <div>
            <div class="flex items-start space-x-3">
              <!-- Project Icon -->
              <img
                v-if="project.icon_url"
                :src="project.icon_url"
                alt="Icon"
                loading="lazy"
                class="w-12 h-12 rounded-lg object-cover bg-sidebar border border-border/80 shrink-0"
              />
              <div
                v-else
                class="w-12 h-12 rounded-lg bg-sidebar border border-border/80 flex items-center justify-center shrink-0 text-cyan-accent"
              >
                <Package class="w-6 h-6" />
              </div>

              <!-- Title, Author, Side Support -->
              <div class="flex-1 min-w-0">
                <div class="flex items-center justify-between gap-1.5">
                  <h3 class="text-xs font-bold text-text-primary group-hover:text-cyan-accent transition-colors truncate">
                    {{ project.title }}
                  </h3>
                  <!-- Client/Server Side Badge -->
                  <span
                    v-if="project.server_side === 'required' || project.client_side === 'required'"
                    class="text-[9px] uppercase font-mono px-1.5 py-0.5 rounded bg-sidebar border border-border text-text-muted shrink-0"
                  >
                    {{ project.server_side === 'required' ? 'Server' : 'Client' }}
                  </span>
                </div>

                <div class="text-[11px] text-text-muted truncate mt-0.5">
                  by <span class="text-text-secondary font-medium">{{ project.author || 'Modrinth Creator' }}</span>
                </div>
              </div>
            </div>

            <!-- Description / Summary -->
            <p class="text-xs text-text-secondary line-clamp-2 mt-2.5 leading-relaxed">
              {{ project.description || 'No description provided for this project.' }}
            </p>

            <!-- Categories / Tags -->
            <div class="flex flex-wrap gap-1 mt-2.5">
              <Badge
                v-for="cat in (project.categories || []).slice(0, 3)"
                :key="cat"
                variant="neutral"
                size="sm"
                class="text-[10px]"
              >
                {{ cat }}
              </Badge>
            </div>
          </div>

          <!-- Bottom: Metrics & Primary Install Action -->
          <div class="mt-4 pt-3 border-t border-border/50 flex items-center justify-between">
            <!-- Downloads & Follows Metrics -->
            <div class="flex items-center space-x-3 text-[10px] text-text-muted font-mono">
              <span title="Total Downloads">
                <span class="text-text-secondary font-semibold">{{ formatNumber(project.downloads) }}</span> dl
              </span>
              <span v-if="project.follows" title="Total Followers">
                &bull; <span class="text-text-secondary font-semibold">{{ formatNumber(project.follows) }}</span> stars
              </span>
            </div>

            <!-- Action Button: Primary Orange Install CTA (#F39A36) -->
            <div class="flex items-center space-x-1.5 shrink-0">
              <Badge
                v-if="isProjectInstalled(project)"
                variant="cyan"
                size="sm"
                class="font-medium"
              >
                <Check class="w-3 h-3 mr-1" />
                <span>Installed</span>
              </Badge>

              <Button
                variant="primary"
                size="sm"
                :disabled="installingMap[project.project_id]"
                :loading="installingMap[project.project_id]"
                @click="install_mod_version(project)"
                title="Install latest compatible version to realm"
              >
                <template #prefix>
                  <Download class="w-3.5 h-3.5" :class="installingMap[project.project_id] ? 'animate-bounce' : ''" />
                </template>
                <span>{{ isProjectInstalled(project) ? 'Reinstall' : 'Install' }}</span>
              </Button>
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>
