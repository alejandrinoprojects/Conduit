<script setup lang="ts">
import { ref, onMounted, nextTick } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

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

interface InstalledMod {
  file_name: string;
  mod_id?: string;
  mod_name?: string;
  version?: string;
  environment: "Both" | "ClientOnly" | "ServerOnly";
  size_bytes: number;
}

interface ModrinthProject {
  project_id: string;
  title: string;
  description: string;
  icon_url: string | null;
  downloads: number;
  client_side: string;
  server_side: string;
}

// App State
const instanceDir = ref<string>("./instance");
const ramGb = ref<number>(6);
const activeTab = ref<"mods" | "browse" | "console">("mods");

// Statuses
const serverStatus = ref<"Stopped" | "Starting" | "Online" | "Error">("Stopped");
const clientStatus = ref<"Stopped" | "Downloading" | "Playing">("Stopped");
const tunnelUrl = ref<string | null>(null);
const tunnelClaimUrl = ref<string | null>(null);

// Auth
const authProfile = ref<MinecraftProfile | null>(null);
const isAuthenticating = ref<boolean>(false);
const deviceCodeData = ref<DeviceCodePayload | null>(null);

// Provisioning
const isInstalling = ref<boolean>(false);
const installProgress = ref<InstallProgressPayload | null>(null);

// Mods
const installedMods = ref<InstalledMod[]>([]);
const isModsLoading = ref<boolean>(false);

// Browse Modrinth
const searchQuery = ref<string>("");
const searchResults = ref<ModrinthProject[]>([]);
const isSearching = ref<boolean>(false);
const installingMods = ref<Record<string, boolean>>({});

// Console
const logs = ref<string[]>([]);
const consoleInput = ref<string>("");
const consoleContainer = ref<HTMLDivElement | null>(null);

// Copy Feedback
const copiedIp = ref<boolean>(false);
const copiedCode = ref<boolean>(false);

onMounted(async () => {
  // Listen to Tauri events
  await listen<string>("server-status", (event) => {
    serverStatus.value = event.payload as any;
  });

  await listen<string>("client-status", (event) => {
    clientStatus.value = event.payload as any;
  });

  await listen<string>("tunnel-status", (event) => {
    tunnelUrl.value = event.payload;
  });

  await listen<string>("tunnel-claim", (event) => {
    tunnelClaimUrl.value = event.payload;
  });

  await listen<string>("server-log", (event) => {
    logs.value.push(event.payload);
    if (logs.value.length > 200) {
      logs.value.shift();
    }
    nextTick(() => {
      if (consoleContainer.value) {
        consoleContainer.value.scrollTop = consoleContainer.value.scrollHeight;
      }
    });
  });

  await listen<DeviceCodePayload>("auth-device-code", (event) => {
    deviceCodeData.value = event.payload;
  });

  await listen<InstallProgressPayload>("install-progress", (event) => {
    installProgress.value = event.payload;
  });

  // Attempt to load saved auth & mods
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

  await loadInstalledMods();
});

// Auth actions
async function handleLogin() {
  isAuthenticating.value = true;
  try {
    const profile = await invoke<MinecraftProfile>("login_microsoft", {
      instanceDir: instanceDir.value,
    });
    authProfile.value = profile;
    deviceCodeData.value = null;
  } catch (e: any) {
    alert("Authentication Error: " + (e?.message || e));
  } finally {
    isAuthenticating.value = false;
  }
}

// Instance Provisioning
async function handleInstallInstance() {
  isInstalling.value = true;
  try {
    await invoke("install_instance", {
      instanceDir: instanceDir.value,
    });
    alert("NeoForge 1.21.1 and Headless Server ready!");
  } catch (e: any) {
    alert("Installation Error: " + (e?.message || e));
  } finally {
    isInstalling.value = false;
    installProgress.value = null;
  }
}

// Server Orchestration
async function handleStartServer() {
  try {
    await invoke("start_server", {
      instanceDir: instanceDir.value,
      ramGb: ramGb.value,
    });
  } catch (e: any) {
    alert("Failed to start server: " + (e?.message || e));
  }
}

async function handleStopServer() {
  try {
    await invoke("stop_server");
  } catch (e: any) {
    alert("Failed to stop server: " + (e?.message || e));
  }
}

