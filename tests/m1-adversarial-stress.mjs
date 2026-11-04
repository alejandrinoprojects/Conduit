import fs from 'node:fs';
import path from 'node:path';
import { execSync } from 'node:child_process';
import process from 'node:process';

const ROOT_DIR = path.resolve('.');
const SRC_DIR = path.join(ROOT_DIR, 'src');
const COMMON_DIR = path.join(SRC_DIR, 'components', 'common');
const STYLE_CSS = path.join(SRC_DIR, 'style.css');

console.log('================================================================');
console.log('  MILESTONE 1 ADVERSARIAL STRESS TEST & VERIFICATION HARNESS');
console.log('================================================================\n');

let passCount = 0;
let failCount = 0;

function assert(condition, testName, failDetail = '') {
  if (condition) {
    console.log(`[PASS] ${testName}`);
    passCount++;
  } else {
    console.error(`[FAIL] ${testName}: ${failDetail}`);
    failCount++;
  }
}

// -----------------------------------------------------------------------------
// Test Suite 1: Exact CSS Variable Definitions in src/style.css
// -----------------------------------------------------------------------------
console.log('--- Test Suite 1: CSS Variable Exact Value Stress Tests ---');
const styleContent = fs.readFileSync(STYLE_CSS, 'utf-8');

const requiredTokens = [
  { varName: '--color-canvas', expected: '#11151A' },
  { varName: '--color-sidebar', expected: '#151A20' },
  { varName: '--color-card', expected: '#1B222A' },
  { varName: '--color-elevated', expected: '#27323D' },
  { varName: '--color-border', expected: '#303B47' },
  { varName: '--color-text-primary', expected: '#E2E8ED' },
  { varName: '--color-text-secondary', expected: '#9AA7B5' },
  { varName: '--color-cyan-accent', expected: '#45C5D9' },
  { varName: '--color-orange-accent', expected: '#F39A36' },
];

for (const { varName, expected } of requiredTokens) {
  // Regex to extract value for variable
  const regex = new RegExp(`${varName}\\s*:\\s*([^;]+);`, 'i');
  const match = styleContent.match(regex);
  const found = match ? match[1].trim() : null;
  assert(
    found && found.toUpperCase() === expected.toUpperCase(),
    `CSS token ${varName} matches ${expected}`,
    `Found '${found}' instead of '${expected}'`
  );
}

// -----------------------------------------------------------------------------
// Test Suite 2: Negative Prohibited Color & Class Scans
// -----------------------------------------------------------------------------
console.log('\n--- Test Suite 2: Prohibited Gold/Amber & Anti-Pattern Scans ---');

const prohibitedHexes = [
  '#f59e0b', '#d97706', '#b45309', '#92400e', '#78350f',
  '#fbbf24', '#fcd34d', '#fef3c7', '#fffbeb', '#fde68a'
];

let prohibitedHexFound = false;
for (const hex of prohibitedHexes) {
  if (styleContent.toLowerCase().includes(hex.toLowerCase())) {
    prohibitedHexFound = true;
    assert(false, `No prohibited hex in src/style.css`, `Found ${hex}`);
  }
}
if (!prohibitedHexFound) {
  assert(true, 'Zero prohibited amber/gold hex codes in src/style.css');
}

// Scan all files in src/components/common
const commonFiles = fs.readdirSync(COMMON_DIR).filter(f => f.endsWith('.vue') || f.endsWith('.ts'));

let commonProhibitedFound = false;
let commonNestedCardsFound = false;

for (const file of commonFiles) {
  const content = fs.readFileSync(path.join(COMMON_DIR, file), 'utf-8');
  for (const hex of prohibitedHexes) {
    if (content.toLowerCase().includes(hex.toLowerCase())) {
      commonProhibitedFound = true;
      assert(false, `No prohibited hex in ${file}`, `Found ${hex}`);
    }
  }

  // Check amber Tailwind utility classes
  const amberClassMatch = content.match(/amber-(?:50|100|200|300|400|500|600|700|800|900)/g);
  if (amberClassMatch) {
    commonProhibitedFound = true;
    assert(false, `No amber classes in ${file}`, `Found ${amberClassMatch.join(', ')}`);
  }

  // Check nested card pattern: multiple <Card tags or <Card inside template of Card.vue
  if (file === 'Card.vue') {
    const cardTagCount = (content.match(/<Card\b/g) || []).length;
    if (cardTagCount > 0) {
      commonNestedCardsFound = true;
      assert(false, `Card.vue does not render nested <Card>`, `Found ${cardTagCount} <Card tags in Card.vue`);
    }
  } else {
    // In other components, check for nested <Card ... <Card
    if (/<Card[^>]*>[\s\S]*?<Card/m.test(content)) {
      commonNestedCardsFound = true;
      assert(false, `No nested Card inside ${file}`, `Found nested <Card> in ${file}`);
    }
  }
}

if (!commonProhibitedFound) {
  assert(true, 'Zero prohibited amber/gold colors or classes in src/components/common/*');
}

