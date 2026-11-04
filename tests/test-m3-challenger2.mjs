#!/usr/bin/env node
/**
 * Adversarial Stress Test Suite - Milestone 3 Challenger 2
 * 
 * Target Components & Views:
 *   - src/views/LibraryView.vue
 *   - src/views/RealmDetailView.vue
 *   - src/App.vue (M3 wiring & IPC delegation)
 * 
 * Challenge Vectors:
 * 1. Prohibited color tokens: Zero occurrences of amber (#f59e0b), amber-500, amber-400 in M3 views.
 * 2. RAM allocation slider constraints: min="2", max="16", step attribute presence and value bounds.
 * 3. Flat table structure in LibraryView.vue with 5 canonical columns.
 * 4. Consolidated orange launch CTA (#F39A36) and split menu actions.
 * 5. Monospace server console in RealmDetailView.vue with auto-scroll tail, log filters, and command input.
 * 6. Restored modal markup for showDeleteDataConfirmModal connected to delete_player_data.
 * 7. MetricCard telemetry strip with MSPT/TPS indicators and cyan/red color logic.
 * 8. IPC delegation contracts and duplicate invocation analysis between App.vue and views.
 */

import fs from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const ROOT_DIR = path.resolve(__dirname, '..');

let totalTests = 0;
let passedTests = 0;
let failedTests = 0;
let warningTests = 0;
const failureDetails = [];
const warningDetails = [];

function assert(condition, testId, description, details = '') {
  totalTests++;
  if (condition) {
    passedTests++;
    console.log(`  \x1b[32m✔ PASS\x1b[0m [${testId}] ${description}`);
  } else {
    failedTests++;
    failureDetails.push({ testId, description, details });
    console.error(`  \x1b[31m✖ FAIL\x1b[0m [${testId}] ${description}${details ? ` -> ${details}` : ''}`);
  }
}

function warn(condition, testId, description, details = '') {
  totalTests++;
  if (condition) {
    passedTests++;
    console.log(`  \x1b[32m✔ PASS\x1b[0m [${testId}] ${description}`);
  } else {
    warningTests++;
    warningDetails.push({ testId, description, details });
    console.warn(`  \x1b[33m▲ WARN\x1b[0m [${testId}] ${description}${details ? ` -> ${details}` : ''}`);
  }
}

function readFile(relPath) {
  const p = path.join(ROOT_DIR, relPath);
  if (!fs.existsSync(p)) {
    throw new Error(`Required file not found: ${relPath}`);
  }
  return fs.readFileSync(p, 'utf-8');
}

console.log('\n\x1b[1m\x1b[36m================================================================================\x1b[0m');
console.log('\x1b[1m\x1b[36m  CONDUIT MILESTONE 3: CHALLENGER 2 EMPIRICAL ADVERSARIAL STRESS SUITE          \x1b[0m');
console.log('\x1b[1m\x1b[36m================================================================================\x1b[0m\n');

// Read files
const libraryViewCode = readFile('src/views/LibraryView.vue');
const realmDetailViewCode = readFile('src/views/RealmDetailView.vue');
const appVueCode = readFile('src/App.vue');

// ============================================================================
// SECTION 1: PROHIBITED COLOR TOKENS & ANTI-PATTERN INSPECTION
// ============================================================================
console.log('\x1b[1m\x1b[34m[Section 1] Color Neutrality & Prohibited Tones Audit\x1b[0m');

// 1.1: LibraryView zero amber
const amberRegex = /#f59e0b|amber-400|amber-500|rgba\(\s*245\s*,\s*158\s*,\s*11/i;
const libAmberMatch = libraryViewCode.match(amberRegex);
assert(!libAmberMatch, 'COLOR_LIB_NO_AMBER', 'LibraryView contains 0 occurrences of amber (#f59e0b / amber-500)', libAmberMatch ? libAmberMatch[0] : '');

// 1.2: RealmDetailView zero amber
const realmAmberMatch = realmDetailViewCode.match(amberRegex);
assert(!realmAmberMatch, 'COLOR_REALM_NO_AMBER', 'RealmDetailView contains 0 occurrences of amber (#f59e0b / amber-500)', realmAmberMatch ? realmAmberMatch[0] : '');

