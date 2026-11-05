<script setup lang="ts">
import { ref, computed, watch, nextTick } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import {
  ArrowLeft,
  Terminal,
  Users,
  Archive,
  Package,
  Radio,
  Sliders,
  Play,
  Square,
  RotateCw,
  Search,
  Check,
  Copy,
  Trash2,
  RefreshCw,
  Cpu,
  Zap,
  Clock,
  Layers,
  ExternalLink,
  Folder,
  Shield,
  MoreVertical,
  AlertTriangle,
  ArrowDown,
  HardDrive
} from 'lucide-vue-next';
import {
  Button,
  Input,
  Badge,
  StatusDot,
  Modal,
  Tabs,
  MetricCard,
  type TabItem
} from '../components/common';
import { useServerState } from '../composables/useServerState';
import ModsManagerView from './ModsManagerView.vue';
import type {
  WorldSaveInfo,
  ServerStatusType,
  ClientStatusType,
  InstalledMod,
  ModVersionOption,
  DeduplicationMetrics
} from '../composables';

export interface OnlinePlayer {
  name: string;
  uuid: string;
  ping: number;
  role: 'Operator' | 'Player';
  status: 'Online' | 'Whitelisted' | 'Banned';
  is_connected: boolean;
  joinedAt: string;
}

export interface BackupItem {
  id: string;
  name: string;
  timestamp: string;
  sizeMb: number;
  type: 'Automatic' | 'Manual';
}

export interface RealmDetailViewProps {
  realm: WorldSaveInfo;
  serverStatus: ServerStatusType;
  clientStatus: ClientStatusType;
  tunnelUrl?: string | null;
  publicIp?: string | null;
  hostRoomCode?: string | null;
  logs: string[];
  currentMspt: number;
  currentTps: number;
  msptHistory: number[];
  tpsHistory: number[];
  jvmUsedHeapMb?: number;
  loadedEntities?: number;
  loadedChunks?: number;
  mainTickThreadUsage?: number;
  totalHostCpuUsage?: number;
  installedMods: InstalledMod[];
  isModsLoading?: boolean;
  dedupMetrics?: DeduplicationMetrics | null;
  backups?: BackupItem[];
  players?: OnlinePlayer[];
  instanceDir?: string;
  serverRamGb?: number;
  clientRamGb?: number;
}

const props = withDefaults(defineProps<RealmDetailViewProps>(), {
  tunnelUrl: '',
  publicIp: '',
  hostRoomCode: '',
  jvmUsedHeapMb: 1840,
  loadedEntities: 312,
  loadedChunks: 48,
  mainTickThreadUsage: 22,
  totalHostCpuUsage: 14,
  isModsLoading: false,
  dedupMetrics: null,
  backups: () => [],
  players: () => [],
  instanceDir: './instance',
  serverRamGb: 4,
  clientRamGb: 4
});

const emit = defineEmits<{
  (e: 'back'): void;
  (e: 'start-server-and-join'): void;
  (e: 'start-server'): void;
  (e: 'stop-server'): void;
  (e: 'join-local-server'): void;
  (e: 'launch-singleplayer'): void;
  (e: 'open-folder'): void;
  (e: 'send-command', command: string): void;
  (e: 'toggle-mod', mod: InstalledMod): void;
  (e: 'delete-mod', fileName: string): void;
  (e: 'refresh-mods'): void;
  (e: 'create-backup'): void;
  (e: 'kick-player', name: string): void;
  (e: 'ban-player', name: string): void;
  (e: 'pardon-player', name: string): void;
  (e: 'op-player', name: string): void;
  (e: 'deop-player', name: string): void;
  (e: 'delete-player-data', payload: { uuid: string; scope: string }): void;
  (e: 'update:serverRamGb', val: number): void;
  (e: 'update:clientRamGb', val: number): void;
}>();

// Navigation Subtabs
type SubTabId = 'console' | 'properties' | 'world' | 'mods' | 'players' | 'network';
const activeSubtab = ref<SubTabId>('console');

const subtabs = computed<TabItem[]>(() => [
  { id: 'console', label: 'Console & Telemetry', icon: Terminal },
  { id: 'properties', label: 'Server Properties', icon: Sliders },
  { id: 'world', label: 'World & Saves', icon: Archive },
  { id: 'mods', label: `Mods (${props.installedMods.length})`, icon: Package },
  { id: 'players', label: `Players (${props.players.length})`, icon: Users },
  { id: 'network', label: 'Conduit Network', icon: Radio },
]);

// Copy feedback states
const copiedIp = ref(false);
const copiedPublicIp = ref(false);
const copiedRoomCode = ref(false);

const displayServerIp = computed(() => {
  return '127.0.0.1:25565';
});

const displayPublicIp = computed(() => {
  if (props.publicIp && !props.publicIp.startsWith('CND-')) {
    return props.publicIp.includes(':') ? props.publicIp : `${props.publicIp}:25565`;
  }
  if (props.tunnelUrl && !props.tunnelUrl.startsWith('CND-') && props.tunnelUrl !== '127.0.0.1:25565') {
    return props.tunnelUrl.includes(':') ? props.tunnelUrl : `${props.tunnelUrl}:25565`;
  }
  return null;
});

async function copyIp() {
  const ip = displayServerIp.value;
  await navigator.clipboard.writeText(ip);
  copiedIp.value = true;
  setTimeout(() => { copiedIp.value = false; }, 2000);
}

async function copyPublicIp() {
  const ip = displayPublicIp.value;
  if (!ip) return;
  await navigator.clipboard.writeText(ip);
  copiedPublicIp.value = true;
  setTimeout(() => { copiedPublicIp.value = false; }, 2000);
}

async function copyRoomCode() {
  const code = props.hostRoomCode || ('CND-' + props.realm.folder_name.toUpperCase());
  await navigator.clipboard.writeText(code);
  copiedRoomCode.value = true;
  setTimeout(() => { copiedRoomCode.value = false; }, 2000);
}

// Server Console & Terminal State
const consoleInput = ref('');
const logFilter = ref<'all' | 'errors' | 'warnings' | 'info'>('all');
const autoScroll = ref(true);
const consoleOutputRef = ref<HTMLElement | null>(null);

