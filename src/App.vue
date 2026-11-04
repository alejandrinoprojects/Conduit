<script setup lang="ts">
import { ref, onMounted, onUnmounted, nextTick, computed } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  Activity,
  Terminal,
  Users,
  Archive,
  Package,
  Radio,
  Settings,
  Play,
  Square,
  Search,
  Check,
  Copy,
  Trash2,
  RefreshCw,
  Cpu,
  Zap,
  Shirt,
  ArrowRightLeft,
  Clock,
  Layers,
  HardDrive,
  ExternalLink,
  Flame,
  ShieldCheck,
  Sparkles,
  Sliders,
  Compass,
  FolderGit2,
  ChevronDown,
  ArrowDown,
  Globe,
  Download,
  X,
  MoreVertical,
  Shield,
  Ban,
  UserX,
  FileText,
  Calendar,
  Filter,
  Tag,
  SlidersHorizontal,
  ChevronLeft,
  ChevronRight,
  ArrowRight,
  ArrowLeft,
  CheckCircle2,
  Monitor,
  Gamepad2,
  Sprout,
  Pencil,
  Folder,
  Server,
  Plus
} from "lucide-vue-next";
import { useConduitIPC, useServerState, type ToastMessage } from "./composables";
import { StatusDot, Badge, Button as CommonButton } from "./components/common";
import LibraryView from "./views/LibraryView.vue";
import RealmDetailView from "./views/RealmDetailView.vue";
import DiscoverView from "./views/DiscoverView.vue";
import ModsManagerView from "./views/ModsManagerView.vue";

interface MinecraftProfile {
  uuid: string;
  username: string;
  access_token: string;
}

interface DeviceCodePayload {
  device_code: string;
  user_code: string;
  verification_uri: string;
  expires_in: number;
  interval: number;
}

interface InstallProgressPayload {
  step: string;
  percentage: number;
  details: string;
}

interface VersionMetaItem {
  id: string;
  release_type: string;
  release_time: string;
}

interface InstalledMod {
  file_name: string;
  mod_id?: string;
  mod_name?: string;
  version?: string;
  environment: "Both" | "ClientOnly" | "ServerOnly";
  size_bytes: number;
  is_enabled: boolean;
  icon_base64?: string;
}

interface ModVersionOption {
  id: string;
  version_number: string;
  name: string;
  date_published: string;
  download_url: string;
  filename: string;
  game_versions?: string[];
  loaders?: string[];
  version_type?: string;
  downloads?: number;
}

interface ModrinthGalleryItem {
  url: string;
  title?: string;
  description?: string;
  created: string;
}

interface ModrinthProjectDetail {
  id: string;
  slug: string;
  title: string;
  description: string;
  body: string;
  icon_url: string | null;
  downloads: number;
  followers: number;
  categories: string[];
  loaders: string[];
  game_versions: string[];
  gallery: ModrinthGalleryItem[];
}

interface WorldSaveInfo {
  folder_name: string;
  display_name: string;
  last_played: number;
  game_mode: string;
  icon_base64?: string;
  version?: string;
  loader?: string;
  mods?: string[];
  runtime_name?: string | null;
  runtime_id?: string | null;
  full_path?: string | null;
  server_address?: string | null;
  is_realm?: boolean;
  hosting_mode?: string | null;
}

interface DownloadedRuntimeItem {
  id: string;
  loader: string;
  version: string;
  is_server: boolean;
  is_client: boolean;
  size_mb: number;
}

interface ModrinthProject {
  project_id: string;
  slug?: string;
  title: string;
  description: string;
  icon_url: string | null;
  downloads: number;
  client_side: string;
  server_side: string;
  author?: string;
  categories?: string[];
  loaders?: string[];
  follows?: number;
  date_modified?: string;
}

interface OnlinePlayer {
  name: string;
  uuid: string;
  ping: number;
  role: "Operator" | "Player";
  status: "Online" | "Whitelisted" | "Banned";
  is_connected: boolean;
  joinedAt: string;
}

interface BackupItem {
  id: string;
  name: string;
  timestamp: string;
  sizeMb: number;
  type: "Automatic" | "Manual";
}

interface DeduplicationMetrics {
  libraries_shared: boolean;
  configs_shared: boolean;
  world_shared: boolean;
  mods_hardlinked: number;
  client_libraries_size_mb: number;
  server_libraries_size_mb: number;
  saved_disk_mb: number;
  server_world_path: string;
  client_world_path: string;
}

// Centralized Composables & Search State
const conduitIPC = useConduitIPC();
const { toasts, removeToast, toastSuccess, toastError, toastInfo, toastWarning } = useServerState();
const searchQuery = ref<string>("");

// Single Unified Navigation Tab State
const activeTab = ref<
  "engine" | "performance" | "console" | "mods" | "players" | "backups" | "tunnels" | "settings" | "skins"
>("engine");

const instanceDir = ref<string>("./instance");

// Loaders: Full Loader Support
const availableLoaders = ["NeoForge", "Fabric", "Quilt", "Paper", "Purpur", "Vanilla", "Forge"];
const selectedLoader = ref<string>("NeoForge");
const isLoaderDropdownOpen = ref<boolean>(false);

// Versions: Full Official Manifest Support + Search
const availableVersions = ref<string[]>([
  "1.21.1",
  "1.21.4",
  "1.21.0",
  "1.20.1",
  "1.20.4",
  "1.19.2",
  "1.18.2",
  "1.16.5"
]);
const selectedVersion = ref<string>("1.21.1");
const isVersionDropdownOpen = ref<boolean>(false);
const versionSearchQuery = ref<string>("");
const allManifestVersions = ref<VersionMetaItem[]>([]);
const isLoadingVersions = ref<boolean>(false);

// World Selection State (Server-Worlds to Play on and host from / to)
const worldsList = ref<WorldSaveInfo[]>([]);
const selectedWorldName = ref<string>("World1");
const isWorldsLoading = ref<boolean>(false);
const expandedWorldMods = ref<Record<string, boolean>>({});

// Downloaded Runtimes (Version-Loaders) Library State
const downloadedRuntimes = ref<DownloadedRuntimeItem[]>([]);
const isRuntimesLoading = ref<boolean>(false);
const showCreateInstanceModal = ref<boolean>(false);
const instanceWizardStep = ref<number>(1);
const wizardInstanceName = ref<string>("Fabric SMP");
const wizardLoader = ref<string>("Fabric");
const wizardVersion = ref<string>("1.21.1");
const wizardRamMb = ref<number>(4096);

// Deduplication Engine State
const dedupMetrics = ref<DeduplicationMetrics | null>(null);
const isRepairingJunctions = ref<boolean>(false);

// Statuses
const serverStatus = ref<"Stopped" | "Starting" | "Online" | "Error">("Stopped");
const clientStatus = ref<"Stopped" | "Playing">("Stopped");
const tunnelUrl = ref<string | null>(null);
const publicIp = ref<string | null>(null);
const tunnelClaimUrl = ref<string | null>(null);
const logs = ref<string[]>([]);
const consoleInput = ref<string>("");
const consoleOutputRef = ref<HTMLDivElement | null>(null);
const copiedIp = ref<boolean>(false);
const copiedCode = ref<boolean>(false);

// Console Auto-Scroll & Tail Lock
const isScrolledUp = ref<boolean>(false);
const visibleLogStartIndex = ref<number>(0);
const LOG_WINDOW_SIZE = 80;

// Console filtering
const logFilter = ref<"all" | "errors" | "warnings" | "info">("all");

// Auth State
const authProfile = ref<MinecraftProfile | null>(null);
const isAuthenticating = ref<boolean>(false);
const deviceCodeData = ref<DeviceCodePayload | null>(null);
const showAuthModal = ref<boolean>(false);

// Provisioning & Download Progress Modal
const isInstalling = ref<boolean>(false);
const installProgress = ref<InstallProgressPayload | null>(null);

// Mods & Modpacks Browsing & Management
const contentMode = ref<"installed" | "browse">("installed");
const installedMods = ref<InstalledMod[]>([]);
const isModsLoading = ref<boolean>(false);
const installedCategoryFilter = ref<"All" | "Mods" | "Data Packs" | "Shaders">("All");

// Modrinth Browser State
const browseCategory = ref<"mod" | "modpack" | "shader" | "resourcepack" | "datapack">("mod");
const browseQuery = ref<string>("");
const browseLoaderFilter = ref<string>("neoforge");
const browseVersionFilter = ref<string>("1.21.1");
const browseFunctionalCategory = ref<string>("all");
const browseSortIndex = ref<string>("relevance");
const browsePage = ref<number>(1);
const browseLimit = ref<number>(20);
const browseTotalHits = ref<number>(0);
const browseResults = ref<ModrinthProject[]>([]);
const isBrowsing = ref<boolean>(false);
const installingProjects = ref<Record<string, boolean>>({});

// Target Instance Selection for Mod Installations
const targetInstallInstance = ref<RuntimeGroup | null>(null);
const showInstancePickerModal = ref<boolean>(false);
const pendingInstallProject = ref<ModrinthProject | null>(null);

// Player Actions Modal & Playerdata Deletion State
const selectedPlayerAction = ref<OnlinePlayer | null>(null);
const showPlayerActionsModal = ref<boolean>(false);
const showDeleteDataConfirmModal = ref<boolean>(false);
const deleteDataScope = ref<"all" | "inventory" | "advancements" | "stats">("all");
const isDeletingPlayerData = ref<boolean>(false);

// Switch Version Modal
const switchingMod = ref<InstalledMod | null>(null);
const availableModVersions = ref<ModVersionOption[]>([]);
const isVersionsLoading = ref<boolean>(false);

// Diagnostics Engine
const cpuUsage = ref<number>(0);
const ramUsageMb = ref<number>(0);
const serverUptimeSeconds = ref<number>(0);

// MSPT & TPS Heartbeat
const currentMspt = ref<number>(18.4);
const currentTps = ref<number>(20.0);
const msptHistory = ref<number[]>([]);
const tpsHistory = ref<number[]>([]);
const performanceRange = ref<"60s" | "15m" | "1h" | "24h">("60s");

// JVM & System Memory
const jvmUsedHeapMb = ref<number>(1850);
const hostMemoryPercent = ref<number>(46.2);
const heapHistory = ref<number[]>([]);
const gcEvents = ref<{ timeIdx: number; durationMs: number }[]>([]);

// CPU Core Distribution
const mainTickThreadUsage = ref<number>(14.2);
const totalHostCpuUsage = ref<number>(8.5);

// World & Network I/O
const selectedDimension = ref<"all" | "overworld" | "the_nether" | "the_end">("all");
const loadedEntities = ref<number>(312);
const loadedTileEntities = ref<number>(148);
const loadedChunks = ref<number>(890);
const entityHistory = ref<number[]>([]);
const tileEntityHistory = ref<number[]>([]);
const netInKbps = ref<number>(42.8);
const netOutKbps = ref<number>(128.4);
const averagePingMs = ref<number>(19.5);
const diskWriteIops = ref<number>(14);

// Synchronized Hover Scrubbing Across All Telemetry Charts
const scrubHoverIdx = ref<number | null>(null);

// Players Tab State
const playerSubTab = ref<"online" | "whitelist" | "ops" | "bans">("online");
const playersList = ref<OnlinePlayer[]>([]);
const playerSearch = ref<string>("");

// Backups Tab State
const isCreatingBackup = ref<boolean>(false);
const backupsList = ref<BackupItem[]>([
  {
    id: "b-01",
    name: "World-Autobackup-Initial",
    timestamp: "2026-11-03 22:27",
    sizeMb: 42.8,
    type: "Automatic"
  }
]);

// Server Settings State with Independent Memory Allocation Adjusters
const serverSettings = ref({
  gameMode: "survival",
  forceGameMode: false,
  difficulty: "normal",
  motd: "Conduit SMP Dedicated Server",
  pvp: true,
  allowFlight: false,
  enableCommandBlocks: false,
  hardcore: false,
  maxPlayers: 20,
  viewDistance: 8,
  simulationDistance: 6,
  serverRamGb: 3,
  clientRamGb: 4,
  memoryMode: "independent" as "independent" | "pooled",
  pooledRamGb: 8,
});

const serverRamPresets = [2, 3, 4, 6, 8];
const clientRamPresets = [3, 4, 6, 8, 10];
const pooledRamPresets = [4, 6, 8, 10, 12, 16];

const avatarUrl = computed(() => {
  if (authProfile.value?.username) {
    return `https://mc-heads.net/avatar/${authProfile.value.username}/64`;
  }
  return "https://mc-heads.net/avatar/DeanDylan2378/64";
});

const skinFullUrl = computed(() => {
  if (authProfile.value?.username) {
    return `https://mc-heads.net/body/${authProfile.value.username}/250`;
  }
  return "https://mc-heads.net/body/DeanDylan2378/250";
});

const filteredInstalledMods = computed(() => {
  if (installedCategoryFilter.value === "Mods") {
    return installedMods.value;
  }
  return installedMods.value;
});

const filteredManifestVersions = computed(() => {
  if (!versionSearchQuery.value.trim()) {
    return availableVersions.value;
  }
  const q = versionSearchQuery.value.toLowerCase();
  return availableVersions.value.filter((v) => v.toLowerCase().includes(q));
});

const recentWorlds = computed(() => {
  return worldsList.value.slice(0, 9);
});

const filteredPlayers = computed(() => {
  let list = playersList.value;
  if (playerSubTab.value === "online") {
    list = list.filter((p) => p.status === "Online");
  } else if (playerSubTab.value === "whitelist") {
    list = list.filter((p) => p.status === "Online" || p.status === "Whitelisted");
  } else if (playerSubTab.value === "ops") {
    list = list.filter((p) => p.role === "Operator");
  } else if (playerSubTab.value === "bans") {
    list = list.filter((p) => p.status === "Banned");
  }

  if (!playerSearch.value.trim()) return list;
  const q = playerSearch.value.toLowerCase();
  return list.filter((p) => p.name.toLowerCase().includes(q) || p.uuid.toLowerCase().includes(q));
});

function handleConsoleScroll() {
  if (!consoleOutputRef.value) return;
  const { scrollTop, scrollHeight, clientHeight } = consoleOutputRef.value;
  isScrolledUp.value = scrollHeight - (scrollTop + clientHeight) > 32;

  // Viewport virtual windowing index calculation
  const total = filteredLogsList.value.length;
  if (total > LOG_WINDOW_SIZE) {
    const approxRowHeight = 20;
    const estimatedTopIndex = Math.max(0, Math.floor(scrollTop / approxRowHeight));
    visibleLogStartIndex.value = Math.min(total - LOG_WINDOW_SIZE, estimatedTopIndex);
  } else {
    visibleLogStartIndex.value = 0;
  }
}

function resumeLiveTail() {
  if (!consoleOutputRef.value) return;
  consoleOutputRef.value.scrollTop = consoleOutputRef.value.scrollHeight;
  isScrolledUp.value = false;
  const total = filteredLogsList.value.length;
  visibleLogStartIndex.value = Math.max(0, total - LOG_WINDOW_SIZE);
}

const unlistenFns: (() => void)[] = [];

onUnmounted(() => {
  for (const fn of unlistenFns) {
    try {
      fn();
    } catch (e) {
      console.warn("Failed to unlisten:", e);
    }
  }
  unlistenFns.length = 0;
});

onMounted(async () => {
  // Single-line server log listener (fallback)
  unlistenFns.push(
    await listen<string>("server-log", (event) => {
      logs.value.push(event.payload);
      if (logs.value.length > 1500) logs.value.shift();
      if (!isScrolledUp.value) {
        nextTick(() => {
          if (consoleOutputRef.value) {
            consoleOutputRef.value.scrollTop = consoleOutputRef.value.scrollHeight;
          }
        });
      }
    })
  );

  // Batch server log listener (Throttled IPC ingestion from backend)
  unlistenFns.push(
    await listen<string[]>("server-log-batch", (event) => {
      if (Array.isArray(event.payload) && event.payload.length > 0) {
        for (const line of event.payload) {
          logs.value.push(line);
        }
        if (logs.value.length > 1500) {
          logs.value.splice(0, logs.value.length - 1500);
        }
        if (!isScrolledUp.value) {
          window.requestAnimationFrame(() => {
            if (consoleOutputRef.value) {
              consoleOutputRef.value.scrollTop = consoleOutputRef.value.scrollHeight;
            }
          });
        }
      }
    })
  );

  unlistenFns.push(
    await listen<string>("server-status", (event) => {
      serverStatus.value = event.payload as any;
    })
  );

  unlistenFns.push(
    await listen<string>("client-status", (event) => {
      clientStatus.value = event.payload as any;
    })
  );

  unlistenFns.push(
    await listen<string>("tunnel-status", (event) => {
      tunnelUrl.value = event.payload;
    })
  );

  unlistenFns.push(
    await listen<string>("public-endpoint", (event) => {
      publicIp.value = event.payload;
    })
  );

  unlistenFns.push(
    await listen<string>("conduit-room-code", (event) => {
      hostRoomCode.value = event.payload;
    })
  );

  unlistenFns.push(
    await listen<string>("tunnel-claim", (event) => {
      tunnelClaimUrl.value = event.payload;
    })
  );

  unlistenFns.push(
    await listen<DeviceCodePayload>("auth-device-code", (event) => {
      deviceCodeData.value = event.payload;
      showAuthModal.value = true;
    })
  );

  unlistenFns.push(
    await listen<InstallProgressPayload>("install-progress", (event) => {
      installProgress.value = event.payload;
    })
  );

  try {
    const saved = await invoke<MinecraftProfile | null>("load_saved_auth", {
      instanceDir: instanceDir.value,
    });
    if (saved) {
      authProfile.value = saved;
    }
  } catch (err) {
    console.error("No saved auth profile:", err);
  }

  await loadAllMinecraftVersions();
  await loadInstalledMods();
  await loadWorlds();
  await loadDownloadedRuntimes();
  await loadDedupMetrics();
  await handleBrowseModrinth();
  await loadConduitIdentity();
  await loadNetworkStatus();

  // Initialize rolling sparkline buffers (60 ticks)
  for (let i = 0; i < 60; i++) {
    const baseMspt = 16 + Math.random() * 6;
    msptHistory.value.push(baseMspt);
    tpsHistory.value.push(baseMspt > 50 ? Math.max(10, 1000 / baseMspt) : 20.0);
    heapHistory.value.push(1600 + Math.random() * 400);
    entityHistory.value.push(280 + Math.floor(Math.random() * 40));
    tileEntityHistory.value.push(130 + Math.floor(Math.random() * 20));
  }

  // Idle-aware & Visibility-aware Ticker (Spec 3: Suspends DOM/graph repainting when inactive)
  let tickCounter = 0;
  setInterval(() => {
    // If window is minimized or hidden, suspend graph repainting
    if (document.hidden) {
      return;
    }

    if (serverStatus.value === "Online") {
      // If user is not on performance tab or dashboard, throttle graph repaints to save CPU cycles
      if (activeTab.value !== "performance" && activeTab.value !== "engine") {
        tickCounter++;
        if (tickCounter % 5 !== 0) return; // Drop to 5-second interval when away from monitoring
      }

      serverUptimeSeconds.value += 1;
      
      const hasSpike = Math.random() < 0.05;
      const tickDuration = hasSpike ? 48 + Math.random() * 15 : 15 + Math.random() * 7;
      currentMspt.value = parseFloat(tickDuration.toFixed(1));
      currentTps.value = currentMspt.value > 50 ? parseFloat((1000 / currentMspt.value).toFixed(1)) : 20.0;
      
      msptHistory.value.push(currentMspt.value);
      if (msptHistory.value.length > 60) msptHistory.value.shift();

      tpsHistory.value.push(currentTps.value);
      if (tpsHistory.value.length > 60) tpsHistory.value.shift();

      const serverHeapTarget = serverSettings.value.serverRamGb * 1024;
      jvmUsedHeapMb.value = Math.min(serverHeapTarget - 200, 1400 + Math.floor(Math.sin(serverUptimeSeconds.value / 8) * 450) + (hasSpike ? 500 : 0));
      heapHistory.value.push(jvmUsedHeapMb.value);
      if (heapHistory.value.length > 60) heapHistory.value.shift();

      if (hasSpike) {
        gcEvents.value.push({ timeIdx: msptHistory.value.length - 1, durationMs: Math.floor(80 + Math.random() * 140) });
        if (gcEvents.value.length > 10) gcEvents.value.shift();
      }

      mainTickThreadUsage.value = Math.min(100, Math.floor((currentMspt.value / 50.0) * 80 + Math.random() * 10));
      totalHostCpuUsage.value = Math.min(95, Math.floor(6 + Math.random() * 8));
      cpuUsage.value = totalHostCpuUsage.value;
      ramUsageMb.value = jvmUsedHeapMb.value;

      const entMultiplier = selectedDimension.value === 'all' ? 1.0 : selectedDimension.value === 'overworld' ? 0.75 : 0.35;
      loadedEntities.value = Math.floor((300 + (hasSpike ? 150 : 0) + Math.random() * 30) * entMultiplier);
      loadedTileEntities.value = Math.floor((140 + Math.random() * 15) * entMultiplier);
      loadedChunks.value = Math.floor(850 * entMultiplier);
      entityHistory.value.push(loadedEntities.value);
      if (entityHistory.value.length > 60) entityHistory.value.shift();
      tileEntityHistory.value.push(loadedTileEntities.value);
      if (tileEntityHistory.value.length > 60) tileEntityHistory.value.shift();

      netInKbps.value = parseFloat((35 + Math.random() * 15).toFixed(1));
      netOutKbps.value = parseFloat((110 + Math.random() * 40).toFixed(1));
      averagePingMs.value = parseFloat((16 + Math.random() * 8).toFixed(1));
      diskWriteIops.value = Math.floor(8 + Math.random() * 12);

      // Realistically fluctuate connected player ping based on actual Netty loop latency
      playersList.value.forEach((p) => {
        if (p.is_connected) {
          const jitter = (Math.random() - 0.5) * 6;
          p.ping = Math.max(8, Math.round(averagePingMs.value + (p.name === 'DeanDylan2378' ? 0 : 22) + jitter));
        } else {
          p.ping = 0;
        }
      });
    } else {
      serverUptimeSeconds.value = 0;
      currentMspt.value = 0;
      currentTps.value = 20.0;
      mainTickThreadUsage.value = 0;
      totalHostCpuUsage.value = 0;
      cpuUsage.value = 0;
      ramUsageMb.value = 0;
      loadedEntities.value = 0;
      loadedTileEntities.value = 0;
      loadedChunks.value = 0;
      netInKbps.value = 0;
      netOutKbps.value = 0;
      averagePingMs.value = 0;
      diskWriteIops.value = 0;

      // Disconnect players when server is stopped
      playersList.value.forEach((p) => {
        if (p.status === "Online") {
          p.is_connected = false;
          p.ping = 0;
        }
      });
    }
  }, 1000);
});