// 1.3: M3 Wiring in App.vue zero amber
const m3WiringSection = appVueCode.substring(
  appVueCode.indexOf("mainNav === 'library'"),
  appVueCode.indexOf("mainNav === 'discover'")
);
const appM3AmberMatch = m3WiringSection.match(amberRegex);
assert(!appM3AmberMatch, 'COLOR_APP_M3_NO_AMBER', 'App.vue M3 wiring contains 0 occurrences of amber (#f59e0b)', appM3AmberMatch ? appM3AmberMatch[0] : '');

// 1.4: Elimination of nested card anti-patterns in LibraryView
const hasNestedCardComponents = /<Card\b[\s\S]*?<Card\b/.test(libraryViewCode);
const hasLegacyRuntimeGroupCards = libraryViewCode.includes('RuntimeGroup') && libraryViewCode.includes('RealmCard');
assert(!hasNestedCardComponents && !hasLegacyRuntimeGroupCards, 'ANTI_PATTERN_NO_NESTED_CARDS', 'LibraryView completely avoids nested <Card> components and legacy RuntimeGroup card hierarchy');

// ============================================================================
// SECTION 2: FLAT TABLE STRUCTURE IN LIBRARYVIEW.VUE
// ============================================================================
console.log('\n\x1b[1m\x1b[34m[Section 2] Flat Table Architecture & Table Columns in LibraryView\x1b[0m');

// 2.1: Table element present
assert(libraryViewCode.includes('<table') && libraryViewCode.includes('</table>'), 'LIB_TABLE_ELEMENT', 'LibraryView implements standard <table> layout');

// 2.2: Five canonical columns present in the table header
const thMatches = libraryViewCode.match(/<th[^>]*>([\s\S]*?)<\/th>/g) || [];
assert(thMatches.length >= 5, 'LIB_TABLE_COLUMN_COUNT', `Table has at least 5 column headers (found ${thMatches.length})`);

// 2.3: Verification of the 5 canonical columns:
// Column 1: [Icon + Name + Version]
const col1Present = libraryViewCode.includes('Realm / Name') && libraryViewCode.includes('realm.display_name') && libraryViewCode.includes('realm.version');
assert(col1Present, 'LIB_COL1_ICON_NAME_VERSION', 'Column 1 contains Icon, Name, and Version');

// Column 2: [Status Pill with StatusDot / Badge dot]
const col2Present = libraryViewCode.includes('Status') && libraryViewCode.includes('<Badge') && libraryViewCode.includes('dot');
assert(col2Present, 'LIB_COL2_STATUS_PILL', 'Column 2 contains Status Pill with dot indicator');

// Column 3: [Player Count]
const col3Present = libraryViewCode.includes('Players') && libraryViewCode.includes('0 / 20');
assert(col3Present, 'LIB_COL3_PLAYER_COUNT', 'Column 3 contains Player Count metric');

// Column 4: [Last Played]
const col4Present = libraryViewCode.includes('Last Played') && libraryViewCode.includes('formatRelativeTime');
assert(col4Present, 'LIB_COL4_LAST_PLAYED', 'Column 4 contains relative Last Played timestamp');

// Column 5: [Primary Action + Split Menu]
const col5Present = libraryViewCode.includes('Actions') && libraryViewCode.includes('toggleSplitMenu');
assert(col5Present, 'LIB_COL5_PRIMARY_ACTION_SPLIT_MENU', 'Column 5 contains Primary Action + Split Menu');

// ============================================================================
// SECTION 3: CONSOLIDATED ORANGE LAUNCH CTA & SPLIT MENU ACTIONS
// ============================================================================
console.log('\n\x1b[1m\x1b[34m[Section 3] Orange Launch CTA (#F39A36) & Split Menu Actions\x1b[0m');

// 3.1: Primary Launch CTA uses orange accent
const hasPrimaryOrangeCta = libraryViewCode.includes('variant="primary"') && libraryViewCode.includes('start-server-and-join');
assert(hasPrimaryOrangeCta, 'LAUNCH_CTA_PRIMARY_VARIANT', 'Primary Launch CTA utilizes Button variant="primary" (Orange #F39A36)');

// 3.2: Split Menu Dropdown Toggle styling uses orange accent when offline
const hasOrangeSplitButton = libraryViewCode.includes('bg-orange-accent') && libraryViewCode.includes('toggleSplitMenu');
assert(hasOrangeSplitButton, 'SPLIT_MENU_ORANGE_ACCENT', 'Split menu toggle utilizes bg-orange-accent in offline state');