const filteredLogs = computed(() => {
  if (logFilter.value === 'all') return props.logs;
  const f = logFilter.value;
  return props.logs.filter(l => {
    const lower = l.toLowerCase();
    if (f === 'errors') return lower.includes('error') || lower.includes('fatal') || lower.includes('exception');
    if (f === 'warnings') return lower.includes('warn');
    if (f === 'info') return !lower.includes('error') && !lower.includes('warn');
    return true;
  });
});

function handleConsoleScroll() {
  if (!consoleOutputRef.value) return;
  const el = consoleOutputRef.value;
  const isNearBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 50;
  autoScroll.value = isNearBottom;
}

function resumeLiveTail() {
  autoScroll.value = true;
  if (consoleOutputRef.value) {
    consoleOutputRef.value.scrollTop = consoleOutputRef.value.scrollHeight;
  }
}

watch(
  () => props.logs.length,
  () => {
    if (autoScroll.value) {
      nextTick(() => {
        if (consoleOutputRef.value) {
          consoleOutputRef.value.scrollTop = consoleOutputRef.value.scrollHeight;
        }
      });
    }
  }
);

async function handleSendCommand() {
  const cmd = consoleInput.value.trim();
  if (!cmd) return;
  consoleInput.value = '';
  try {
    await invoke('send_server_command', { command: cmd });
  } catch (err) {
    console.error('Failed to send server command:', err);
  }
  emit('send-command', cmd);
}

// Restart server helper
function handleRestartServer() {
  emit('stop-server');
  setTimeout(() => {
    emit('start-server');
  }, 1600);
}

const { toastSuccess, toastError, toastInfo } = useServerState();

// Server Properties & Memory Settings
const serverRamGb = ref<number>(props.serverRamGb ?? 4);
const clientRamGb = ref<number>(props.clientRamGb ?? 4);

watch(serverRamGb, (val) => {
  emit('update:serverRamGb', val);
});
watch(clientRamGb, (val) => {
  emit('update:clientRamGb', val);
});
const serverMotd = ref<string>(`A Minecraft Realm hosted with Conduit - ${props.realm.display_name}`);
const serverDifficulty = ref<string>('normal');
const serverGameMode = ref<string>(props.realm.game_mode || 'survival');
const serverPvp = ref<boolean>(true);
const maxPlayers = ref<number>(20);
const viewDistance = ref<number>(10);
const simulationDistance = ref<number>(8);

// World & Saves sub-state
const worldSection = ref<'backups' | 'pruner'>('backups');

// Player Management & MODAL RESTORATION
const showPlayerActionsModal = ref<boolean>(false);
const showDeleteDataConfirmModal = ref<boolean>(false);
const selectedPlayerAction = ref<OnlinePlayer | null>(null);
const deleteDataScope = ref<string>('all');
const isDeletingPlayerData = ref<boolean>(false);

function openPlayerActions(player: OnlinePlayer) {
  selectedPlayerAction.value = player;
  showPlayerActionsModal.value = true;
}

function promptDeletePlayerData(player: OnlinePlayer) {
  selectedPlayerAction.value = player;
  deleteDataScope.value = 'all';
  showPlayerActionsModal.value = false;
  showDeleteDataConfirmModal.value = true;
}

async function executeDeletePlayerData() {
  if (!selectedPlayerAction.value) return;
  isDeletingPlayerData.value = true;
  try {
    const res = await invoke<string>('delete_player_data', {
      instanceDir: props.instanceDir,
      uuid: selectedPlayerAction.value.uuid,
      scope: deleteDataScope.value
    });
    toastSuccess('Player Data Deleted', res || 'Player data reset successfully.');
    showDeleteDataConfirmModal.value = false;
    emit('delete-player-data', {
      uuid: selectedPlayerAction.value.uuid,
      scope: deleteDataScope.value
    });
  } catch (err: any) {
    toastError('Delete Failed', 'Failed to delete player data: ' + (err?.message || err));
  } finally {
    isDeletingPlayerData.value = false;
  }
}

async function handleKickSelectedPlayer() {
  if (!selectedPlayerAction.value) return;
  const name = selectedPlayerAction.value.name;
  await invoke('send_server_command', { command: `kick ${name}` });
  showPlayerActionsModal.value = false;
  emit('kick-player', name);
}

async function handleBanSelectedPlayer() {
  if (!selectedPlayerAction.value) return;
  const name = selectedPlayerAction.value.name;
  await invoke('send_server_command', { command: `ban ${name}` });
  showPlayerActionsModal.value = false;
  emit('ban-player', name);
}

async function handleOpSelectedPlayer() {
  if (!selectedPlayerAction.value) return;
  const name = selectedPlayerAction.value.name;
  await invoke('send_server_command', { command: `op ${name}` });
  showPlayerActionsModal.value = false;
  emit('op-player', name);
}

async function handleDeopSelectedPlayer() {
  if (!selectedPlayerAction.value) return;
  const name = selectedPlayerAction.value.name;
  await invoke('send_server_command', { command: `deop ${name}` });
  showPlayerActionsModal.value = false;
  emit('deop-player', name);
}

// Mod Version Switcher Modal state
const switchingMod = ref<boolean>(false);
const selectedModForSwitch = ref<InstalledMod | null>(null);
const availableModVersions = ref<ModVersionOption[]>([]);
const isVersionsLoading = ref<boolean>(false);

async function openSwitchVersion(mod: InstalledMod) {
  selectedModForSwitch.value = mod;
  switchingMod.value = true;
  isVersionsLoading.value = true;
  try {
    const versions = await invoke<ModVersionOption[]>('get_mod_versions', {
      projectId: mod.mod_id || mod.file_name.replace('.jar', ''),
      loader: props.realm.loader || 'NeoForge',
      gameVersion: props.realm.version || '1.21.1'
    });
    availableModVersions.value = versions || [];
  } catch (err) {
    console.warn('Failed to load mod versions:', err);
    availableModVersions.value = [];
  } finally {
    isVersionsLoading.value = false;
  }
}

