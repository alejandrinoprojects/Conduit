/**
 * Conduit Centralized Tauri IPC Composable
 * 
 * Provides type-safe wrappers for all 24 Tauri `invoke` commands and 9 `listen` channels
 * matching backend Rust Serde schemas with strict camelCase to snake_case mapping.
 */

import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

// ============================================================================
// TypeScript Interface Definitions (Matching Serde Schemas in src-tauri)
// ============================================================================

export interface MinecraftProfile {
  uuid: string;
  username: string;
  access_token: string;
}

export interface DeviceCodePayload {
  device_code: string;
  user_code: string;
  verification_uri: string;
  expires_in: number;
  interval: number;
}

export interface InstallProgressPayload {
  step: string;
  percentage: number;
  details: string;
}

export interface VersionMetaItem {
  id: string;
  release_type: string;
  release_time: string;
}

export interface InstalledMod {
  file_name: string;
  mod_id?: string;
  mod_name?: string;
  version?: string;
  environment: 'Both' | 'ClientOnly' | 'ServerOnly';
  size_bytes: number;
  is_enabled: boolean;
  icon_base64?: string;
}

export interface ModVersionOption {
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

export interface ModrinthGalleryItem {
  url: string;
  title?: string;
  description?: string;
  created: string;
}

export interface ModrinthProjectDetail {
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

export interface WorldSaveInfo {
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

export interface DownloadedRuntimeItem {
  id: string;
  loader: string;
  version: string;
  is_server: boolean;
  is_client: boolean;
  size_mb: number;
}

export interface ModrinthProject {
  project_id: string;
  title: string;
  description: string;
  icon_url: string | null;
  downloads: number;
  client_side: string;
  server_side: string;
  author?: string;
  categories?: string[];
}

export interface DeduplicationMetrics {
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

export interface JavaInstallation {
  name: string;
  path: string;
  version: string;
  is_recommended: boolean;
}

export interface ConduitIdentity {
  public_key: string;
  short_id: string;
  handle: string;
  created_at: number;
}

export interface NetworkStatusPayload {
  state: 'idle' | 'hosting' | 'connected';
  identity: ConduitIdentity;
  room_code: string | null;
  endpoint: string | null;
  local_proxy_port: number | null;
  is_direct: boolean;
  bytes_sent: number;
  bytes_received: number;
  latency_ms: number;
}

export interface CreateServerPayload {
  server_type: string;
  name: string;
  save_location: string;
  ram_mb: number;
  java_path: string;
  mc_version: string;
  seed: string;
  world_type: string;
  game_mode: string;
  is_imported_world: boolean;
  imported_world_path: string | null;
  enable_tunnel: boolean;
  enable_backups: boolean;
  hosting_mode: string;
  instance_dir?: string;
}

export interface GatewayConfig {
  mode: 'Disabled' | 'AlwaysOnline' | 'OnDemand' | 'SleepWhenEmpty';
  gateway_port: number;
  backend_port: number;
  idle_timeout_minutes: number;
  max_players: number;
  custom_motd: string | null;
  wake_message: string | null;
}

export interface GatewayStatePayload {
  status: 'Disabled' | 'Listening' | 'Starting' | 'Online' | 'Sleeping';
  mode: 'Disabled' | 'AlwaysOnline' | 'OnDemand' | 'SleepWhenEmpty';
  gateway_port: number;
  backend_port: number;
  active_connections: number;
  memory_saved_mb: number;
}

export interface DimensionPruneSummary {
  dimension: string;
  total_chunks: number;
  prunable_chunks: number;
  reclaimable_bytes: number;
}

export interface RoomHostingInfo {
  room_code: string;
  port: number;
}

// ============================================================================
// Event Listener Handler Interfaces
// ============================================================================

export interface GlobalEventHandlers {
  onServerLog?: (log: string) => void;
  onServerLogBatch?: (logs: string[]) => void;
  onServerStatus?: (status: string) => void;
  onClientStatus?: (status: string) => void;
  onTunnelStatus?: (url: string) => void;
  onConduitRoomCode?: (code: string) => void;
  onTunnelClaim?: (claimUrl: string) => void;
  onAuthDeviceCode?: (deviceCode: DeviceCodePayload) => void;
  onInstallProgress?: (progress: InstallProgressPayload) => void;
}

// ============================================================================
// Composable Implementation
// ============================================================================

export function useConduitIPC() {
  // Generic invoke wrapper
  async function invokeCommand<T>(cmd: string, args?: Record<string, any>): Promise<T> {
    return invoke<T>(cmd, args);
  }

  // 1. Auth & Profiles
  async function loadSavedAuth(instanceDir: string): Promise<MinecraftProfile | null> {
    return invoke<MinecraftProfile | null>('load_saved_auth', { instanceDir });
  }

  async function loginMicrosoft(instanceDir: string): Promise<MinecraftProfile> {
    return invoke<MinecraftProfile>('login_microsoft', { instanceDir });
  }

  // 2. Minecraft Versions & Runtime Management
  async function listAllMinecraftVersions(): Promise<VersionMetaItem[]> {
    return invoke<VersionMetaItem[]>('list_all_minecraft_versions');
  }

  async function listDownloadedRuntimes(instanceDir: string): Promise<DownloadedRuntimeItem[]> {
    return invoke<DownloadedRuntimeItem[]>('list_downloaded_runtimes', { instanceDir });
  }

  async function deleteDownloadedRuntime(instanceDir: string, runtimeId: string): Promise<void> {
    return invoke<void>('delete_downloaded_runtime', { instanceDir, runtimeId });
  }

  async function installInstance(instanceDir: string, loader: string, version: string): Promise<void> {
    return invoke<void>('install_instance', { instanceDir, loader, version });
  }

  // 3. Deduplication & Storage
  async function getDeduplicationMetrics(instanceDir: string): Promise<DeduplicationMetrics> {
    return invoke<DeduplicationMetrics>('get_deduplication_metrics', { instanceDir });
  }

  async function verifyAndRepairJunctions(instanceDir: string, targetWorldFolder: string = ''): Promise<any> {
    return invoke<any>('verify_and_repair_junctions', { instanceDir, targetWorldFolder });
  }

  // 4. Worlds & Saves
  async function listWorlds(instanceDir: string): Promise<WorldSaveInfo[]> {
    return invoke<WorldSaveInfo[]>('list_worlds', { instanceDir });
  }

  async function importExternalSaves(instanceDir: string): Promise<number> {
    return invoke<number>('import_external_saves', { instanceDir });
  }

  // 5. Process & Server Controls
  async function startServer(instanceDir: string, ramGb: number, worldName?: string): Promise<void> {
    return invoke<void>('start_server', { instanceDir, ramGb, worldName });
  }

  async function stopServer(): Promise<void> {
    return invoke<void>('stop_server');
  }

  async function joinLocalServer(instanceDir: string, ramGb: number): Promise<void> {
    return invoke<void>('join_local_server', { instanceDir, ramGb });
  }

  async function launchClient(instanceDir: string, ramGb: number, mode: string): Promise<void> {
    return invoke<void>('launch_client', { instanceDir, ramGb, mode });
  }

  async function openInstanceFolder(instanceDir: string): Promise<void> {
    return invoke<void>('open_instance_folder', { instanceDir });
  }

  async function sendServerCommand(command: string): Promise<void> {
    return invoke<void>('send_server_command', { command });
  }

  // 6. Installed Mods Management
  async function listInstalledMods(instanceDir: string): Promise<InstalledMod[]> {
    return invoke<InstalledMod[]>('list_installed_mods', { instanceDir });
  }

  async function toggleMod(instanceDir: string, fileName: string, enable: boolean): Promise<void> {
    return invoke<void>('toggle_mod', { instanceDir, fileName, enable });
  }

  async function getModVersions(projectId: string, loader?: string, gameVersion?: string): Promise<ModVersionOption[]> {
    return invoke<ModVersionOption[]>('get_mod_versions', { projectId, loader, gameVersion });
  }

  async function switchModVersion(
    instanceDir: string,
    oldFileName: string,
    downloadUrl: string,
    newFileName: string
  ): Promise<void> {
    return invoke<void>('switch_mod_version', { instanceDir, oldFileName, downloadUrl, newFileName });
  }

  async function deleteMod(instanceDir: string, fileName: string): Promise<void> {
    return invoke<void>('delete_mod', { instanceDir, fileName });
  }

  // 7. Modrinth Discovery & Installation
  async function searchModrinth(params: {
    query: string;
    loader?: string;
    gameVersion?: string;
    projectType?: string;
    category?: string;
    limit?: number;
  }): Promise<ModrinthProject[]> {
    const raw = await invoke<any>('search_modrinth', {
      query: params.query,
      loader: params.loader || '',
      gameVersion: params.gameVersion || '',
      projectType: params.projectType || 'mod',
      category: params.category || '',
      limit: params.limit || 20
    });

    // Safely normalize response variations (Vec<ModrinthProject> direct vs { hits: ModrinthProject[] })
    if (Array.isArray(raw)) {
      return raw;
    }
    if (raw && Array.isArray(raw.hits)) {
      return raw.hits;
    }
    return [];
  }

  async function installModrinthProject(params: {
    instanceDir: string;
    projectId: string;
    projectType?: string;
    loader?: string;
    gameVersion?: string;
  }): Promise<string> {
    return invoke<string>('install_modrinth_project', {
      instanceDir: params.instanceDir,
      projectId: params.projectId,
      projectType: params.projectType || 'mod',
      loader: params.loader || '',
      gameVersion: params.gameVersion || ''
    });
  }

  async function getModrinthProjectDetail(projectIdOrSlug: string): Promise<ModrinthProjectDetail> {
    return invoke<ModrinthProjectDetail>('get_modrinth_project_detail', { projectIdOrSlug });
  }

  async function installModrinthMod(instanceDir: string, downloadUrl: string, filename: string): Promise<void> {
    return invoke<void>('install_modrinth_mod', { instanceDir, downloadUrl, filename });
  }

  // 8. Player Management
  async function deletePlayerData(instanceDir: string, uuid: string, scope: string): Promise<string> {
    return invoke<string>('delete_player_data', { instanceDir, uuid, scope });
  }

  // 9. Conduit Network & P2P
  async function conduitGetIdentity(): Promise<ConduitIdentity> {
    return invoke<ConduitIdentity>('conduit_get_identity');
  }

  async function conduitGetNetworkStatus(): Promise<NetworkStatusPayload> {
    return invoke<NetworkStatusPayload>('conduit_get_network_status');
  }

  async function conduitConnectRealm(params: {
    endpoint: string;
    hostPubkeyB64: string;
    desiredLocalPort?: number;
  }): Promise<number> {
    return invoke<number>('conduit_connect_realm', {
      endpoint: params.endpoint,
      hostPubkeyB64: params.hostPubkeyB64,
      desiredLocalPort: params.desiredLocalPort ?? 25565
    });
  }

  async function conduitHostRealm(realmName: string, localPort?: number): Promise<RoomHostingInfo> {
    return invoke<RoomHostingInfo>('conduit_host_realm', { realmName, localPort });
  }

  async function conduitStopHosting(): Promise<boolean> {
    return invoke<boolean>('conduit_stop_hosting');
  }

  async function conduitDisconnectRealm(): Promise<boolean> {
    return invoke<boolean>('conduit_disconnect_realm');
  }

  // 10. Java & Creation Wizards
  async function detectInstalledJavas(): Promise<JavaInstallation[]> {
    return invoke<JavaInstallation[]>('detect_installed_javas');
  }

  async function createServerWizard(payload: CreateServerPayload): Promise<string> {
    return invoke<string>('create_server_wizard', { payload });
  }

  async function createNewRealm(instanceDir: string, realmName: string, gameMode: string, seed: string): Promise<string> {
    return invoke<string>('create_new_realm', { instanceDir, realmName, gameMode, seed });
  }

  // 11. Gateway On-Demand Automation
  async function getGatewayConfig(instanceDir: string): Promise<GatewayConfig> {
    return invoke<GatewayConfig>('get_gateway_config', { instanceDir });
  }

  async function saveGatewayConfig(instanceDir: string, config: GatewayConfig): Promise<void> {
    return invoke<void>('save_gateway_config', { instanceDir, config });
  }

  async function startGateway(instanceDir: string, ramGb: number, worldName?: string): Promise<void> {
    return invoke<void>('start_gateway', { instanceDir, ramGb, worldName });
  }

  async function stopGateway(): Promise<void> {
    return invoke<void>('stop_gateway');
  }

  async function getGatewayState(): Promise<GatewayStatePayload> {
    return invoke<GatewayStatePayload>('get_gateway_state');
  }

  // 12. Realm World Pruning
  async function getRealmPruneInfo(instanceDir: string, worldFolder: string, dimension: string): Promise<DimensionPruneSummary> {
    return invoke<DimensionPruneSummary>('get_realm_prune_info', { instanceDir, worldFolder, dimension });
  }

  async function executeRealmPrune(
    instanceDir: string,
    worldFolder: string,
    dimension: string,
    chunksToPrune: [number, number][],
    mode: string
  ): Promise<string> {
    return invoke<string>('execute_realm_prune', { instanceDir, worldFolder, dimension, chunksToPrune, mode });
  }

  // ============================================================================
  // Event Subscription Setup with Clean Unsubscribe
  // ============================================================================

  async function setupGlobalListeners(handlers: GlobalEventHandlers): Promise<() => void> {
    const unlistenFns: UnlistenFn[] = [];

    // 1. Single-line server log
    const unlistenLog = await listen<string>('server-log', (event) => {
      handlers.onServerLog?.(event.payload);
    });
    unlistenFns.push(unlistenLog);

    // 2. Batch server logs (high throughput)
    const unlistenBatch = await listen<string[]>('server-log-batch', (event) => {
      handlers.onServerLogBatch?.(event.payload);
    });
    unlistenFns.push(unlistenBatch);

    // 3. Server status
    const unlistenServerStatus = await listen<string>('server-status', (event) => {
      handlers.onServerStatus?.(event.payload);
    });
    unlistenFns.push(unlistenServerStatus);

    // 4. Client status
    const unlistenClientStatus = await listen<string>('client-status', (event) => {
      handlers.onClientStatus?.(event.payload);
    });
    unlistenFns.push(unlistenClientStatus);

    // 5. Tunnel status
    const unlistenTunnel = await listen<string>('tunnel-status', (event) => {
      handlers.onTunnelStatus?.(event.payload);
    });
    unlistenFns.push(unlistenTunnel);

    // 6. Conduit room code
    const unlistenRoom = await listen<string>('conduit-room-code', (event) => {
      handlers.onConduitRoomCode?.(event.payload);
    });
    unlistenFns.push(unlistenRoom);

    // 7. Tunnel claim URL
    const unlistenClaim = await listen<string>('tunnel-claim', (event) => {
      handlers.onTunnelClaim?.(event.payload);
    });
    unlistenFns.push(unlistenClaim);

    // 8. Auth device code
    const unlistenAuth = await listen<DeviceCodePayload>('auth-device-code', (event) => {
      handlers.onAuthDeviceCode?.(event.payload);
    });
    unlistenFns.push(unlistenAuth);

    // 9. Installation progress
    const unlistenProgress = await listen<InstallProgressPayload>('install-progress', (event) => {
      handlers.onInstallProgress?.(event.payload);
    });
    unlistenFns.push(unlistenProgress);

    // Return cleanup function to unlisten from all channels
    return () => {
      for (const unlisten of unlistenFns) {
        try {
          unlisten();
        } catch (err) {
          console.warn('Error during Tauri event unlisten:', err);
        }
      }
      unlistenFns.length = 0;
    };
  }

  return {
    invokeCommand,
    loadSavedAuth,
    loginMicrosoft,
    listAllMinecraftVersions,
    listDownloadedRuntimes,
    deleteDownloadedRuntime,
    installInstance,
    getDeduplicationMetrics,
    verifyAndRepairJunctions,
    listWorlds,
    importExternalSaves,
    startServer,
    stopServer,
    joinLocalServer,
    launchClient,
    openInstanceFolder,
    sendServerCommand,
    listInstalledMods,
    toggleMod,
    getModVersions,
    switchModVersion,
    deleteMod,
    searchModrinth,
    installModrinthProject,
    getModrinthProjectDetail,
    installModrinthMod,
    deletePlayerData,
    conduitGetIdentity,
    conduitGetNetworkStatus,
    conduitConnectRealm,
    conduitHostRealm,
    conduitStopHosting,
    conduitDisconnectRealm,
    detectInstalledJavas,
    createServerWizard,
    createNewRealm,
    getGatewayConfig,
    saveGatewayConfig,
    startGateway,
    stopGateway,
    getGatewayState,
    getRealmPruneInfo,
    executeRealmPrune,
    setupGlobalListeners
  };
}