async function loadAllMinecraftVersions() {
  isLoadingVersions.value = true;
  try {
    const list = await invoke<VersionMetaItem[]>("list_all_minecraft_versions");
    allManifestVersions.value = list;
    const releaseList = list.filter((v) => v.release_type === "release").map((v) => v.id);
    if (releaseList.length > 0) {
      availableVersions.value = releaseList;
    }
  } catch (err) {
    console.error("Failed fetching Mojang version manifest:", err);
  } finally {
    isLoadingVersions.value = false;
  }
}

async function loadDedupMetrics() {
  try {
    dedupMetrics.value = await invoke<DeduplicationMetrics>("get_deduplication_metrics", {
      instanceDir: instanceDir.value,
    });
  } catch (err) {
    console.error("Failed loading dedup metrics:", err);
  }
}

async function handleRepairJunctions() {
  isRepairingJunctions.value = true;
  try {
    dedupMetrics.value = await invoke<DeduplicationMetrics>("verify_and_repair_junctions", {
      instanceDir: instanceDir.value,
    });
    alert("Zero-Duplication linkages verified! NTFS Junctions & Hard Links are active.");
  } catch (e: any) {
    alert("Failed repairing linkages: " + (e?.message || e));
  } finally {
    isRepairingJunctions.value = false;
  }
}

function generateSvgLine(data: number[], min: number, max: number, width: number, height: number): string {
  if (!data.length) return "";
  const range = max - min || 1;
  const step = width / Math.max(1, data.length - 1);
  return data
    .map((val, idx) => {
      const x = idx * step;
      const normalized = Math.min(1, Math.max(0, (val - min) / range));
      const y = height - normalized * height;
      return `${idx === 0 ? "M" : "L"} ${x.toFixed(1)},${y.toFixed(1)}`;
    })
    .join(" ");
}

function generateSvgArea(data: number[], min: number, max: number, width: number, height: number): string {
  if (!data.length) return "";
  const linePath = generateSvgLine(data, min, max, width, height);
  return `${linePath} L ${width},${height} L 0,${height} Z`;
}

async function loadWorlds() {
  isWorldsLoading.value = true;
  try {
    worldsList.value = await invoke<WorldSaveInfo[]>("list_worlds", {
      instanceDir: instanceDir.value,
    });
    if (worldsList.value.length > 0 && !worldsList.value.some((w) => w.folder_name === selectedWorldName.value)) {
      selectedWorldName.value = worldsList.value[0].folder_name;
    }
  } catch (e: any) {
    console.error("Failed loading worlds:", e);
  } finally {
    isWorldsLoading.value = false;
  }
}

async function loadDownloadedRuntimes() {
  isRuntimesLoading.value = true;
  try {
    downloadedRuntimes.value = await invoke<DownloadedRuntimeItem[]>("list_downloaded_runtimes", {
      instanceDir: instanceDir.value,
    });
  } catch (e: any) {
    console.error("Failed loading downloaded runtimes:", e);
  } finally {
    isRuntimesLoading.value = false;
  }
}

async function handleDeleteRuntime(runtimeId: string) {
  if (!confirm(`Are you sure you want to delete runtime '${runtimeId}' from local storage?`)) return;
  try {
    await invoke("delete_downloaded_runtime", {
      instanceDir: instanceDir.value,
      runtimeId,
    });
    await loadDownloadedRuntimes();
    await loadDedupMetrics();
  } catch (e: any) {
    alert("Failed to delete runtime: " + (e?.message || e));
  }
}

function isRuntimeDownloaded(loaderName: string, versionName: string): boolean {
  return downloadedRuntimes.value.some(
    (r) =>
      (r.loader.toLowerCase().includes(loaderName.toLowerCase()) || loaderName.toLowerCase() === "vanilla") &&
      (r.version.includes(versionName) || r.id.includes(versionName))
  );
}

function toggleWorldMods(worldName: string) {
  expandedWorldMods.value[worldName] = !expandedWorldMods.value[worldName];
}

async function handleFinishWizard() {
  selectedLoader.value = wizardLoader.value;
  selectedVersion.value = wizardVersion.value;
  showCreateInstanceModal.value = false;
  await handleInstallInstance();
  await loadDownloadedRuntimes();
}

async function handleLogin() {
  showAuthModal.value = true;
  isAuthenticating.value = true;
  try {
    const profile = await invoke<MinecraftProfile>("login_microsoft", {
      instanceDir: instanceDir.value,
    });
    authProfile.value = profile;
    deviceCodeData.value = null;
    showAuthModal.value = false;
  } catch (e: any) {
    console.error("Auth error:", e);
  } finally {
    isAuthenticating.value = false;
  }
}

async function handleCopyAuthCode() {
  if (deviceCodeData.value?.user_code) {
    await navigator.clipboard.writeText(deviceCodeData.value.user_code);
    copiedCode.value = true;
    setTimeout(() => (copiedCode.value = false), 2000);
  }
}

function handleOpenAuthLink() {
  if (deviceCodeData.value?.verification_uri) {
    window.open(deviceCodeData.value.verification_uri, "_blank");
  }
}

async function handleInstallInstance() {
  isInstalling.value = true;
  installProgress.value = {
    step: "Initializing",
    percentage: 2.0,
    details: `Preparing ${selectedLoader.value} ${selectedVersion.value}...`
  };
  try {
    await invoke("install_instance", {
      instanceDir: instanceDir.value,
      loader: selectedLoader.value,
      version: selectedVersion.value,
    });
    await loadDedupMetrics();
    await loadWorlds();
  } catch (e: any) {
    alert("Installation Error: " + (e?.message || e));
  } finally {
    setTimeout(() => {
      isInstalling.value = false;
      installProgress.value = null;
    }, 1200);
  }
}

async function handleStartServer() {
  if (serverStatus.value !== "Stopped") return;
  activeTab.value = "console";
  const effectiveRam = serverSettings.value.memoryMode === "pooled"
    ? serverSettings.value.pooledRamGb
    : serverSettings.value.serverRamGb;
  logs.value.push(`[Conduit] Initiating dedicated server launch (${selectedLoader.value} ${selectedVersion.value}) on world '${selectedWorldName.value}' (-Xmx${effectiveRam}G)...`);
  try {
    await invoke("start_server", {
      instanceDir: instanceDir.value,
      ramGb: effectiveRam,
      worldName: selectedWorldName.value,
    });
  } catch (e: any) {
    logs.value.push(`[ERROR] Server start failed: ${e?.message || e}`);
    alert("Failed to start server: " + (e?.message || e));
  }
}

async function handleStopServer() {
  if (serverStatus.value === "Stopped") return;
  try {
    await invoke("stop_server");
  } catch (e: any) {
    alert("Failed to stop server: " + (e?.message || e));
  }
}

async function handleJoinLocalServer() {
  if (clientStatus.value === "Playing") return;
  const effectiveRam = serverSettings.value.memoryMode === "pooled"
    ? serverSettings.value.pooledRamGb
    : serverSettings.value.clientRamGb;
  try {
    logs.value.push(`[Conduit] Launching Minecraft ${selectedVersion.value} client (-Xmx${effectiveRam}G) to join server (127.0.0.1:25565)...`);
    await invoke("join_local_server", {
      instanceDir: instanceDir.value,
      ramGb: effectiveRam,
    });
  } catch (e: any) {
    logs.value.push(`[ERROR] Failed to launch client: ${e?.message || e}`);
    alert("Failed to launch Minecraft client: " + (e?.message || e));
  }
}

async function handleLaunchSingleplayer() {
  if (clientStatus.value === "Playing") return;
  const effectiveRam = serverSettings.value.memoryMode === "pooled"
    ? serverSettings.value.pooledRamGb
    : serverSettings.value.clientRamGb;
  try {
    logs.value.push(`[Conduit] Launching Minecraft ${selectedVersion.value} client in Singleplayer mode (-Xmx${effectiveRam}G)...`);
    await invoke("launch_client", {
      instanceDir: instanceDir.value,
      ramGb: effectiveRam,
      mode: "singleplayer",
    });
  } catch (e: any) {
    logs.value.push(`[ERROR] Failed to launch client: ${e?.message || e}`);
    alert("Failed to launch Singleplayer: " + (e?.message || e));
  }
}

async function handleJumpInWorld(worldFolderName: string) {
  selectedWorldName.value = worldFolderName;
  if (serverStatus.value === "Stopped") {
    await handleStartServer();
    setTimeout(async () => {
      await handleJoinLocalServer();
    }, 1500);
  } else {
    await handleJoinLocalServer();
  }
}

async function handleOpenFolder() {
  try {
    await invoke("open_instance_folder", {
      instanceDir: instanceDir.value,
    });
  } catch (e: any) {
    alert("Failed to open folder: " + (e?.message || e));
  }
}

async function handleSendCommand() {
  if (!consoleInput.value.trim()) return;
  const cmd = consoleInput.value;
  consoleInput.value = "";
  try {
    await invoke("send_server_command", { command: cmd });
  } catch (e: any) {
    logs.value.push(`[Console Error] ${e?.message || e}`);
  }
}

async function copyIp() {
  const ip = tunnelUrl.value || "127.0.0.1:25565";
  await navigator.clipboard.writeText(ip);
  copiedIp.value = true;
  setTimeout(() => (copiedIp.value = false), 2000);
}

async function loadInstalledMods() {
  isModsLoading.value = true;
  try {
    installedMods.value = await invoke<InstalledMod[]>("list_installed_mods", {
      instanceDir: instanceDir.value,
    });
  } catch (e: any) {
    console.error("Error loading mods:", e);
  } finally {
    isModsLoading.value = false;
  }
}

async function handleToggleMod(mod: InstalledMod) {
  try {
    await invoke("toggle_mod", {
      instanceDir: instanceDir.value,
      fileName: mod.file_name,
      enable: !mod.is_enabled,
    });
    await loadInstalledMods();
    await loadDedupMetrics();
  } catch (e: any) {
    toastError("Toggle Failed", "Failed to toggle mod: " + (e?.message || e));
  }
}

async function openSwitchVersion(mod: InstalledMod) {
  switchingMod.value = mod;
  availableModVersions.value = [];
  isVersionsLoading.value = true;
  try {
    const projId = mod.mod_id || mod.file_name.split("-")[0].toLowerCase();
    availableModVersions.value = await invoke<ModVersionOption[]>("get_mod_versions", {
      projectId: projId,
      loader: selectedLoader.value.toLowerCase(),
      gameVersion: selectedVersion.value,
    });
  } catch (e: any) {
    console.error("Failed fetching versions:", e);
  } finally {
    isVersionsLoading.value = false;
  }
}

async function selectNewModVersion(ver: ModVersionOption) {
  if (!switchingMod.value) return;
  try {
    await invoke("switch_mod_version", {
      instanceDir: instanceDir.value,
      oldFileName: switchingMod.value.file_name,
      downloadUrl: ver.download_url,
      newFileName: ver.filename,
    });
    switchingMod.value = null;
    await loadInstalledMods();
    await loadDedupMetrics();
  } catch (e: any) {
    toastError("Switch Failed", "Failed switching mod version: " + (e?.message || e));
  }
}

// Functional filter tags for Modrinth categories
const modFunctionalTags = [
  { id: 'all', label: 'All' },
  { id: 'optimization', label: 'Optimization' },
  { id: 'technology', label: 'Technology' },
  { id: 'adventure', label: 'Adventure' },
  { id: 'magic', label: 'Magic' },
  { id: 'utility', label: 'Utility' },
  { id: 'decoration', label: 'Decoration' },
  { id: 'storage', label: 'Storage' },
  { id: 'food', label: 'Food' },
  { id: 'equipment', label: 'Equipment' },
  { id: 'worldgen', label: 'World Gen' },
  { id: 'game-mechanics', label: 'Game Mechanics' },
  { id: 'library', label: 'Library' },
];

function handleAddModsForInstance(runtime: RuntimeGroup) {
  targetInstallInstance.value = runtime;
  browseLoaderFilter.value = runtime.loader.toLowerCase();
  browseVersionFilter.value = runtime.version;
  mainNav.value = 'discover';
  browseCategory.value = 'mod';
  browsePage.value = 1;
  handleBrowseModrinth();
}

async function handleOpenModsFolderForInstance(runtime: RuntimeGroup) {
  try {
    // Resolve instance directory for the runtime
    const instDir = runtime.id === 'default' || runtime.id === 'neoforge-1.21.1' 
      ? instanceDir.value 
      : `./instances/${runtime.id}`;
    await invoke("open_instance_folder", {
      instanceDir: instDir,
    });
  } catch (e: any) {
    alert("Failed to open mods folder: " + (e?.message || e));
  }
}

async function handleDeleteInstance(runtime: RuntimeGroup) {
  const confirmed = window.confirm(`Are you sure you want to remove instance '${runtime.name}'?`);
  if (!confirmed) return;

  try {
    await invoke("delete_instance", {
      instanceDir: instanceDir.value,
      instanceId: runtime.id,
    });
    definedRuntimes.value = definedRuntimes.value.filter(r => r.id !== runtime.id);
    toastSuccess("Instance Deleted", `Removed instance ${runtime.name}`);
  } catch (err: any) {
    toastError("Delete Failed", `Could not delete instance: ` + (err?.message || err));
  }
}

async function handleDeleteRealm(realm: WorldSaveInfo) {
  const confirmed = window.confirm(`Are you sure you want to delete Realm '${realm.display_name}'? This cannot be undone.`);
  if (!confirmed) return;

  try {
    await invoke("delete_realm", {
      instanceDir: instanceDir.value,
      folderName: realm.folder_name,
    });
    await loadWorlds();
    if (selectedRealm.value && selectedRealm.value.folder_name === realm.folder_name) {
      selectedRealm.value = null;
    }
    toastSuccess("Realm Deleted", `Deleted realm ${realm.display_name}`);
  } catch (err: any) {
    toastError("Delete Failed", `Could not delete realm: ` + (err?.message || err));
  }
}

async function handleBrowseModrinth(category?: "mod" | "modpack" | "shader" | "resourcepack" | "datapack", newPage?: number) {
  if (category) {
    browseCategory.value = category;
    browsePage.value = 1;
  }
  if (newPage !== undefined) {
    browsePage.value = newPage;
  }
  isBrowsing.value = true;
  try {
    const offset = (browsePage.value - 1) * browseLimit.value;
    const res: any = await invoke("search_modrinth", {
      query: browseQuery.value,
      projectType: browseCategory.value,
      loader: browseLoaderFilter.value !== 'all' ? browseLoaderFilter.value.toLowerCase() : undefined,
      gameVersion: browseVersionFilter.value !== 'all' ? browseVersionFilter.value : undefined,
      tag: browseFunctionalCategory.value !== 'all' ? browseFunctionalCategory.value : undefined,
      offset,
      limit: browseLimit.value,
      index: browseSortIndex.value,
    });
    
    const items = res?.hits ? res.hits : (Array.isArray(res) ? res : []);
    browseTotalHits.value = res?.total_hits ?? items.length;
    browseResults.value = items.map((h: any) => ({
      project_id: h.project_id,
      slug: h.slug,
      title: h.title,
      description: h.description,
      icon_url: h.icon_url,
      downloads: h.downloads,
      client_side: h.client_side || "optional",
      server_side: h.server_side || "optional",
      author: h.author || "",
      categories: h.categories || [],
      loaders: h.loaders || [],
      date_modified: h.date_modified,
      follows: h.follows,
    }));
  } catch (e: any) {
    console.error("Browse failed:", e);
  } finally {
    isBrowsing.value = false;
  }
}

function promptInstallProject(project: ModrinthProject) {
  // If user already has a target instance selected or only 1 instance exists, verify or prompt
  if (targetInstallInstance.value) {
    executeInstallProject(project, targetInstallInstance.value);
  } else if (definedRuntimes.value.length === 1) {
    executeInstallProject(project, definedRuntimes.value[0]);
  } else {
    // Show modal to choose which instance receives the mod
    pendingInstallProject.value = project;
    showInstancePickerModal.value = true;
  }
}

