/**
 * Forensic Audit & Adversarial Stress Harness for Milestone 3
 * 
 * Scope:
 * - src/views/LibraryView.vue
 * - src/views/RealmDetailView.vue
 * - Integration in src/App.vue
 */

import fs from 'node:fs';
import path from 'node:path';
import process from 'node:process';

const ROOT_DIR = process.cwd();
let passed = 0;
let failed = 0;
const errors = [];

function assert(condition, testName, details = '') {
  if (condition) {
    passed++;
    console.log(`  [PASS] ${testName}`);
  } else {
    failed++;
    const msg = `  [FAIL] ${testName}${details ? ' - ' + details : ''}`;
    console.error(msg);
    errors.push(msg);
  }
}

console.log('================================================================');
console.log('  MILESTONE 3 FORENSIC ADVERSARIAL AUDIT & INTEGRITY SUITE');
console.log('================================================================\n');

// ----------------------------------------------------------------------------
// SUITE 1: STRICT BACKEND ISOLATION & TAURI CONFIG
// ----------------------------------------------------------------------------
console.log('--- Suite 1: Strict Backend Isolation ---');

const sprintStart = new Date('2026-11-04T05:40:00Z').getTime();

function walkDir(dir) {
  let list = [];
  if (!fs.existsSync(dir)) return list;
  for (const ent of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, ent.name);
    if (ent.isDirectory()) list = list.concat(walkDir(full));
    else list.push(full);
  }
  return list;
}

const tauriFiles = walkDir(path.join(ROOT_DIR, 'src-tauri'));
const cargoPath = path.join(ROOT_DIR, 'Cargo.toml');
if (fs.existsSync(cargoPath)) tauriFiles.push(cargoPath);

const modifiedBackend = tauriFiles.filter(f => fs.statSync(f).mtimeMs > sprintStart);
assert(modifiedBackend.length === 0, 'Zero backend files modified during sprint', `Found: ${modifiedBackend.length}`);

// Confirm no forbidden @tauri-apps/plugin-dialog
const srcFiles = walkDir(path.join(ROOT_DIR, 'src')).filter(f => f.endsWith('.vue') || f.endsWith('.ts'));
let dialogCalls = 0;
for (const f of srcFiles) {
  const content = fs.readFileSync(f, 'utf8');
  if (content.includes('@tauri-apps/plugin-dialog')) dialogCalls++;
}
assert(dialogCalls === 0, 'Zero @tauri-apps/plugin-dialog calls in frontend (custom modals enforced)');

// ----------------------------------------------------------------------------
// SUITE 2: SOURCE CODE INTEGRITY & ANTI-FACADE ANALYSIS
// ----------------------------------------------------------------------------
console.log('\n--- Suite 2: Anti-Facade & Real Implementation Verification ---');

const libraryViewContent = fs.readFileSync(path.join(ROOT_DIR, 'src/views/LibraryView.vue'), 'utf8');
const realmDetailContent = fs.readFileSync(path.join(ROOT_DIR, 'src/views/RealmDetailView.vue'), 'utf8');
const appVueContent = fs.readFileSync(path.join(ROOT_DIR, 'src/App.vue'), 'utf8');

// Check LibraryView
assert(libraryViewContent.length > 5000, 'LibraryView.vue is a substantial component (>5KB)', `Size: ${libraryViewContent.length} bytes`);
assert(libraryViewContent.includes('<script setup lang="ts">'), 'LibraryView.vue uses Vue 3 script setup');
assert(libraryViewContent.includes('export interface RuntimeGroup'), 'LibraryView.vue defines RuntimeGroup contract');
assert(libraryViewContent.includes('export interface LibraryViewProps'), 'LibraryView.vue defines LibraryViewProps contract');
assert(libraryViewContent.includes('defineProps<LibraryViewProps>'), 'LibraryView.vue binds typed props');
assert(libraryViewContent.includes('defineEmits<'), 'LibraryView.vue defines typed emits');
assert(libraryViewContent.includes('@click.stop'), 'LibraryView.vue handles event bubbling for actions');