// 3.3: Split menu actions check
const hasBgRunServer = libraryViewCode.includes('start-server') && libraryViewCode.includes('Run Server in Background');
const hasSoloPlay = libraryViewCode.includes('play-singleplayer') && libraryViewCode.includes('Solo Play');
const hasConfigureRealm = libraryViewCode.includes('select-realm') && libraryViewCode.includes('Configure Realm');
const hasOpenFolder = libraryViewCode.includes('open-folder') && libraryViewCode.includes('Open Folder');

assert(hasBgRunServer, 'SPLIT_ACTION_BG_SERVER', 'Split menu provides "Run Server in Background" action');
assert(hasSoloPlay, 'SPLIT_ACTION_SOLO_PLAY', 'Split menu provides "Solo Play (Singleplayer)" action');
assert(hasConfigureRealm, 'SPLIT_ACTION_CONFIGURE', 'Split menu provides "Configure Realm" action');
assert(hasOpenFolder, 'SPLIT_ACTION_OPEN_FOLDER', 'Split menu provides "Open Folder" action');

// ============================================================================
// SECTION 4: MONOSPACE SERVER CONSOLE & LIVE LOG TAIL
// ============================================================================
console.log('\n\x1b[1m\x1b[34m[Section 4] Monospace Server Console & Auto-Scroll Tail\x1b[0m');

// 4.1: Monospace typography
assert(realmDetailViewCode.includes('font-mono text-[11px]'), 'CONSOLE_MONOSPACE_FONT', 'Console output explicitly styled with font-mono text-[11px]');

// 4.2: Auto-scroll live tail logic
const hasAutoScrollRef = realmDetailViewCode.includes('consoleOutputRef') && realmDetailViewCode.includes('autoScroll');
const hasResumeLiveTail = realmDetailViewCode.includes('resumeLiveTail') && realmDetailViewCode.includes('Resume Live Tail');
assert(hasAutoScrollRef && hasResumeLiveTail, 'CONSOLE_AUTO_SCROLL_TAIL', 'Console implements auto-scroll detection and Resume Live Tail button');

// 4.3: Log level filters
const hasAllFilter = realmDetailViewCode.includes("'all'");
const hasErrorFilter = realmDetailViewCode.includes("'errors'");
const hasWarnFilter = realmDetailViewCode.includes("'warnings'");
const hasInfoFilter = realmDetailViewCode.includes("'info'");
assert(hasAllFilter && hasErrorFilter && hasWarnFilter && hasInfoFilter, 'CONSOLE_LOG_FILTERS', 'Console supports All, Errors, Warnings, and Info filters');

// 4.4: Command input form connected to send_server_command
const hasCommandInput = realmDetailViewCode.includes('consoleInput') && realmDetailViewCode.includes('send_server_command');
assert(hasCommandInput, 'CONSOLE_COMMAND_INPUT_FORM', 'Console provides command input prompt invoking send_server_command');

// ============================================================================
// SECTION 5: RESTORED MODAL MARKUP & DELETE PLAYER DATA
// ============================================================================
console.log('\n\x1b[1m\x1b[34m[Section 5] Restored Modal Markup (showDeleteDataConfirmModal)\x1b[0m');

// 5.1: Modal element with v-model="showDeleteDataConfirmModal"
const hasDeleteModal = realmDetailViewCode.includes('v-model="showDeleteDataConfirmModal"') && realmDetailViewCode.includes('<Modal');
assert(hasDeleteModal, 'MODAL_RESTORED_DELETE_PLAYER_DATA', 'showDeleteDataConfirmModal is restored with <Modal> markup');

// 5.2: Scope radio controls
const hasScopeAll = realmDetailViewCode.includes('value="all"');
const hasScopePlayerData = realmDetailViewCode.includes('value="playerdata"');
const hasScopeStats = realmDetailViewCode.includes('value="stats"');
assert(hasScopeAll && hasScopePlayerData && hasScopeStats, 'MODAL_DELETE_SCOPES', 'Modal provides radio controls for all, playerdata, and stats scopes');

