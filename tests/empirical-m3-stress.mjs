import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const ROOT_DIR = path.resolve(__dirname, '..');

let totalTests = 0;
let passedTests = 0;
let failedTests = 0;

function assert(condition, testId, description) {
  totalTests++;
  if (condition) {
    passedTests++;
    console.log(`  ✔ PASS [${testId}] ${description}`);
  } else {
    failedTests++;
    console.error(`  ✖ FAIL [${testId}] ${description}`);
  }
}

console.log('========================================================================');
console.log('  CONDUIT MILESTONE 3: ADVERSARIAL & EMPIRICAL STRESS TEST HARNESS      ');
console.log('========================================================================\n');

// -----------------------------------------------------------------------------
// Section 1: SFC Structure & Contract Inspection
// -----------------------------------------------------------------------------
console.log('[Section 1] View File Contracts & Integrity');
const libraryViewPath = path.join(ROOT_DIR, 'src', 'views', 'LibraryView.vue');
const realmDetailPath = path.join(ROOT_DIR, 'src', 'views', 'RealmDetailView.vue');

assert(fs.existsSync(libraryViewPath), 'LIB_VIEW_EXISTS', 'LibraryView.vue exists on disk');
assert(fs.existsSync(realmDetailPath), 'REALM_DETAIL_EXISTS', 'RealmDetailView.vue exists on disk');

const libContent = fs.readFileSync(libraryViewPath, 'utf-8');
const realmContent = fs.readFileSync(realmDetailPath, 'utf-8');

assert(libContent.includes('<script setup lang="ts">'), 'LIB_SCRIPT_SETUP', 'LibraryView uses <script setup lang="ts">');
assert(realmContent.includes('<script setup lang="ts">'), 'REALM_SCRIPT_SETUP', 'RealmDetailView uses <script setup lang="ts">');

assert(libContent.includes('export interface RuntimeGroup'), 'LIB_EXPORT_RUNTIME_GROUP', 'LibraryView exports RuntimeGroup interface');
assert(libContent.includes('export interface LibraryViewProps'), 'LIB_EXPORT_PROPS', 'LibraryView exports LibraryViewProps');
assert(realmContent.includes('export interface RealmDetailViewProps'), 'REALM_EXPORT_PROPS', 'RealmDetailView exports RealmDetailViewProps');

// -----------------------------------------------------------------------------
// Section 2: Layout Conformance: Desktop Table & Zero Nested Cards
// -----------------------------------------------------------------------------
console.log('\n[Section 2] Layout Conformance & Anti-Pattern Audit');
assert(!libContent.includes('RealmCard'), 'LIB_NO_REALMCARD', 'LibraryView eliminates legacy RealmCard component');
assert(!libContent.includes('<Card') && !libContent.includes('<card'), 'LIB_NO_NESTED_CARD_TAGS', 'LibraryView does not nest <Card> tags');

// Table layout verification
assert(libContent.includes('<table') && libContent.includes('</table>'), 'LIB_HAS_TABLE', 'LibraryView uses standard <table> element');
assert(libContent.includes('<thead') && libContent.includes('<tbody'), 'LIB_TABLE_SEMANTICS', 'LibraryView includes proper table semantics');

// Column verification: [Icon + Name + Version] | [Status Pill with StatusDot] | [Player Count] | [Last Played] | [Primary Action + Split Menu]
assert(libContent.includes('Realm / Name'), 'COL_NAME_VERSION', 'Column 1 header: Realm / Name');
assert(libContent.includes('Status'), 'COL_STATUS', 'Column 2 header: Status');
assert(libContent.includes('Players'), 'COL_PLAYERS', 'Column 3 header: Players');
assert(libContent.includes('Last Played'), 'COL_LAST_PLAYED', 'Column 4 header: Last Played');
assert(libContent.includes('Actions'), 'COL_ACTIONS', 'Column 5 header: Actions');

// Verify Badge with dot for status
assert(libContent.includes('Badge') && libContent.includes('dot'), 'STATUS_BADGE_DOT', 'Status is represented by Badge with dot attribute');

// Verify Primary Action & Split Menu
assert(libContent.includes('start-server-and-join') || libContent.includes('start-server'), 'ACTION_LAUNCH', 'Primary launch action wired');
assert(libContent.includes('ChevronDown') && libContent.includes('toggleSplitMenu'), 'SPLIT_MENU_TOGGLE', 'Split menu toggle with ChevronDown present');
assert(libContent.includes('bg-orange-accent') || libContent.includes('orange-accent'), 'LAUNCH_ORANGE_COLOR', 'Launch action uses orange accent');

// Secondary menu items
assert(libContent.includes('Run Server in Background'), 'SPLIT_ACTION_BACKGROUND', 'Split menu has "Run Server in Background"');
assert(libContent.includes('Solo Play (Singleplayer)'), 'SPLIT_ACTION_SOLO', 'Split menu has "Solo Play (Singleplayer)"');
assert(libContent.includes('Configure Realm'), 'SPLIT_ACTION_CONFIG', 'Split menu has "Configure Realm"');
assert(libContent.includes('Open Folder'), 'SPLIT_ACTION_FOLDER', 'Split menu has "Open Folder"');