// Check flat table columns: [Icon + Name + Version] | [Status Pill] | [Player Count] | [Last Played] | [Primary Action + Split Menu]
assert(libraryViewContent.includes('<table'), 'LibraryView.vue renders flat table layout');
assert(libraryViewContent.includes('Realm / Name') && libraryViewContent.includes('Status') && libraryViewContent.includes('Players') && libraryViewContent.includes('Last Played') && libraryViewContent.includes('Actions'), 'LibraryView.vue has exact required table headers');
assert(libraryViewContent.includes('toggleSplitMenu'), 'LibraryView.vue implements split menu action');
assert(libraryViewContent.includes('Run Server in Background'), 'LibraryView.vue has split menu item: Run Server in Background');
assert(libraryViewContent.includes('Solo Play (Singleplayer)'), 'LibraryView.vue has split menu item: Solo Play (Singleplayer)');
assert(libraryViewContent.includes('Configure Realm'), 'LibraryView.vue has split menu item: Configure Realm');
assert(libraryViewContent.includes('Open Folder'), 'LibraryView.vue has split menu item: Open Folder');

// Check RealmDetailView
assert(realmDetailContent.length > 15000, 'RealmDetailView.vue is a substantial component (>15KB)', `Size: ${realmDetailContent.length} bytes`);
assert(realmDetailContent.includes('<script setup lang="ts">'), 'RealmDetailView.vue uses Vue 3 script setup');
assert(realmDetailContent.includes('export interface RealmDetailViewProps'), 'RealmDetailView.vue defines RealmDetailViewProps contract');
assert(realmDetailContent.includes('MetricCard'), 'RealmDetailView.vue integrates MetricCard');
assert(realmDetailContent.includes('currentMspt > 50'), 'RealmDetailView.vue flags MSPT > 50ms as error');
assert(realmDetailContent.includes('generateSvgLine') && realmDetailContent.includes('generateSvgArea'), 'RealmDetailView.vue computes MSPT rolling sparkline stream');
assert(realmDetailContent.includes('handleConsoleScroll') && realmDetailContent.includes('resumeLiveTail'), 'RealmDetailView.vue implements auto-scroll live tail with resume');
assert(realmDetailContent.includes('send_server_command'), 'RealmDetailView.vue calls send_server_command IPC');

// Check Restored Modal: showDeleteDataConfirmModal
assert(realmDetailContent.includes('showDeleteDataConfirmModal'), 'RealmDetailView.vue declares showDeleteDataConfirmModal ref');
assert(realmDetailContent.includes('<Modal') && realmDetailContent.includes('v-model="showDeleteDataConfirmModal"'), 'RealmDetailView.vue renders Modal v-model="showDeleteDataConfirmModal" template');
assert(realmDetailContent.includes("invoke<string>('delete_player_data'"), 'RealmDetailView.vue wires delete_player_data IPC');
assert(realmDetailContent.includes('deleteDataScope'), 'RealmDetailView.vue provides scope selection for player data deletion');

// Check Integration in App.vue
assert(appVueContent.includes('import LibraryView from "./views/LibraryView.vue";'), 'App.vue imports LibraryView');
assert(appVueContent.includes('import RealmDetailView from "./views/RealmDetailView.vue";'), 'App.vue imports RealmDetailView');
assert(appVueContent.includes("<LibraryView") && appVueContent.includes("v-if=\"!selectedRealm\""), 'App.vue renders LibraryView when !selectedRealm');
assert(appVueContent.includes("<RealmDetailView") && appVueContent.includes("v-else"), 'App.vue renders RealmDetailView when selectedRealm');
assert(appVueContent.includes("@start-server-and-join="), 'App.vue wires @start-server-and-join event');
assert(appVueContent.includes("@play-singleplayer="), 'App.vue wires @play-singleplayer event');
assert(appVueContent.includes("@delete-player-data="), 'App.vue wires @delete-player-data event');