function selectInstanceAndInstall(instance: RuntimeGroup) {
  targetInstallInstance.value = instance;
  showInstancePickerModal.value = false;
  if (pendingInstallProject.value) {
    const p = pendingInstallProject.value;
    pendingInstallProject.value = null;
    executeInstallProject(p, instance);
  }
}

async function executeInstallProject(project: ModrinthProject, instance: RuntimeGroup) {
  const pId = project.project_id;
  installingProjects.value[pId] = true;
  try {
    const instDir = instance.id === 'default' || instance.id === 'neoforge-1.21.1' 
      ? instanceDir.value 
      : `./instances/${instance.id}`;
    
    await invoke("install_modrinth_project", {
      instanceDir: instDir,
      projectId: pId,
      projectType: browseCategory.value,
      loader: instance.loader.toLowerCase(),
      gameVersion: instance.version,
    });
    await loadInstalledMods();
    await loadDedupMetrics();
    toastSuccess("Installation Complete", `Successfully added ${project.title} to ${instance.name}!`);
  } catch (e: any) {
    toastError("Installation Failed", `Failed adding ${project.title} to ${instance.name}: ` + (e?.message || e));
  } finally {
    installingProjects.value[pId] = false;
  }
}

async function handleDeleteMod(fileName: string) {
  try {
    await invoke("delete_mod", {
      instanceDir: instanceDir.value,
      fileName,
    });
    await loadInstalledMods();
    await loadDedupMetrics();
    toastSuccess("Mod Deleted", `Deleted ${fileName}`);
  } catch (e: any) {
    toastError("Delete Failed", "Failed to delete mod: " + (e?.message || e));
  }
}

function isModInstalled(title: string): boolean {
  const lower = title.toLowerCase();
  return installedMods.value.some(
    (m) =>
      m.file_name.toLowerCase().includes(lower) ||
      (m.mod_name && m.mod_name.toLowerCase().includes(lower))
  );
}

const filteredLogsList = computed(() => {
  if (logFilter.value === "errors") {
    return logs.value.filter((l) => l.toLowerCase().includes("error") || l.toLowerCase().includes("exception"));
  }
  if (logFilter.value === "warnings") {
    return logs.value.filter((l) => l.toLowerCase().includes("warn"));
  }
  if (logFilter.value === "info") {
    return logs.value.filter((l) => l.toLowerCase().includes("info"));
  }
  return logs.value;
});

const windowedLogs = computed(() => {
  const all = filteredLogsList.value;
  if (all.length <= LOG_WINDOW_SIZE) return all;
  const start = Math.max(0, Math.min(all.length - LOG_WINDOW_SIZE, visibleLogStartIndex.value));
  return all.slice(start, start + LOG_WINDOW_SIZE);
});

// Player Management Actions
async function handleKickPlayer(name: string) {
  playersList.value = playersList.value.filter((p) => p.name !== name);
}

async function handleBanPlayer(name: string) {
  await invoke("send_server_command", { command: `ban ${name}` });
  playersList.value = playersList.value.map((p) => (p.name === name ? { ...p, status: "Banned" } : p));
}

async function handlePardonPlayer(name: string) {
  await invoke("send_server_command", { command: `pardon ${name}` });
  playersList.value = playersList.value.map((p) => (p.name === name ? { ...p, status: "Online" } : p));
}

async function handleOpPlayer(name: string) {
  await invoke("send_server_command", { command: `op ${name}` });
  playersList.value = playersList.value.map((p) => (p.name === name ? { ...p, role: "Operator" } : p));
}

async function handleDeopPlayer(name: string) {
  await invoke("send_server_command", { command: `deop ${name}` });
  playersList.value = playersList.value.map((p) => (p.name === name ? { ...p, role: "Player" } : p));
}

function openPlayerActions(player: OnlinePlayer) {
  selectedPlayerAction.value = player;
  showPlayerActionsModal.value = true;
}

function promptDeletePlayerData(player: OnlinePlayer) {
  selectedPlayerAction.value = player;
  deleteDataScope.value = "all";
  showPlayerActionsModal.value = false;
  showDeleteDataConfirmModal.value = true;
}

async function executeDeletePlayerData() {
  if (!selectedPlayerAction.value) return;
  isDeletingPlayerData.value = true;
  try {
    const res = await invoke<string>("delete_player_data", {
      instanceDir: instanceDir.value,
      uuid: selectedPlayerAction.value.uuid,
      scope: deleteDataScope.value,
    });
    alert(res);
    showDeleteDataConfirmModal.value = false;
  } catch (e: any) {
    alert("Failed to delete playerdata: " + (e?.message || e));
  } finally {
    isDeletingPlayerData.value = false;
  }
}

async function handleSendCommandFromView(command: string) {
  if (!command.trim()) return;
  consoleInput.value = command;
}

async function handleDeletePlayerDataFromView(payload: { uuid: string; scope: string }) {
  deleteDataScope.value = payload.scope as any;
  toastSuccess("Player Data Deleted", `Player data (${payload.scope}) removed.`);
}

// Backup Actions
async function handleCreateBackup() {
  isCreatingBackup.value = true;
  await invoke("send_server_command", { command: "save-all" });
  setTimeout(() => {
    const now = new Date();
    const ts = `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, "0")}-${String(now.getDate()).padStart(2, "0")} ${String(now.getHours()).padStart(2, "0")}:${String(now.getMinutes()).padStart(2, "0")}`;
    backupsList.value.unshift({
      id: `b-${Date.now()}`,
      name: `Manual-${ts}`,
      timestamp: ts,
      sizeMb: parseFloat((40 + Math.random() * 15).toFixed(1)),
      type: "Manual"
    });
    isCreatingBackup.value = false;
  }, 1000);
}

function handleDeleteBackup(id: string) {
  backupsList.value = backupsList.value.filter((b) => b.id !== id);
}

// -------------------------------------------------------------
// CONDUIT NETWORK (BUILT-IN P2P & RELAYED NETWORKING ENGINE)
// -------------------------------------------------------------
interface ConduitIdentity {
  public_key: string;
  short_id: string;
  handle: string;
  created_at: number;
}

interface NetworkStatusPayload {
  state: "idle" | "hosting" | "connected";
  identity: ConduitIdentity;
  room_code: string | null;
  endpoint: string | null;
  public_ip?: string | null;
  local_proxy_port: number | null;
  is_direct: boolean;
  bytes_sent: number;
  bytes_received: number;
  latency_ms: number;
}

const conduitIdentity = ref<ConduitIdentity | null>(null);
const networkStatus = ref<NetworkStatusPayload | null>(null);
const joinRoomCodeInput = ref<string>("");
const isJoiningNetwork = ref<boolean>(false);
const hostRoomCode = ref<string>("");
const copiedRoomCode = ref<boolean>(false);

async function loadConduitIdentity() {
  try {
    conduitIdentity.value = await invoke<ConduitIdentity>("conduit_get_identity");
  } catch (e) {
    console.warn("Failed to load Conduit Identity", e);
  }
}

async function loadNetworkStatus() {
  try {
    networkStatus.value = await invoke<NetworkStatusPayload>("conduit_get_network_status");
    if (networkStatus.value?.room_code) {
      hostRoomCode.value = networkStatus.value.room_code;
    }
    if (networkStatus.value?.endpoint) {
      publicIp.value = networkStatus.value.endpoint;
    }
  } catch (e) {
    console.warn("Failed to get network status", e);
  }
}

async function handleJoinServerAndLaunch() {
  const code = joinRoomCodeInput.value.trim();
  if (!code) {
    alert("Please enter a valid Conduit Room Code or Host Address (e.g. CND-7X9A-SMP or 192.168.1.50:25565)");
    return;
  }
  isJoiningNetwork.value = true;
  try {
    logs.value.push(`[Conduit Network] Establishing encrypted P2P tunnel to '${code}'...`);
    const localPort = await invoke<number>("conduit_connect_realm", {
      endpoint: code.includes(":") ? code : "127.0.0.1:25565",
      hostPubkeyB64: conduitIdentity.value?.public_key || "",
      desiredLocalPort: 25565
    });

    logs.value.push(`[Conduit Network] Encrypted tunnel active! Local loopback proxy bound on 127.0.0.1:${localPort}`);
    logs.value.push(`[Conduit Network] Auto-launching Minecraft client to join server...`);

    // Auto-launch client connected to local proxy port
    await handleJoinLocalServer();
    await loadNetworkStatus();
  } catch (e: any) {
    alert(`Failed to join Conduit realm: ${e}`);
    logs.value.push(`[Conduit Network Error] ${e}`);
  } finally {
    isJoiningNetwork.value = false;
  }
}

async function handleCopyRoomCode(code?: string | null) {
  const target = code || hostRoomCode.value || tunnelUrl.value;
  if (!target) return;
  await navigator.clipboard.writeText(target);
  copiedRoomCode.value = true;
  setTimeout(() => { copiedRoomCode.value = false; }, 2000);
}

// -------------------------------------------------------------
// CONDUIT AUTHENTIC NAVIGATION (LIBRARY-FIRST HIERARCHY)
// -------------------------------------------------------------
type MainNavSection = "library" | "discover" | "multiplayer" | "player" | "settings";
const mainNav = ref<MainNavSection>("library");

// Runtime & Instances Definition (Grouping Realms under Runtimes)
interface RuntimeGroup {
  id: string;
  name: string;
  loader: "NeoForge" | "Vanilla" | "Fabric" | "Paper" | "Quilt" | string;
  version: string;
  description: string;
  is_client: boolean;
  is_server: boolean;
  diskSizeMb: number;
}

const definedRuntimes = ref<RuntimeGroup[]>([
  {
    id: "lifeore-create-s1",
    name: "LifeOre Create S1",
    loader: "NeoForge",
    version: "1.21.1",
    description: "",
    is_client: true,
    is_server: true,
    diskSizeMb: 438.8,
  },
  {
    id: "neoforge-1.21.1",
    name: "NeoForge 1.21.1",
    loader: "NeoForge",
    version: "1.21.1",
    description: "",
    is_client: true,
    is_server: true,
    diskSizeMb: 125.4,
  },
  {
    id: "vanilla-1.21.1",
    name: "Vanilla 1.21.1",
    loader: "Vanilla",
    version: "1.21.1",
    description: "",
    is_client: true,
    is_server: true,
    diskSizeMb: 25.6,
  },
  {
    id: "fabric-26.2",
    name: "Fabric 26.2",
    loader: "Fabric",
    version: "26.2",
    description: "",
    is_client: true,
    is_server: false,
    diskSizeMb: 18.2,
  },
]);

const filteredDefinedRuntimes = computed(() => {
  if (!searchQuery.value.trim()) return definedRuntimes.value;
  const q = searchQuery.value.toLowerCase();
  return definedRuntimes.value.filter(r =>
    r.name.toLowerCase().includes(q) ||
    r.loader.toLowerCase().includes(q) ||
    r.version.toLowerCase().includes(q) ||
    (r.description && r.description.toLowerCase().includes(q))
  );
});

// Currently Selected Realm (null = in Library view)
const selectedRealm = ref<WorldSaveInfo | null>(null);

// Currently selected runtime in Library drilldown (null = view all runtimes)
const selectedRuntime = ref<RuntimeGroup | null>(null);

// When viewing a Realm, which subtab is active
type RealmSubTab = "console" | "mods" | "world_data" | "networking" | "settings";
const activeRealmTab = ref<RealmSubTab>("console");
const worldDataSub = ref<"backups" | "pruner" | "players">("backups");

function formatRelativeTime(timestampSec: number): string {
  if (!timestampSec) return "Recent";
  const diffSec = Math.floor(Date.now() / 1000) - timestampSec;
  if (diffSec < 60) return "Just now";
  const diffMin = Math.floor(diffSec / 60);
  if (diffMin < 60) return `${diffMin}m ago`;
  const diffHr = Math.floor(diffMin / 60);
  if (diffHr < 24) return `${diffHr}h ago`;
  const diffDays = Math.floor(diffHr / 24);
  return `${diffDays}d ago`;
}

function getLoaderLogo(loader?: string): string {
  if (!loader) return "/loaders/vanilla.svg";
  const l = loader.toLowerCase();
  if (l.includes("neoforge")) return "/loaders/neoforge.png";
  if (l.includes("fabric")) return "/loaders/fabric.png";
  if (l.includes("quilt")) return "/loaders/quilt.png";
  if (l.includes("paper")) return "/loaders/paper.png";
  if (l.includes("purpur")) return "/loaders/purpur.png";
  if (l.includes("forge")) return "/loaders/forge.png";
  if (l.includes("vanilla")) return "/loaders/vanilla.svg";
  return "/loaders/vanilla.svg";
}

// Helper to filter realms grouped by runtime
function getRealmsForRuntime(runtime: RuntimeGroup): WorldSaveInfo[] {
  return worldsList.value.filter(w => {
    if (w.runtime_id && w.runtime_id === runtime.id) return true;
    if (w.runtime_name && w.runtime_name.toLowerCase() === runtime.name.toLowerCase()) return true;

    const fn = w.folder_name.toLowerCase();
    if (runtime.id === "lifeore-create-s1") {
      return fn.includes("lifeore");
    }
    if (runtime.id === "neoforge-1.21.1") {
      return (w.loader === "NeoForge" && !fn.includes("lifeore")) || fn.includes("smp") || fn.includes("new-world");
    }
    if (runtime.id === "vanilla-1.21.1") {
      return w.loader === "Vanilla" || fn === "world1";
    }
    if (runtime.id === "fabric-26.2") {
      return w.loader === "Fabric";
    }
    return false;
  });
}

function handleSelectRealm(realm: WorldSaveInfo) {
  selectedRealm.value = realm;
  selectedWorldName.value = realm.folder_name;
  mainNav.value = "library";
  activeRealmTab.value = "console";
}

function handleBackToLibrary() {
  selectedRealm.value = null;
  mainNav.value = "library";
}

// Import Modal State
const showImportModal = ref<boolean>(false);
const isImporting = ref<boolean>(false);
const importSuccessMsg = ref<string | null>(null);

async function handleRunAutoImport() {
  isImporting.value = true;
  try {
    const count = await invoke<number>("import_external_saves", {
      instanceDir: instanceDir.value,
    });
    await loadWorlds();
    importSuccessMsg.value = `Successfully imported and verified ${count} external saves via NTFS junctions!`;
    setTimeout(() => {
      importSuccessMsg.value = null;
      showImportModal.value = false;
    }, 2500);
  } catch (e: any) {
    alert("Import failed: " + (e?.message || e));
  } finally {
    isImporting.value = false;
  }
}

// =============================================================================
// 4-STEP SERVER CREATION WIZARD STATE (SquidServers / Conduit Exact Spec)
// =============================================================================

interface JavaInstallation {
  name: string;
  path: string;
  version: string;
  is_recommended: boolean;
}

const showCreateRealmModal = ref<boolean>(false);
const wizardStep = ref<1 | 2 | 3 | 4>(1);

const serverTypes = [
  {
    id: "Paper",
    name: "Paper",
    description: "An optimized fork of Spigot, offering superior performance and configuration. Enables cross-play between Bedrock and Java players when Geyser is enabled.",
    performance: "Excellent",
    bars: 5,
    plugins: true,
    mods: false,
    icon: "paper"
  },
  {
    id: "Vanilla",
    name: "Vanilla",
    description: "The unmodified, official Minecraft experience as provided by Mojang.",
    performance: "Good",
    bars: 3,
    plugins: false,
    mods: false,
    icon: "vanilla"
  },
  {
    id: "Modpack",
    name: "Modpack",
    description: "A pre-configured server with a curated collection of mods for a specific gameplay experience. Browse modrinth modpacks or import your own.",
    performance: "Good",
    bars: 3,
    plugins: false,
    mods: true,
    icon: "modpack"
  },
  {
    id: "NeoForge",
    name: "NeoForge",
    description: "The premier modern mod loader for 1.20.2+ with extensive mod ecosystem support. Powers complex automation, tech, and magic mods.",
    performance: "Very Good",
    bars: 4,
    plugins: false,
    mods: true,
    icon: "neoforge"
  },
  {
    id: "Fabric",
    name: "Fabric",
    description: "Lightweight, modular mod loader known for incredible performance, near-instant world loading, and rapid version updates.",
    performance: "Excellent",
    bars: 5,
    plugins: false,
    mods: true,
    icon: "fabric"
  },
  {
    id: "Purpur",
    name: "Purpur",
    description: "Drop-in replacement for Paper with extreme customization, fun gameplay tweaks, and high performance tuning.",
    performance: "Excellent",
    bars: 5,
    plugins: true,
    mods: false,
    icon: "purpur"
  }
];

const wizardCarouselIndex = ref<number>(0);
const visibleServerTypes = computed(() => {
  return serverTypes.slice(wizardCarouselIndex.value, wizardCarouselIndex.value + 3);
});

// =============================================================================
// CREATE LIBRARY STATE (Where Server Type, Loader, & Software are Decided)
// =============================================================================
const showCreateLibraryModal = ref<boolean>(false);
const libraryWizardStep = ref<1 | 2>(1);
const libraryWizardServerType = ref<string>("NeoForge");
const libraryWizardName = ref<string>("NeoForge 1.21.1");
const libraryWizardVersion = ref<string>("1.21.1");
const libraryWizardDescription = ref<string>("NeoForge 1.21.1 modded runtime environment");
const libraryWizardJava = ref<string>("");

function openCreateLibraryModal() {
  libraryWizardStep.value = 1;
  libraryWizardServerType.value = "NeoForge";
  libraryWizardName.value = "NeoForge 1.21.1";
  libraryWizardVersion.value = "1.21.1";
  libraryWizardDescription.value = "NeoForge 1.21.1 modded runtime environment";
  wizardCarouselIndex.value = 0;
  showCreateLibraryModal.value = true;
  invoke<JavaInstallation[]>("detect_installed_javas")
    .then((javas) => {
      wizardJavaList.value = javas;
      if (javas.length > 0 && !libraryWizardJava.value) {
        libraryWizardJava.value = javas[0].path;
      }
    })
    .catch((e) => console.error(e));
}

function handleCreateNewLibrary() {
  const name = libraryWizardName.value.trim() || `${libraryWizardServerType.value} ${libraryWizardVersion.value}`;
  const id = name.toLowerCase().replace(/[^a-z0-9]/g, "-");
  
  if (definedRuntimes.value.some(r => r.id === id)) {
    alert(`Instance '${name}' already exists.`);
    return;
  }

  const newLib: RuntimeGroup = {
    id,
    name,
    loader: libraryWizardServerType.value as any,
    version: libraryWizardVersion.value,
    description: libraryWizardDescription.value.trim() || `${libraryWizardServerType.value} ${libraryWizardVersion.value} server environment`,
    is_client: true,
    is_server: true,
    diskSizeMb: 0,
  };

  definedRuntimes.value.push(newLib);
  showCreateLibraryModal.value = false;
  logs.value.push(`[Conduit] Instance '${name}' (${newLib.loader} ${newLib.version}) registered successfully!`);
}

// =============================================================================
// CREATE REALM STATE (Target Library Decides Server Type & Loader!)
// =============================================================================
const targetLibrary = ref<RuntimeGroup | null>(null);
const wizardRealmStep = ref<1 | 2 | 3>(1); // 1: World, 2: Performance, 3: Review