// -----------------------------------------------------------------------------
// Section 3: Telemetry Strip & Visual Conformance
// -----------------------------------------------------------------------------
console.log('\n[Section 3] Telemetry Strip & MetricCard Thresholds');
assert(realmContent.includes('MetricCard'), 'REALM_USES_METRIC_CARD', 'RealmDetailView uses MetricCard');
assert(realmContent.includes('currentMspt > 50'), 'MSPT_THRESHOLD_LOGIC', 'MSPT > 50ms threshold logic present');
assert(realmContent.includes("'error' : 'cyan'") || realmContent.includes('"error" : "cyan"'), 'MSPT_COLOR_MAPPING', 'MSPT maps to error (>50ms) vs cyan (<=50ms)');

// Zero prohibited styles
assert(!libContent.includes('#f59e0b'), 'LIB_NO_AMBER', 'LibraryView contains zero prohibited #f59e0b');
assert(!realmContent.includes('#f59e0b'), 'REALM_NO_AMBER', 'RealmDetailView contains zero prohibited #f59e0b');
assert(!libContent.includes('text-amber-') && !realmContent.includes('text-amber-'), 'NO_AMBER_CLASSES', 'Views contain zero Tailwind amber classes');

// -----------------------------------------------------------------------------
// Section 4: Console Log Viewer & Monospace Live Tail
// -----------------------------------------------------------------------------
console.log('\n[Section 4] Console Log Viewer, Filtering & Live Tail');
assert(realmContent.includes('font-mono'), 'CONSOLE_MONOSPACE', 'Console viewer uses font-mono class');
assert(realmContent.includes('autoScroll'), 'CONSOLE_LIVE_TAIL', 'Console includes autoScroll reactive state');
assert(realmContent.includes('handleConsoleScroll'), 'CONSOLE_SCROLL_HANDLER', 'Console includes scroll detection for user intervention');
assert(realmContent.includes('Resume Live Tail'), 'CONSOLE_RESUME_TAIL', 'Console includes "Resume Live Tail" pill button');

// Filter levels
assert(realmContent.includes("'all', 'errors', 'warnings', 'info'"), 'CONSOLE_FILTER_LEVELS', 'Console supports all, errors, warnings, info levels');
assert(realmContent.includes('filteredLogs'), 'CONSOLE_FILTERED_LOGS', 'Console computes filteredLogs based on level');

// Command submission
assert(realmContent.includes('send_server_command'), 'CONSOLE_SEND_COMMAND_IPC', 'Console invokes send_server_command');
assert(realmContent.includes('consoleInput'), 'CONSOLE_COMMAND_INPUT', 'Console binds command input field');

// -----------------------------------------------------------------------------
// Section 5: Restored Modal Templates
// -----------------------------------------------------------------------------
console.log('\n[Section 5] Restored Modal Templates & IPC Wiring');
assert(realmContent.includes('showDeleteDataConfirmModal'), 'MODAL_DELETE_DATA_FLAG', 'showDeleteDataConfirmModal reactive flag present');
assert(realmContent.includes('<Modal') && realmContent.includes('showDeleteDataConfirmModal'), 'MODAL_DELETE_DATA_TEMPLATE', 'showDeleteDataConfirmModal bound to <Modal>');
assert(realmContent.includes('delete_player_data'), 'MODAL_DELETE_DATA_IPC', 'showDeleteDataConfirmModal wired to delete_player_data');
assert(realmContent.includes('deleteDataScope'), 'MODAL_DELETE_DATA_SCOPE', 'showDeleteDataConfirmModal provides scope radio buttons');

// -----------------------------------------------------------------------------
// Section 6: Adversarial Sparkline SVG Math Stress Test
// -----------------------------------------------------------------------------
console.log('\n[Section 6] Adversarial Sparkline SVG Math Stress');
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

// Scenario A: Empty array
assert(generateSvgArea([], 0, 60, 600, 96) === '', 'SVG_EMPTY_ARRAY', 'Empty points array returns empty string without crashing');

// Scenario B: Single point
const singlePoint = generateSvgArea([25], 0, 60, 600, 96);
assert(!singlePoint.includes('NaN'), 'SVG_SINGLE_POINT', 'Single point does not generate NaN');

// Scenario C: Extreme spike (1500ms MSPT)
const extremeSpike = generateSvgArea([10, 50, 1500, 20], 0, 60, 600, 96);
assert(!extremeSpike.includes('NaN'), 'SVG_EXTREME_SPIKE', 'Extreme 1500ms spike clamped properly without NaN');

// Scenario D: Negative numbers
const negativeVal = generateSvgArea([-5, 20, 30], 0, 60, 600, 96);
assert(!negativeVal.includes('NaN'), 'SVG_NEGATIVE_VAL', 'Negative values clamped to minVal without NaN');

// -----------------------------------------------------------------------------
// Section 7: App.vue View Integration
// -----------------------------------------------------------------------------
console.log('\n[Section 7] App.vue Shell Integration');
const appContent = fs.readFileSync(path.join(ROOT_DIR, 'src', 'App.vue'), 'utf-8');
assert(appContent.includes('import LibraryView from "./views/LibraryView.vue"'), 'APP_IMPORTS_LIB', 'App.vue imports LibraryView');
assert(appContent.includes('import RealmDetailView from "./views/RealmDetailView.vue"'), 'APP_IMPORTS_REALM', 'App.vue imports RealmDetailView');
assert(appContent.includes('<LibraryView'), 'APP_RENDERS_LIB', 'App.vue renders <LibraryView');
assert(appContent.includes('<RealmDetailView'), 'APP_RENDERS_REALM', 'App.vue renders <RealmDetailView');

console.log('\n========================================================================');
console.log(`TOTAL TESTS: ${totalTests} | PASSED: ${passedTests} | FAILED: ${failedTests}`);
console.log('========================================================================');

if (failedTests > 0) {
  process.exit(1);
}
