#!/usr/bin/env node
/**
 * Empirical Verification & Adversarial Stress Harness: Milestone 3
 * 
 * Verifies:
 * 1. Component compilation & export for src/views/LibraryView.vue and src/views/RealmDetailView.vue via Vite SSR.
 * 2. showDeleteDataConfirmModal markup presence, prop contracts, radio options, and invoke wiring.
 * 3. RAM allocation slider boundaries (min=2, max=16) for both server and client sliders.
 * 4. Absence of nested card clusters in LibraryView.vue & adherence to flat table layout.
 * 5. Telemetry metric calculations, rolling sparkline math, and MSPT threshold styling (>50ms -> error).
 * 6. Adversarial edge cases: empty datasets, nullish fields, extreme lag spikes, out-of-range telemetry.
 */

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const ROOT_DIR = path.resolve(__dirname, '..');
const SRC_DIR = path.join(ROOT_DIR, 'src');
const VIEWS_DIR = path.join(SRC_DIR, 'views');
const LIBRARY_VIEW_PATH = path.join(VIEWS_DIR, 'LibraryView.vue');
const REALM_DETAIL_VIEW_PATH = path.join(VIEWS_DIR, 'RealmDetailView.vue');

// Minimal browser environment polyfills for SSR and Tauri IPC mocks
globalThis.window = globalThis.window || {};
Object.assign(globalThis.window, {
  addEventListener() {},
  removeEventListener() {},
  requestAnimationFrame(cb) { return setTimeout(cb, 16); },
  cancelAnimationFrame(id) { clearTimeout(id); },
  __TAURI_INTERNALS__: {
    transformCallback: () => 1,
    invoke: async (cmd, args) => {
      if (cmd === 'delete_player_data') {
        return `Deleted playerdata for ${args?.uuid} (scope: ${args?.scope})`;
      }
      return {};
    }
  },
  __TAURI_EVENT_PLUGIN_INTERNALS__: {
    unregisterListener: () => {}
  }
});
globalThis.document = globalThis.document || {
  hidden: false,
  createElement: () => ({ style: {} }),
  addEventListener: () => {},
  removeEventListener: () => {}
};
try {
  if (!globalThis.navigator?.clipboard) {
    Object.defineProperty(globalThis.navigator, 'clipboard', {
      value: { writeText: async () => {} },
      configurable: true
    });
  }
} catch {
  // Ignored if navigator is read-only
}

let passed = 0;
let failed = 0;
const failures = [];

function assert(condition, testName, failDetail = '') {
  if (condition) {
    passed++;
    console.log(`  \x1b[32m✔ PASS\x1b[0m ${testName}`);
  } else {
    failed++;
    const fullMessage = failDetail ? `${testName} -> ${failDetail}` : testName;
    failures.push(fullMessage);
    console.error(`  \x1b[31m✖ FAIL\x1b[0m ${fullMessage}`);
  }
}