async function handleJoinLocalServer() {
  try {
    await invoke("join_local_server", {
      instanceDir: instanceDir.value,
      ramGb: ramGb.value,
    });
  } catch (e: any) {
    alert("Failed to launch Minecraft client: " + (e?.message || e));
  }
}

async function handleLaunchSingleplayer() {
  try {
    await invoke("launch_client", {
      instanceDir: instanceDir.value,
      ramGb: ramGb.value,
      mode: "singleplayer",
    });
  } catch (e: any) {
    alert("Failed to launch Singleplayer: " + (e?.message || e));
  }
}

// Mods Actions
async function loadInstalledMods() {
  isModsLoading.value = true;
  try {
    const list = await invoke<InstalledMod[]>("list_installed_mods", {
      instanceDir: instanceDir.value,
    });
    installedMods.value = list;
  } catch (e) {
    console.error("Failed to list installed mods:", e);
  } finally {
    isModsLoading.value = false;
  }
}

async function handleDeleteMod(fileName: string) {
  if (!confirm(`Delete mod ${fileName}?`)) return;
  try {
    await invoke("delete_mod", {
      instanceDir: instanceDir.value,
      fileName,
    });
    await loadInstalledMods();
  } catch (e: any) {
    alert("Error deleting mod: " + e);
  }
}

// Browse Modrinth
async function handleSearchModrinth() {
  isSearching.value = true;
  try {
    const data = await invoke<any>("search_modrinth", {
      query: searchQuery.value,
      offset: 0,
    });
    searchResults.value = data?.hits || [];
  } catch (e: any) {
    alert("Failed searching Modrinth: " + e);
  } finally {
    isSearching.value = false;
  }
}

async function handleInstallMod(projectId: string) {
  installingMods.value[projectId] = true;
  try {
    await invoke("install_modrinth_mod", {
      instanceDir: instanceDir.value,
      projectId,
    });
    await loadInstalledMods();
  } catch (e: any) {
    alert("Failed to install mod: " + e);
  } finally {
    installingMods.value[projectId] = false;
  }
}

// Console input
async function handleSendCommand() {
  if (!consoleInput.value.trim()) return;
  const cmd = consoleInput.value;
  consoleInput.value = "";
  try {
    await invoke("send_server_command", { command: cmd });
  } catch (e: any) {
    logs.value.push(`[System Error] ${e}`);
  }
}

function copyIp() {
  if (tunnelUrl.value) {
    navigator.clipboard.writeText(tunnelUrl.value);
    copiedIp.value = true;
    setTimeout(() => (copiedIp.value = false), 2000);
  }
}

function copyUserCode() {
  if (deviceCodeData.value) {
    navigator.clipboard.writeText(deviceCodeData.value.user_code);
    copiedCode.value = true;
    setTimeout(() => (copiedCode.value = false), 2000);
  }
}
</script>