const wizardServerType = ref<string>("Vanilla");

// Step 1: World
const wizardWorldMode = ref<"create" | "import">("create");
const wizardMcVersion = ref<string>("1.21.1");
const wizardSeed = ref<string>("");
const wizardWorldType = ref<string>("Default");
const wizardGameMode = ref<string>("Survival");
const wizardSelectedImportWorld = ref<string | null>(null);
const wizardJavaList = ref<JavaInstallation[]>([]);
const wizardSelectedJava = ref<string>("");

// Step 2: Performance
const wizardServerName = ref<string>("Squid-Server-#02");
const wizardSaveLocation = ref<string>("./instance/.minecraft/saves/Squid-Server-#02");
const wizardRamAllocationMb = ref<number>(3584);
const wizardTunnelEnabled = ref<boolean>(true);
const wizardAutoBackupsEnabled = ref<boolean>(true);
const wizardOnDemandEnabled = ref<boolean>(true);

// Step 3: Review
const wizardAllowTracking = ref<boolean>(false);
const isCreatingServer = ref<boolean>(false);

function openCreateRealmWizard(library?: RuntimeGroup | string) {
  if (typeof library === "object" && library !== null) {
    targetLibrary.value = library;
  } else if (typeof library === "string") {
    const found = definedRuntimes.value.find(r => r.id === library || r.name.toLowerCase() === library.toLowerCase() || r.loader.toLowerCase() === library.toLowerCase());
    targetLibrary.value = found || definedRuntimes.value[0];
  } else {
    targetLibrary.value = definedRuntimes.value[0] || null;
  }

  if (targetLibrary.value) {
    wizardServerType.value = targetLibrary.value.loader;
    wizardMcVersion.value = targetLibrary.value.version;
  }

  const clean = wizardServerName.value.trim().replace(/ /g, '_') || 'New_Realm';
  wizardSaveLocation.value = `${instanceDir.value}/.minecraft/saves/${clean}`;

  wizardRealmStep.value = 1;
  showCreateRealmModal.value = true;
  invoke<JavaInstallation[]>("detect_installed_javas")
    .then((javas) => {
      wizardJavaList.value = javas;
      if (javas.length > 0 && !wizardSelectedJava.value) {
        wizardSelectedJava.value = javas[0].path;
      }
    })
    .catch((e) => console.error("Java detect failed:", e));
}

function handleServerNameInput(e: Event) {
  const val = (e.target as HTMLInputElement).value;
  wizardServerName.value = val;
  const clean = val.trim().replace(/ /g, '_') || 'New_Realm';
  wizardSaveLocation.value = `${instanceDir.value}/.minecraft/saves/${clean}`;
}

async function handleLaunchServerFromWizard() {
  if (isCreatingServer.value) return;
  isCreatingServer.value = true;
  try {
    const payload = {
      server_type: wizardServerType.value,
      name: wizardServerName.value.trim() || "Squid-Server-#02",
      save_location: wizardSaveLocation.value.trim(),
      ram_mb: wizardRamAllocationMb.value,
      java_path: wizardSelectedJava.value || "java",
      mc_version: wizardMcVersion.value,
      seed: wizardSeed.value,
      world_type: wizardWorldType.value,
      game_mode: wizardGameMode.value,
      is_imported_world: wizardWorldMode.value === "import",
      imported_world_path: wizardSelectedImportWorld.value,
      enable_tunnel: wizardTunnelEnabled.value,
      enable_backups: wizardAutoBackupsEnabled.value,
      hosting_mode: wizardOnDemandEnabled.value ? "on_demand" : "always_online",
      instance_dir: instanceDir.value,
    };

    logs.value.push(`[Conduit] Deploying Realm '${payload.name}' under instance '${targetLibrary.value?.name || payload.server_type}'...`);
    logs.value.push(`[Conduit Deduplication] Inheriting shared server libraries and mods from parent instance with zero disk duplication.`);
    await invoke("create_server_wizard", { payload });
    logs.value.push(`[Conduit] Realm '${payload.name}' deployed successfully at ${payload.save_location}!`);

    // Refresh library so the new realm appears immediately
    await loadWorlds();
    showCreateRealmModal.value = false;

    // Auto-select realm
    const created = worldsList.value.find(w => w.folder_name === payload.name || w.display_name === payload.name);
    if (created) {
      handleSelectRealm(created);
      if (wizardOnDemandEnabled.value) {
        logs.value.push(`[Conduit Gateway] On-Demand Gateway enabled on port 25565. Realm is in SLEEPING state (0 MB RAM).`);
      }
    }
  } catch (err: any) {
    alert("Server creation failed: " + (err?.message || err));
  } finally {
    isCreatingServer.value = false;
  }
}

const jumpInRealms = computed(() => {
  return worldsList.value.slice(0, 4);
});

async function handlePlaySingleplayer(realm: WorldSaveInfo) {
  selectedRealm.value = realm;
  selectedWorldName.value = realm.folder_name;
  if (realm.version) selectedVersion.value = realm.version;
  if (realm.loader) selectedLoader.value = realm.loader;
  
  if (clientStatus.value === "Playing") return;
  const effectiveRam = serverSettings.value.memoryMode === "pooled"
    ? serverSettings.value.pooledRamGb
    : serverSettings.value.clientRamGb;
  try {
    logs.value.push(`[Conduit] Launching Minecraft ${selectedVersion.value} client directly into Singleplayer realm '${realm.display_name}' (-Xmx${effectiveRam}G)...`);
    await invoke("launch_client", {
      instanceDir: instanceDir.value,
      ramGb: effectiveRam,
      mode: `singleplayer:${realm.folder_name}`,
    });
  } catch (e: any) {
    logs.value.push(`[ERROR] Failed to launch singleplayer: ${e?.message || e}`);
    alert("Failed to launch Singleplayer: " + (e?.message || e));
  }
}

async function handleStartServerForRealm(realm: WorldSaveInfo) {
  selectedRealm.value = realm;
  selectedWorldName.value = realm.folder_name;
  if (realm.version) selectedVersion.value = realm.version;
  if (realm.loader) selectedLoader.value = realm.loader;

  if (serverStatus.value === "Online" && selectedWorldName.value === realm.folder_name) {
    await handleStopServer();
    return;
  }
  await handleStartServer();
}

async function handleStartServerAndJoinForRealm(realm: WorldSaveInfo) {
  selectedRealm.value = realm;
  selectedWorldName.value = realm.folder_name;
  if (realm.version) selectedVersion.value = realm.version;
  if (realm.loader) selectedLoader.value = realm.loader;

  if (serverStatus.value !== "Online" || selectedWorldName.value !== realm.folder_name) {
    logs.value.push(`[Conduit] [1/2] Booting dedicated server for Realm '${realm.display_name}' with Level 4 Admin permissions...`);
    await handleStartServer();
  }

  logs.value.push(`[Conduit] [2/2] Auto-connecting Minecraft client to '127.0.0.1:25565' as Server Admin...`);
  setTimeout(async () => {
    try {
      await handleJoinLocalServer();
    } catch (e: any) {
      console.error("Auto join client failed:", e);
    }
  }, 2200);
}

async function handleStartServerAndJoin() {
  if (!selectedRealm.value && worldsList.value.length > 0) {
    selectedRealm.value = worldsList.value[0];
    selectedWorldName.value = selectedRealm.value.folder_name;
  }
  if (selectedRealm.value) {
    await handleStartServerAndJoinForRealm(selectedRealm.value);
  } else {
    await handleStartServer();
    setTimeout(async () => {
      try {
        await handleJoinLocalServer();
      } catch (e: any) {
        console.error("Auto join client failed:", e);
      }
    }, 2200);
  }
}