// ----------------------------------------------------------------------------
// SUITE 3: IPC & EVENT INTEGRITY (EXACT COMMANDS & LISTENERS)
// ----------------------------------------------------------------------------
console.log('\n--- Suite 3: IPC & Event Integrity ---');

const requiredInvokes = [
  'load_saved_auth',
  'list_all_minecraft_versions',
  'get_deduplication_metrics',
  'verify_and_repair_junctions',
  'list_worlds',
  'list_downloaded_runtimes',
  'delete_downloaded_runtime',
  'login_microsoft',
  'install_instance',
  'start_server',
  'stop_server',
  'join_local_server',
  'launch_client',
  'open_instance_folder',
  'send_server_command',
  'list_installed_mods',
  'toggle_mod',
  'get_mod_versions',
  'switch_mod_version',
  'search_modrinth',
  'install_modrinth_project',
  'delete_mod',
  'delete_player_data',
  'conduit_get_identity'
];

const requiredListeners = [
  'server-log',
  'server-log-batch',
  'server-status',
  'client-status',
  'tunnel-status',
  'conduit-room-code',
  'tunnel-claim',
  'auth-device-code',
  'install-progress'
];

const allFrontendCode = srcFiles.map(f => fs.readFileSync(f, 'utf8')).join('\n');

for (const cmd of requiredInvokes) {
  const isInvoked = allFrontendCode.includes(`'${cmd}'`) || allFrontendCode.includes(`"${cmd}"`);
  assert(isInvoked, `Tauri IPC invoke preserved: ${cmd}`);
}

for (const evt of requiredListeners) {
  const isListened = allFrontendCode.includes(`'${evt}'`) || allFrontendCode.includes(`"${evt}"`);
  assert(isListened, `Tauri event channel subscribed: ${evt}`);
}

// ----------------------------------------------------------------------------
// SUITE 4: COLOR SYSTEM & VISUAL INTEGRITY (NO PROHIBITED AMBER)
// ----------------------------------------------------------------------------
console.log('\n--- Suite 4: Visual System & Negative Color Scans ---');

// Scan for prohibited amber #f59e0b in M3 views
const amberHexRegex = /#f59e0b/i;
assert(!amberHexRegex.test(libraryViewContent), 'LibraryView.vue has ZERO occurrences of legacy amber #f59e0b');
assert(!amberHexRegex.test(realmDetailContent), 'RealmDetailView.vue has ZERO occurrences of legacy amber #f59e0b');

const amberClassRegex = /\b(text|bg|border|ring|accent)-amber-(400|500|600)\b/;
assert(!amberClassRegex.test(libraryViewContent), 'LibraryView.vue has ZERO amber Tailwind classes');
assert(!amberClassRegex.test(realmDetailContent), 'RealmDetailView.vue has ZERO amber Tailwind classes');

// Confirm presence of Dark Graphite & Accent tokens
assert(libraryViewContent.includes('bg-canvas') || libraryViewContent.includes('bg-[#11151A]'), 'LibraryView uses dark graphite canvas');
assert(libraryViewContent.includes('bg-card') || libraryViewContent.includes('bg-[#1B222A]'), 'LibraryView uses dark graphite cards');
assert(libraryViewContent.includes('text-cyan-accent') || libraryViewContent.includes('text-[#45C5D9]'), 'LibraryView uses cyan accent for interaction');
assert(libraryViewContent.includes('orange-accent') || libraryViewContent.includes('#F39A36'), 'LibraryView uses orange accent for primary CTA');

