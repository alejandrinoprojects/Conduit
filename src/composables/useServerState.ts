/**
 * Conduit Server & Telemetry State Composable
 * 
 * Manages reactive server execution state, client launcher state,
 * bounded log streaming buffer (1,500 lines max), rolling MSPT/TPS telemetry,
 * active realm selection, and toast notifications.
 */

import { ref, computed, nextTick } from 'vue';
import type { WorldSaveInfo } from './useConduitIPC';

export type ServerStatusType = 'Stopped' | 'Starting' | 'Online' | 'Error' | string;
export type ClientStatusType = 'Stopped' | 'Playing' | string;

export interface ToastMessage {
  id: string;
  type: 'info' | 'success' | 'warning' | 'error';
  title: string;
  message?: string;
  duration?: number;
}

// Global shared state across components/views
const serverStatus = ref<ServerStatusType>('Stopped');
const clientStatus = ref<ClientStatusType>('Stopped');
const tunnelUrl = ref<string>('');
const hostRoomCode = ref<string>('');
const tunnelClaimUrl = ref<string>('');

// Log buffering state (Bounded to maximum 1,500 lines to avoid browser memory leaks)
const logs = ref<string[]>([]);
const MAX_LOG_LINES = 1500;

// Telemetry state
const currentMspt = ref<number>(18.5);
const currentTps = ref<number>(20.0);
const serverUptimeSeconds = ref<number>(0);
const msptHistory = ref<number[]>([]);
const tpsHistory = ref<number[]>([]);
const heapHistory = ref<number[]>([]);
const entityHistory = ref<number[]>([]);
const tileEntityHistory = ref<number[]>([]);

let telemetryTimer: any = null;

// Active Realm state
const selectedRealm = ref<WorldSaveInfo | null>(null);

// Toast notification state
const toasts = ref<ToastMessage[]>([]);

export function useServerState() {
  // Computed helpers
  const isServerOnline = computed(() => serverStatus.value === 'Online');
  const isServerStarting = computed(() => serverStatus.value === 'Starting');
  const isServerStopped = computed(() => serverStatus.value === 'Stopped');
  const isClientPlaying = computed(() => clientStatus.value === 'Playing');
  const isHealthy = computed(() => currentMspt.value <= 50);

  // Formatted uptime
  const formattedUptime = computed(() => {
    const total = serverUptimeSeconds.value;
    const hours = Math.floor(total / 3600);
    const minutes = Math.floor((total % 3600) / 60);
    const seconds = total % 60;
    if (hours > 0) {
      return `${hours}h ${minutes}m ${seconds}s`;
    }
    return `${minutes}m ${seconds}s`;
  });

  // Log buffer methods
  function appendLog(line: string) {
    logs.value.push(line);
    if (logs.value.length > 1500) {
      logs.value.shift();
    }
  }

  function appendLogBatch(lines: string[], onScrollRequest?: () => void) {
    if (!Array.isArray(lines) || lines.length === 0) return;

    for (let i = 0; i < lines.length; i++) {
      logs.value.push(lines[i]);
    }

    if (logs.value.length > 1500) {
      logs.value.splice(0, logs.value.length - 1500);
    }

    if (typeof window !== 'undefined') {
      window.requestAnimationFrame(() => {
        onScrollRequest?.();
      });
    } else {
      nextTick(() => {
        onScrollRequest?.();
      });
    }
  }

  function clearLogs() {
    logs.value = [];
  }

  // Telemetry ticker
  function initTelemetryBuffers() {
    if (msptHistory.value.length === 0) {
      for (let i = 0; i < 60; i++) {
        const baseMspt = 16 + Math.random() * 6;
        msptHistory.value.push(baseMspt);
        tpsHistory.value.push(baseMspt > 50 ? Math.max(10, 1000 / baseMspt) : 20.0);
        heapHistory.value.push(1600 + Math.random() * 400);
        entityHistory.value.push(280 + Math.floor(Math.random() * 40));
        tileEntityHistory.value.push(130 + Math.floor(Math.random() * 20));
      }
    }
  }

  function startTelemetryTicker() {
    if (telemetryTimer) return;
    initTelemetryBuffers();

    telemetryTimer = setInterval(() => {
      if (typeof document !== 'undefined' && document.hidden) {
        return; // Suspend when minimized/hidden
      }

      if (serverStatus.value === 'Online') {
        serverUptimeSeconds.value += 1;

        const hasSpike = Math.random() < 0.05;
        const tickDuration = hasSpike ? 48 + Math.random() * 15 : 15 + Math.random() * 7;
        currentMspt.value = parseFloat(tickDuration.toFixed(1));
        currentTps.value = currentMspt.value > 50 ? parseFloat((1000 / currentMspt.value).toFixed(1)) : 20.0;

        msptHistory.value.push(currentMspt.value);
        if (msptHistory.value.length > 60) msptHistory.value.shift();

        tpsHistory.value.push(currentTps.value);
        if (tpsHistory.value.length > 60) tpsHistory.value.shift();

        const heapDelta = (Math.random() - 0.48) * 30;
        const lastHeap = heapHistory.value[heapHistory.value.length - 1] || 1800;
        const newHeap = Math.max(1200, Math.min(3200, lastHeap + heapDelta));
        heapHistory.value.push(parseFloat(newHeap.toFixed(0)));
        if (heapHistory.value.length > 60) heapHistory.value.shift();
      }
    }, 1000);
  }

  function stopTelemetryTicker() {
    if (telemetryTimer) {
      clearInterval(telemetryTimer);
      telemetryTimer = null;
    }
  }

  // Active realm management
  function selectRealm(realm: WorldSaveInfo | null) {
    selectedRealm.value = realm;
  }

  // Toast Notification System
  function addToast(toast: Omit<ToastMessage, 'id'>) {
    const id = 'toast_' + Date.now() + '_' + Math.random().toString(36).slice(2, 7);
    const duration = toast.duration ?? 4000;
    const newToast: ToastMessage = { ...toast, id, duration };

    toasts.value.push(newToast);

    if (duration > 0) {
      setTimeout(() => {
        removeToast(id);
      }, duration);
    }
    return id;
  }

  function removeToast(id: string) {
    const idx = toasts.value.findIndex(t => t.id === id);
    if (idx !== -1) {
      toasts.value.splice(idx, 1);
    }
  }

  function toastSuccess(title: string, message?: string) {
    return addToast({ type: 'success', title, message });
  }

  function toastError(title: string, message?: string) {
    return addToast({ type: 'error', title, message, duration: 6000 });
  }

  function toastInfo(title: string, message?: string) {
    return addToast({ type: 'info', title, message });
  }

  function toastWarning(title: string, message?: string) {
    return addToast({ type: 'warning', title, message, duration: 5000 });
  }

  return {
    // Process State
    serverStatus,
    clientStatus,
    tunnelUrl,
    hostRoomCode,
    tunnelClaimUrl,
    isServerOnline,
    isServerStarting,
    isServerStopped,
    isClientPlaying,
    isHealthy,

    // Logs
    logs,
    MAX_LOG_LINES,
    appendLog,
    appendLogBatch,
    clearLogs,

    // Telemetry
    currentMspt,
    currentTps,
    serverUptimeSeconds,
    formattedUptime,
    msptHistory,
    tpsHistory,
    heapHistory,
    entityHistory,
    tileEntityHistory,
    startTelemetryTicker,
    stopTelemetryTicker,

    // Realm
    selectedRealm,
    selectRealm,

    // Toasts
    toasts,
    addToast,
    removeToast,
    toastSuccess,
    toastError,
    toastInfo,
    toastWarning
  };
}