<template>
  <div class="min-h-screen bg-[#111215] text-[#f3f4f6] flex flex-col font-sans select-none">
    <!-- Top Navigation Bar -->
    <header class="h-16 bg-[#17181c] border-b border-[#26282e] px-6 flex items-center justify-between">
      <div class="flex items-center space-x-3">
        <div class="w-8 h-8 rounded-lg bg-gradient-to-br from-[#1bd96a] to-[#5865F2] flex items-center justify-center font-black text-white text-lg shadow-lg">
          C
        </div>
        <div class="flex flex-col">
          <span class="font-bold tracking-tight text-white flex items-center gap-2">
            Conduit
            <span class="text-xs bg-[#26282e] text-[#1bd96a] font-mono px-2 py-0.5 rounded-full border border-[#1bd96a]/20">NeoForge 1.21.1</span>
          </span>
          <span class="text-[11px] text-gray-400 font-mono">{{ instanceDir }}</span>
        </div>
      </div>

      <!-- Action items & Profile -->
      <div class="flex items-center space-x-4">
        <!-- Provisioning Progress / Button -->
        <div v-if="isInstalling && installProgress" class="flex flex-col items-end mr-2">
          <div class="text-xs text-[#1bd96a] font-medium flex items-center gap-1.5">
            <span class="w-2 h-2 rounded-full bg-[#1bd96a] animate-ping"></span>
            {{ installProgress.step }} ({{ installProgress.percentage.toFixed(0) }}%)
          </div>
          <span class="text-[10px] text-gray-400">{{ installProgress.details }}</span>
        </div>
        <button
          v-else
          @click="handleInstallInstance"
          class="px-3 py-1.5 rounded-md bg-[#26282e] hover:bg-[#32353d] text-xs font-semibold text-gray-300 transition-colors border border-gray-700/50 flex items-center gap-1.5"
        >
          <span>⚡</span> Install / Repair 1.21.1
        </button>

        <!-- Microsoft Auth Pill -->
        <div v-if="authProfile" class="flex items-center space-x-2.5 bg-[#26282e] border border-[#3b3e47] rounded-full px-3 py-1">
          <img
            :src="`https://mc-heads.net/avatar/${authProfile.username}/24`"
            alt="Skin"
            class="w-6 h-6 rounded-full border border-gray-600 bg-gray-800"
          />
          <span class="text-xs font-semibold text-white">{{ authProfile.username }}</span>
          <span class="w-2 h-2 rounded-full bg-[#1bd96a]"></span>
        </div>
        <button
          v-else
          @click="handleLogin"
          :disabled="isAuthenticating"
          class="px-4 py-1.5 rounded-full bg-[#5865F2] hover:bg-[#4752c4] text-xs font-semibold text-white transition-all shadow-md flex items-center gap-2"
        >
          <span v-if="isAuthenticating" class="animate-spin text-sm">↻</span>
          <span>Login with Microsoft</span>
        </button>
      </div>
    </header>

    <!-- Main Workspace Container -->
    <main class="flex-1 p-6 max-w-7xl w-full mx-auto flex flex-col space-y-6 overflow-y-auto">
      <!-- SquidServers Hero Tunnel Card -->
      <div class="bg-[#17181c] border border-[#26282e] rounded-2xl p-6 shadow-xl relative overflow-hidden">
        <!-- Glow effect background -->
        <div
          class="absolute -right-20 -top-20 w-64 h-64 rounded-full blur-3xl pointer-events-none opacity-20"
          :class="{
            'bg-[#1bd96a]': serverStatus === 'Online',
            'bg-amber-400': serverStatus === 'Starting',
            'bg-gray-600': serverStatus === 'Stopped',
            'bg-red-500': serverStatus === 'Error',
          }"
        ></div>

        <!-- First-Time Claim Banner -->
        <div v-if="tunnelClaimUrl" class="mb-5 bg-amber-500/10 border border-amber-500/30 rounded-xl p-3.5 flex items-center justify-between">
          <div class="flex items-center space-x-2.5 text-xs text-amber-300">
            <span class="text-base">⚠️</span>
            <span><strong>First-Time Tunnel Setup:</strong> Claim your permanent Playit.gg IP address:</span>
          </div>
          <a
            :href="tunnelClaimUrl"
            target="_blank"
            class="px-3 py-1 bg-amber-500 hover:bg-amber-400 text-black font-bold text-xs rounded-lg transition-colors shadow"
          >
            Claim Permanent IP →
          </a>
        </div>

        <div class="flex flex-col md:flex-row md:items-center justify-between gap-6 relative z-10">
          <!-- Server Status & Tunnel URL Display -->
          <div class="space-y-2">
            <div class="flex items-center space-x-3">
              <span class="relative flex h-3.5 w-3.5">
                <span
                  v-if="serverStatus === 'Online'"
                  class="animate-ping absolute inline-flex h-full w-full rounded-full bg-[#1bd96a] opacity-75"
                ></span>
                <span
                  class="relative inline-flex rounded-full h-3.5 w-3.5"
                  :class="{
                    'bg-[#1bd96a]': serverStatus === 'Online',
                    'bg-amber-400 animate-pulse': serverStatus === 'Starting',
                    'bg-gray-500': serverStatus === 'Stopped',
                    'bg-red-500': serverStatus === 'Error',
                  }"
                ></span>
              </span>
              <span class="text-sm font-bold tracking-wide uppercase"
                :class="{
                  'text-[#1bd96a]': serverStatus === 'Online',
                  'text-amber-400': serverStatus === 'Starting',
                  'text-gray-400': serverStatus === 'Stopped',
                  'text-red-400': serverStatus === 'Error',
                }"
              >
                {{ serverStatus === 'Online' ? 'Server Online' : serverStatus === 'Starting' ? 'Server Starting...' : 'Server Offline' }}
              </span>
            </div>

            <div class="flex items-baseline space-x-3">
              <div class="font-mono text-2xl md:text-3xl font-extrabold tracking-tight text-white">
                {{ tunnelUrl ? tunnelUrl : '127.0.0.1:25565' }}
              </div>
              <button
                v-if="tunnelUrl"
                @click="copyIp"
                class="px-2.5 py-1 rounded bg-[#26282e] hover:bg-[#32353d] text-xs font-semibold text-gray-300 transition-colors border border-gray-700/60"
              >
                {{ copiedIp ? '✓ Copied' : 'Copy IP' }}
              </button>
            </div>
            <p class="text-xs text-gray-400">
              Direct connection for host: <code class="text-[#1bd96a] bg-black/30 px-1 py-0.5 rounded">127.0.0.1:25565</code> | Friends connect via Playit tunnel.
            </p>
          </div>

          <!-- Dynamic Contextual Action Bar -->
          <div class="flex items-center space-x-3">
            <!-- Mode 1: Server Stopped -->
            <template v-if="serverStatus === 'Stopped'">
              <button
                @click="handleStartServer"
                class="px-6 py-3 rounded-xl bg-[#1bd96a] hover:bg-[#16b859] text-black font-extrabold text-sm transition-all shadow-lg hover:shadow-[#1bd96a]/20 flex items-center space-x-2"
              >
                <span>▶ Start Dedicated Server</span>
              </button>
              <button
                @click="handleLaunchSingleplayer"
                class="px-5 py-3 rounded-xl bg-[#26282e] hover:bg-[#32353d] text-gray-200 font-bold text-sm transition-all border border-gray-700/60"
              >
                <span>Play Singleplayer</span>
              </button>
            </template>

            <!-- Mode 2: Server Starting -->
            <template v-else-if="serverStatus === 'Starting'">
              <div class="px-6 py-3 rounded-xl bg-[#26282e] text-amber-300 font-bold text-sm border border-amber-500/30 flex items-center space-x-2">
                <span class="animate-spin">↻</span>
                <span>Booting Server & Tunnel...</span>
              </div>
              <button
                @click="handleStopServer"
                class="px-4 py-3 rounded-xl bg-red-500/10 hover:bg-red-500/20 text-red-400 font-bold text-sm border border-red-500/30 transition-colors"
              >
                <span>Cancel</span>
              </button>
            </template>

            <!-- Mode 3: Server Online & Client Stopped -->
            <template v-else-if="serverStatus === 'Online' && clientStatus === 'Stopped'">
              <button
                @click="handleJoinLocalServer"
                class="px-6 py-3 rounded-xl bg-[#5865F2] hover:bg-[#4752c4] text-white font-extrabold text-sm transition-all shadow-lg shadow-[#5865F2]/25 flex items-center space-x-2"
              >
                <span>🎮 Join Game Directly (127.0.0.1)</span>
              </button>
              <button
                @click="handleStopServer"
                class="px-5 py-3 rounded-xl bg-red-500/10 hover:bg-red-500/20 text-red-400 font-bold text-sm border border-red-500/30 transition-colors flex items-center space-x-2"
              >
                <span>⏹ Stop Server</span>
              </button>
            </template>

            <!-- Mode 4: Server Online & Client Playing -->
            <template v-else-if="serverStatus === 'Online' && clientStatus === 'Playing'">
              <div class="px-6 py-3 rounded-xl bg-[#1bd96a]/15 text-[#1bd96a] font-bold text-sm border border-[#1bd96a]/30 flex items-center space-x-2 shadow-inner">
                <span class="w-2.5 h-2.5 rounded-full bg-[#1bd96a] animate-ping"></span>
                <span>🟢 Playing on Server</span>
              </div>
              <button
                @click="handleStopServer"
                class="px-5 py-3 rounded-xl bg-red-500/10 hover:bg-red-500/20 text-red-400 font-bold text-sm border border-red-500/30 transition-colors"
              >
                <span>⏹ Stop Server</span>
              </button>
            </template>
          </div>
        </div>
      </div>

      <!-- 3-Tab Workspace Navigation -->
      <div class="flex items-center space-x-2 border-b border-[#26282e] pb-1">
        <button
          @click="activeTab = 'mods'"
          class="px-4 py-2 font-bold text-sm rounded-t-lg transition-colors border-b-2"
          :class="activeTab === 'mods' ? 'border-[#1bd96a] text-white bg-[#17181c]' : 'border-transparent text-gray-400 hover:text-gray-200'"
        >
          Installed Mods ({{ installedMods.length }})
        </button>
        <button
          @click="activeTab = 'browse'"
          class="px-4 py-2 font-bold text-sm rounded-t-lg transition-colors border-b-2"
          :class="activeTab === 'browse' ? 'border-[#1bd96a] text-white bg-[#17181c]' : 'border-transparent text-gray-400 hover:text-gray-200'"
        >
          Browse Modrinth
        </button>
        <button
          @click="activeTab = 'console'"
          class="px-4 py-2 font-bold text-sm rounded-t-lg transition-colors border-b-2"
          :class="activeTab === 'console' ? 'border-[#1bd96a] text-white bg-[#17181c]' : 'border-transparent text-gray-400 hover:text-gray-200'"
        >
          Server Console & Config
        </button>
      </div>

      <!-- Tab 1: Installed Mods -->
      <section v-if="activeTab === 'mods'" class="space-y-4">
        <div class="flex items-center justify-between">
          <p class="text-xs text-gray-400">
            Mods are installed in <code class="text-gray-300">.minecraft/mods</code>. Client-only visual mods are automatically filtered out from <code class="text-gray-300">server/mods</code>.
          </p>
          <button
            @click="loadInstalledMods"
            class="px-3 py-1 bg-[#26282e] hover:bg-[#32353d] text-xs font-semibold rounded-md border border-gray-700/60 transition-colors"
          >
            ↻ Refresh List
          </button>
        </div>

        <div v-if="installedMods.length === 0" class="bg-[#17181c] border border-[#26282e] rounded-xl p-12 text-center text-gray-400">
          <span class="text-3xl block mb-2">📦</span>
          <p class="font-bold text-sm">No mods installed yet.</p>
          <p class="text-xs mt-1">Switch to the "Browse Modrinth" tab to install NeoForge 1.21.1 mods with 1 click.</p>
        </div>

        <div v-else class="grid grid-cols-1 md:grid-cols-2 gap-3">
          <div
            v-for="mod in installedMods"
            :key="mod.file_name"
            class="bg-[#17181c] border border-[#26282e] hover:border-gray-700 rounded-xl p-3.5 flex items-center justify-between transition-all"
          >
            <div class="flex flex-col min-w-0 pr-3">
              <span class="font-bold text-sm text-white truncate">
                {{ mod.mod_name || mod.file_name }}
              </span>
              <span class="text-[11px] font-mono text-gray-400 truncate">
                {{ mod.file_name }}
              </span>
              <div class="flex items-center space-x-2 mt-1.5">
                <span
                  class="text-[10px] font-bold px-2 py-0.5 rounded-full uppercase tracking-wider"
                  :class="mod.environment === 'ClientOnly' ? 'bg-blue-500/15 text-blue-400 border border-blue-500/30' : 'bg-[#1bd96a]/15 text-[#1bd96a] border border-[#1bd96a]/30'"
                >
                  {{ mod.environment === 'ClientOnly' ? 'Client Only (Ignored by Server)' : 'Universal (Client + Server)' }}
                </span>
                <span class="text-[10px] text-gray-500">
                  {{ (mod.size_bytes / 1024 / 1024).toFixed(1) }} MB
                </span>
              </div>
            </div>

            <button
              @click="handleDeleteMod(mod.file_name)"
              class="p-2 text-gray-500 hover:text-red-400 hover:bg-red-500/10 rounded-lg transition-colors"
              title="Delete Mod"
            >
              🗑️
            </button>
          </div>
        </div>
      </section>

      <!-- Tab 2: Browse Modrinth -->
      <section v-if="activeTab === 'browse'" class="space-y-4">
        <!-- Search Input -->
        <div class="flex items-center space-x-3">
          <input
            v-model="searchQuery"
            @keyup.enter="handleSearchModrinth"
            type="text"
            placeholder="Search NeoForge 1.21.1 mods on Modrinth (e.g. FerriteCore, JEI, JourneyMap)..."
            class="flex-1 bg-[#17181c] border border-[#26282e] focus:border-[#1bd96a] rounded-xl px-4 py-2.5 text-sm text-white focus:outline-none transition-colors"
          />
          <button
            @click="handleSearchModrinth"
            :disabled="isSearching"
            class="px-5 py-2.5 bg-[#1bd96a] hover:bg-[#16b859] text-black font-extrabold text-sm rounded-xl transition-all shadow"
          >
            {{ isSearching ? 'Searching...' : 'Search' }}
          </button>
        </div>

        <div v-if="searchResults.length === 0 && !isSearching" class="bg-[#17181c] border border-[#26282e] rounded-xl p-12 text-center text-gray-400">
          <span class="text-3xl block mb-2">🔍</span>
          <p class="font-bold text-sm">Search Modrinth for NeoForge 1.21.1 mods</p>
          <p class="text-xs mt-1">Required dependencies are resolved and installed automatically.</p>
        </div>

        <div v-else class="grid grid-cols-1 md:grid-cols-2 gap-3">
          <div
            v-for="proj in searchResults"
            :key="proj.project_id"
            class="bg-[#17181c] border border-[#26282e] rounded-xl p-4 flex space-x-3.5 hover:border-gray-700 transition-all"
          >
            <img
              v-if="proj.icon_url"
              :src="proj.icon_url"
              alt="Icon"
              class="w-12 h-12 rounded-lg bg-gray-800 object-cover flex-shrink-0"
            />
            <div v-else class="w-12 h-12 rounded-lg bg-[#26282e] flex items-center justify-center font-bold text-gray-400 flex-shrink-0">
              📦
            </div>

            <div class="flex-1 min-w-0 flex flex-col justify-between">
              <div>
                <div class="flex items-center justify-between">
                  <span class="font-bold text-sm text-white truncate">{{ proj.title }}</span>
                  <span class="text-[10px] text-gray-400 flex items-center gap-1">
                    ⬇ {{ (proj.downloads / 1000).toFixed(0) }}k
                  </span>
                </div>
                <p class="text-xs text-gray-400 line-clamp-2 mt-1">{{ proj.description }}</p>
              </div>

              <div class="flex items-center justify-between mt-3 pt-2 border-t border-[#26282e]">
                <div class="flex items-center space-x-1.5 text-[10px] text-gray-400 font-mono">
                  <span>Side: {{ proj.client_side }} / {{ proj.server_side }}</span>
                </div>
                <button
                  @click="handleInstallMod(proj.project_id)"
                  :disabled="installingMods[proj.project_id]"
                  class="px-3 py-1 bg-[#1bd96a] hover:bg-[#16b859] disabled:bg-gray-700 text-black font-bold text-xs rounded-lg transition-colors shadow"
                >
                  <span v-if="installingMods[proj.project_id]">Installing...</span>
                  <span v-else>+ Install</span>
                </button>
              </div>
            </div>
          </div>
        </div>
      </section>

      <!-- Tab 3: Server Console & RAM Slider -->
      <section v-if="activeTab === 'console'" class="space-y-4">
        <!-- RAM Allocation Slider Card -->
        <div class="bg-[#17181c] border border-[#26282e] rounded-xl p-4 flex items-center justify-between">
          <div class="flex flex-col">
            <span class="font-bold text-sm text-white">JVM Memory Allocation</span>
            <span class="text-xs text-gray-400">Allocated to NeoForge Dedicated Server & Client (Aikar's G1GC)</span>
          </div>
          <div class="flex items-center space-x-4">
            <input
              v-model.number="ramGb"
              type="range"
              min="4"
              max="16"
              step="1"
              class="w-48 accent-[#1bd96a]"
            />
            <span class="font-mono font-extrabold text-[#1bd96a] text-base w-16 text-right">
              {{ ramGb }} GB
            </span>
          </div>
        </div>

        <!-- Terminal Console Drawer -->
        <div class="bg-[#17181c] border border-[#26282e] rounded-xl overflow-hidden flex flex-col shadow-xl">
          <div class="bg-[#1e2025] px-4 py-2.5 flex items-center justify-between border-b border-[#26282e]">
            <span class="text-xs font-mono font-bold text-gray-300 flex items-center gap-2">
              <span class="w-2 h-2 rounded-full" :class="serverStatus === 'Online' ? 'bg-[#1bd96a]' : 'bg-gray-500'"></span>
              Server Terminal Output (Last 200 Lines)
            </span>
            <button
              @click="logs = []"
              class="text-xs text-gray-400 hover:text-gray-200 transition-colors"
            >
              Clear Log
            </button>
          </div>

          <!-- Log Content -->
          <div
            ref="consoleContainer"
            class="h-96 bg-[#0c0d0e] p-4 font-mono text-xs overflow-y-auto space-y-0.5 select-text leading-relaxed"
          >
            <div v-if="logs.length === 0" class="text-gray-600 italic">
              Terminal idle. Start the server to stream logs...
            </div>
            <div
              v-for="(line, idx) in logs"
              :key="idx"
              class="break-all"
              :class="{
                'text-red-400': line.includes('ERROR') || line.includes('[STDERR]'),
                'text-amber-300': line.includes('WARN'),
                'text-[#1bd96a]': line.includes('Done ('),
                'text-gray-300': !line.includes('ERROR') && !line.includes('WARN') && !line.includes('Done ('),
              }"
            >
              {{ line }}
            </div>
          </div>

          <!-- Command Input -->
          <div class="p-3 bg-[#17181c] border-t border-[#26282e] flex items-center space-x-2">
            <span class="text-gray-500 font-mono text-xs pl-1">/</span>
            <input
              v-model="consoleInput"
              @keyup.enter="handleSendCommand"
              type="text"
              placeholder="Send command to server (e.g. op Player, stop, say Hello)..."
              class="flex-1 bg-black/40 border border-[#26282e] focus:border-[#1bd96a] rounded-lg px-3 py-1.5 text-xs text-white font-mono focus:outline-none"
            />
            <button
              @click="handleSendCommand"
              class="px-4 py-1.5 bg-[#26282e] hover:bg-[#32353d] text-xs font-bold rounded-lg text-gray-200 border border-gray-700/60 transition-colors"
            >
              Send
            </button>
          </div>
        </div>
      </section>
    </main>

    <!-- Microsoft Device Code Modal -->
    <div
      v-if="deviceCodeData"
      class="fixed inset-0 bg-black/70 backdrop-blur-sm flex items-center justify-center p-4 z-50"
    >
      <div class="bg-[#17181c] border border-[#26282e] rounded-2xl p-6 max-w-md w-full shadow-2xl space-y-5">
        <div class="flex items-center space-x-3">
          <div class="w-9 h-9 rounded-xl bg-[#5865F2] flex items-center justify-center text-white font-black">
            M
          </div>
          <div>
            <h3 class="font-extrabold text-white text-base">Microsoft Sign-In Required</h3>
            <p class="text-xs text-gray-400">Authenticate your Minecraft account</p>
          </div>
        </div>

        <div class="bg-black/40 rounded-xl p-4 text-center space-y-2 border border-gray-800">
          <span class="text-xs text-gray-400">1. Enter this code:</span>
          <div class="font-mono text-3xl font-black text-[#1bd96a] tracking-widest py-1">
            {{ deviceCodeData.user_code }}
          </div>
          <button
            @click="copyUserCode"
            class="px-3 py-1 bg-[#26282e] hover:bg-[#32353d] text-xs font-semibold rounded-md border border-gray-700 text-gray-300"
          >
            {{ copiedCode ? '✓ Copied!' : 'Copy Code' }}
          </button>
        </div>

        <div class="space-y-3">
          <a
            :href="deviceCodeData.verification_uri"
            target="_blank"
            class="w-full py-2.5 bg-[#5865F2] hover:bg-[#4752c4] text-white font-bold text-sm rounded-xl text-center block transition-colors shadow"
          >
            2. Open {{ deviceCodeData.verification_uri }} →
          </a>
          <p class="text-[11px] text-gray-400 text-center">
            Waiting for confirmation from browser. This window will close automatically once authorized.
          </p>
        </div>
      </div>
    </div>
  </div>
</template>