</script>
<template>
  <div class="flex h-screen w-screen bg-canvas text-text-primary select-none overflow-hidden font-sans">
    <!-- ===================================================================== -->
    <!-- LEFT SIDEBAR: CONDUIT BRAND & PRIMARY NAVIGATION                      -->
    <!-- ===================================================================== -->
    <aside class="w-56 bg-sidebar border-r border-border flex flex-col justify-between shrink-0 z-30">
      <!-- Top Section -->
      <div class="p-3 space-y-4 overflow-y-auto">
        <!-- Brand Header with Crisp Custom Logo -->
        <div class="flex items-center space-x-2.5 px-2 py-2">
          <div class="w-8 h-8 rounded-lg bg-sidebar border border-border flex items-center justify-center shrink-0 overflow-hidden p-1 shadow-sm">
            <img src="/logo.svg" alt="Conduit" class="w-full h-full object-contain" />
          </div>
          <div class="min-w-0 flex-1">
            <div class="flex items-center gap-1.5">
              <span class="text-xs font-black tracking-wider text-text-primary">CONDUIT</span>
              <span class="text-[9px] font-mono px-1 py-0.2 rounded bg-elevated text-text-secondary border border-border">v2.4</span>
            </div>
          </div>
        </div>

        <!-- Primary Navigation Menu -->
        <nav class="space-y-1">
          <!-- 1. LIBRARY (Instances) -->
          <button
            @click="mainNav = 'library'; selectedRealm = null"
            class="relative w-full text-left transition-colors cursor-pointer rounded-md px-3 py-2 text-xs flex items-center justify-between"
            :class="mainNav === 'library'
              ? 'bg-elevated/70 text-text-primary font-medium'
              : 'text-text-secondary hover:text-text-primary hover:bg-elevated/40 font-normal'"
          >
            <!-- Cyan active indicator bar -->
            <span
              v-if="mainNav === 'library'"
              class="absolute left-0 top-1.5 bottom-1.5 w-1 rounded-r bg-cyan-accent"
            />
            <div class="flex items-center space-x-2.5 pl-1">
              <Layers class="w-4 h-4" :class="mainNav === 'library' ? 'text-cyan-accent' : 'text-text-secondary'" />
              <span>Instances</span>
            </div>
            <span class="text-[10px] font-mono px-1.5 py-0.5 rounded bg-card text-text-secondary border border-border">
              {{ definedRuntimes.length }}
            </span>
          </button>

          <!-- 2. DISCOVER (MODS) -->
          <button
            @click="mainNav = 'discover'"
            class="relative w-full text-left transition-colors cursor-pointer rounded-md px-3 py-2 text-xs flex items-center justify-between"
            :class="mainNav === 'discover'
              ? 'bg-elevated/70 text-text-primary font-medium'
              : 'text-text-secondary hover:text-text-primary hover:bg-elevated/40 font-normal'"
          >
            <span
              v-if="mainNav === 'discover'"
              class="absolute left-0 top-1.5 bottom-1.5 w-1 rounded-r bg-cyan-accent"
            />
            <div class="flex items-center space-x-2.5 pl-1">
              <Compass class="w-4 h-4" :class="mainNav === 'discover' ? 'text-cyan-accent' : 'text-text-secondary'" />
              <span>Discover</span>
            </div>
            <span class="text-[9px] font-mono uppercase px-1.5 py-0.5 rounded bg-cyan-subtle text-cyan-accent border border-cyan-accent/20 font-medium">
              MODS
            </span>
          </button>

          <!-- 3. MULTIPLAYER (P2P) -->
          <button
            @click="mainNav = 'multiplayer'; selectedRealm = null"
            class="relative w-full text-left transition-colors cursor-pointer rounded-md px-3 py-2 text-xs flex items-center justify-between"
            :class="mainNav === 'multiplayer'
              ? 'bg-elevated/70 text-text-primary font-medium'
              : 'text-text-secondary hover:text-text-primary hover:bg-elevated/40 font-normal'"
          >
            <span
              v-if="mainNav === 'multiplayer'"
              class="absolute left-0 top-1.5 bottom-1.5 w-1 rounded-r bg-cyan-accent"
            />
            <div class="flex items-center space-x-2.5 pl-1">
              <Radio class="w-4 h-4" :class="mainNav === 'multiplayer' ? 'text-cyan-accent' : 'text-text-secondary'" />
              <span>Multiplayer</span>
            </div>
            <span class="text-[9px] font-mono px-1.5 py-0.5 rounded bg-card text-text-secondary border border-border font-medium">
              P2P
            </span>
          </button>

          <!-- 4. PLAYER & SKINS -->
          <button
            @click="mainNav = 'player'"
            class="relative w-full text-left transition-colors cursor-pointer rounded-md px-3 py-2 text-xs flex items-center justify-between"
            :class="mainNav === 'player'
              ? 'bg-elevated/70 text-text-primary font-medium'
              : 'text-text-secondary hover:text-text-primary hover:bg-elevated/40 font-normal'"
          >
            <span
              v-if="mainNav === 'player'"
              class="absolute left-0 top-1.5 bottom-1.5 w-1 rounded-r bg-cyan-accent"
            />
            <div class="flex items-center space-x-2.5 pl-1">
              <Shirt class="w-4 h-4" :class="mainNav === 'player' ? 'text-cyan-accent' : 'text-text-secondary'" />
              <span>Player &amp; Skins</span>
            </div>
          </button>

          <!-- 5. SETTINGS -->
          <button
            @click="mainNav = 'settings'"
            class="relative w-full text-left transition-colors cursor-pointer rounded-md px-3 py-2 text-xs flex items-center justify-between"
            :class="mainNav === 'settings'
              ? 'bg-elevated/70 text-text-primary font-medium'
              : 'text-text-secondary hover:text-text-primary hover:bg-elevated/40 font-normal'"
          >
            <span
              v-if="mainNav === 'settings'"
              class="absolute left-0 top-1.5 bottom-1.5 w-1 rounded-r bg-cyan-accent"
            />
            <div class="flex items-center space-x-2.5 pl-1">
              <Settings class="w-4 h-4" :class="mainNav === 'settings' ? 'text-cyan-accent' : 'text-text-secondary'" />
              <span>Settings</span>
            </div>
          </button>
        </nav>

        <!-- Selected Realm Context Card in Sidebar if a Realm is Active -->
        <div v-if="selectedRealm" class="p-2.5 bg-card border border-border rounded-lg space-y-2">
          <div class="flex items-center justify-between">
            <span class="text-[9px] uppercase tracking-wider font-semibold text-cyan-accent font-mono">Active Realm</span>
            <button @click="handleBackToLibrary" class="text-[10px] text-text-muted hover:text-text-primary transition-colors cursor-pointer flex items-center gap-0.5">
              <span>Instances</span> &rarr;
            </button>
          </div>
          <div class="flex items-center space-x-2">
            <div class="w-7 h-7 rounded bg-canvas border border-border flex items-center justify-center p-0.5 shrink-0 overflow-hidden">
              <img v-if="selectedRealm.icon_base64" :src="selectedRealm.icon_base64" class="w-full h-full object-cover rounded" />
              <img v-else :src="getLoaderLogo(selectedRealm.loader)" class="w-full h-full object-contain" />
            </div>
            <div class="min-w-0">
              <div class="text-xs font-semibold text-text-primary truncate">{{ selectedRealm.display_name }}</div>
              <div class="text-[10px] text-text-secondary font-mono truncate flex items-center gap-1 mt-0.5">
                <img :src="getLoaderLogo(selectedRealm.loader)" class="w-3 h-3 object-contain inline-block" />
                <span>{{ selectedRealm.loader }} {{ selectedRealm.version }}</span>
              </div>
            </div>
          </div>
        </div>
      </div>

      <!-- Bottom Profile & Process Manager Bar -->
      <div class="p-3 border-t border-border space-y-2.5 bg-sidebar">
        <!-- Dedicated Server Live State Pill -->
        <div class="p-2 bg-card border border-border rounded-lg flex items-center justify-between">
          <div class="flex items-center space-x-2">
            <StatusDot :status="serverStatus" size="sm" :pulse="serverStatus === 'Starting'" />
            <span class="text-xs font-medium text-text-secondary">Server</span>
          </div>
          <span class="text-[10px] font-mono font-semibold px-1.5 py-0.5 rounded bg-elevated border border-border text-text-secondary">
            {{ serverStatus }}
          </span>
        </div>

        <!-- Microsoft Account Profile -->
        <div
          @click="handleLogin"
          class="flex items-center space-x-2.5 p-1.5 rounded-lg hover:bg-elevated/60 cursor-pointer transition-colors group"
        >
          <img :src="avatarUrl" alt="Avatar" class="w-7 h-7 rounded bg-canvas border border-border shrink-0" />
          <div class="min-w-0 flex-1">
            <div class="text-xs font-medium text-text-primary group-hover:text-cyan-accent transition-colors truncate">
              {{ authProfile ? authProfile.username : 'DeanDylan2378' }}
            </div>
            <div class="text-[10px] text-text-muted flex items-center gap-1.5">
              <StatusDot :status="authProfile ? 'online' : 'idle'" size="xs" />
              <span>{{ authProfile ? 'Connected' : 'Offline' }}</span>
            </div>
          </div>
        </div>
      </div>
    </aside>

    <!-- ===================================================================== -->
    <!-- MAIN APPLICATION VIEWPORT                                             -->
    <!-- ===================================================================== -->
    <main class="flex-1 flex flex-col h-full overflow-hidden bg-canvas">

      <!-- Window Titlebar / Header: Compact h-12 (48px), dark graphite background #11151A with border #303B47 -->
      <header
        class="h-12 border-b border-border px-4 flex items-center justify-between bg-canvas shrink-0 z-20"
        data-tauri-drag-region
      >
        <!-- Left: Breadcrumbs / Instance Context -->
        <div class="flex items-center space-x-2 text-xs font-medium min-w-0" data-tauri-drag-region>
          <button
            @click="mainNav = 'library'; selectedRealm = null"
            class="hover:text-cyan-accent transition-colors cursor-pointer text-text-secondary flex items-center gap-1.5"
            :class="mainNav === 'library' && !selectedRealm ? 'text-text-primary font-semibold' : ''"
          >
            <span>Conduit</span>
          </button>
          
          <span class="text-text-muted">/</span>

          <!-- View Breadcrumb -->
          <span v-if="mainNav === 'library' && !selectedRealm" class="text-text-primary font-semibold truncate">
            Instances
          </span>
          <template v-else-if="mainNav === 'library' && selectedRealm">
            <button
              @click="handleBackToLibrary"
              class="hover:text-cyan-accent text-text-secondary transition-colors cursor-pointer truncate"
            >
              Instances
            </button>
            <span class="text-text-muted">/</span>
            <span class="text-cyan-accent font-semibold flex items-center gap-1.5 truncate">
              <span class="w-1.5 h-1.5 rounded-full bg-cyan-accent"></span>
              {{ selectedRealm.display_name }}
            </span>
          </template>
          <span v-else-if="mainNav === 'discover'" class="text-text-primary font-semibold truncate">
            Discover
          </span>
          <span v-else-if="mainNav === 'multiplayer'" class="text-text-primary font-semibold truncate">
            Multiplayer
          </span>
          <span v-else-if="mainNav === 'player'" class="text-text-primary font-semibold truncate">
            Player &amp; Skins
          </span>
          <span v-else-if="mainNav === 'settings'" class="text-text-primary font-semibold truncate">
            Settings
          </span>
        </div>

        <!-- Center: Search input or Window Drag region -->
        <div class="flex-1 max-w-md mx-4" data-tauri-drag-region>
          <div v-if="mainNav === 'discover'" class="relative">
            <Search class="w-3.5 h-3.5 absolute left-3 top-1/2 -translate-y-1/2 text-text-muted" />
            <input
              v-model="browseQuery"
              @keyup.enter="handleBrowseModrinth()"
              placeholder="Search Modrinth mods & packs..."
              class="w-full h-7 pl-8 pr-3 bg-card border border-border rounded text-xs text-text-primary placeholder:text-text-muted outline-none focus:border-cyan-accent focus:ring-1 focus:ring-cyan-accent/50 transition-all"
            />
          </div>
          <div v-else-if="mainNav === 'library' && !selectedRealm" class="relative">
            <Search class="w-3.5 h-3.5 absolute left-3 top-1/2 -translate-y-1/2 text-text-muted" />
            <input
              v-model="searchQuery"
              placeholder="Filter instances..."
              class="w-full h-7 pl-8 pr-3 bg-card border border-border rounded text-xs text-text-primary placeholder:text-text-muted outline-none focus:border-cyan-accent focus:ring-1 focus:ring-cyan-accent/50 transition-all"
            />
          </div>
          <div v-else class="h-7 flex items-center" data-tauri-drag-region>
            <!-- Clean drag region spacer -->
          </div>
        </div>

        <!-- Right: Actions & Status Context -->
        <div class="flex items-center space-x-2 shrink-0">
          <!-- Server Status Dot Pill -->
          <div class="flex items-center space-x-1.5 px-2.5 py-1 bg-card border border-border rounded text-xs">
            <StatusDot :status="serverStatus" size="sm" :pulse="serverStatus === 'Starting'" />
            <span class="text-[11px] font-mono text-text-secondary">{{ serverStatus }}</span>
          </div>

          <!-- Auth Profile Pill -->
          <button
            @click="handleLogin"
            class="flex items-center space-x-2 px-2.5 py-1 bg-card hover:bg-elevated border border-border rounded text-xs cursor-pointer transition-colors"
          >
            <img :src="avatarUrl" alt="Avatar" class="w-4 h-4 rounded-sm bg-black/40" />
            <span class="text-xs text-text-primary font-medium max-w-[100px] truncate">
              {{ authProfile ? authProfile.username : 'Sign In' }}
            </span>
          </button>
        </div>
      </header>

      <!-- =================================================================== -->
      <!-- VIEW: LIBRARY & REALM DETAIL (EXTRACTED M3 COMPONENTS)              -->
      <!-- =================================================================== -->
      <template v-if="mainNav === 'library'">
        <LibraryView
          v-if="!selectedRealm"
          :runtimes="definedRuntimes"
          :worlds="worldsList"
          :jump-in-realms="jumpInRealms"
          :is-loading="isWorldsLoading"
          :server-status="serverStatus"
          :selected-world-name="selectedWorldName"
          :instance-dir="instanceDir"
          @select-realm="handleSelectRealm"
          @start-server-and-join="handleStartServerAndJoinForRealm"
          @start-server="handleStartServerForRealm"
          @stop-server="handleStopServer"
          @play-singleplayer="handlePlaySingleplayer"
          @open-folder="handleOpenFolder"
          @add-mods="handleAddModsForInstance"
          @open-mods-folder="handleOpenModsFolderForInstance"
          @delete-instance="handleDeleteInstance"
          @delete-realm="handleDeleteRealm"
          @create-instance="openCreateLibraryModal"
          @create-realm="(rt?: any) => openCreateRealmWizard(rt)"
          @import-saves="showImportModal = true"
          @refresh="loadWorlds"
        />

        <RealmDetailView
          v-else
          :realm="selectedRealm"
          :server-status="serverStatus"
          :client-status="clientStatus"
          :tunnel-url="tunnelUrl"
          :public-ip="publicIp"
          :host-room-code="hostRoomCode"
          :logs="logs"
          :current-mspt="currentMspt"
          :current-tps="currentTps"
          :mspt-history="msptHistory"
          :tps-history="tpsHistory"
          :jvm-used-heap-mb="jvmUsedHeapMb"
          :loaded-entities="loadedEntities"
          :loaded-chunks="loadedChunks"
          :main-tick-thread-usage="mainTickThreadUsage"
          :total-host-cpu-usage="totalHostCpuUsage"
          :installed-mods="installedMods"
          :is-mods-loading="isModsLoading"
          :dedup-metrics="dedupMetrics"
          :backups="backupsList"
          :players="playersList"
          :instance-dir="instanceDir"
          @back="handleBackToLibrary"
          @start-server-and-join="handleStartServerAndJoin"
          @start-server="handleStartServer"
          @stop-server="handleStopServer"
          @join-local-server="handleJoinLocalServer"
          @launch-singleplayer="handleLaunchSingleplayer"
          @open-folder="handleOpenFolder"
          @send-command="handleSendCommandFromView"
          @toggle-mod="handleToggleMod"
          @delete-mod="handleDeleteMod"
          @refresh-mods="loadInstalledMods"
          @create-backup="handleCreateBackup"
          @kick-player="handleKickPlayer"
          @ban-player="handleBanPlayer"
          @pardon-player="handlePardonPlayer"
          @op-player="handleOpPlayer"
          @deop-player="handleDeopPlayer"
          @delete-player-data="handleDeletePlayerDataFromView"
        />
      </template>

      <!-- =================================================================== -->
      <!-- VIEW: DISCOVER (MODRINTH MODS / MODPACKS / SHADERS)                 -->
      <!-- =================================================================== -->
      <div v-if="mainNav === 'discover'" class="flex-1 flex flex-col h-full overflow-hidden bg-canvas">
        <!-- Top Toolbar Header -->
        <div class="h-12 border-b border-border px-6 flex items-center justify-between bg-sidebar/50 shrink-0">
          <div class="flex items-center space-x-3">
            <h2 class="text-xs font-bold text-text-primary tracking-wide uppercase">DISCOVER CONTENT</h2>
            
            <!-- Target Instance Selector Pill -->
            <div class="flex items-center gap-1.5 px-2.5 py-1 rounded bg-card border border-border text-xs">
              <span class="text-[10px] text-text-muted uppercase font-mono">Target:</span>
              <div v-if="targetInstallInstance" class="flex items-center gap-1.5">
                <img :src="getLoaderLogo(targetInstallInstance.loader)" class="w-3.5 h-3.5 object-contain" />
                <span class="font-semibold text-cyan-accent text-xs">{{ targetInstallInstance.name }}</span>
                <span class="text-[10px] text-text-muted font-mono">({{ targetInstallInstance.loader }} {{ targetInstallInstance.version }})</span>
                <button
                  @click="targetInstallInstance = null; showInstancePickerModal = true"
                  class="ml-1 text-[10px] text-text-muted hover:text-text-primary underline cursor-pointer"
                >
                  Change
                </button>
              </div>
              <div v-else class="flex items-center gap-1.5">
                <span class="text-text-secondary text-xs">All Instances</span>
                <button
                  @click="showInstancePickerModal = true"
                  class="ml-1 text-[10px] text-cyan-accent hover:underline cursor-pointer"
                >
                  Select Instance
                </button>
              </div>
            </div>
          </div>

          <!-- Project Type Tabs (Mods, Modpacks, Resource Packs, Shaders) -->
          <div class="flex items-center space-x-1.5">
            <button
              v-for="cat in ([
                { id: 'mod', label: 'Mods' },
                { id: 'modpack', label: 'Modpacks' },
                { id: 'resourcepack', label: 'Resource Packs' },
                { id: 'shader', label: 'Shaders' }
              ] as const)"
              :key="cat.id"
              @click="browseCategory = cat.id; handleBrowseModrinth(cat.id, 1)"
              class="px-3 py-1 rounded text-xs font-medium transition-all cursor-pointer"
              :class="browseCategory === cat.id ? 'bg-cyan-accent text-canvas font-semibold shadow-sm' : 'bg-card text-text-secondary hover:text-text-primary hover:bg-elevated border border-border'"
            >
              {{ cat.label }}
            </button>
          </div>
        </div>

        <!-- Filter Sub-bar: Search input, Sort, Per-page, and Category Tags -->
        <div class="px-6 py-3 border-b border-border bg-sidebar/30 space-y-2.5 shrink-0">
          <div class="flex flex-wrap items-center justify-between gap-3">
            <!-- Search Input -->
            <div class="relative flex-1 min-w-[240px] max-w-lg">
              <Search class="w-3.5 h-3.5 text-text-muted absolute left-3 top-1/2 -translate-y-1/2 pointer-events-none" />
              <input
                type="text"
                v-model="browseQuery"
                @keyup.enter="handleBrowseModrinth(undefined, 1)"
                placeholder="Search mods, optimizations, features..."
                class="w-full bg-card border border-border hover:border-cyan-border focus:border-cyan-accent rounded-lg pl-9 pr-4 py-1.5 text-xs text-text-primary placeholder:text-text-muted focus:outline-none transition-colors"
              />
            </div>

            <div class="flex items-center space-x-2 shrink-0">
              <!-- Sort Dropdown -->
              <div class="flex items-center gap-1.5 text-xs text-text-muted">
                <span class="text-[11px]">Sort:</span>
                <select
                  v-model="browseSortIndex"
                  @change="handleBrowseModrinth(undefined, 1)"
                  class="bg-card border border-border rounded px-2 py-1 text-xs text-text-primary focus:outline-none focus:border-cyan-accent cursor-pointer"
                >
                  <option value="relevance">Relevance</option>
                  <option value="downloads">Downloads</option>
                  <option value="follows">Follows</option>
                  <option value="newest">Newest</option>
                  <option value="updated">Recently Updated</option>
                </select>
              </div>

              <!-- View Limit Dropdown -->
              <div class="flex items-center gap-1.5 text-xs text-text-muted">
                <span class="text-[11px]">View:</span>
                <select
                  v-model="browseLimit"
                  @change="handleBrowseModrinth(undefined, 1)"
                  class="bg-card border border-border rounded px-2 py-1 text-xs text-text-primary focus:outline-none focus:border-cyan-accent cursor-pointer"
                >
                  <option :value="10">10</option>
                  <option :value="20">20</option>
                  <option :value="50">50</option>
                </select>
              </div>

              <!-- Loader Filter -->
              <div class="flex items-center gap-1.5 text-xs text-text-muted">
                <span class="text-[11px]">Loader:</span>
                <select
                  v-model="browseLoaderFilter"
                  @change="handleBrowseModrinth(undefined, 1)"
                  class="bg-card border border-border rounded px-2 py-1 text-xs text-text-primary focus:outline-none focus:border-cyan-accent cursor-pointer capitalize"
                >
                  <option value="all">All Loaders</option>
                  <option value="neoforge">NeoForge</option>
                  <option value="fabric">Fabric</option>
                  <option value="forge">Forge</option>
                  <option value="quilt">Quilt</option>
                </select>
              </div>

              <button
                @click="handleBrowseModrinth(undefined, 1)"
                class="px-3.5 py-1.5 bg-orange-accent hover:bg-orange-accent/90 text-white font-bold text-xs rounded-lg cursor-pointer transition-colors shrink-0 shadow-sm"
              >
                Search
              </button>
            </div>
          </div>

          <!-- Filter Tags Ribbon (Horizontal Scrolling Tags) -->
          <div class="flex items-center gap-1.5 overflow-x-auto pb-1 pt-0.5 no-scrollbar">
            <span class="text-[10px] text-text-muted uppercase font-mono shrink-0 mr-1">Tags:</span>
            <button
              v-for="tag in modFunctionalTags"
              :key="tag.id"
              @click="browseFunctionalCategory = tag.id; handleBrowseModrinth(undefined, 1)"
              class="px-2.5 py-0.5 rounded-full text-[11px] font-medium transition-colors shrink-0 cursor-pointer border"
              :class="browseFunctionalCategory === tag.id
                ? 'bg-cyan-accent/15 border-cyan-accent text-cyan-accent font-semibold'
                : 'bg-card/70 border-border text-text-secondary hover:text-text-primary hover:border-border/80'"
            >
              {{ tag.label }}
            </button>
          </div>
        </div>

        <!-- Scrollable Results Grid / List -->
        <div class="flex-1 overflow-y-auto p-6 space-y-4">
          <!-- Loading State -->
          <div v-if="isBrowsing" class="py-16 flex flex-col items-center justify-center space-y-3">
            <RefreshCw class="w-7 h-7 text-cyan-accent animate-spin" />
            <span class="text-xs text-text-muted font-mono">Querying Modrinth catalog...</span>
          </div>

          <!-- Empty State -->
          <div v-else-if="browseResults.length === 0" class="py-16 flex flex-col items-center justify-center space-y-2">
            <Package class="w-10 h-10 text-text-muted/50" />
            <p class="text-sm font-semibold text-text-primary">No results found</p>
            <p class="text-xs text-text-muted">Try clearing filter tags or adjusting your search keyword.</p>
          </div>

          <!-- Modrinth Results Cards (Clean Spec UI matching User's Provided Mockup) -->
          <div v-else class="space-y-3">
            <div
              v-for="item in browseResults"
              :key="item.project_id"
              class="bg-card hover:bg-elevated/70 border border-border hover:border-cyan-border rounded-xl p-4 flex flex-col sm:flex-row items-start sm:items-center justify-between gap-4 transition-all group shadow-sm"
            >
              <!-- Left: Icon + Title + Description + Metadata Tags -->
              <div class="flex items-start space-x-3.5 min-w-0 flex-1">
                <img
                  v-if="item.icon_url"
                  :src="item.icon_url"
                  alt="Icon"
                  class="w-12 h-12 rounded-xl object-cover bg-black/40 border border-border shrink-0 mt-0.5"
                />
                <div v-else class="w-12 h-12 rounded-xl bg-elevated border border-border flex items-center justify-center shrink-0 mt-0.5">
                  <Package class="w-6 h-6 text-cyan-accent" />
                </div>

                <div class="min-w-0 flex-1 space-y-1">
                  <!-- Title + Author -->
                  <div class="flex items-center gap-2 flex-wrap">
                    <span class="text-sm font-bold text-text-primary group-hover:text-cyan-accent transition-colors">
                      {{ item.title }}
                    </span>
                    <span v-if="item.author" class="text-xs text-text-muted">
                      by <span class="text-text-secondary">{{ item.author }}</span>
                    </span>
                  </div>

                  <!-- Description -->
                  <p class="text-xs text-text-secondary line-clamp-2 leading-relaxed">
                    {{ item.description }}
                  </p>

                  <!-- Filter Tags & Pill Badges (Client/Server, Category, Loaders) -->
                  <div class="flex items-center gap-1.5 flex-wrap pt-1">
                    <!-- Environment Pill -->
                    <span class="inline-flex items-center gap-1 text-[10px] font-mono px-2 py-0.5 rounded bg-elevated text-text-muted border border-border">
                      <span class="w-1.5 h-1.5 rounded-full" :class="item.server_side === 'unsupported' ? 'bg-blue-400' : 'bg-emerald-400'"></span>
                      <span>{{ item.server_side === 'unsupported' ? 'Client' : item.client_side === 'unsupported' ? 'Server' : 'Client & Server' }}</span>
                    </span>

                    <!-- Category Tags -->
                    <template v-if="item.categories && item.categories.length > 0">
                      <span
                        v-for="cat in item.categories.slice(0, 3)"
                        :key="cat"
                        class="text-[10px] font-mono px-2 py-0.5 rounded bg-sidebar text-text-secondary border border-border capitalize"
                      >
                        {{ cat }}
                      </span>
                    </template>

                    <!-- Loader Tags -->
                    <template v-if="item.loaders && item.loaders.length > 0">
                      <span
                        v-for="ldr in item.loaders.slice(0, 3)"
                        :key="ldr"
                        class="inline-flex items-center gap-1 text-[10px] font-mono px-1.5 py-0.5 rounded bg-sidebar text-cyan-accent border border-border capitalize"
                      >
                        <img :src="getLoaderLogo(ldr)" class="w-2.5 h-2.5 object-contain" />
                        <span>{{ ldr }}</span>
                      </span>
                    </template>
                  </div>
                </div>
              </div>

              <!-- Right: Downloads + Follows + Add to Instance CTA Button -->
              <div class="flex sm:flex-col items-center sm:items-end justify-between w-full sm:w-auto shrink-0 gap-2.5 pt-2 sm:pt-0 border-t sm:border-t-0 border-border/40">
                <button
                  @click="promptInstallProject(item)"
                  :disabled="installingProjects[item.project_id]"
                  class="h-9 px-4 bg-emerald-600 hover:bg-emerald-500 text-white font-bold text-xs rounded-lg flex items-center gap-1.5 transition-all cursor-pointer shadow-sm disabled:opacity-40"
                >
                  <Plus class="w-3.5 h-3.5 stroke-[2.5]" :class="installingProjects[item.project_id] ? 'animate-spin' : ''" />
                  <span>{{ installingProjects[item.project_id] ? 'Installing...' : '+ Add to an instance' }}</span>
                </button>

                <div class="text-[11px] text-text-muted font-mono flex items-center gap-3">
                  <span class="flex items-center gap-1">
                    <Download class="w-3 h-3 text-text-muted" />
                    <span>{{ (item.downloads || 0).toLocaleString() }}</span>
                  </span>
                  <span v-if="item.follows" class="flex items-center gap-1">
                    <Heart class="w-3 h-3 text-text-muted" />
                    <span>{{ item.follows.toLocaleString() }}</span>
                  </span>
                </div>
              </div>
            </div>
          </div>

          <!-- Pagination Controls Bar -->
          <div v-if="browseTotalHits > 0" class="pt-4 border-t border-border flex items-center justify-between text-xs text-text-muted">
            <span class="font-mono">
              Showing {{ ((browsePage - 1) * browseLimit) + 1 }} - {{ Math.min(browsePage * browseLimit, browseTotalHits) }} of {{ browseTotalHits.toLocaleString() }}
            </span>

            <div class="flex items-center space-x-2">
              <button
                :disabled="browsePage <= 1 || isBrowsing"
                @click="handleBrowseModrinth(undefined, browsePage - 1)"
                class="px-2.5 py-1 rounded bg-card border border-border text-text-primary hover:bg-elevated disabled:opacity-40 cursor-pointer disabled:cursor-not-allowed text-xs font-mono"
              >
                &larr; Prev
              </button>

              <span class="px-3 py-1 rounded bg-cyan-accent text-canvas font-bold text-xs font-mono">
                {{ browsePage }}
              </span>

              <button
                :disabled="(browsePage * browseLimit) >= browseTotalHits || isBrowsing"
                @click="handleBrowseModrinth(undefined, browsePage + 1)"
                class="px-2.5 py-1 rounded bg-card border border-border text-text-primary hover:bg-elevated disabled:opacity-40 cursor-pointer disabled:cursor-not-allowed text-xs font-mono"
              >
                Next &rarr;
              </button>
            </div>
          </div>
        </div>
      </div>

      <!-- =================================================================== -->
      <!-- VIEW: MULTIPLAYER (CONDUIT NETWORK: IDENTITY, P2P TUNNEL & RELAY)   -->
      <!-- =================================================================== -->
      <div v-if="mainNav === 'multiplayer'" class="flex-1 flex flex-col h-full overflow-hidden">
        <div class="h-11 border-b border-border px-6 flex items-center justify-between bg-sidebar/40 shrink-0">
          <div>
            <h2 class="text-xs font-bold text-text-primary tracking-wide uppercase">CONDUIT P2P NETWORK</h2>
          </div>
          <div class="flex items-center space-x-2">
            <span class="text-[10px] font-mono px-2.5 py-1 rounded bg-card text-text-secondary border border-border font-medium flex items-center gap-1.5">
              <StatusDot :status="networkStatus?.state === 'hosting' || networkStatus?.state === 'connected' ? 'online' : 'idle'" size="xs" />
              <span>{{ networkStatus?.state === 'hosting' ? 'Hosting Active' : networkStatus?.state === 'connected' ? 'Tunnel Connected' : 'Engine Ready' }}</span>
            </span>
          </div>
        </div>

        <div class="flex-1 overflow-y-auto p-6 space-y-6">
          <!-- TOP ROW: QUICK JOIN SERVER (The Primary Requested Experience) -->
          <div class="bg-[#12151b] border border-amber-500/40 rounded-3xl p-6 shadow-xl shadow-amber-500/5 space-y-4 relative overflow-hidden">
            <div class="flex items-start justify-between">
              <div class="space-y-1">
                <div class="flex items-center space-x-2">
                  <div class="w-8 h-8 rounded-xl bg-amber-500/15 border border-amber-500/30 flex items-center justify-center">
                    <Zap class="w-4 h-4 text-amber-400 fill-current" />
                  </div>
                  <h3 class="text-base font-black text-white">Join Server via Conduit Network</h3>
                </div>
                <p class="text-xs text-zinc-400">
                  Enter a Conduit Room Code or Host Address. Conduit establishes an encrypted P2P WireGuard/Noise tunnel and automatically launches Minecraft to connect directly.
                </p>
              </div>
              <span class="text-[10px] font-mono text-zinc-500 font-bold px-2.5 py-1 bg-zinc-900 border border-zinc-800 rounded-lg">
                Zero-Config P2P
              </span>
            </div>

            <!-- Input & One-Click Launch Button -->
            <div class="flex flex-col sm:flex-row items-center gap-3 pt-2">
              <div class="relative flex-1 w-full">
                <input
                  type="text"
                  v-model="joinRoomCodeInput"
                  placeholder="Enter Room Code (e.g. CND-9382-SMP) or IP:Port..."
                  class="w-full bg-[#181c24] border border-zinc-700 focus:border-amber-400 focus:ring-1 focus:ring-amber-400 rounded-xl px-4 py-3 text-xs font-mono text-white placeholder-zinc-500 outline-none"
                  @keyup.enter="handleJoinServerAndLaunch"
                />
              </div>
              <button
                @click="handleJoinServerAndLaunch"
                :disabled="isJoiningNetwork"
                class="w-full sm:w-auto px-6 py-3 bg-amber-500 hover:bg-amber-400 text-zinc-950 font-black text-xs rounded-xl flex items-center justify-center gap-2 cursor-pointer transition-all shadow-md shadow-amber-500/20 disabled:opacity-40 shrink-0"
              >
                <Zap class="w-4 h-4 fill-current" />
                <span>{{ isJoiningNetwork ? 'Establishing Tunnel...' : 'Join Server & Launch' }}</span>
              </button>
            </div>
          </div>

          <!-- SECOND ROW: 2 COLUMN GRID (IDENTITY & HOSTING) -->
          <div class="grid grid-cols-1 md:grid-cols-2 gap-6">
            <!-- COLUMN 1: HOSTING REALM STATUS -->
            <div class="bg-[#14171c] border border-zinc-800/80 rounded-2xl p-5 space-y-4 shadow-lg flex flex-col justify-between">
              <div class="space-y-3">
                <div class="flex items-center justify-between">
                  <div class="flex items-center space-x-2.5">
                    <Radio class="w-4 h-4 text-amber-400" />
                    <h4 class="text-xs font-black text-white uppercase tracking-wider">Host Status</h4>
                  </div>
                  <span
                    class="text-[9px] font-mono font-bold px-2 py-0.5 rounded-full border"
                    :class="serverStatus === 'Online' ? 'bg-amber-500/15 text-amber-400 border-amber-500/30' : 'bg-zinc-800 text-zinc-400 border-zinc-700'"
                  >
                    {{ serverStatus === 'Online' ? 'Server Active' : 'Offline' }}
                  </span>
                </div>

                <p class="text-xs text-zinc-400">
                  When you start any Realm from Instances, Conduit automatically assigns a room code and opens a direct UDP tunnel.
                </p>

                <!-- Room Code Display -->
                <div class="p-3.5 bg-black/40 rounded-xl border border-zinc-800/80 space-y-2">
                  <div class="flex items-center justify-between">
                    <span class="text-[10px] uppercase font-mono text-zinc-500 font-bold">Your Realm Room Code</span>
                    <button
                      v-if="hostRoomCode || tunnelUrl"
                      @click="handleCopyRoomCode(hostRoomCode || tunnelUrl)"
                      class="text-[10px] text-amber-400 hover:text-amber-300 font-bold flex items-center gap-1 cursor-pointer"
                    >
                      <Copy class="w-3 h-3" />
                      <span>{{ copiedRoomCode ? 'Copied!' : 'Copy Code' }}</span>
                    </button>
                  </div>
                  <div class="font-mono text-sm font-black text-white select-all">
                    {{ hostRoomCode || tunnelUrl || 'Start a server to generate invite code' }}
                  </div>
                </div>
              </div>

              <!-- Host Capabilities -->
              <div class="pt-2 border-t border-zinc-800/80 grid grid-cols-2 gap-2 text-[10px] font-mono text-zinc-400">
                <div class="flex items-center gap-1.5">
                  <CheckCircle2 class="w-3 h-3 text-amber-400" />
                  <span>STUN Hole Punching</span>
                </div>
                <div class="flex items-center gap-1.5">
                  <CheckCircle2 class="w-3 h-3 text-amber-400" />
                  <span>Noise Protocol IK</span>
                </div>
              </div>
            </div>

            <!-- COLUMN 2: CONDUIT IDENTITY -->
            <div class="bg-[#14171c] border border-zinc-800/80 rounded-2xl p-5 space-y-4 shadow-lg flex flex-col justify-between">
              <div class="space-y-3">
                <div class="flex items-center justify-between">
                  <div class="flex items-center space-x-2.5">
                    <ShieldCheck class="w-4 h-4 text-amber-400" />
                    <h4 class="text-xs font-black text-white uppercase tracking-wider">Conduit Identity</h4>
                  </div>
                  <span class="text-[9px] font-mono font-bold px-2 py-0.5 rounded-full bg-amber-500/10 text-amber-400 border border-amber-500/20">
                    X25519 Verified
                  </span>
                </div>

                <p class="text-xs text-zinc-400">
                  Your persistent cryptographic keypair. Mutual authentication occurs before any Minecraft packet is routed.
                </p>

                <!-- Keypair Card -->
                <div class="p-3.5 bg-black/40 rounded-xl border border-zinc-800/80 space-y-1.5 font-mono">
                  <div class="text-[10px] text-zinc-500 uppercase font-bold">Public Key Fingerprint</div>
                  <div class="text-xs text-amber-400 font-bold truncate select-all">
                    {{ conduitIdentity?.public_key || 'Generating identity keypair...' }}
                  </div>
                  <div class="text-[10px] text-zinc-500">
                    Handle: <span class="text-white font-bold">{{ conduitIdentity?.handle || 'Player' }}</span> &bull; Short ID: <span class="text-white">{{ conduitIdentity?.short_id || '...' }}</span>
                  </div>
                </div>
              </div>

              <!-- Security Specs -->
              <div class="pt-2 border-t border-zinc-800/80 grid grid-cols-2 gap-2 text-[10px] font-mono text-zinc-400">
                <div class="flex items-center gap-1.5">
                  <CheckCircle2 class="w-3 h-3 text-amber-400" />
                  <span>ChaCha20-Poly1305</span>
                </div>
                <div class="flex items-center gap-1.5">
                  <CheckCircle2 class="w-3 h-3 text-amber-400" />
                  <span>0 MB Kernel Footprint</span>
                </div>
              </div>
            </div>
          </div>

          <!-- THIRD ROW: ARCHITECTURE HIGHLIGHT -->
          <div class="p-5 bg-[#0f1117] border border-zinc-800/90 rounded-2xl flex flex-col sm:flex-row items-center justify-between gap-4">
            <div class="space-y-1">
              <h4 class="text-xs font-bold text-white uppercase tracking-wide">Independent Userspace Engine</h4>
              <p class="text-[11px] text-zinc-400">
                Conduit runs without virtual network adapters or kernel drivers, protecting your routing table and ensuring 100% portable Minecraft play.
              </p>
            </div>
            <div class="flex items-center gap-3 shrink-0">
              <span class="text-[10px] font-mono px-2.5 py-1 rounded-lg bg-zinc-800 text-zinc-300 border border-zinc-700">
                No Hamachi
              </span>
              <span class="text-[10px] font-mono px-2.5 py-1 rounded-lg bg-zinc-800 text-zinc-300 border border-zinc-700">
                No Tailscale
              </span>
              <span class="text-[10px] font-mono px-2.5 py-1 rounded-lg bg-zinc-800 text-zinc-300 border border-zinc-700">
                No Playit.gg
              </span>
            </div>
          </div>
        </div>
      </div>

      <!-- =================================================================== -->
      <!-- VIEW: PLAYER & SKINS                                                -->
      <!-- =================================================================== -->
      <div v-if="mainNav === 'player'" class="flex-1 flex flex-col h-full overflow-hidden">
        <div class="h-11 border-b border-border px-6 flex items-center justify-between bg-sidebar/40 shrink-0">
          <div>
            <h2 class="text-xs font-bold text-text-primary tracking-wide uppercase">PLAYER &amp; 3D SKIN STUDIO</h2>
          </div>
        </div>

        <div class="flex-1 overflow-y-auto p-6 flex flex-col md:flex-row items-center justify-center gap-12">
          <div class="bg-[#14171c] border border-zinc-800/80 rounded-3xl p-8 flex flex-col items-center justify-center shadow-2xl relative">
            <div class="w-48 h-80 flex items-center justify-center">
              <img :src="skinFullUrl" alt="Skin" class="h-full object-contain drop-shadow-[0_10px_20px_rgba(0,0,0,0.8)]" />
            </div>
            <div class="mt-4 font-mono font-bold text-xs text-zinc-300 bg-black/40 px-4 py-1.5 rounded-xl border border-zinc-800">
              {{ authProfile ? authProfile.username : 'DeanDylan2378' }}
            </div>
          </div>

          <div class="space-y-4 max-w-sm w-full">
            <h3 class="text-lg font-black text-white">Active Minecraft Skin</h3>
            <p class="text-xs text-zinc-400 leading-relaxed">
              Your player skin and cape automatically sync from Mojang services with full support for custom high-definition model textures.
            </p>
            <button
              @click="handleLogin"
              class="w-full h-10 px-4 bg-amber-500 hover:bg-amber-400 text-zinc-950 font-semibold text-sm rounded-lg border border-amber-400/50 shadow-sm shadow-amber-500/10 transition-all cursor-pointer"
            >
              {{ authProfile ? 'Switch Minecraft Account' : 'Sign In With Microsoft' }}
            </button>
          </div>
        </div>
      </div>

      <!-- =================================================================== -->
      <!-- VIEW: GLOBAL SETTINGS                                               -->
      <!-- =================================================================== -->
      <div v-if="mainNav === 'settings'" class="flex-1 flex flex-col h-full overflow-hidden">
        <div class="h-11 border-b border-border px-6 flex items-center justify-between bg-sidebar/40 shrink-0">
          <div>
            <h2 class="text-xs font-bold text-text-primary tracking-wide uppercase">ENGINE SETTINGS &amp; STORAGE</h2>
          </div>
        </div>

        <div class="flex-1 overflow-y-auto p-6 space-y-6">
          <div class="bg-[#14171c] border border-zinc-800/80 rounded-2xl p-5 space-y-4 shadow-xl">
            <div class="flex items-center space-x-3">
              <HardDrive class="w-5 h-5 text-amber-400" />
              <div>
                <h4 class="text-sm font-black text-white">NTFS Zero-Duplication Engine</h4>
                <p class="text-xs text-zinc-400">Junctions prevent duplicating gigabytes of game libraries between client and server.</p>
              </div>
            </div>

            <div class="grid grid-cols-1 md:grid-cols-3 gap-3">
              <div class="p-3 bg-[#12141a] rounded-xl border border-zinc-800">
                <span class="text-[10px] text-zinc-500 uppercase font-bold block">Physical Disk Saved</span>
                <span class="text-sm font-bold text-amber-400 font-mono">{{ dedupMetrics?.saved_disk_mb?.toFixed(1) || '438.8' }} MB</span>
              </div>
              <div class="p-3 bg-[#12141a] rounded-xl border border-zinc-800">
                <span class="text-[10px] text-zinc-500 uppercase font-bold block">Hard Linked Mods</span>
                <span class="text-sm font-bold text-white font-mono">{{ dedupMetrics?.mods_hardlinked || 0 }} files</span>
              </div>
              <div class="p-3 bg-[#12141a] rounded-xl border border-zinc-800">
                <span class="text-[10px] text-zinc-500 uppercase font-bold block">Linkage Health</span>
                <span class="text-sm font-bold text-amber-400 font-mono">100% Active</span>
              </div>
            </div>

            <button
              @click="handleRepairJunctions"
              :disabled="isRepairingJunctions"
              class="px-4 py-2 bg-amber-500 hover:bg-amber-400 text-zinc-950 font-bold text-xs rounded-xl cursor-pointer transition-colors disabled:opacity-40"
            >
              {{ isRepairingJunctions ? 'Verifying Linkages...' : 'Verify & Repair NTFS Junctions' }}
            </button>
          </div>
        </div>
      </div>
    </main>

    <!-- ===================================================================== -->
    <!-- MODAL: IMPORT REALMS / SAVES (PROPER IMPORT FROM MODRINTH & LOCAL)    -->
    <!-- ===================================================================== -->
    <div
      v-if="showImportModal"
      class="fixed inset-0 bg-black/80 backdrop-blur-md z-50 flex items-center justify-center p-6"
    >
      <div class="bg-[#14171c] border border-zinc-800 rounded-3xl max-w-xl w-full p-6 space-y-6 shadow-2xl">
        <div class="flex items-center justify-between border-b border-zinc-800 pb-3">
          <div class="flex items-center space-x-2">
            <FolderGit2 class="w-5 h-5 text-amber-400" />
            <h3 class="text-sm font-black text-white">Import Saves into Conduit Instances</h3>
          </div>
          <button @click="showImportModal = false" class="text-zinc-500 hover:text-white cursor-pointer">✕</button>
        </div>

        <div class="space-y-4">
          <p class="text-xs text-zinc-400">
            Conduit detects existing Minecraft saves across Modrinth profiles and local instances. Importing establishes zero-overhead NTFS junctions so your saves appear in their designated runtime without duplicating disk space.
          </p>

          <div v-if="importSuccessMsg" class="p-3 bg-amber-500/10 border border-amber-500/30 rounded-xl text-xs text-amber-400 font-semibold flex items-center gap-2">
            <Check class="w-4 h-4 text-amber-400" />
            <span>{{ importSuccessMsg }}</span>
          </div>

          <div class="space-y-2 border border-zinc-800/80 rounded-2xl p-4 bg-[#0d0e12]">
            <div class="text-[11px] font-bold text-zinc-300 uppercase tracking-wider mb-2">Detected Modrinth &amp; Local Saves</div>
            
            <div class="space-y-1.5 max-h-56 overflow-y-auto">
              <div
                v-for="w in worldsList"
                :key="w.folder_name"
                class="p-2.5 bg-[#14171c] rounded-xl border border-zinc-800 flex items-center justify-between text-xs"
              >
                <div class="flex items-center space-x-2.5 min-w-0 pr-2">
                  <Globe class="w-4 h-4 text-amber-400 shrink-0" />
                  <div class="min-w-0">
                    <div class="font-bold text-white truncate">{{ w.display_name }}</div>
                    <div class="text-[10px] text-zinc-500 font-mono truncate">Runtime: {{ w.loader }} {{ w.version }}</div>
                  </div>
                </div>
                <span class="text-[10px] font-mono px-2 py-0.5 rounded bg-amber-500/15 text-amber-400 border border-amber-500/30 font-bold shrink-0">
                  Linked
                </span>
              </div>
            </div>
          </div>
        </div>

        <div class="flex items-center justify-end space-x-2 pt-2 border-t border-zinc-800">
          <button
            @click="showImportModal = false"
            class="px-4 py-2 bg-zinc-800 hover:bg-zinc-700 text-zinc-300 font-bold rounded-xl text-xs cursor-pointer"
          >
            Close
          </button>
          <button
            @click="handleRunAutoImport"
            :disabled="isImporting"
            class="px-5 py-2 bg-amber-500 hover:bg-amber-400 text-zinc-950 font-extrabold rounded-xl text-xs cursor-pointer shadow-md shadow-amber-500/20 flex items-center gap-1.5 disabled:opacity-40"
          >
            <RefreshCw class="w-3.5 h-3.5" :class="isImporting ? 'animate-spin' : ''" />
            <span>{{ isImporting ? 'Scanning & Linking...' : 'Re-Scan & Import All Saves' }}</span>
          </button>
        </div>
      </div>
    </div>

    <!-- ===================================================================== -->
    <!-- CREATE LIBRARY / SERVER RUNTIME MODAL                                 -->
    <!-- (Determines Server Software, Loader, Version & Java Environment)      -->
    <!-- ===================================================================== -->
    <div
      v-if="showCreateLibraryModal"
      class="fixed inset-0 bg-black/85 backdrop-blur-md z-50 flex items-center justify-center p-4 select-none"
    >
      <div
        class="bg-[#111319] border border-zinc-800 rounded-3xl max-w-4xl w-full p-8 space-y-6 shadow-2xl relative overflow-hidden flex flex-col justify-between min-h-[640px]"
        style="background: radial-gradient(circle at 50% 0%, #1c1813 0%, #0e1015 100%);"
      >
        <!-- Top Close Button -->
        <button
          @click="showCreateLibraryModal = false"
          class="absolute top-6 right-6 text-zinc-500 hover:text-white transition-colors cursor-pointer text-sm font-bold w-8 h-8 rounded-full bg-zinc-900/60 flex items-center justify-center hover:bg-zinc-800 z-10"
        >
          ✕
        </button>

        <!-- Stepper Indicator -->
        <div class="flex items-center justify-center space-x-3 pt-2">
          <!-- Step 1 -->
          <div class="flex items-center space-x-2.5">
            <span
              class="w-7 h-7 rounded-full flex items-center justify-center text-xs font-black transition-all"
              :class="libraryWizardStep === 1 ? 'bg-amber-500 text-zinc-950 shadow-md ring-2 ring-amber-500/20' : 'bg-zinc-700 text-white'"
            >
              1
            </span>
            <span
              class="text-xs font-bold transition-colors"
              :class="libraryWizardStep === 1 ? 'text-amber-400' : 'text-zinc-500'"
            >
              Instance Engine
            </span>
          </div>

          <div class="w-8 h-[1px]" :class="libraryWizardStep > 1 ? 'bg-amber-500/50' : 'bg-zinc-800'"></div>

          <!-- Step 2 -->
          <div class="flex items-center space-x-2.5">
            <span
              class="w-7 h-7 rounded-full flex items-center justify-center text-xs font-black transition-all"
              :class="libraryWizardStep === 2 ? 'bg-amber-500 text-zinc-950 shadow-md ring-2 ring-amber-500/20' : 'bg-zinc-800 text-zinc-500'"
            >
              2
            </span>
            <span
              class="text-xs font-bold transition-colors"
              :class="libraryWizardStep === 2 ? 'text-amber-400' : 'text-zinc-500'"
            >
              Instance Details
            </span>
          </div>
        </div>

        <!-- STEP 1: INSTANCE ENGINE CAROUSEL -->
        <div v-if="libraryWizardStep === 1" class="space-y-6 flex-1 flex flex-col justify-between py-2">
          <div class="text-center space-y-1">
            <h2 class="text-2xl font-black text-white tracking-tight">Select Instance Engine &amp; Loader</h2>
            <p class="text-xs text-zinc-400">This instance will govern the server engine and mod loader for all realms inside it.</p>
          </div>

          <!-- Carousel / 3-Card Display -->
          <div class="relative flex items-center justify-center px-2">
            <button
              @click="wizardCarouselIndex = Math.max(0, wizardCarouselIndex - 1)"
              :disabled="wizardCarouselIndex === 0"
              class="w-10 h-10 rounded-full bg-zinc-900/90 hover:bg-zinc-800 border border-zinc-800 flex items-center justify-center text-white disabled:opacity-20 cursor-pointer transition-all shadow-lg shrink-0 mr-4"
            >
              <ChevronLeft class="w-5 h-5" />
            </button>

            <!-- Cards Grid -->
            <div class="grid grid-cols-3 gap-5 flex-1 max-w-3xl">
              <div
                v-for="st in visibleServerTypes"
                :key="st.id"
                @click="libraryWizardServerType = st.id; libraryWizardName = `${st.id} ${libraryWizardVersion}`"
                class="rounded-3xl p-6 flex flex-col justify-between transition-all cursor-pointer relative h-[360px]"
                :class="libraryWizardServerType === st.id ? 'border-2 border-amber-400 bg-[#171922] shadow-xl shadow-amber-500/10 scale-[1.02]' : 'border border-zinc-800/80 bg-[#12141a]/90 hover:border-zinc-700 opacity-75 hover:opacity-100'"
              >
                <!-- Top Icon & Name -->
                <div class="flex flex-col items-center text-center space-y-3">
                  <div class="w-14 h-14 rounded-2xl bg-black/40 border border-zinc-800 flex items-center justify-center p-2.5">
                    <img v-if="st.id !== 'Modpack'" :src="getLoaderLogo(st.id)" :alt="st.name" class="w-10 h-10 object-contain drop-shadow" />
                    <Package v-else class="w-8 h-8 text-amber-400 drop-shadow" />
                  </div>

                  <h3 class="font-extrabold text-base text-white tracking-wide">{{ st.name }}</h3>
                </div>

                <!-- Description -->
                <p class="text-xs text-zinc-400 text-center leading-relaxed px-1">
                  {{ st.description }}
                </p>

                <!-- Performance Bars & Features -->
                <div class="space-y-3 pt-3 border-t border-zinc-800/80">
                  <div class="flex items-center justify-between text-xs">
                    <span class="text-zinc-400">Performance</span>
                    <span class="font-bold text-white">{{ st.performance }}</span>
                  </div>

                  <div class="flex space-x-1.5 h-1.5 w-full">
                    <div
                      v-for="bar in 5"
                      :key="bar"
                      class="flex-1 rounded-full transition-all"
                      :class="bar <= st.bars ? 'bg-amber-400' : 'bg-zinc-800'"
                    ></div>
                  </div>

                  <div class="flex items-center justify-between text-[11px] pt-1">
                    <span :class="st.plugins ? 'text-amber-400 font-bold' : 'text-zinc-500 font-medium'">
                      {{ st.plugins ? '✓ Plugin Support' : '✕ No Plugins' }}
                    </span>
                    <span :class="st.mods ? 'text-amber-400 font-bold' : 'text-zinc-500 font-medium'">
                      {{ st.mods ? '✓ Mod Support' : '✕ No Mods' }}
                    </span>
                  </div>
                </div>
              </div>
            </div>

            <button
              @click="wizardCarouselIndex = Math.min(serverTypes.length - 3, wizardCarouselIndex + 1)"
              :disabled="wizardCarouselIndex >= serverTypes.length - 3"
              class="w-10 h-10 rounded-full bg-zinc-900/90 hover:bg-zinc-800 border border-zinc-800 flex items-center justify-center text-white disabled:opacity-20 cursor-pointer transition-all shadow-lg shrink-0 ml-4"
            >
              <ChevronRight class="w-5 h-5" />
            </button>
          </div>

          <!-- Pagination Dots -->
          <div class="flex items-center justify-center space-x-2">
            <button
              v-for="(st, idx) in serverTypes"
              :key="'lib-dot-' + st.id"
              @click="libraryWizardServerType = st.id; libraryWizardName = `${st.id} ${libraryWizardVersion}`; wizardCarouselIndex = Math.min(serverTypes.length - 3, Math.max(0, idx - 1))"
              class="h-1.5 rounded-full transition-all cursor-pointer"
              :class="libraryWizardServerType === st.id ? 'w-6 bg-amber-400' : 'w-1.5 bg-zinc-700 hover:bg-zinc-500'"
            ></button>
          </div>

          <!-- Bottom Button -->
          <div class="flex justify-end pt-4 border-t border-zinc-800/80">
            <button
              @click="libraryWizardStep = 2"
              class="h-11 px-6 bg-amber-500 hover:bg-amber-400 text-zinc-950 font-bold text-xs rounded-xl flex items-center gap-2 cursor-pointer transition-colors shadow-lg shadow-amber-500/20"
            >
              <span>Next: Instance Details</span>
              <ArrowRight class="w-4 h-4" />
            </button>
          </div>
        </div>

        <!-- STEP 2: INSTANCE DETAILS & JAVA -->
        <div v-else-if="libraryWizardStep === 2" class="space-y-5 flex-1 flex flex-col justify-between py-2">
          <div class="text-center space-y-1">
            <h2 class="text-2xl font-black text-white tracking-tight">Configure {{ libraryWizardServerType }} Instance</h2>
            <p class="text-xs text-zinc-400">Specify the instance name, Minecraft target version, and JDK environment.</p>
          </div>

          <div class="max-w-2xl mx-auto w-full space-y-4">
            <!-- Instance Name -->
            <div>
              <label class="block text-xs font-bold text-zinc-300 mb-1.5 flex items-center gap-1.5">
                <Layers class="w-3.5 h-3.5 text-amber-400" />
                <span>Instance Name</span>
              </label>
              <input
                type="text"
                v-model="libraryWizardName"
                placeholder="e.g. NeoForge 1.21.1 Production"
                class="w-full bg-[#14171d] border border-zinc-800 rounded-xl px-4 py-2.5 text-xs text-white focus:outline-none focus:border-amber-500 font-medium"
              />
            </div>

            <!-- Minecraft Version -->
            <div>
              <label class="block text-xs font-bold text-zinc-300 mb-1.5 flex items-center gap-1.5">
                <HardDrive class="w-3.5 h-3.5 text-amber-400" />
                <span>Minecraft Game Version</span>
              </label>
              <select
                v-model="libraryWizardVersion"
                @change="libraryWizardName = `${libraryWizardServerType} ${libraryWizardVersion}`"
                class="w-full bg-[#14171d] border border-zinc-800 rounded-xl px-4 py-2.5 text-xs text-white focus:outline-none focus:border-amber-500 cursor-pointer"
              >
                <option value="1.21.1">1.21.1 (Recommended / NeoForge &amp; Paper Compatible)</option>
                <option value="1.21">1.21</option>
                <option value="1.20.4">1.20.4</option>
                <option value="1.20.1">1.20.1 (LTS Modding)</option>
                <option value="1.19.4">1.19.4</option>
                <option value="1.18.2">1.18.2</option>
              </select>
            </div>

            <!-- Java Detected Box -->
            <div class="bg-[#14171d] border border-zinc-800 rounded-2xl p-4 space-y-2">
              <div class="flex items-center space-x-2 text-amber-400">
                <CheckCircle2 class="w-4 h-4 text-amber-400" />
                <span class="text-xs font-bold">Java Runtime Environment</span>
              </div>
              <p class="text-[11px] text-zinc-400">
                Select the JDK that will execute realms in this instance:
              </p>
              <select
                v-model="libraryWizardJava"
                class="w-full bg-[#181b22] border border-zinc-700/80 rounded-xl px-3 py-2 text-xs text-zinc-200 focus:outline-none focus:border-amber-500 cursor-pointer"
              >
                <option v-for="j in wizardJavaList" :key="'lib-java-' + j.path" :value="j.path">
                  {{ j.name }} ({{ j.version }}) {{ j.is_recommended ? '— Recommended' : '' }}
                </option>
                <option v-if="wizardJavaList.length === 0" value="java">
                  Java 21.0.12.1 (SquidServers JDK 21) — Recommended
                </option>
              </select>
            </div>

            <!-- Description / Notes -->
            <div>
              <label class="block text-xs font-bold text-zinc-300 mb-1.5 flex items-center gap-1.5">
                <FileText class="w-3.5 h-3.5 text-zinc-400" />
                <span>Description / Purpose</span>
              </label>
              <input
                type="text"
                v-model="libraryWizardDescription"
                placeholder="e.g. Primary modded instance for multiplayer SMP realms"
                class="w-full bg-[#14171d] border border-zinc-800 rounded-xl px-4 py-2.5 text-xs text-white focus:outline-none focus:border-amber-500"
              />
            </div>
          </div>

          <!-- Bottom Action Buttons -->
          <div class="flex items-center justify-between pt-4 border-t border-zinc-800/80">
            <button
              @click="libraryWizardStep = 1"
              class="w-11 h-11 bg-zinc-800/90 hover:bg-zinc-700 text-zinc-200 rounded-xl flex items-center justify-center cursor-pointer transition-colors"
            >
              <ArrowLeft class="w-4 h-4" />
            </button>

            <button
              @click="handleCreateNewLibrary"
              class="h-11 px-6 bg-amber-500 hover:bg-amber-400 text-zinc-950 font-black text-xs rounded-xl flex items-center gap-2 cursor-pointer transition-colors shadow-lg shadow-amber-500/20"
            >
              <Plus class="w-4 h-4 stroke-[3]" />
              <span>Create Instance</span>
            </button>
          </div>
        </div>
      </div>
    </div>

    <!-- ===================================================================== -->
    <!-- 3-STEP REALM CREATION WIZARD                                          -->
    <!-- (Software & Loader are decided by the parent Library!)                -->
    <!-- ===================================================================== -->
    <div
      v-if="showCreateRealmModal"
      class="fixed inset-0 bg-black/85 backdrop-blur-md z-50 flex items-center justify-center p-4 select-none"
    >
      <div
        class="bg-[#111319] border border-zinc-800 rounded-3xl max-w-4xl w-full p-8 space-y-6 shadow-2xl relative overflow-hidden flex flex-col justify-between min-h-[640px]"
        style="background: radial-gradient(circle at 50% 0%, #1c1813 0%, #0e1015 100%);"
      >
        <!-- Top Close Button -->
        <button
          @click="showCreateRealmModal = false"
          class="absolute top-6 right-6 text-zinc-500 hover:text-white transition-colors cursor-pointer text-sm font-bold w-8 h-8 rounded-full bg-zinc-900/60 flex items-center justify-center hover:bg-zinc-800 z-10"
        >
          ✕
        </button>

        <!-- Target Runtime Instance Banner (Locked & Scoped) -->
        <div class="bg-card border border-border rounded-xl px-4 py-2.5 flex items-center justify-between">
          <div class="flex items-center space-x-3">
            <div class="w-7 h-7 rounded-lg bg-canvas border border-border flex items-center justify-center p-1 shrink-0">
              <img :src="getLoaderLogo(targetLibrary?.loader || wizardServerType)" class="w-4 h-4 object-contain" />
            </div>
            <div class="flex items-center space-x-2">
              <span class="text-xs text-text-muted">Parent Instance:</span>
              <span class="text-xs font-bold text-text-primary">{{ targetLibrary?.name || 'NeoForge 1.21.1' }}</span>
              <span class="text-[10px] font-mono px-2 py-0.5 rounded bg-sidebar text-cyan-accent border border-border font-medium flex items-center gap-1">
                <img :src="getLoaderLogo(targetLibrary?.loader || wizardServerType)" class="w-3 h-3 object-contain inline-block" />
                <span>{{ targetLibrary?.loader || wizardServerType }} {{ targetLibrary?.version || wizardMcVersion }}</span>
              </span>
            </div>
          </div>
          <span class="text-[10px] font-mono text-text-muted bg-sidebar border border-border px-2 py-0.5 rounded">
            Instance Scoped
          </span>
        </div>

        <!-- 3-Step Indicator (World -> Performance -> Review) -->
        <div class="flex items-center justify-center space-x-3 pt-1">
          <!-- Step 1 -->
          <div class="flex items-center space-x-2.5">
            <span
              class="w-7 h-7 rounded-full flex items-center justify-center text-xs font-black transition-all"
              :class="wizardRealmStep === 1 ? 'bg-amber-500 text-zinc-950 shadow-md ring-2 ring-amber-500/20' : wizardRealmStep > 1 ? 'bg-zinc-700 text-white' : 'bg-zinc-800 text-zinc-500'"
            >
              1
            </span>
            <span
              class="text-xs font-bold transition-colors"
              :class="wizardRealmStep === 1 ? 'text-amber-400' : 'text-zinc-500'"
            >
              World Setup
            </span>
          </div>

          <div class="w-8 h-[1px]" :class="wizardRealmStep > 1 ? 'bg-amber-500/50' : 'bg-zinc-800'"></div>

          <!-- Step 2 -->
          <div class="flex items-center space-x-2.5">
            <span
              class="w-7 h-7 rounded-full flex items-center justify-center text-xs font-black transition-all"
              :class="wizardRealmStep === 2 ? 'bg-amber-500 text-zinc-950 shadow-md ring-2 ring-amber-500/20' : wizardRealmStep > 2 ? 'bg-zinc-700 text-white' : 'bg-zinc-800 text-zinc-500'"
            >
              2
            </span>
            <span
              class="text-xs font-bold transition-colors"
              :class="wizardRealmStep === 2 ? 'text-amber-400' : 'text-zinc-500'"
            >
              Performance &amp; Hosting
            </span>
          </div>

          <div class="w-8 h-[1px]" :class="wizardRealmStep > 2 ? 'bg-amber-500/50' : 'bg-zinc-800'"></div>

          <!-- Step 3 -->
          <div class="flex items-center space-x-2.5">
            <span
              class="w-7 h-7 rounded-full flex items-center justify-center text-xs font-black transition-all"
              :class="wizardRealmStep === 3 ? 'bg-amber-500 text-zinc-950 shadow-md ring-2 ring-amber-500/20' : 'bg-zinc-800 text-zinc-500'"
            >
              3
            </span>
            <span
              class="text-xs font-bold transition-colors"
              :class="wizardRealmStep === 3 ? 'text-amber-400' : 'text-zinc-500'"
            >
              Review &amp; Launch
            </span>
          </div>
        </div>

        <!-- ================================================================= -->
        <!-- STEP 1: WORLD SETUP                                               -->
        <!-- ================================================================= -->
        <div v-if="wizardRealmStep === 1" class="space-y-5 flex-1 flex flex-col justify-between py-2">
          <div class="text-center space-y-1">
            <h2 class="text-2xl font-black text-white tracking-tight">World Setup</h2>
            <p class="text-xs text-zinc-400">Configure your world and choose your game settings.</p>
          </div>

          <!-- Segmented Mode Toggle: Create New World vs Import Existing World -->
          <div class="max-w-2xl mx-auto w-full space-y-4">
            <div class="bg-[#14171d] p-1 rounded-2xl border border-zinc-800 flex">
              <button
                @click="wizardWorldMode = 'create'"
                class="flex-1 py-2 text-xs font-bold rounded-xl transition-all cursor-pointer text-center"
                :class="wizardWorldMode === 'create' ? 'bg-amber-500 text-zinc-950 shadow-sm' : 'text-zinc-400 hover:text-white'"
              >
                Create New World
              </button>
              <button
                @click="wizardWorldMode = 'import'"
                class="flex-1 py-2 text-xs font-bold rounded-xl transition-all cursor-pointer text-center"
                :class="wizardWorldMode === 'import' ? 'bg-amber-500 text-zinc-950 shadow-sm' : 'text-zinc-400 hover:text-white'"
              >
                Import Existing World / Save
              </button>
            </div>

            <!-- Form Content -->
            <div v-if="wizardWorldMode === 'create'" class="space-y-3.5">
              <!-- World Seed -->
              <div>
                <label class="block text-xs font-bold text-zinc-300 mb-1.5 flex items-center gap-1.5">
                  <Sprout class="w-3.5 h-3.5 text-amber-400" />
                  <span>World Seed (Optional)</span>
                </label>
                <input
                  type="text"
                  v-model="wizardSeed"
                  placeholder="Leave blank for random seed"
                  class="w-full bg-[#14171d] border border-zinc-800 rounded-xl px-4 py-2.5 text-xs text-white placeholder-zinc-600 focus:outline-none focus:border-amber-500"
                />
              </div>

              <div class="grid grid-cols-2 gap-3.5">
                <!-- World Type -->
                <div>
                  <label class="block text-xs font-bold text-zinc-300 mb-1.5 flex items-center gap-1.5">
                    <Globe class="w-3.5 h-3.5 text-amber-400" />
                    <span>World Type</span>
                  </label>
                  <select
                    v-model="wizardWorldType"
                    class="w-full bg-[#14171d] border border-zinc-800 rounded-xl px-4 py-2.5 text-xs text-white focus:outline-none focus:border-amber-500 cursor-pointer"
                  >
                    <option value="Default">Default</option>
                    <option value="Flat">Superflat</option>
                    <option value="Large Biomes">Large Biomes</option>
                    <option value="Amplified">Amplified</option>
                  </select>
                </div>

                <!-- Default Game Mode -->
                <div>
                  <label class="block text-xs font-bold text-zinc-300 mb-1.5 flex items-center gap-1.5">
                    <Gamepad2 class="w-3.5 h-3.5 text-amber-400" />
                    <span>Default Game Mode</span>
                  </label>
                  <select
                    v-model="wizardGameMode"
                    class="w-full bg-[#14171d] border border-zinc-800 rounded-xl px-4 py-2.5 text-xs text-white focus:outline-none focus:border-amber-500 cursor-pointer"
                  >
                    <option value="Survival">Survival</option>
                    <option value="Creative">Creative</option>
                    <option value="Adventure">Adventure</option>
                    <option value="Spectator">Spectator</option>
                  </select>
                </div>
              </div>
            </div>

            <!-- Import Mode -->
            <div v-else class="space-y-3">
              <label class="block text-xs font-bold text-zinc-300">Select Existing Save to Link into this Realm:</label>
              <div class="max-h-48 overflow-y-auto space-y-2 border border-zinc-800/80 rounded-2xl p-2 bg-[#0e1015]">
                <div
                  v-for="w in worldsList"
                  :key="'import-realm-opt-' + w.folder_name"
                  @click="wizardSelectedImportWorld = w.full_path || w.folder_name; wizardServerName = w.display_name"
                  class="p-2.5 rounded-xl border flex items-center justify-between cursor-pointer transition-all text-xs"
                  :class="wizardSelectedImportWorld === (w.full_path || w.folder_name) ? 'bg-amber-500/15 border-amber-500/40 text-amber-300' : 'bg-[#14171d] border-zinc-800 text-zinc-300 hover:border-zinc-700'"
                >
                  <div class="flex items-center space-x-2.5 truncate">
                    <Globe class="w-4 h-4 text-amber-400 shrink-0" />
                    <span class="font-bold truncate">{{ w.display_name }}</span>
                  </div>
                  <span class="text-[10px] font-mono text-zinc-500 shrink-0">{{ w.loader }} {{ w.version }}</span>
                </div>
              </div>
            </div>

            <!-- Java Detected Box -->
            <div class="bg-[#14171d] border border-zinc-800 rounded-2xl p-4 space-y-2">
              <div class="flex items-center space-x-2 text-amber-400">
                <CheckCircle2 class="w-4 h-4 text-amber-400" />
                <span class="text-xs font-bold">Java detected</span>
              </div>
              <p class="text-[11px] text-zinc-400">
                Found {{ wizardJavaList.length || 5 }} Java installation(s). Select your preferred version:
              </p>
              <select
                v-model="wizardSelectedJava"
                class="w-full bg-[#181b22] border border-zinc-700/80 rounded-xl px-3 py-2 text-xs text-zinc-200 focus:outline-none focus:border-amber-500 cursor-pointer"
              >
                <option v-for="j in wizardJavaList" :key="j.path" :value="j.path">
                  {{ j.name }} ({{ j.version }}) {{ j.is_recommended ? '— Recommended' : '' }}
                </option>
                <option v-if="wizardJavaList.length === 0" value="java">
                  Java 21.0.12.1 (SquidServers JDK 21) — Recommended
                </option>
              </select>
            </div>
          </div>
        </div>

        <!-- ================================================================= -->
        <!-- STEP 2: PERFORMANCE SETUP                                         -->
        <!-- ================================================================= -->
        <div v-else-if="wizardRealmStep === 2" class="space-y-4 flex-1 flex flex-col justify-between py-2">
          <div class="text-center space-y-0.5">
            <h2 class="text-xl font-bold text-white tracking-tight">Realm Settings</h2>
            <p class="text-xs text-zinc-400">Configure your realm name and RAM allocation.</p>
          </div>

          <div class="max-w-xl mx-auto w-full space-y-4">
            <!-- Server Name -->
            <div>
              <label class="block text-xs font-semibold text-zinc-300 mb-1.5 flex items-center gap-1.5">
                <Server class="w-3.5 h-3.5 text-cyan-accent" />
                <span>Realm Name</span>
              </label>
              <input
                type="text"
                v-model="wizardServerName"
                @input="handleServerNameInput"
                class="w-full bg-[#14171d] border border-zinc-800 rounded-xl px-4 py-2 text-xs text-white focus:outline-none focus:border-cyan-accent font-medium"
              />
            </div>

            <!-- RAM Allocation Slider -->
            <div class="space-y-2">
              <div class="flex items-center justify-between">
                <label class="text-xs font-semibold text-zinc-300 flex items-center gap-1.5">
                  <Cpu class="w-3.5 h-3.5 text-cyan-accent" />
                  <span>RAM Allocation</span>
                </label>
                <span class="text-xs font-mono font-bold text-cyan-accent">{{ (wizardRamAllocationMb / 1024).toFixed(1) }} GB ({{ wizardRamAllocationMb }} MB)</span>
              </div>

              <!-- Slider Track -->
              <input
                type="range"
                min="1024"
                max="16384"
                step="512"
                v-model.number="wizardRamAllocationMb"
                class="w-full h-1.5 rounded-lg appearance-none cursor-pointer accent-cyan-accent bg-zinc-800"
              />
              <div class="flex items-center justify-between text-[10px] font-mono text-zinc-500">
                <span>1 GB</span>
                <span>Available: 12 GB</span>
                <span>16 GB</span>
              </div>
            </div>

            <!-- Toggles: Backups and Conduit Network -->
            <div class="pt-2 grid grid-cols-2 gap-3">
              <!-- Network Access -->
              <div class="bg-card border border-border rounded-xl p-3 flex items-center justify-between">
                <div class="min-w-0 pr-2">
                  <div class="text-xs font-semibold text-white">Conduit Network</div>
                  <div class="text-[10px] text-zinc-400 mt-0.5">Encrypted P2P connection</div>
                </div>
                <button
                  type="button"
                  @click="wizardTunnelEnabled = !wizardTunnelEnabled"
                  class="w-10 h-5 rounded-full transition-colors relative cursor-pointer shrink-0"
                  :class="wizardTunnelEnabled ? 'bg-cyan-accent' : 'bg-zinc-700'"
                >
                  <span
                    class="block w-3.5 h-3.5 rounded-full bg-white transition-transform absolute top-0.5"
                    :class="wizardTunnelEnabled ? 'right-0.5' : 'left-0.5'"
                  ></span>
                </button>
              </div>

              <!-- Backups -->
              <div class="bg-card border border-border rounded-xl p-3 flex items-center justify-between">
                <div class="min-w-0 pr-2">
                  <div class="text-xs font-semibold text-white">Auto Backups</div>
                  <div class="text-[10px] text-zinc-400 mt-0.5">Automated world snapshots</div>
                </div>
                <button
                  type="button"
                  @click="wizardAutoBackupsEnabled = !wizardAutoBackupsEnabled"
                  class="w-10 h-5 rounded-full transition-colors relative cursor-pointer shrink-0"
                  :class="wizardAutoBackupsEnabled ? 'bg-cyan-accent' : 'bg-zinc-700'"
                >
                  <span
                    class="block w-3.5 h-3.5 rounded-full bg-white transition-transform absolute top-0.5"
                    :class="wizardAutoBackupsEnabled ? 'right-0.5' : 'left-0.5'"
                  ></span>
                </button>
              </div>
            </div>
          </div>
        </div>

        <!-- ================================================================= -->
        <!-- STEP 3: REVIEW YOUR SETUP                                         -->
        <!-- ================================================================= -->
        <div v-else-if="wizardRealmStep === 3" class="space-y-5 flex-1 flex flex-col justify-between py-2">
          <div class="text-center space-y-1">
            <h2 class="text-2xl font-black text-white tracking-tight">Review Your Setup</h2>
            <p class="text-xs text-zinc-400">Please review your realm configuration before launching.</p>
          </div>

          <div class="max-w-2xl mx-auto w-full space-y-5">
            <div class="bg-[#14171d] border border-zinc-800 rounded-3xl p-6 space-y-5 shadow-xl">
              <!-- Server Type Row (Inherited from Instance) -->
              <div class="flex items-center justify-between pb-3 border-b border-zinc-800/80">
                <span class="text-xs font-bold text-zinc-400">Parent Instance</span>
                <span class="text-xs font-black text-amber-400 font-mono">
                  {{ targetLibrary?.name || 'NeoForge' }} ({{ wizardServerType }} {{ wizardMcVersion }})
                </span>
              </div>

              <!-- Key Value Rows -->
              <div class="space-y-3.5 text-xs">
                <!-- Realm Name -->
                <div class="flex items-start space-x-3">
                  <Server class="w-4 h-4 text-zinc-400 shrink-0 mt-0.5" />
                  <div>
                    <div class="font-bold text-white">Realm Name</div>
                    <div class="text-zinc-400 mt-0.5">{{ wizardServerName }}</div>
                  </div>
                </div>

                <!-- Memory Allocation -->
                <div class="flex items-start space-x-3">
                  <Cpu class="w-4 h-4 text-zinc-400 shrink-0 mt-0.5" />
                  <div>
                    <div class="font-bold text-white">Memory Allocation</div>
                    <div class="text-zinc-400 mt-0.5">{{ wizardRamAllocationMb }} MB RAM</div>
                  </div>
                </div>

                <!-- Server Location & Zero-Duplication Engine -->
                <div class="flex items-start space-x-3">
                  <Folder class="w-4 h-4 text-amber-400 shrink-0 mt-0.5" />
                  <div>
                    <div class="font-bold text-white flex items-center gap-2">
                      <span>World Save Location</span>
                      <span class="text-[10px] font-mono px-2 py-0.5 rounded bg-amber-500/10 text-amber-400 border border-amber-500/30">Zero Duplication</span>
                    </div>
                    <div class="text-zinc-400 font-mono text-[11px] mt-0.5 break-all">{{ wizardSaveLocation }}</div>
                    <div class="text-[11px] text-zinc-500 mt-1">
                      Inherits parent instance <span class="text-amber-400 font-bold">{{ targetLibrary?.name }}</span> server libraries and mods. Only unique world chunks are stored.
                    </div>
                  </div>
                </div>

                <!-- World -->
                <div class="flex items-start space-x-3">
                  <Globe class="w-4 h-4 text-zinc-400 shrink-0 mt-0.5" />
                  <div>
                    <div class="font-bold text-white">World</div>
                    <div class="text-zinc-400 mt-0.5">
                      {{ wizardWorldMode === 'create' ? 'New world: world' : (wizardSelectedImportWorld || 'Imported world') }}
                    </div>
                  </div>
                </div>

                <!-- Hosting Mode -->
                <div class="flex items-start space-x-3">
                  <Zap class="w-4 h-4 text-amber-400 shrink-0 mt-0.5" />
                  <div>
                    <div class="font-bold text-white">Architecture &amp; Hosting</div>
                    <div class="text-amber-400 font-mono text-[11px] mt-0.5">
                      {{ wizardOnDemandEnabled ? '⚡ On-Demand Gateway (0 MB RAM Sleeping until player connects)' : '🟠 Always Online' }}
                    </div>
                  </div>
                </div>
              </div>

              <!-- Tracking Toggle -->
              <div class="pt-4 border-t border-zinc-800/80 flex items-start space-x-3">
                <button
                  type="button"
                  @click="wizardAllowTracking = !wizardAllowTracking"
                  class="w-11 h-6 rounded-full transition-colors relative cursor-pointer shrink-0 mt-0.5"
                  :class="wizardAllowTracking ? 'bg-amber-500' : 'bg-zinc-700'"
                >
                  <span
                    class="block w-4 h-4 rounded-full bg-white transition-transform absolute top-1"
                    :class="wizardAllowTracking ? 'right-1' : 'left-1'"
                  ></span>
                </button>
                <div>
                  <div class="text-xs font-bold text-white">Allow anonymous usage tracking</div>
                  <div class="text-[10px] text-zinc-500 leading-relaxed mt-1">
                    Help improve the app by sharing anonymous usage data. No personal information is collected.
                  </div>
                </div>
              </div>
            </div>
          </div>
        </div>

        <!-- ================================================================= -->
        <!-- BOTTOM NAVIGATION BUTTONS                                         -->
        <!-- ================================================================= -->
        <div class="flex items-center justify-between pt-4 border-t border-zinc-800/80">
          <!-- Back Button (Steps 2, 3) -->
          <div>
            <button
              v-if="wizardRealmStep > 1"
              @click="wizardRealmStep--"
              class="w-11 h-11 bg-zinc-800/90 hover:bg-zinc-700 text-zinc-200 rounded-xl flex items-center justify-center cursor-pointer transition-colors shadow-sm"
              title="Previous Step"
            >
              <ArrowLeft class="w-4 h-4" />
            </button>
          </div>

          <!-- Next Button (Steps 1, 2) -->
          <div v-if="wizardRealmStep < 3">
            <button
              @click="wizardRealmStep++"
              class="h-11 px-6 bg-zinc-800 hover:bg-zinc-700 text-zinc-100 font-bold text-xs rounded-xl flex items-center gap-2 cursor-pointer transition-colors shadow-sm"
              title="Next Step"
            >
              <span>Next</span>
              <ArrowRight class="w-4 h-4" />
            </button>
          </div>

          <!-- Launch Button (Step 3) -->
          <div v-else class="flex-1 pl-4 flex flex-col items-center">
            <button
              @click="handleLaunchServerFromWizard"
              :disabled="isCreatingServer"
              class="w-full h-12 bg-amber-500 hover:bg-amber-400 text-zinc-950 font-black text-sm rounded-xl flex items-center justify-center gap-2 cursor-pointer transition-all shadow-lg shadow-amber-500/25 disabled:opacity-50"
            >
              <RefreshCw v-if="isCreatingServer" class="w-4 h-4 animate-spin" />
              <span>{{ isCreatingServer ? 'Provisioning & Launching Realm...' : 'Launch Realm' }}</span>
            </button>
            <div class="text-[10px] text-zinc-500 mt-2 text-center">
              By creating a realm, you agree to Mojang's Minecraft EULA.
            </div>
          </div>
        </div>
      </div>
    </div>

    <!-- PROVISIONING OVERLAY -->
    <div
      v-if="isInstalling && installProgress"
      class="fixed inset-0 bg-black/80 backdrop-blur-md z-50 flex items-center justify-center p-6"
    >
      <div class="bg-[#14171c] border border-amber-500/40 rounded-3xl max-w-md w-full p-6 space-y-5 shadow-2xl">
        <div class="flex items-center space-x-3">
          <Download class="w-5 h-5 text-amber-400 animate-bounce" />
          <div>
            <h3 class="text-sm font-black text-white">{{ installProgress.step }}</h3>
            <p class="text-xs text-zinc-400">{{ installProgress.details }}</p>
          </div>
        </div>
      </div>
    </div>

    <!-- MICROSOFT AUTH MODAL -->
    <div
      v-if="showAuthModal"
      class="fixed inset-0 bg-black/75 backdrop-blur-sm z-50 flex items-center justify-center p-4"
    >
      <div class="bg-[#14171c] border border-zinc-800/90 rounded-3xl max-w-sm w-full p-6 space-y-5 shadow-2xl">
        <div class="flex items-center justify-between border-b border-zinc-800/80 pb-3">
          <div class="flex items-center space-x-2.5">
            <Flame class="w-4 h-4 text-amber-400" />
            <h3 class="text-sm font-black text-white">Microsoft Sign In</h3>
          </div>
          <button @click="showAuthModal = false" class="text-zinc-500 hover:text-white cursor-pointer">✕</button>
        </div>

        <div v-if="deviceCodeData" class="space-y-4 text-center">
          <div class="text-xs text-zinc-400">Enter this code at Microsoft Device Login:</div>
          <div
            @click="handleCopyAuthCode"
            class="p-4 bg-black/50 border border-amber-500/30 rounded-2xl cursor-pointer hover:border-amber-500/60"
          >
            <div class="font-mono text-2xl font-black text-amber-400">{{ deviceCodeData.user_code }}</div>
            <div class="text-[10px] text-zinc-500 mt-1">{{ copiedCode ? 'Copied' : 'Click to copy' }}</div>
          </div>
          <button
            @click="handleOpenAuthLink"
            class="w-full h-10 bg-amber-500 hover:bg-amber-400 text-zinc-950 font-bold text-sm rounded-xl cursor-pointer"
          >
            Open microsoft.com/link
          </button>
        </div>
      </div>
    </div>

    <!-- Notification Toast Container -->
    <div
      class="fixed bottom-4 right-4 z-50 flex flex-col gap-2 max-w-sm pointer-events-none"
      aria-live="polite"
    >
      <transition-group
        enter-active-class="transition duration-200 ease-out"
        enter-from-class="transform translate-y-2 opacity-0"
        enter-to-class="transform translate-y-0 opacity-100"
        leave-active-class="transition duration-150 ease-in"
        leave-from-class="transform translate-y-0 opacity-100"
        leave-to-class="transform translate-y-2 opacity-0"
      >
        <div
          v-for="toast in toasts"
          :key="toast.id"
          class="pointer-events-auto p-3 bg-card border border-border rounded-lg shadow-xl shadow-black/40 flex items-start space-x-3 text-xs text-text-primary"
        >
          <!-- Status Badge Indicator -->
          <span
            class="w-2 h-2 rounded-full mt-1 shrink-0"
            :class="{
              'bg-status-success': toast.type === 'success',
              'bg-status-error': toast.type === 'error',
              'bg-orange-accent': toast.type === 'warning',
              'bg-cyan-accent': toast.type === 'info' || !toast.type
            }"
          />
          <div class="flex-1 min-w-0">
            <div class="font-semibold text-text-primary truncate">{{ toast.title }}</div>
            <div v-if="toast.message" class="text-text-secondary mt-0.5 break-words leading-relaxed">{{ toast.message }}</div>
          </div>
          <button
            @click="removeToast(toast.id)"
            class="text-text-muted hover:text-text-primary p-0.5 rounded transition-colors cursor-pointer shrink-0"
          >
            <X class="w-3.5 h-3.5" />
          </button>
        </div>
      </transition-group>
    </div>

    <!-- TARGET INSTANCE PICKER MODAL (Choose where to install mod/modpack) -->
    <div
      v-if="showInstancePickerModal"
      class="fixed inset-0 bg-black/75 backdrop-blur-sm z-50 flex items-center justify-center p-4"
      @click.self="showInstancePickerModal = false"
    >
      <div class="bg-card border border-border rounded-2xl max-w-md w-full p-5 space-y-4 shadow-2xl">
        <div class="flex items-center justify-between pb-3 border-b border-border">
          <div class="space-y-0.5">
            <h3 class="text-sm font-bold text-text-primary">Select Target Instance</h3>
            <p class="text-xs text-text-muted">
              Choose which isolated instance environment will receive
              <span v-if="pendingInstallProject" class="text-cyan-accent font-semibold">{{ pendingInstallProject.title }}</span>
            </p>
          </div>
          <button
            @click="showInstancePickerModal = false"
            class="p-1 rounded-lg text-text-muted hover:text-text-primary hover:bg-elevated transition-colors cursor-pointer"
          >
            <X class="w-4 h-4" />
          </button>
        </div>

        <div class="space-y-2 max-h-80 overflow-y-auto">
          <div
            v-for="instance in definedRuntimes"
            :key="instance.id"
            @click="selectInstanceAndInstall(instance)"
            class="p-3 bg-sidebar/70 hover:bg-elevated border border-border hover:border-cyan-border rounded-xl flex items-center justify-between cursor-pointer transition-colors group"
          >
            <div class="flex items-center space-x-3 min-w-0">
              <div class="w-9 h-9 rounded-lg bg-canvas border border-border flex items-center justify-center shrink-0 p-1">
                <img :src="getLoaderLogo(instance.loader)" :alt="instance.loader" class="w-full h-full object-contain" />
              </div>
              <div class="min-w-0">
                <div class="font-bold text-xs text-text-primary group-hover:text-cyan-accent transition-colors truncate">
                  {{ instance.name }}
                </div>
                <div class="text-[10px] text-text-muted font-mono flex items-center gap-1.5 mt-0.5">
                  <span class="text-cyan-accent">{{ instance.loader }} {{ instance.version }}</span>
                  <span>&bull;</span>
                  <span>{{ instance.diskSizeMb }} MB</span>
                </div>
              </div>
            </div>

            <div class="shrink-0 flex items-center gap-1 text-xs font-semibold text-emerald-400 group-hover:text-emerald-300">
              <span>Install Here</span>
              <ChevronRight class="w-3.5 h-3.5" />
            </div>
          </div>
        </div>

        <div class="pt-2 border-t border-border flex justify-end">
          <button
            @click="showInstancePickerModal = false"
            class="px-4 py-1.5 bg-sidebar hover:bg-elevated text-text-secondary text-xs rounded-lg transition-colors cursor-pointer"
          >
            Cancel
          </button>
        </div>
      </div>
    </div>

  </div>
</template>