async function selectNewModVersion(ver: ModVersionOption) {
  if (!selectedModForSwitch.value) return;
  try {
    await invoke('switch_mod_version', {
      instanceDir: props.instanceDir,
      oldFileName: selectedModForSwitch.value.file_name,
      downloadUrl: ver.download_url,
      newFileName: ver.filename
    });
    switchingMod.value = false;
    emit('refresh-mods');
  } catch (err: any) {
    toastError('Switch Failed', 'Failed to switch mod version: ' + (err?.message || err));
  }
}

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

// MSPT Sparkline SVG Helpers (Cyan Accent #45C5D9 & Muted Red #E05252 threshold)
function generateSvgArea(points: number[], minVal: number, maxVal: number, w: number, h: number): string {
  if (!points || points.length === 0) return '';
  const step = w / Math.max(1, points.length - 1);
  let d = `M 0,${h}`;
  points.forEach((val, i) => {
    const clamped = Math.max(minVal, Math.min(maxVal, val));
    const y = h - ((clamped - minVal) / (maxVal - minVal)) * h;
    d += ` L ${i * step},${y}`;
  });
  d += ` L ${(points.length - 1) * step},${h} Z`;
  return d;
}

function generateSvgLine(points: number[], minVal: number, maxVal: number, w: number, h: number): string {
  if (!points || points.length === 0) return '';
  const step = w / Math.max(1, points.length - 1);
  return points.map((val, i) => {
    const clamped = Math.max(minVal, Math.min(maxVal, val));
    const y = h - ((clamped - minVal) / (maxVal - minVal)) * h;
    return `${i === 0 ? 'M' : 'L'} ${i * step},${y}`;
  }).join(' ');
}
</script>