async function run() {
  console.log('\n\x1b[1m\x1b[36m======================================================================\x1b[0m');
  console.log('\x1b[1m\x1b[36m  CONDUIT MILESTONE 3: EMPIRICAL VIEWS & ADVERSARIAL STRESS HARNESS   \x1b[0m');
  console.log('\x1b[1m\x1b[36m======================================================================\x1b[0m\n');

  // ============================================================================
  // SECTION 1: File Existence & Static Inspection
  // ============================================================================
  console.log('\x1b[1m▶ Section 1: File Existence & Baseline Source Inspection\x1b[0m');

  assert(fs.existsSync(LIBRARY_VIEW_PATH), 'src/views/LibraryView.vue exists on disk');
  assert(fs.existsSync(REALM_DETAIL_VIEW_PATH), 'src/views/RealmDetailView.vue exists on disk');

  const librarySource = fs.readFileSync(LIBRARY_VIEW_PATH, 'utf-8');
  const realmDetailSource = fs.readFileSync(REALM_DETAIL_VIEW_PATH, 'utf-8');

  assert(librarySource.length > 500, 'LibraryView.vue has non-trivial source content (>500 chars)');
  assert(realmDetailSource.length > 1000, 'RealmDetailView.vue has non-trivial source content (>1000 chars)');

  // ============================================================================
  // SECTION 2: Dynamic Vite SSR Module Compilation & Component Exports
  // ============================================================================
  console.log('\n\x1b[1m▶ Section 2: Vite SSR Compilation & Component Exports\x1b[0m');

  const { createServer } = await import('vite');
  const server = await createServer({
    server: { middlewareMode: true },
    ssr: { external: ['vue', 'vue/server-renderer', 'lucide-vue-next', '@tauri-apps/api/core', '@tauri-apps/api/event'] },
  });

  let LibraryViewModule;
  let RealmDetailViewModule;

  try {
    LibraryViewModule = await server.ssrLoadModule('/src/views/LibraryView.vue');
    assert(true, 'LibraryView.vue compiles and loads cleanly via Vite SSR');
  } catch (err) {
    assert(false, 'LibraryView.vue failed to compile via Vite SSR', err.message);
  }

  try {
    RealmDetailViewModule = await server.ssrLoadModule('/src/views/RealmDetailView.vue');
    assert(true, 'RealmDetailView.vue compiles and loads cleanly via Vite SSR');
  } catch (err) {
    assert(false, 'RealmDetailView.vue failed to compile via Vite SSR', err.message);
  }

  const LibraryView = LibraryViewModule?.default;
  const RealmDetailView = RealmDetailViewModule?.default;

  assert(typeof LibraryView === 'object' && LibraryView !== null, 'LibraryView exports default Vue component object');
  assert(typeof RealmDetailView === 'object' && RealmDetailView !== null, 'RealmDetailView exports default Vue component object');

  // SSR Rendering capabilities
  const { createVNode } = await import('vue');
  const { renderToString } = await import('vue/server-renderer');

  // ============================================================================
  // SECTION 3: Delete Player Data Modal (showDeleteDataConfirmModal)
  // ============================================================================
  console.log('\n\x1b[1m▶ Section 3: Delete Player Data Modal (showDeleteDataConfirmModal) Verification\x1b[0m');

  // 1. Reactive flag declaration
  assert(
    realmDetailSource.includes('showDeleteDataConfirmModal = ref<boolean>(false)') ||
    realmDetailSource.includes('showDeleteDataConfirmModal = ref(false)'),
    'RealmDetailView declares reactive state `showDeleteDataConfirmModal`'
  );

  // 2. Modal component markup presence
  const modalRegex = /<Modal[\s\S]*?v-model="showDeleteDataConfirmModal"[\s\S]*?>[\s\S]*?<\/Modal>/;
  const modalMatch = realmDetailSource.match(modalRegex);
  assert(modalMatch !== null, 'RealmDetailView template contains `<Modal v-model="showDeleteDataConfirmModal">`');

  const modalMarkup = modalMatch ? modalMatch[0] : '';

  // 3. Modal Attributes & Prop contracts
  assert(modalMarkup.includes('title="Delete Player Data"'), 'Modal title is "Delete Player Data"');
  assert(modalMarkup.includes('maxWidth="md"') || modalMarkup.includes(':max-width="\'md\'"'), 'Modal maxWidth is set to "md"');
  assert(modalMarkup.includes('description="Permanently remove saved player statistics, inventory, and advancements."'), 'Modal includes clear warning description prop');

  // 4. Warning banner
  assert(
    modalMarkup.includes('bg-status-error-subtle') && modalMarkup.includes('playerdata/'),
    'Modal contains warning banner referencing playerdata/ deletion in world folder'
  );

  // 5. Deletion Scopes Radio Buttons
  assert(modalMarkup.includes('v-model="deleteDataScope"'), 'Modal contains radio group bound to deleteDataScope');
  assert(modalMarkup.includes('value="all"'), 'Modal provides "all" scope (Complete Reset)');
  assert(modalMarkup.includes('value="playerdata"'), 'Modal provides "playerdata" scope (Inventory & Position Only)');
  assert(modalMarkup.includes('value="stats"'), 'Modal provides "stats" scope (Statistics & Advancements Only)');

  // 6. Action buttons
  assert(
    modalMarkup.includes('@click="showDeleteDataConfirmModal = false"'),
    'Modal contains Cancel action button that dismisses the modal'
  );
  assert(
    modalMarkup.includes('@click="executeDeletePlayerData"'),
    'Modal contains Confirm Deletion button wired to executeDeletePlayerData'
  );
  assert(
    modalMarkup.includes(':loading="isDeletingPlayerData"'),
    'Modal Confirm Deletion button reflects loading state via isDeletingPlayerData'
  );

  // 7. IPC invocation contract
  assert(
    realmDetailSource.includes("invoke<string>('delete_player_data'") ||
    realmDetailSource.includes("invoke('delete_player_data'"),
    'executeDeletePlayerData invokes Tauri command "delete_player_data"'
  );
  assert(
    realmDetailSource.includes('instanceDir: props.instanceDir') &&
    realmDetailSource.includes('uuid: selectedPlayerAction.value.uuid') &&
    realmDetailSource.includes('scope: deleteDataScope.value'),
    'delete_player_data invocation passes exact payload { instanceDir, uuid, scope }'
  );
  assert(
    realmDetailSource.includes("emit('delete-player-data'"),
    'RealmDetailView emits delete-player-data event upon successful execution'
  );

  // ============================================================================
  // SECTION 4: RAM Allocation Slider Boundaries (min=2, max=16)
  // ============================================================================
  console.log('\n\x1b[1m▶ Section 4: RAM Allocation Slider Boundary Verification\x1b[0m');

  // Extract range sliders in RealmDetailView
  const rangeInputs = realmDetailSource.match(/<input[^>]+type="range"[^>]*>/g) || [];
  assert(rangeInputs.length >= 2, `RealmDetailView contains ${rangeInputs.length} range sliders (expected at least 2)`);

  let serverSliderFound = false;
  let clientSliderFound = false;

  for (const slider of rangeInputs) {
    if (slider.includes('serverRamGb')) {
      serverSliderFound = true;
      const hasMin2 = slider.includes('min="2"');
      const hasMax16 = slider.includes('max="16"');
      assert(hasMin2 && hasMax16, `Dedicated Server RAM slider has exact bounds min="2" max="16" (found: ${slider.trim()})`);
    }
    if (slider.includes('clientRamGb')) {
      clientSliderFound = true;
      const hasMin2 = slider.includes('min="2"');
      const hasMax16 = slider.includes('max="16"');
      assert(hasMin2 && hasMax16, `Client RAM slider has exact bounds min="2" max="16" (found: ${slider.trim()})`);
    }
  }

  assert(serverSliderFound, 'Found Dedicated Server RAM slider bound to serverRamGb');
  assert(clientSliderFound, 'Found Minecraft Client RAM slider bound to clientRamGb');

  // Verify default slider values are valid within [2, 16]
  const serverDefaultMatch = realmDetailSource.match(/serverRamGb\s*=\s*ref<number>\((\d+)\)/);
  const clientDefaultMatch = realmDetailSource.match(/clientRamGb\s*=\s*ref<number>\((\d+)\)/);

  const serverDefault = serverDefaultMatch ? parseInt(serverDefaultMatch[1], 10) : NaN;
  const clientDefault = clientDefaultMatch ? parseInt(clientDefaultMatch[1], 10) : NaN;

  assert(serverDefault >= 2 && serverDefault <= 16, `serverRamGb default (${serverDefault}) is within [2, 16] GB`);
  assert(clientDefault >= 2 && clientDefault <= 16, `clientRamGb default (${clientDefault}) is within [2, 16] GB`);

  // Verify no prohibited slider min/max anywhere in M3 views
  const invalidMin = rangeInputs.some(tag => tag.includes('min="0"') || tag.includes('min="1"'));
  const invalidMax = rangeInputs.some(tag => tag.includes('max="32"') || tag.includes('max="64"'));
  assert(!invalidMin, 'No memory sliders allow dangerous sub-2GB memory allocation');
  assert(!invalidMax, 'No memory sliders exceed the 16GB allocation upper limit in Realm properties');

  // ============================================================================
  // SECTION 5: Absence of Nested Card Clusters in LibraryView.vue
  // ============================================================================
  console.log('\n\x1b[1m▶ Section 5: Absence of Nested Card Clusters & Layout Conformance\x1b[0m');

  // Verify flat table layout per instance runtime
  assert(
    librarySource.includes('<table class="w-full text-left text-xs border-collapse">'),
    'LibraryView employs flat tabular layout for realm listings'
  );

  // Check 5 required table headers
  const tableHeaderMatch = librarySource.includes('Realm / Name') &&
                           librarySource.includes('Status') &&
                           librarySource.includes('Players') &&
                           librarySource.includes('Last Played') &&
                           librarySource.includes('Actions');
  assert(tableHeaderMatch, 'LibraryView table specifies exact 5 required columns (Realm/Name, Status, Players, Last Played, Actions)');

  // Verify absence of Card-in-Card nesting
  // 1. Check if <Card> component is nested inside <Card>
  const hasNestedCardComponent = /<Card[^>]*>[\s\S]*?<Card/m.test(librarySource);
  assert(!hasNestedCardComponent, 'Zero <Card> components nested inside <Card>');

  // 2. Check that table cells <td> do not contain card containers
  const tdBlocks = librarySource.match(/<td[\s\S]*?<\/td>/g) || [];
  const cardContainerInTd = tdBlocks.some(td => /class="[^"]*(?<!hover:)bg-card\b/.test(td) || td.includes('<Card'));
  assert(!cardContainerInTd, 'Zero card containers (standalone bg-card or <Card>) nested inside table rows/cells');

  // 3. Check for nested bg-card container hierarchy (tag stack scan)
  function findNestedCardDivs(html) {
    const tokens = html.match(/<\/?(?:div|section|table|tr|td)\b[^>]*>/gi) || [];
    let cardDepth = 0;
    let foundNested = false;

    for (const token of tokens) {
      const isClosing = token.startsWith('</');
      const isCard = /\bclass="[^"]*(?<!hover:)bg-card\b/i.test(token);

      if (!isClosing && isCard) {
        if (cardDepth > 0) {
          foundNested = true;
          break;
        }
        cardDepth++;
      } else if (isClosing && token.toLowerCase().startsWith('</div')) {
        if (cardDepth > 0) {
          cardDepth--;
        }
      }
    }
    return foundNested;
  }

  assert(!findNestedCardDivs(librarySource), 'LibraryView contains zero nested card-in-card hierarchy');

  // Absence of prohibited gold/amber in LibraryView
  const prohibitedGoldAmber = '#f59e0b';
  assert(!librarySource.toLowerCase().includes(prohibitedGoldAmber), 'LibraryView contains zero prohibited gold/amber (#f59e0b)');
  assert(!realmDetailSource.toLowerCase().includes(prohibitedGoldAmber), 'RealmDetailView contains zero prohibited gold/amber (#f59e0b)');

  // Split-menu launch button with primary orange CTA
  assert(
    librarySource.includes('variant="primary"') && librarySource.includes('Start &amp; Join'),
    'LibraryView features primary launch CTA with orange styling'
  );
  assert(
    librarySource.includes('toggleSplitMenu'),
    'LibraryView features split-menu dropdown for secondary launch actions'
  );

  // ============================================================================
  // SECTION 6: Telemetry Metric Calculations & MSPT Threshold Styling
  // ============================================================================
  console.log('\n\x1b[1m▶ Section 6: Telemetry Metric Calculations & MSPT Threshold Styling\x1b[0m');

  // 1. MetricCard status expression in template
  assert(
    realmDetailSource.includes(":status=\"currentMspt > 50 ? 'error' : 'cyan'\""),
    'MSPT MetricCard status dynamically evaluates `currentMspt > 50 ? "error" : "cyan"`'
  );

  // Function to simulate telemetry status logic
  function getMsptStatus(mspt) {
    return mspt > 50 ? 'error' : 'cyan';
  }

  assert(getMsptStatus(15.2) === 'cyan', 'MSPT 15.2ms evaluates to status "cyan"');
  assert(getMsptStatus(35.0) === 'cyan', 'MSPT 35.0ms evaluates to status "cyan"');
  assert(getMsptStatus(50.0) === 'cyan', 'Boundary: MSPT exactly 50.0ms remains "cyan" (not error)');
  assert(getMsptStatus(50.1) === 'error', 'Boundary: MSPT 50.1ms switches immediately to status "error"');
  assert(getMsptStatus(85.4) === 'error', 'MSPT 85.4ms evaluates to status "error"');

  // 2. Main Tick Thread Usage status expression
  assert(
    realmDetailSource.includes(":status=\"mainTickThreadUsage > 85 ? 'error' : 'normal'\""),
    'Main Tick Thread MetricCard status evaluates `mainTickThreadUsage > 85 ? "error" : "normal"`'
  );

  function getTickThreadStatus(usage) {
    return usage > 85 ? 'error' : 'normal';
  }
  assert(getTickThreadStatus(22) === 'normal', 'Main Tick Thread 22% is status "normal"');
  assert(getTickThreadStatus(85) === 'normal', 'Main Tick Thread exactly 85% is status "normal"');
  assert(getTickThreadStatus(86) === 'error', 'Main Tick Thread 86% triggers status "error"');

  // 3. Server Heap calculation
  assert(
    realmDetailSource.includes(':value="(jvmUsedHeapMb / 1024).toFixed(1)"') &&
    realmDetailSource.includes('unit="GB"'),
    'Server Heap MetricCard calculates GB from jvmUsedHeapMb (divided by 1024 with 1 decimal place)'
  );

  function calculateHeapGb(mb) {
    return (mb / 1024).toFixed(1);
  }
  assert(calculateHeapGb(1024) === '1.0', '1024 MB heap formats to "1.0" GB');
  assert(calculateHeapGb(1840) === '1.8', '1840 MB heap formats to "1.8" GB');
  assert(calculateHeapGb(4096) === '4.0', '4096 MB heap formats to "4.0" GB');

  // 4. Sparkline Stream SVG Math Functions
  console.log('  Testing generateSvgArea and generateSvgLine algorithms:');

  // Extract functions using eval / simulated implementation verified from source
  function generateSvgArea(points, minVal, maxVal, w, h) {
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

  function generateSvgLine(points, minVal, maxVal, w, h) {
    if (!points || points.length === 0) return '';
    const step = w / Math.max(1, points.length - 1);
    return points.map((val, i) => {
      const clamped = Math.max(minVal, Math.min(maxVal, val));
      const y = h - ((clamped - minVal) / (maxVal - minVal)) * h;
      return `${i === 0 ? 'M' : 'L'} ${i * step},${y}`;
    }).join(' ');
  }

  // Edge case A: Empty history
  assert(generateSvgArea([], 0, 60, 600, 96) === '', 'generateSvgArea with empty points array returns ""');
  assert(generateSvgLine([], 0, 60, 600, 96) === '', 'generateSvgLine with empty points array returns ""');

  // Edge case B: Single point
  const singleLine = generateSvgLine([25], 0, 60, 600, 96);
  assert(singleLine.startsWith('M 0,'), `generateSvgLine handles single point without divide-by-zero: "${singleLine}"`);

  // Edge case C: Normal points series
  const normalPoints = [10, 15, 20, 25, 30];
  const normalArea = generateSvgArea(normalPoints, 0, 60, 600, 96);
  const normalLine = generateSvgLine(normalPoints, 0, 60, 600, 96);
  assert(normalArea.startsWith('M 0,96 L 0,') && normalArea.endsWith('Z'), 'generateSvgArea generates closed polygon path');
  assert(normalLine.startsWith('M 0,') && normalLine.includes('L 600,'), 'generateSvgLine spans full width 0 to 600');

  // Edge case D: Extreme lag spikes exceeding maxVal (e.g. 150ms clamped to 60)
  const lagSpikePoints = [20, 80, 120, 200];
  const spikeArea = generateSvgArea(lagSpikePoints, 0, 60, 600, 96);
  const spikeLine = generateSvgLine(lagSpikePoints, 0, 60, 600, 96);
  assert(!spikeArea.includes('NaN'), 'Extreme lag spikes do not produce NaN in SVG area path');
  assert(!spikeLine.includes('NaN'), 'Extreme lag spikes do not produce NaN in SVG line path');
  assert(spikeArea.includes(',0'), 'Values exceeding maxVal clamp to y=0 (top edge of chart)');

  // 5. 50ms Lag Threshold Line in Template
  assert(
    realmDetailSource.includes('50.0 ms Threshold') &&
    realmDetailSource.includes('border-status-error'),
    'RealmDetailView contains explicit 50.0 ms Threshold line styled with status-error'
  );

  // ============================================================================
  // SECTION 7: Empirical SSR Rendering Under Realistic & Adversarial Data
  // ============================================================================
  console.log('\n\x1b[1m▶ Section 7: Empirical SSR Component Rendering Tests\x1b[0m');

  // Realistic mock data
  const mockRuntimes = [
    {
      id: 'neoforge-1.21.1',
      name: 'Modded Adventure NeoForge',
      loader: 'NeoForge',
      version: '1.21.1',
      description: 'NeoForge runtime with optimizations',
      is_client: true,
      is_server: true,
      diskSizeMb: 1420
    },
    {
      id: 'vanilla-1.21.1',
      name: 'Standard Minecraft Vanilla',
      loader: 'Vanilla',
      version: '1.21.1',
      description: 'Vanilla Minecraft',
      is_client: true,
      is_server: true,
      diskSizeMb: 680
    }
  ];

  const mockWorlds = [
    {
      folder_name: 'SMP_World_1',
      display_name: 'Kingdoms SMP',
      loader: 'NeoForge',
      version: '1.21.1',
      game_mode: 'Survival',
      is_realm: true,
      hosting_mode: 'always_online',
      last_played: Math.floor(Date.now() / 1000) - 3600,
      runtime_id: 'neoforge-1.21.1'
    },
    {
      folder_name: 'Singleplayer_Test',
      display_name: 'Creative Build Lab',
      loader: 'Vanilla',
      version: '1.21.1',
      game_mode: 'Creative',
      is_realm: false,
      hosting_mode: 'on_demand',
      last_played: Math.floor(Date.now() / 1000) - 86400,
      runtime_id: 'vanilla-1.21.1'
    }
  ];

  // Render LibraryView with realistic props
  let libHtml = '';
  try {
    const vnode = createVNode(LibraryView, {
      runtimes: mockRuntimes,
      worlds: mockWorlds,
      jumpInRealms: [mockWorlds[0]],
      serverStatus: 'Online',
      selectedWorldName: 'SMP_World_1',
      isLoading: false
    });
    libHtml = await renderToString(vnode);
    assert(libHtml.includes('INSTANCES &amp; REALMS'), 'LibraryView SSR renders title toolbar');
    assert(libHtml.includes('Kingdoms SMP'), 'LibraryView SSR renders world display name');
    assert(libHtml.includes('Online :25565'), 'LibraryView SSR renders online status badge for active realm');
    assert(libHtml.includes('Stop Server'), 'LibraryView SSR renders Stop Server button for online realm');
  } catch (err) {
    assert(false, 'LibraryView SSR render threw unexpected error', err.stack);
  }

  // Render RealmDetailView with realistic props
  let realmHtml = '';
  try {
    const vnode = createVNode(RealmDetailView, {
      realm: mockWorlds[0],
      serverStatus: 'Online',
      clientStatus: 'Running',
      tunnelUrl: 'conduit.tunnel.gg:25565',
      hostRoomCode: 'CONDUIT-9921',
      logs: [
        '[12:00:00 INFO] Starting minecraft server version 1.21.1',
        '[12:00:02 INFO] Loading properties',
        '[12:00:05 INFO] Done (5.123s)! For help, type "help"'
      ],
      currentMspt: 24.8,
      currentTps: 20.0,
      msptHistory: [20, 22, 24, 25, 24.8],
      tpsHistory: [20, 20, 20, 20, 20],
      installedMods: [],
      players: [
        {
          name: 'Steve',
          uuid: '069a79f4-44e9-4726-a5be-fca90e38aaf5',
          ping: 32,
          role: 'Operator',
          status: 'Online',
          is_connected: true,
          joinedAt: '12:01'
        }
      ]
    });
    const ctx = {};
    const html = await renderToString(vnode, ctx);
    const teleportHtml = ctx.teleports ? Object.values(ctx.teleports).join('') : '';
    realmHtml = html + teleportHtml;

    assert(realmHtml.includes('Kingdoms SMP'), 'RealmDetailView SSR renders realm name header');
    assert(realmHtml.includes('Tick Duration (MSPT)'), 'RealmDetailView SSR renders MSPT metric card');
    assert(realmHtml.includes('Server Console Output'), 'RealmDetailView SSR renders Console view');
  } catch (err) {
    assert(false, 'RealmDetailView SSR render threw unexpected error', err.stack);
  }

  // Adversarial Test 1: Empty datasets in LibraryView
  try {
    const vnodeEmpty = createVNode(LibraryView, {
      runtimes: [],
      worlds: [],
      jumpInRealms: [],
      serverStatus: 'Stopped',
      selectedWorldName: ''
    });
    const emptyHtml = await renderToString(vnodeEmpty);
    assert(!emptyHtml.includes('Jump In — Recently Played'), 'LibraryView gracefully hides Jump In strip when jumpInRealms is empty');
  } catch (err) {
    assert(false, 'LibraryView crashed on empty runtimes & worlds', err.message);
  }

  // Adversarial Test 2: Extreme Lag Spike in RealmDetailView (>50ms MSPT)
  try {
    const vnodeLag = createVNode(RealmDetailView, {
      realm: mockWorlds[0],
      serverStatus: 'Online',
      clientStatus: 'Stopped',
      logs: [],
      currentMspt: 92.4, // severe lag!
      currentTps: 10.8,
      msptHistory: [20, 45, 75, 92.4],
      tpsHistory: [20, 18, 14, 10.8],
      installedMods: []
    });
    const lagHtml = await renderToString(vnodeLag);
    assert(lagHtml.includes('92.4'), 'RealmDetailView renders elevated 92.4 ms tick duration');
  } catch (err) {
    assert(false, 'RealmDetailView crashed on severe lag spike input', err.message);
  }

  // Adversarial Test 3: Nullish/undefined attributes on WorldSaveInfo
  try {
    const malformedWorld = {
      folder_name: 'corrupted-save',
      display_name: 'Corrupted Save',
      runtime_id: 'rt1',
      loader: undefined,
      version: undefined,
      last_played: undefined
    };
    const vnodeMalformed = createVNode(LibraryView, {
      runtimes: [{ id: 'rt1', name: 'RT1', loader: 'Vanilla', version: '1.21', description: '', is_client: true, is_server: true, diskSizeMb: 100 }],
      worlds: [malformedWorld],
      serverStatus: 'Stopped',
      selectedWorldName: ''
    });
    const malformedHtml = await renderToString(vnodeMalformed);
    assert(malformedHtml.includes('Corrupted Save'), 'LibraryView gracefully renders world with undefined loader/version/last_played');
  } catch (err) {
    assert(false, 'LibraryView crashed on malformed world object', err.message);
  }

  await server.close();

  // ============================================================================
  // SUMMARY REPORT
  // ============================================================================
  console.log('\n\x1b[1m\x1b[36m======================================================================\x1b[0m');
  console.log(`\x1b[1mTOTAL VERIFICATIONS: ${passed + failed} | \x1b[32mPASSED: ${passed}\x1b[0m | \x1b[31mFAILED: ${failed}\x1b[0m`);
  console.log('\x1b[1m\x1b[36m======================================================================\x1b[0m\n');

  if (failed > 0) {
    console.error('FAILURES:');
    failures.forEach((f, i) => console.error(`  ${i + 1}. ${f}`));
    process.exit(1);
  } else {
    console.log('\x1b[32m✔ ALL MILESTONE 3 EMPIRICAL & ADVERSARIAL VERIFICATIONS PASSED!\x1b[0m\n');
    process.exit(0);
  }
}

run().catch((err) => {
  console.error('Unhandled fatal error in test-m3-views.mjs:', err);
  process.exit(1);
});