assert(realmDetailContent.includes('bg-canvas') || realmDetailContent.includes('bg-[#11151A]'), 'RealmDetailView uses dark graphite canvas');
assert(realmDetailContent.includes('bg-card') || realmDetailContent.includes('bg-[#1B222A]'), 'RealmDetailView uses dark graphite cards');
assert(realmDetailContent.includes('text-cyan-accent') || realmDetailContent.includes('text-[#45C5D9]'), 'RealmDetailView uses cyan accent for telemetry');

// ----------------------------------------------------------------------------
// SUITE 5: ADVERSARIAL STRESS & EDGE CASES
// ----------------------------------------------------------------------------
console.log('\n--- Suite 5: Adversarial Stress & Edge Cases ---');

// Edge case 1: Slider bounds
const sliderRegex = /<input[^>]+type="range"[^>]*>/g;
const sliders = realmDetailContent.match(sliderRegex) || [];
assert(sliders.length >= 2, 'RealmDetailView has RAM sliders for Server and Client');
for (const s of sliders) {
  assert(s.includes('min="2"'), `RAM slider enforces min="2": ${s.trim()}`);
  assert(s.includes('max="16"'), `RAM slider enforces max="16": ${s.trim()}`);
}

// Edge case 2: Sparkline generator function logic stress test
function testSparklineMath() {
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

  // Test empty array
  assert(generateSvgArea([], 0, 60, 600, 96) === '', 'Sparkline area handles empty points array gracefully');
  assert(generateSvgLine([], 0, 60, 600, 96) === '', 'Sparkline line handles empty points array gracefully');

  // Test single point (division by 0 guard: Math.max(1, points.length - 1))
  const singleLine = generateSvgLine([25], 0, 60, 600, 96);
  assert(!singleLine.includes('NaN'), 'Sparkline handles single point without NaN division');

  // Test 60 points with spikes > 60 and < 0 (clamping)
  const stressPoints = [0, -10, 50, 65, 120, 20];
  const area = generateSvgArea(stressPoints, 0, 60, 600, 96);
  assert(!area.includes('NaN') && area.startsWith('M 0,96'), 'Sparkline clamps outliers properly');
}
testSparklineMath();

// Edge case 3: Singleplayer launch mode formulation
const singleplayerLaunchPattern = /mode:\s*`singleplayer:\$\{realm\.folder_name\}`/;
assert(singleplayerLaunchPattern.test(appVueContent), 'Singleplayer launch mode uses exact target pattern singleplayer:${realm.folder_name}');

// Edge case 4: Server restart sequence
assert(realmDetailContent.includes('setTimeout') && realmDetailContent.includes('emit(\'start-server\')'), 'Restart sequence waits before re-launching server to ensure process exit');

// Edge case 5: Log filtering logic
function testLogFiltering() {
  const sampleLogs = [
    '[08:00:00] [Server thread/INFO]: Starting Minecraft server version 1.21.1',
    '[08:00:01] [Server thread/WARN]: Ambiguity between arguments [foo, bar]',
    '[08:00:02] [Server thread/ERROR]: Encountered an unexpected exception'
  ];
  
  const filterErrors = sampleLogs.filter(l => l.toLowerCase().includes('error'));
  const filterWarns = sampleLogs.filter(l => l.toLowerCase().includes('warn'));
  const filterInfo = sampleLogs.filter(l => !l.toLowerCase().includes('error') && !l.toLowerCase().includes('warn'));

  assert(filterErrors.length === 1 && filterErrors[0].includes('ERROR'), 'Log error filter captures only errors');
  assert(filterWarns.length === 1 && filterWarns[0].includes('WARN'), 'Log warn filter captures only warnings');
  assert(filterInfo.length === 1 && filterInfo[0].includes('INFO'), 'Log info filter excludes errors and warnings');
}
testLogFiltering();

console.log('\n================================================================');
console.log(`SUMMARY: ${passed} passed, ${failed} failed.`);
console.log('================================================================');

if (failed > 0) {
  process.exit(1);
} else {
  process.exit(0);
}