if (!commonNestedCardsFound) {
  assert(true, 'Zero nested Card anti-patterns in src/components/common/*');
}

// -----------------------------------------------------------------------------
// Test Suite 3: Common Component Export & Interface Contracts
// -----------------------------------------------------------------------------
console.log('\n--- Test Suite 3: Common Component Exports & Interface Contracts ---');

const indexContent = fs.readFileSync(path.join(COMMON_DIR, 'index.ts'), 'utf-8');

const expectedExports = [
  'Button', 'ButtonProps',
  'Input', 'InputProps',
  'Badge', 'BadgeProps',
  'StatusDot', 'StatusDotProps',
  'Card', 'CardProps',
  'Modal', 'ModalProps',
  'Tabs', 'TabsProps', 'TabItem',
  'MetricCard', 'MetricCardProps'
];

for (const exp of expectedExports) {
  const regex = new RegExp(`\\b${exp}\\b`);
  assert(regex.test(indexContent), `index.ts exports ${exp}`);
}

// Verify Button.vue interface contracts
const buttonContent = fs.readFileSync(path.join(COMMON_DIR, 'Button.vue'), 'utf-8');
assert(buttonContent.includes("'primary' | 'secondary' | 'accent' | 'ghost' | 'danger'"), 'Button.vue specifies all 5 variants');
assert(buttonContent.includes("'sm' | 'md' | 'lg' | 'icon'"), 'Button.vue specifies sizes sm, md, lg, icon');
assert(buttonContent.includes('loading?: boolean'), 'Button.vue supports loading prop');
assert(buttonContent.includes('disabled?: boolean'), 'Button.vue supports disabled prop');

// Verify Input.vue interface contracts
const inputContent = fs.readFileSync(path.join(COMMON_DIR, 'Input.vue'), 'utf-8');
assert(inputContent.includes('modelValue'), 'Input.vue supports modelValue');
assert(inputContent.includes('update:modelValue'), 'Input.vue emits update:modelValue');

// Verify Modal.vue interface contracts
const modalContent = fs.readFileSync(path.join(COMMON_DIR, 'Modal.vue'), 'utf-8');
assert(modalContent.includes('modelValue: boolean'), 'Modal.vue specifies modelValue: boolean');
assert(modalContent.includes('close'), 'Modal.vue emits close');
assert(modalContent.includes('<Teleport to="body">'), 'Modal.vue teleports to body');

// -----------------------------------------------------------------------------
// Test Suite 4: Subsystem Boundary & Backend Zero-Modification Audit
// -----------------------------------------------------------------------------
console.log('\n--- Test Suite 4: Subsystem Isolation & Git Status Audit ---');

const SPRINT_START_MS = new Date('2026-11-04T05:42:00Z').getTime();

function checkDirModifications(dirPath) {
  const entries = fs.readdirSync(dirPath, { withFileTypes: true });
  const modified = [];
  for (const entry of entries) {
    const full = path.join(dirPath, entry.name);
    if (entry.isDirectory()) {
      modified.push(...checkDirModifications(full));
    } else {
      const stat = fs.statSync(full);
      if (stat.mtimeMs > SPRINT_START_MS) {
        modified.push(path.relative(ROOT_DIR, full));
      }
    }
  }
  return modified;
}

const tauriModified = checkDirModifications(path.join(ROOT_DIR, 'src-tauri'));
assert(
  tauriModified.length === 0,
  'Zero files in src-tauri/ modified during sprint',
  `Modified: ${tauriModified.join(', ')}`
);

// -----------------------------------------------------------------------------
// Test Suite 5: Rigorous Build & Typecheck Verification
// -----------------------------------------------------------------------------
console.log('\n--- Test Suite 5: Typecheck & Production Build Execution ---');

try {
  console.log('Running: npx vue-tsc --noEmit...');
  const tscOutput = execSync('npx vue-tsc --noEmit', { cwd: ROOT_DIR, encoding: 'utf-8' });
  assert(true, 'vue-tsc --noEmit completes with zero TypeScript/Vue errors');
} catch (err) {
  assert(false, 'vue-tsc --noEmit', err.stdout || err.message);
}

try {
  console.log('Running: npm run build...');
  const buildOutput = execSync('npm run build', { cwd: ROOT_DIR, encoding: 'utf-8' });
  const hasIndexHtml = fs.existsSync(path.join(ROOT_DIR, 'dist', 'index.html'));
  assert(hasIndexHtml, 'npm run build succeeds and creates dist/index.html');
} catch (err) {
  assert(false, 'npm run build', err.stdout || err.message);
}

// -----------------------------------------------------------------------------
// Summary
// -----------------------------------------------------------------------------
console.log('\n================================================================');
console.log(`SUMMARY: ${passCount} passed, ${failCount} failed.`);
console.log('================================================================');

if (failCount > 0) {
  process.exit(1);
} else {
  process.exit(0);
}