<template>
  <div class="flex-1 flex flex-col h-full overflow-hidden bg-canvas select-none">
    
    <!-- Top Navigation & Realm Header -->
    <header class="h-12 border-b border-border px-6 flex items-center justify-between bg-sidebar/50 shrink-0 gap-4">
      <!-- Left: Back Button & Realm Title -->
      <div class="flex items-center space-x-3 min-w-0">
        <Button
          variant="ghost"
          size="sm"
          @click="$emit('back')"
        >
          <template #prefix>
            <ArrowLeft class="w-3.5 h-3.5" />
          </template>
          <span>Instances</span>
        </Button>

        <div class="h-4 w-px bg-border" />

        <div class="w-6 h-6 rounded bg-card border border-border flex items-center justify-center p-0.5 shrink-0 overflow-hidden">
          <img v-if="realm.icon_base64" :src="realm.icon_base64" class="w-full h-full object-cover rounded-sm" />
          <img v-else :src="getLoaderLogo(realm.loader)" class="w-full h-full object-contain" />
        </div>

        <h2 class="text-xs font-bold text-text-primary tracking-wide uppercase truncate">
          REALM: {{ realm.display_name }}
        </h2>

        <Badge variant="cyan" size="sm">
          <template #icon>
            <img :src="getLoaderLogo(realm.loader)" class="w-3 h-3 object-contain inline-block mr-1" />
          </template>
          <span>{{ realm.loader }} {{ realm.version }}</span>
        </Badge>

        <Badge
          v-if="serverStatus === 'Online'"
          variant="success"
          dot
          size="sm"
        >
          Online
        </Badge>
        <Badge
          v-else-if="serverStatus === 'Starting'"
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
          Stopped
        </Badge>
      </div>

      <!-- Right: Open Directory Action -->
      <div class="flex items-center space-x-2 shrink-0">
        <Button
          variant="secondary"
          size="sm"
          title="Open Directory in File Explorer"
          @click="$emit('open-folder')"
        >
          <template #prefix>
            <ExternalLink class="w-3.5 h-3.5 text-text-muted" />
          </template>
          <span>Open Folder</span>
        </Button>
      </div>
    </header>

    <!-- Main Realm Body -->
    <div class="flex-1 flex flex-col overflow-hidden">
      
      <!-- Primary Server Controls & Connection Bar -->
      <div class="p-6 pb-0 shrink-0">
        <div class="bg-card border border-border p-3.5 rounded-lg flex flex-col md:flex-row items-stretch md:items-center justify-between gap-3 shadow-sm">
          <!-- Connection IP & Room Code -->
          <div class="flex flex-wrap items-center gap-3">
            <!-- Server IP (Admin Loopback) Chip -->
            <div class="flex items-center bg-sidebar border border-border rounded px-3 py-1.5 gap-2.5">
              <div class="flex flex-col">
                <span class="text-[9px] uppercase font-bold text-text-muted tracking-wider">Host / Local Loopback</span>
                <span class="font-mono text-xs font-bold text-text-primary select-all">
                  {{ displayServerIp }}
                </span>
              </div>
              <button
                type="button"
                class="h-6 px-2 rounded text-[11px] font-medium transition-colors flex items-center gap-1 cursor-pointer bg-elevated hover:bg-elevated-hover text-text-secondary hover:text-text-primary border border-border"
                @click="copyIp"
              >
                <Check v-if="copiedIp" class="w-3 h-3 text-status-success" />
                <Copy v-else class="w-3 h-3 text-text-muted" />
                <span>{{ copiedIp ? 'Copied' : 'Copy' }}</span>
              </button>
            </div>

            <!-- Public Direct Address for Friends without Conduit -->
            <div v-if="displayPublicIp" class="flex items-center bg-sidebar border border-amber-500/40 rounded px-3 py-1.5 gap-2.5">
              <div class="flex flex-col">
                <div class="flex items-center gap-1.5">
                  <span class="w-1.5 h-1.5 rounded-full bg-amber-400"></span>
                  <span class="text-[9px] uppercase font-bold text-amber-400 tracking-wider">Vanilla Direct Join (No Conduit)</span>
                </div>
                <span class="font-mono text-xs font-bold text-amber-200 select-all tracking-wide">
                  {{ displayPublicIp }}
                </span>
              </div>
              <button
                type="button"
                class="h-6 px-2.5 rounded text-[11px] font-medium transition-colors flex items-center gap-1 cursor-pointer bg-amber-500/10 hover:bg-amber-500/20 text-amber-300 border border-amber-500/30"
                @click="copyPublicIp"
                title="Friends can paste this directly into normal vanilla Minecraft without Conduit."
              >
                <Check v-if="copiedPublicIp" class="w-3.5 h-3.5 text-status-success" />
                <Copy v-else class="w-3.5 h-3.5 text-amber-400" />
                <span>{{ copiedPublicIp ? 'Copied' : 'Copy Direct IP' }}</span>
              </button>
            </div>

            <!-- Conduit End-to-End Encrypted Room Code (Zero-Leak) -->
            <div class="flex items-center bg-cyan-500/10 border border-cyan-500/40 rounded px-3.5 py-1.5 gap-3 shadow-inner">
              <div class="flex flex-col">
                <div class="flex items-center gap-1.5">
                  <span class="w-1.5 h-1.5 rounded-full bg-cyan-400 animate-pulse"></span>
                  <span class="text-[9px] uppercase font-bold text-cyan-400 tracking-wider">Conduit Secure Room Code</span>
                  <span class="text-[9px] font-semibold text-emerald-400 bg-emerald-950/60 border border-emerald-500/30 px-1 py-0.2 rounded">Encrypted • IP Hidden</span>
                </div>
                <span class="font-mono text-xs font-bold text-cyan-300 select-all tracking-wide">
                  {{ hostRoomCode || ('CND-' + realm.folder_name.toUpperCase()) }}
                </span>
              </div>
              <button
                type="button"
                class="h-6 px-2.5 rounded text-[11px] font-semibold transition-colors flex items-center gap-1.5 cursor-pointer bg-cyan-500/20 hover:bg-cyan-500/30 text-cyan-200 border border-cyan-500/40"
                @click="copyRoomCode"
                title="Share this secure code with friends. Keeps your home IP and network hidden."
              >
                <Check v-if="copiedRoomCode" class="w-3.5 h-3.5 text-status-success" />
                <Copy v-else class="w-3.5 h-3.5" />
                <span>{{ copiedRoomCode ? 'Copied' : 'Share Room Code' }}</span>
              </button>
            </div>
          </div>

          <!-- Primary Server Actions -->
          <div class="flex items-center gap-2">
            <!-- Stopped State Actions -->
            <template v-if="serverStatus === 'Stopped'">
              <Button
                variant="primary"
                size="md"
                @click="$emit('start-server-and-join')"
              >
                <template #prefix>
                  <Zap class="w-3.5 h-3.5" />
                </template>
                <span>Start &amp; Join</span>
              </Button>

              <Button
                variant="secondary"
                size="md"
                @click="$emit('start-server')"
              >
                <template #prefix>
                  <Terminal class="w-3.5 h-3.5 text-text-muted" />
                </template>
                <span>Start Server</span>
              </Button>

              <Button
                variant="ghost"
                size="md"
                @click="$emit('launch-singleplayer')"
              >
                <template #prefix>
                  <Play class="w-3.5 h-3.5 text-text-muted" />
                </template>
                <span>Solo Play</span>
              </Button>
            </template>

            <!-- Starting State -->
            <template v-else-if="serverStatus === 'Starting'">
              <Button
                variant="secondary"
                size="md"
                loading
              >
                <span>Starting Server...</span>
              </Button>
            </template>

            <!-- Online State Actions -->
            <template v-else>
              <Button
                variant="primary"
                size="md"
                @click="$emit('join-local-server')"
              >
                <template #prefix>
                  <Play class="w-3.5 h-3.5" />
                </template>
                <span>Join Server</span>
              </Button>

              <Button
                variant="secondary"
                size="md"
                title="Restart Dedicated Server"
                @click="handleRestartServer"
              >
                <template #prefix>
                  <RotateCw class="w-3.5 h-3.5 text-text-muted" />
                </template>
                <span>Restart</span>
              </Button>

              <Button
                variant="danger"
                size="md"
                title="Gracefully Stop Dedicated Server"
                @click="$emit('stop-server')"
              >
                <template #prefix>
                  <Square class="w-3 h-3 fill-current" />
                </template>
                <span>Stop Server</span>
              </Button>
            </template>
          </div>
        </div>
      </div>

      <!-- Navigation Subtabs Header -->
      <div class="px-6 pt-4 shrink-0">
        <Tabs
          v-model="activeSubtab"
          :tabs="subtabs"
          variant="line"
          size="md"
        />
      </div>

      <!-- Subtab Content Viewport -->
      <div class="flex-1 overflow-y-auto p-6 space-y-6">
        
        <!-- SUBTAB 1: CONSOLE & PERFORMANCE -->
        <div v-if="activeSubtab === 'console'" class="space-y-6">
          
          <!-- Compact Telemetry Strip using MetricCard -->
          <div class="grid grid-cols-2 lg:grid-cols-4 gap-3">
            <MetricCard
              label="Tick Duration (MSPT)"
              :value="currentMspt.toFixed(1)"
              unit="ms"
              :status="currentMspt > 50 ? 'error' : 'cyan'"
              secondary-label="Target TPS"
              :secondary-value="`${currentTps.toFixed(1)} / 20.0`"
            >
              <template #icon>
                <Clock class="w-4 h-4 text-cyan-accent" />
              </template>
            </MetricCard>

            <MetricCard
              label="Server Heap"
              :value="(jvmUsedHeapMb / 1024).toFixed(1)"
              unit="GB"
              status="cyan"
              secondary-label="Max Allocated"
              :secondary-value="`${serverRamGb} GB`"
            >
              <template #icon>
                <Zap class="w-4 h-4 text-cyan-accent" />
              </template>
            </MetricCard>

            <MetricCard
              label="Active Entities"
              :value="loadedEntities"
              status="normal"
              secondary-label="Loaded Chunks"
              :secondary-value="loadedChunks"
            >
              <template #icon>
                <Layers class="w-4 h-4 text-text-muted" />
              </template>
            </MetricCard>

            <MetricCard
              label="Main Tick Thread"
              :value="`${mainTickThreadUsage}%`"
              :status="mainTickThreadUsage > 85 ? 'error' : 'normal'"
              secondary-label="Host CPU"
              :secondary-value="`${totalHostCpuUsage}%`"
            >
              <template #icon>
                <Cpu class="w-4 h-4 text-text-muted" />
              </template>
            </MetricCard>
          </div>

          <!-- MSPT & TPS Rolling Sparkline Stream (Dark Graphite, Cyan Line, Dotted Red 50ms Threshold) -->
          <div class="bg-card border border-border rounded-lg p-4 space-y-3 shadow-sm">
            <div class="flex items-center justify-between text-xs">
              <div class="flex items-center space-x-2">
                <Clock class="w-3.5 h-3.5 text-cyan-accent" />
                <span class="font-semibold text-text-primary">Tick Duration (MSPT) Continuous Stream</span>
                <span class="text-[10px] text-cyan-accent font-mono">&bull; Cyan MSPT</span>
                <span class="text-[10px] text-status-error font-mono">&bull; 50ms Lag Threshold</span>
              </div>
              <span class="text-[10px] font-mono text-text-muted">60-Second Rolling Window</span>
            </div>

            <!-- SVG Chart Canvas -->
            <div class="h-24 w-full relative bg-canvas rounded border border-border/80 overflow-hidden">
              <!-- Grid background -->
              <div class="absolute inset-0 flex flex-col justify-between p-2 pointer-events-none opacity-20">
                <div class="border-b border-border w-full"></div>
                <div class="border-b border-border w-full"></div>
                <div class="border-b border-border w-full"></div>
              </div>

              <!-- 50ms Threshold Line (Dotted Red) -->
              <div class="absolute left-0 right-0 top-[20%] border-b border-dashed border-status-error/60 pointer-events-none">
                <span class="absolute right-2 -top-3.5 text-[9px] font-mono text-status-error font-medium">50.0 ms Threshold</span>
              </div>

              <svg class="w-full h-full" preserveAspectRatio="none" viewBox="0 0 600 96">
                <defs>
                  <linearGradient id="msptCyanGrad" x1="0" y1="0" x2="0" y2="1">
                    <stop offset="0%" stop-color="#45C5D9" stop-opacity="0.2" />
                    <stop offset="100%" stop-color="#45C5D9" stop-opacity="0.0" />
                  </linearGradient>
                </defs>
                <path :d="generateSvgArea(msptHistory, 0, 60, 600, 96)" fill="url(#msptCyanGrad)" />
                <path :d="generateSvgLine(msptHistory, 0, 60, 600, 96)" fill="none" stroke="#45C5D9" stroke-width="1.5" />
              </svg>
            </div>
          </div>

          <!-- Monospace Dedicated Server Console Viewer -->
          <div class="bg-card border border-border rounded-lg overflow-hidden flex flex-col h-[400px] shadow-sm relative">
            <!-- Console Header Toolbar -->
            <div class="px-4 py-2.5 bg-sidebar/80 border-b border-border flex items-center justify-between text-xs">
              <div class="flex items-center space-x-2">
                <Terminal class="w-3.5 h-3.5 text-cyan-accent" />
                <span class="font-semibold text-text-primary">Server Console Output</span>
              </div>

              <div class="flex items-center space-x-2">
                <!-- Log Filters -->
                <div class="flex items-center space-x-1 bg-elevated/80 p-0.5 rounded border border-border text-[10px]">
                  <button
                    v-for="f in (['all', 'errors', 'warnings', 'info'] as const)"
                    :key="f"
                    type="button"
                    class="px-2 py-0.5 rounded uppercase font-medium transition-colors cursor-pointer"
                    :class="logFilter === f ? 'bg-cyan-accent text-[#11151A] font-semibold' : 'text-text-secondary hover:text-text-primary'"
                    @click="logFilter = f"
                  >
                    {{ f }}
                  </button>
                </div>

                <Button
                  variant="ghost"
                  size="sm"
                  @click="emit('send-command', '')"
                >
                  <span>Clear</span>
                </Button>
              </div>
            </div>

            <!-- Terminal Output Stream -->
            <div
              ref="consoleOutputRef"
              class="flex-1 p-4 font-mono text-[11px] overflow-y-auto space-y-1 select-text bg-canvas leading-relaxed"
              @scroll="handleConsoleScroll"
            >
              <div v-if="filteredLogs.length === 0" class="text-text-muted italic py-4 text-center">
                Awaiting server process output... Click 'Start Server' to boot the dedicated instance.
              </div>
              <div
                v-for="(log, idx) in filteredLogs"
                :key="idx"
                class="break-all whitespace-pre-wrap flex items-start gap-2"
                :class="log.toLowerCase().includes('error') ? 'text-status-error' : log.toLowerCase().includes('warn') ? 'text-orange-accent' : 'text-text-primary'"
              >
                <span class="text-text-muted select-none w-8 text-right shrink-0">{{ idx + 1 }}</span>
                <span class="flex-1">{{ log }}</span>
              </div>
            </div>

            <!-- Auto-scroll Resume Pill -->
            <div v-if="!autoScroll" class="absolute bottom-14 right-4 z-20">
              <Button
                variant="primary"
                size="sm"
                class="shadow-lg"
                @click="resumeLiveTail"
              >
                <template #prefix>
                  <ArrowDown class="w-3.5 h-3.5" />
                </template>
                <span>Resume Live Tail</span>
              </Button>
            </div>

            <!-- Command Input Form -->
            <form
              class="bg-sidebar border-t border-border flex items-center px-3 py-1.5 gap-2"
              @submit.prevent="handleSendCommand"
            >
              <span class="text-cyan-accent font-mono text-sm font-bold select-none">&gt;</span>
              <input
                v-model="consoleInput"
                type="text"
                placeholder="Type command (e.g. list, help, op <player>, save-all)..."
                class="flex-1 bg-transparent text-text-primary font-mono text-xs focus:outline-none placeholder:text-text-muted"
              />
              <Button
                type="submit"
                variant="secondary"
                size="sm"
              >
                <span>Send</span>
              </Button>
            </form>
          </div>

        </div>

        <!-- SUBTAB 2: SERVER PROPERTIES & SETTINGS -->
        <div v-else-if="activeSubtab === 'properties'" class="space-y-6">
          <div class="bg-card border border-border rounded-lg p-5 space-y-6 shadow-sm">
            <h3 class="text-xs font-bold text-text-primary uppercase tracking-wider">Realm Memory Allocations</h3>
            
            <div class="grid grid-cols-1 md:grid-cols-2 gap-6">
              <!-- Dedicated Server RAM Slider -->
              <div class="space-y-2.5 bg-sidebar p-4 rounded-lg border border-border">
                <div class="flex items-center justify-between text-xs">
                  <span class="font-semibold text-text-primary">Dedicated Server RAM</span>
                  <Badge variant="cyan" size="sm">{{ serverRamGb }} GB</Badge>
                </div>
                <input
                  type="range"
                  min="2"
                  max="16"
                  step="1"
                  v-model.number="serverRamGb"
                  class="w-full accent-cyan-accent cursor-pointer"
                />
                <div class="flex justify-between text-[10px] text-text-muted font-mono">
                  <span>2 GB</span>
                  <span>8 GB</span>
                  <span>16 GB</span>
                </div>
              </div>

              <!-- Client RAM Slider -->
              <div class="space-y-2.5 bg-sidebar p-4 rounded-lg border border-border">
                <div class="flex items-center justify-between text-xs">
                  <span class="font-semibold text-text-primary">Minecraft Client RAM</span>
                  <Badge variant="cyan" size="sm">{{ clientRamGb }} GB</Badge>
                </div>
                <input
                  type="range"
                  min="2"
                  max="16"
                  step="1"
                  v-model.number="clientRamGb"
                  class="w-full accent-cyan-accent cursor-pointer"
                />
                <div class="flex justify-between text-[10px] text-text-muted font-mono">
                  <span>2 GB</span>
                  <span>8 GB</span>
                  <span>16 GB</span>
                </div>
              </div>
            </div>

            <!-- Server Properties Grid -->
            <h3 class="text-xs font-bold text-text-primary uppercase tracking-wider pt-2">Server Properties (server.properties)</h3>
            <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4">
              <div class="space-y-1.5">
                <label class="text-xs text-text-secondary">Difficulty</label>
                <select
                  v-model="serverDifficulty"
                  class="w-full h-8.5 px-3 bg-sidebar border border-border rounded text-xs text-text-primary focus:outline-none focus:border-cyan-accent"
                >
                  <option value="peaceful">Peaceful</option>
                  <option value="easy">Easy</option>
                  <option value="normal">Normal</option>
                  <option value="hard">Hard</option>
                </select>
              </div>

              <div class="space-y-1.5">
                <label class="text-xs text-text-secondary">Game Mode</label>
                <select
                  v-model="serverGameMode"
                  class="w-full h-8.5 px-3 bg-sidebar border border-border rounded text-xs text-text-primary focus:outline-none focus:border-cyan-accent capitalize"
                >
                  <option value="survival">Survival</option>
                  <option value="creative">Creative</option>
                  <option value="adventure">Adventure</option>
                  <option value="spectator">Spectator</option>
                </select>
              </div>

              <div class="space-y-1.5">
                <label class="text-xs text-text-secondary">Max Players</label>
                <Input
                  v-model="maxPlayers"
                  type="number"
                  placeholder="20"
                />
              </div>

              <div class="space-y-1.5">
                <label class="text-xs text-text-secondary">View Distance</label>
                <Input
                  v-model="viewDistance"
                  type="number"
                  placeholder="10"
                />
              </div>

              <div class="space-y-1.5 sm:col-span-2">
                <label class="text-xs text-text-secondary">Server MOTD</label>
                <Input
                  v-model="serverMotd"
                  placeholder="A Conduit Minecraft Server"
                />
              </div>
            </div>
          </div>
        </div>

        <!-- SUBTAB 3: WORLD & SAVES -->
        <div v-else-if="activeSubtab === 'world'" class="space-y-6">
          <!-- Metadata strip -->
          <div class="bg-card border border-border p-4 rounded-lg flex flex-wrap items-center justify-between gap-4">
            <div class="space-y-1">
              <div class="text-xs font-bold text-text-primary uppercase tracking-wide">
                World Save: {{ realm.display_name }}
              </div>
              <div class="text-[11px] font-mono text-text-muted">
                {{ realm.full_path || ('./instance/.minecraft/saves/' + realm.folder_name) }}
              </div>
            </div>

            <div class="flex items-center space-x-2">
              <Badge variant="cyan" size="sm">
                NTFS Saved: {{ dedupMetrics?.saved_disk_mb?.toFixed(1) || '438.8' }} MB
              </Badge>
              <Button
                variant="primary"
                size="sm"
                @click="$emit('create-backup')"
              >
                <template #prefix>
                  <Archive class="w-3.5 h-3.5" />
                </template>
                <span>Create Snapshot</span>
              </Button>
            </div>
          </div>

          <!-- Backups List -->
          <div class="bg-card border border-border rounded-lg overflow-hidden shadow-sm">
            <div class="px-4 py-3 bg-sidebar/80 border-b border-border text-xs font-semibold text-text-primary">
              Saved Snapshots &amp; Backups
            </div>

            <div v-if="backups && backups.length > 0" class="divide-y divide-border/50">
              <div
                v-for="b in backups"
                :key="b.id"
                class="px-4 py-3 flex items-center justify-between text-xs hover:bg-elevated/40 transition-colors"
              >
                <div>
                  <div class="font-semibold text-text-primary">{{ b.name }}</div>
                  <div class="text-[10px] text-text-muted font-mono mt-0.5">
                    {{ b.timestamp }} &bull; {{ b.sizeMb }} MB &bull; {{ b.type }}
                  </div>
                </div>

                <Button
                  variant="secondary"
                  size="sm"
                >
                  <span>Restore</span>
                </Button>
              </div>
            </div>

            <div v-else class="p-6 text-center text-xs text-text-muted font-mono">
              No manual snapshots yet. Click 'Create Snapshot' to save world state.
            </div>
          </div>
        </div>

        <!-- SUBTAB 4: MODS (EXTRACTED M4 COMPONENT) -->
        <div v-else-if="activeSubtab === 'mods'" class="space-y-4">
          <ModsManagerView
            :instance-dir="instanceDir"
            :installed-mods="installedMods"
            :is-mods-loading="isModsLoading"
            :loader="realm.loader"
            :game-version="realm.version"
            :realm-name="realm.display_name"
            @refresh-mods="$emit('refresh-mods')"
            @toggle-mod="$emit('toggle-mod', $event)"
            @delete-mod="$emit('delete-mod', $event)"
            @open-folder="$emit('open-folder')"
          />
        </div>

        <!-- SUBTAB 5: PLAYERS -->
        <div v-else-if="activeSubtab === 'players'" class="space-y-4">
          <div class="flex items-center justify-between text-xs">
            <span class="font-semibold text-text-primary">Connected &amp; Registered Players ({{ players.length }})</span>
          </div>

          <div class="bg-card border border-border rounded-lg overflow-hidden shadow-sm">
            <table class="w-full text-left text-xs border-collapse">
              <thead>
                <tr class="border-b border-border bg-sidebar/50 text-[10px] font-semibold text-text-muted uppercase">
                  <th class="py-2.5 px-4">Player</th>
                  <th class="py-2.5 px-4">Role</th>
                  <th class="py-2.5 px-4">Status</th>
                  <th class="py-2.5 px-4 text-right">Actions</th>
                </tr>
              </thead>
              <tbody class="divide-y divide-border/40">
                <tr
                  v-for="p in players"
                  :key="p.uuid"
                  class="hover:bg-elevated/40 transition-colors"
                >
                  <td class="py-3 px-4 font-medium text-text-primary">
                    <div class="flex items-center space-x-2.5">
                      <div class="w-7 h-7 rounded bg-elevated border border-border flex items-center justify-center font-bold text-cyan-accent text-xs">
                        {{ p.name[0] }}
                      </div>
                      <div>
                        <div>{{ p.name }}</div>
                        <div class="text-[10px] text-text-muted font-mono">{{ p.uuid }}</div>
                      </div>
                    </div>
                  </td>
                  <td class="py-3 px-4 font-mono">
                    <Badge :variant="p.role === 'Operator' ? 'cyan' : 'neutral'" size="sm">
                      {{ p.role }}
                    </Badge>
                  </td>
                  <td class="py-3 px-4">
                    <Badge :variant="p.status === 'Online' ? 'success' : 'neutral'" dot size="sm">
                      {{ p.status }}
                    </Badge>
                  </td>
                  <td class="py-3 px-4 text-right">
                    <div class="inline-flex items-center space-x-1">
                      <Button
                        variant="secondary"
                        size="sm"
                        @click="openPlayerActions(p)"
                      >
                        <span>Manage</span>
                      </Button>
                      <Button
                        variant="danger"
                        size="sm"
                        title="Delete Player Data"
                        @click="promptDeletePlayerData(p)"
                      >
                        <span>Delete Data</span>
                      </Button>
                    </div>
                  </td>
                </tr>

                <tr v-if="players.length === 0">
                  <td colspan="4" class="py-6 text-center text-text-muted font-mono">
                    No active players logged in yet.
                  </td>
                </tr>
              </tbody>
            </table>
          </div>
        </div>

        <!-- SUBTAB 6: CONDUIT NETWORK -->
        <div v-else-if="activeSubtab === 'network'" class="space-y-4">
          <div class="bg-card border border-border rounded-lg p-5 space-y-4 shadow-sm">
            <div class="flex items-center space-x-3">
              <Radio class="w-5 h-5 text-cyan-accent" />
              <div>
                <h4 class="text-sm font-semibold text-text-primary">Conduit Encrypted Network</h4>
              </div>
            </div>

            <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
              <!-- Public Direct IP -->
              <div class="p-4 bg-sidebar rounded border border-border flex flex-col justify-between gap-3">
                <div>
                  <div class="text-[10px] text-amber-400 uppercase font-bold tracking-wider">Public WAN Address</div>
                  <div class="font-mono text-sm font-bold text-text-primary mt-1 select-all">
                    {{ displayPublicIp || 'Resolving WAN IP...' }}
                  </div>
                </div>
                <Button
                  v-if="displayPublicIp"
                  variant="secondary"
                  size="sm"
                  @click="copyPublicIp"
                >
                  <Check v-if="copiedPublicIp" class="w-3.5 h-3.5 text-status-success" />
                  <Copy v-else class="w-3.5 h-3.5 text-text-muted" />
                  <span>{{ copiedPublicIp ? 'Copied' : 'Copy Direct IP' }}</span>
                </Button>
              </div>

              <!-- WireGuard Mesh Code -->
              <div class="p-4 bg-sidebar rounded border border-border flex flex-col justify-between gap-3">
                <div>
                  <div class="text-[10px] text-cyan-accent uppercase font-bold tracking-wider">Room Code</div>
                  <div class="font-mono text-sm font-bold text-cyan-accent mt-1 select-all">
                    {{ hostRoomCode || ('CND-' + realm.folder_name.toUpperCase()) }}
                  </div>
                </div>
                <Button
                  variant="secondary"
                  size="sm"
                  @click="copyRoomCode"
                >
                  <Check v-if="copiedRoomCode" class="w-3.5 h-3.5 text-status-success" />
                  <Copy v-else class="w-3.5 h-3.5 text-text-muted" />
                  <span>{{ copiedRoomCode ? 'Copied' : 'Copy Code' }}</span>
                </Button>
              </div>
            </div>
          </div>
        </div>

      </div>
    </div>

    <!-- ===================================================================== -->
    <!-- RESTORED MODAL TEMPLATES (Fulfilling M3, M4, M5 Requirements)         -->
    <!-- ===================================================================== -->

    <!-- MODAL 1: Delete Player Data Modal (showDeleteDataConfirmModal) [M3 REQUIREMENT] -->
    <Modal
      v-model="showDeleteDataConfirmModal"
      title="Delete Player Data"
      description="Permanently remove saved player statistics, inventory, and advancements."
      maxWidth="md"
    >
      <div class="space-y-4">
        <div class="p-3 bg-status-error-subtle border border-status-error/30 rounded text-xs text-status-error">
          Warning: This action permanently deletes player data files (.dat) in the world's <code>playerdata/</code> folder. This operation cannot be undone.
        </div>

        <div v-if="selectedPlayerAction" class="p-3 bg-sidebar border border-border rounded flex items-center gap-3">
          <div class="w-8 h-8 rounded bg-elevated border border-border flex items-center justify-center font-bold text-cyan-accent text-xs">
            {{ selectedPlayerAction.name[0] }}
          </div>
          <div>
            <div class="font-semibold text-text-primary">{{ selectedPlayerAction.name }}</div>
            <div class="text-[10px] text-text-muted font-mono">{{ selectedPlayerAction.uuid }}</div>
          </div>
        </div>

        <div class="space-y-2">
          <label class="text-xs font-semibold text-text-primary uppercase tracking-wider">Deletion Scope</label>
          <div class="space-y-2">
            <label class="flex items-center gap-2 text-xs text-text-secondary cursor-pointer">
              <input type="radio" v-model="deleteDataScope" value="all" class="accent-cyan-accent" />
              <span>Complete Reset (Inventory, Ender Chest, Stats, Advancements)</span>
            </label>
            <label class="flex items-center gap-2 text-xs text-text-secondary cursor-pointer">
              <input type="radio" v-model="deleteDataScope" value="playerdata" class="accent-cyan-accent" />
              <span>Inventory &amp; Position Only</span>
            </label>
            <label class="flex items-center gap-2 text-xs text-text-secondary cursor-pointer">
              <input type="radio" v-model="deleteDataScope" value="stats" class="accent-cyan-accent" />
              <span>Statistics &amp; Advancements Only</span>
            </label>
          </div>
        </div>
      </div>

      <template #footer>
        <Button
          variant="ghost"
          size="sm"
          @click="showDeleteDataConfirmModal = false"
        >
          Cancel
        </Button>
        <Button
          variant="danger"
          size="sm"
          :loading="isDeletingPlayerData"
          @click="executeDeletePlayerData"
        >
          Confirm Deletion
        </Button>
      </template>
    </Modal>

    <!-- MODAL 2: Player Actions Modal (showPlayerActionsModal) [M5 REQUIREMENT] -->
    <Modal
      v-model="showPlayerActionsModal"
      :title="'Manage Player: ' + (selectedPlayerAction?.name || '')"
      maxWidth="sm"
    >
      <div v-if="selectedPlayerAction" class="space-y-4">
        <div class="p-3 bg-sidebar border border-border rounded flex items-center gap-3">
          <div class="w-10 h-10 rounded bg-elevated border border-border flex items-center justify-center font-bold text-cyan-accent text-sm">
            {{ selectedPlayerAction.name[0] }}
          </div>
          <div>
            <div class="font-semibold text-text-primary text-xs">{{ selectedPlayerAction.name }}</div>
            <div class="text-[10px] text-text-muted font-mono">{{ selectedPlayerAction.uuid }}</div>
            <div class="text-[10px] text-text-secondary mt-0.5">Role: {{ selectedPlayerAction.role }}</div>
          </div>
        </div>

        <div class="grid grid-cols-2 gap-2 pt-2">
          <Button
            v-if="selectedPlayerAction.role === 'Operator'"
            variant="secondary"
            size="sm"
            @click="handleDeopSelectedPlayer"
          >
            Demote from OP
          </Button>
          <Button
            v-else
            variant="accent"
            size="sm"
            @click="handleOpSelectedPlayer"
          >
            Promote to OP
          </Button>

          <Button
            variant="secondary"
            size="sm"
            @click="handleKickSelectedPlayer"
          >
            Kick Player
          </Button>

          <Button
            variant="danger"
            size="sm"
            @click="handleBanSelectedPlayer"
          >
            Ban Player
          </Button>

          <Button
            variant="danger"
            size="sm"
            @click="promptDeletePlayerData(selectedPlayerAction)"
          >
            Delete Data...
          </Button>
        </div>
      </div>
      <template #footer>
        <Button
          variant="ghost"
          size="sm"
          @click="showPlayerActionsModal = false"
        >
          Close
        </Button>
      </template>
    </Modal>

    <!-- MODAL 3: Mod Version Switcher Modal (switchingMod) [M4 REQUIREMENT] -->
    <Modal
      v-model="switchingMod"
      :title="'Switch Version: ' + (selectedModForSwitch?.mod_name || selectedModForSwitch?.file_name || '')"
      maxWidth="md"
    >
      <div class="space-y-3">
        <p class="text-xs text-text-secondary">
          Select target Modrinth version compatible with {{ realm.loader }} {{ realm.version }}.
        </p>

        <div v-if="isVersionsLoading" class="py-6 text-center text-xs text-text-muted font-mono">
          Loading compatible versions from Modrinth...
        </div>

        <div v-else-if="availableModVersions.length > 0" class="max-h-64 overflow-y-auto space-y-1.5 divide-y divide-border/50">
          <div
            v-for="ver in availableModVersions"
            :key="ver.id"
            class="pt-2 flex items-center justify-between text-xs hover:bg-elevated/40 p-2 rounded transition-colors"
          >
            <div>
              <div class="font-semibold text-text-primary">{{ ver.version_number }} ({{ ver.name }})</div>
              <div class="text-[10px] text-text-muted font-mono">
                {{ ver.date_published?.substring(0, 10) }} &bull; {{ ver.filename }}
              </div>
            </div>
            <Button
              variant="accent"
              size="sm"
              @click="selectNewModVersion(ver)"
            >
              Select
            </Button>
          </div>
        </div>

        <div v-else class="py-6 text-center text-xs text-text-muted font-mono">
          No alternative versions found for this loader/version.
        </div>
      </div>
      <template #footer>
        <Button
          variant="ghost"
          size="sm"
          @click="switchingMod = false"
        >
          Cancel
        </Button>
      </template>
    </Modal>

  </div>
</template>