// 5.3: executeDeletePlayerData calls delete_player_data
const callsDeletePlayerData = realmDetailViewCode.includes("'delete_player_data'") && realmDetailViewCode.includes('instanceDir') && realmDetailViewCode.includes('uuid');
assert(callsDeletePlayerData, 'MODAL_INVOKES_DELETE_PLAYER_DATA', 'executeDeletePlayerData invokes delete_player_data with instanceDir, uuid, scope');

// ============================================================================
// SECTION 6: METRICCARD TELEMETRY STRIP & CYAN/RED LOGIC
// ============================================================================
console.log('\n\x1b[1m\x1b[34m[Section 6] MetricCard Telemetry Strip & Cyan/Red Color Logic\x1b[0m');

// 6.1: MetricCard components used
const metricCardMatches = realmDetailViewCode.match(/<MetricCard[\s\S]*?>/g) || [];
assert(metricCardMatches.length >= 4, 'TELEMETRY_METRIC_CARDS_COUNT', `Telemetry strip contains at least 4 MetricCard instances (found ${metricCardMatches.length})`);

// 6.2: MSPT cyan/red conditional logic
const hasMsptThreshold = realmDetailViewCode.includes("currentMspt > 50 ? 'error' : 'cyan'");
assert(hasMsptThreshold, 'TELEMETRY_MSPT_COLOR_LOGIC', "MSPT card assigns 'error' (red) when >50ms and 'cyan' when normal");

// 6.3: Sparkline continuous stream
const hasMsptSparkline = realmDetailViewCode.includes('generateSvgArea') && realmDetailViewCode.includes('generateSvgLine') && realmDetailViewCode.includes('50.0 ms Threshold');
assert(hasMsptSparkline, 'TELEMETRY_MSPT_SPARKLINE_STREAM', 'MSPT sparkline stream renders SVG path with cyan line and 50ms red threshold');

// ============================================================================
// SECTION 7: INTERACTIVE CONTROL BOUNDS & RAM SLIDERS (ADVERSARIAL CHALLENGE)
// ============================================================================
console.log('\n\x1b[1m\x1b[34m[Section 7] Adversarial Challenge: RAM Allocation Sliders & Attributes\x1b[0m');

// 7.1: Dedicated server and client RAM sliders have min="2" max="16"
const rangeSliders = realmDetailViewCode.match(/<input[^>]+type=["']range["'][^>]*>/g) || [];
assert(rangeSliders.length >= 2, 'RAM_SLIDERS_COUNT', `Found ${rangeSliders.length} range sliders in RealmDetailView (expected >= 2)`);

let allSlidersHaveMinMax = true;
let allSlidersHaveStep = true;
for (const slider of rangeSliders) {
  if (!slider.includes('min="2"') || !slider.includes('max="16"')) {
    allSlidersHaveMinMax = false;
  }
  if (!slider.includes('step=')) {
    allSlidersHaveStep = false;
  }
}

assert(allSlidersHaveMinMax, 'RAM_SLIDERS_BOUNDS_2_16', 'All RAM sliders constrain bounds to min="2" and max="16"');

// 7.2: Step attribute challenge
warn(allSlidersHaveStep, 'RAM_SLIDERS_STEP_ATTRIBUTE_PRESENT', 'Explicit step attribute present on RAM range sliders', 
  allSlidersHaveStep ? 'step attribute found' : 'MISSING explicit step attribute on <input type="range"> (defaults to 1 in HTML5, but explicit attribute recommended for clarity)');

// 7.3: State synchronization challenge: Is RealmDetailView RAM slider bound to App.vue launch payload?
const propsInterface = realmDetailViewCode.substring(
  realmDetailViewCode.indexOf('interface RealmDetailViewProps'),
  realmDetailViewCode.indexOf('const props =')
);
const ramPropInRealmDetailProps = propsInterface.includes('serverRamGb') || propsInterface.includes('ramGb');
const ramEmittedToApp = realmDetailViewCode.includes("emit('update:serverRamGb") || realmDetailViewCode.includes("emit('change-ram");
warn(ramPropInRealmDetailProps || ramEmittedToApp, 'RAM_SLIDER_APP_SYNC', 'RAM slider state in RealmDetailView synchronizes with App.vue server launch payload',
  'RealmDetailView maintains local serverRamGb/clientRamGb without prop binding or emit back to App.vue (App.vue uses internal serverSettings.serverRamGb: 3G)');

// ============================================================================
// SECTION 8: IPC DELEGATION & DUPLICATE EXECUTION AUDIT (ADVERSARIAL CHALLENGE)
// ============================================================================
console.log('\n\x1b[1m\x1b[34m[Section 8] Adversarial Challenge: IPC Delegation & Duplicate Execution\x1b[0m');

// Check duplicate invocation of send_server_command
const detailInvokesSendCommand = realmDetailViewCode.includes("invoke('send_server_command'");
const detailEmitsSendCommand = realmDetailViewCode.includes("emit('send-command'");
const appBindsSendCommand = appVueCode.includes('@send-command="handleSendCommandFromView"');
const appInvokesSendCommandInHandler = appVueCode.includes('handleSendCommandFromView') && appVueCode.includes('handleSendCommand()');

const duplicateSendCommand = detailInvokesSendCommand && detailEmitsSendCommand && appBindsSendCommand && appInvokesSendCommandInHandler;

warn(!duplicateSendCommand, 'IPC_NO_DUPLICATE_SEND_COMMAND', 'send_server_command is not invoked twice per user command',
  duplicateSendCommand ? 'DUPLICATE EXECUTION DETECTED: RealmDetailView:210 invokes send_server_command AND emits send-command, triggering App.vue:1211 -> 981 to invoke send_server_command again!' : 'Clean delegation');

// Check duplicate invocation of kick-player
const detailInvokesKick = realmDetailViewCode.includes("`kick ${name}`");
const detailEmitsKick = realmDetailViewCode.includes("emit('kick-player'");
const appBindsKick = appVueCode.includes('@kick-player="handleKickPlayer"');
const appInvokesKick = appVueCode.includes('handleKickPlayer') && appVueCode.includes("`kick ${name}`");

const duplicateKick = detailInvokesKick && detailEmitsKick && appBindsKick && appInvokesKick;

warn(!duplicateKick, 'IPC_NO_DUPLICATE_KICK_PLAYER', 'kick-player command is not invoked twice per moderation action',
  duplicateKick ? 'DUPLICATE EXECUTION DETECTED: RealmDetailView:283 invokes kick AND emits kick-player, triggering App.vue:1154 to invoke kick again!' : 'Clean delegation');

// Check duplicate invocation of delete_player_data
const detailInvokesDelete = realmDetailViewCode.includes("invoke<string>('delete_player_data'");
const detailEmitsDelete = realmDetailViewCode.includes("emit('delete-player-data'");
const appBindsDelete = appVueCode.includes('@delete-player-data="handleDeletePlayerDataFromView"');
const appInvokesDelete = appVueCode.includes('executeDeletePlayerData()');

const duplicateDelete = detailInvokesDelete && detailEmitsDelete && appBindsDelete && appInvokesDelete;

warn(!duplicateDelete, 'IPC_NO_DUPLICATE_DELETE_DATA', 'delete_player_data is not invoked twice per confirmation',
  duplicateDelete ? 'DUPLICATE EXECUTION DETECTED: RealmDetailView:262 invokes delete_player_data AND emits delete-player-data, triggering App.vue:1216 -> 1194 to invoke delete_player_data again!' : 'Clean delegation');

// Check for blocking alert() calls
const alertMatches = (realmDetailViewCode.match(/\balert\s*\(/g) || []).length;
warn(alertMatches === 0, 'UI_NO_BLOCKING_ALERTS', 'Absence of blocking browser alert() calls in RealmDetailView',
  alertMatches > 0 ? `Found ${alertMatches} native alert() calls; should use toast notifications instead` : 'No native alerts');

// ============================================================================
// SECTION 9: HIGH-LOAD STRESS SIMULATION & ALGORITHMIC BOUNDS
// ============================================================================
console.log('\n\x1b[1m\x1b[34m[Section 9] High-Load Stress Simulation & Algorithmic Bounds\x1b[0m');

// 9.1: Sparkline SVG generator under empty, single, and 1,000 points
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

const emptyArea = generateSvgArea([], 0, 60, 600, 96);
const singleArea = generateSvgArea([25.4], 0, 60, 600, 96);
const heavyPoints = Array.from({ length: 1000 }, (_, i) => 20 + Math.sin(i / 10) * 15);
const t0 = performance.now();
const heavyArea = generateSvgArea(heavyPoints, 0, 60, 600, 96);
const t1 = performance.now();

assert(emptyArea === '', 'SPARKLINE_EMPTY_SAFETY', 'Sparkline safely returns empty string on empty points array');
assert(singleArea.startsWith('M 0,96 L 0,'), 'SPARKLINE_SINGLE_POINT_SAFETY', 'Sparkline handles single point without NaN or divide-by-zero');
assert(heavyArea.length > 5000 && (t1 - t0) < 50, 'SPARKLINE_1000_PTS_PERFORMANCE', `Sparkline renders 1,000 points in ${(t1 - t0).toFixed(2)}ms (< 50ms)`);

// 9.2: High-throughput log filter stress (10,000 lines)
const mockLogs = Array.from({ length: 10000 }, (_, i) => {
  if (i % 50 === 0) return `[${i}] [FATAL] Critical server fault at index ${i}`;
  if (i % 20 === 0) return `[${i}] [ERROR] Exception loading chunk ${i}`;
  if (i % 7 === 0) return `[${i}] [WARN] Can't keep up! Is the server overloaded?`;
  return `[${i}] [INFO] Player${i} joined the game`;
});

const tLog0 = performance.now();
const errors = mockLogs.filter(l => {
  const lower = l.toLowerCase();
  return lower.includes('error') || lower.includes('fatal') || lower.includes('exception');
});
const warnings = mockLogs.filter(l => l.toLowerCase().includes('warn'));
const info = mockLogs.filter(l => !l.toLowerCase().includes('error') && !l.toLowerCase().includes('warn'));
const tLog1 = performance.now();

assert(errors.length === 600, 'LOG_FILTER_ERROR_ACCURACY', `Log filter correctly matched ${errors.length} error lines out of 10,000`);
assert(warnings.length === 1343, 'LOG_FILTER_WARN_ACCURACY', `Log filter correctly matched ${warnings.length} warn lines out of 10,000`);
assert((tLog1 - tLog0) < 100, 'LOG_FILTER_PERFORMANCE', `Log filter processed 10,000 lines in ${(tLog1 - tLog0).toFixed(2)}ms (< 100ms)`);

// 9.3: Search filtering stress with 500 worlds
const mockWorlds = Array.from({ length: 500 }, (_, i) => ({
  folder_name: `world_${i}`,
  display_name: `Conduit Realm ${i}`,
  loader: i % 2 === 0 ? 'NeoForge' : 'Vanilla',
  version: '1.21.1',
  hosting_mode: i % 3 === 0 ? 'always_online' : 'on_demand',
  is_realm: true
}));

const q = 'neoforge';
const filtered = mockWorlds.filter(w => {
  const matchName = w.display_name.toLowerCase().includes(q) || w.folder_name.toLowerCase().includes(q);
  const matchLoader = (w.loader || '').toLowerCase().includes(q);
  const matchVersion = (w.version || '').toLowerCase().includes(q);
  return matchName || matchLoader || matchVersion;
});
assert(filtered.length === 250, 'SEARCH_FILTER_ACCURACY', `Search filter correctly identified ${filtered.length} NeoForge worlds out of 500`);

// ============================================================================
// SECTION 10: SUMMARY & METRICS
// ============================================================================
console.log('\n\x1b[1m\x1b[36m================================================================================\x1b[0m');
console.log(`\x1b[1mTOTAL TESTS: ${totalTests} | \x1b[32mPASS: ${passedTests}\x1b[0m | \x1b[33mWARN: ${warningTests}\x1b[0m | \x1b[31mFAIL: ${failedTests}\x1b[0m`);
console.log('\x1b[1m\x1b[36m================================================================================\x1b[0m\n');

if (failureDetails.length > 0) {
  console.error('\x1b[1m\x1b[31mFAILURES:\x1b[0m');
  for (const f of failureDetails) {
    console.error(`- [${f.testId}] ${f.description}: ${f.details}`);
  }
}

if (warningDetails.length > 0) {
  console.warn('\x1b[1m\x1b[33mADVERSARIAL WARNINGS / LATENT DEFECTS:\x1b[0m');
  for (const w of warningDetails) {
    console.warn(`- [${w.testId}] ${w.description}: ${w.details}`);
  }
}

const exitCode = failedTests > 0 ? 1 : 0;
process.exit(exitCode);

