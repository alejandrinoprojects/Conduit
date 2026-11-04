#!/usr/bin/env node
/**
 * Test Harness: Milestone 1 Common UI Components & Design System
 * 
 * Verifies:
 * 1. CSS variables & Tailwind v4 theme tokens in src/style.css against required palette.
 * 2. Component exports from src/components/common/index.ts.
 * 3. Empirical runtime rendering & prop validations for all 8 components via Vue SSR.
 * 4. Boundary and adversarial edge cases.
 */

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const ROOT_DIR = path.resolve(__dirname, '..');
const STYLE_PATH = path.join(ROOT_DIR, 'src', 'style.css');
const COMMON_DIR = path.join(ROOT_DIR, 'src', 'components', 'common');

// Polyfill window for SSR environment if needed
globalThis.window = {
  addEventListener() {},
  removeEventListener() {},
};

let passed = 0;
let failed = 0;
const failures = [];

function assert(condition, message, details = '') {
  if (condition) {
    passed++;
    console.log(`  \x1b[32m✔ PASS\x1b[0m ${message}`);
  } else {
    failed++;
    const fullMessage = details ? `${message} -> ${details}` : message;
    failures.push(fullMessage);
    console.log(`  \x1b[31m✖ FAIL\x1b[0m ${fullMessage}`);
  }
}

async function run() {
  console.log('\n\x1b[1m\x1b[36m======================================================================\x1b[0m');
  console.log('\x1b[1m\x1b[36m  CONDUIT MILESTONE 1: EMPIRICAL COMPONENT & DESIGN SYSTEM TEST SUITE \x1b[0m');
  console.log('\x1b[1m\x1b[36m======================================================================\x1b[0m\n');

  // ============================================================================
  // SECTION 1: CSS & Design System Conformance (src/style.css)
  // ============================================================================
  console.log('\x1b[1m▶ Section 1: CSS & Design System Conformance (src/style.css)\x1b[0m');
  assert(fs.existsSync(STYLE_PATH), 'src/style.css exists');
  const styleContent = fs.readFileSync(STYLE_PATH, 'utf-8');

  assert(styleContent.includes('@import "tailwindcss";'), 'style.css imports Tailwind v4');
  assert(styleContent.includes('@theme {'), 'style.css declares @theme block');

  const requiredPalette = [
    { name: 'canvas (#11151A)', hex: '#11151A', token: '--color-canvas' },
    { name: 'sidebar (#151A20)', hex: '#151A20', token: '--color-sidebar' },
    { name: 'card (#1B222A)', hex: '#1B222A', token: '--color-card' },
    { name: 'elevated (#27323D)', hex: '#27323D', token: '--color-elevated' },
    { name: 'border (#303B47)', hex: '#303B47', token: '--color-border' },
    { name: 'text-primary (#E2E8ED)', hex: '#E2E8ED', token: '--color-text-primary' },
    { name: 'secondary text (#9AA7B5)', hex: '#9AA7B5', token: '--color-text-secondary' },
    { name: 'cyan accent (#45C5D9)', hex: '#45C5D9', token: '--color-cyan-accent' },
    { name: 'cyan highlight (#76DCE8)', hex: '#76DCE8', token: '--color-cyan-highlight' },
    { name: 'orange accent (#F39A36)', hex: '#F39A36', token: '--color-orange-accent' },
    { name: 'muted green (#3DB877)', hex: '#3DB877', token: '--color-status-success' },
    { name: 'muted red (#E05252)', hex: '#E05252', token: '--color-status-error' },
  ];

  for (const item of requiredPalette) {
    const hasToken = styleContent.toLowerCase().includes(item.token.toLowerCase());
    const hasHex = styleContent.toUpperCase().includes(item.hex.toUpperCase());
    assert(hasToken && hasHex, `Palette token ${item.name} declared in @theme with ${item.hex}`);
  }

  // Verify anti-patterns
  const prohibitedGoldAmber = '#F59E0B';
  assert(!styleContent.toUpperCase().includes(prohibitedGoldAmber), 'Absence of prohibited gold/amber (#f59e0b) in style.css');

  // Verify custom controls
  assert(styleContent.includes('::-webkit-scrollbar'), 'Custom scrollbar styling present in style.css');
  assert(styleContent.includes('input[type="range"]'), 'Custom range slider styling present in style.css');

  // ============================================================================
  // SECTION 2: Dynamic Vite SSR Module Loader & Common Component Imports
  // ============================================================================
  console.log('\n\x1b[1m▶ Section 2: Component Exports & Module Loading\x1b[0m');

  const { createServer } = await import('vite');
  const server = await createServer({
    server: { middlewareMode: true },
    ssr: { external: ['vue', 'vue/server-renderer', 'lucide-vue-next'] },
  });

  const commonIndex = await server.ssrLoadModule('/src/components/common/index.ts');
  const { createVNode, h } = await import('vue');
  const { renderToString } = await import('vue/server-renderer');

  const expectedComponents = [
    'Button',
    'Input',
    'Badge',
    'StatusDot',
    'Card',
    'Modal',
    'Tabs',
    'MetricCard',
  ];

  for (const name of expectedComponents) {
    assert(commonIndex[name] !== undefined, `src/components/common/index.ts exports ${name}`);
  }

  const { Button, Input, Badge, StatusDot, Card, Modal, Tabs, MetricCard } = commonIndex;

  // Helper to render component to HTML string (and handle teleports)
  async function renderComponent(comp, props = {}, slots = {}) {
    const ctx = {};
    const vnode = createVNode(comp, props, slots);
    const html = await renderToString(vnode, ctx);
    const teleportHtml = ctx.teleports ? Object.values(ctx.teleports).join('') : '';
    return { html, teleportHtml, full: html + teleportHtml };
  }

  // ============================================================================
  // SECTION 3: Button Runtime & Prop Tests
  // ============================================================================
  console.log('\n\x1b[1m▶ Section 3: Button Component Runtime Verification\x1b[0m');

  // Default Button
  const btnDefault = await renderComponent(Button, {}, { default: () => 'Click Me' });
  assert(btnDefault.html.includes('<button') && btnDefault.html.includes('bg-elevated'), 'Button default renders secondary variant (bg-elevated)');
  assert(btnDefault.html.includes('Click Me'), 'Button default slot text rendered');

  // Button Variants
  const btnPrimary = await renderComponent(Button, { variant: 'primary' }, { default: () => 'Save' });
  assert(btnPrimary.html.includes('bg-orange-accent') && btnPrimary.html.includes('text-[#11151A]'), 'Button primary has orange accent & contrast text');

  const btnAccent = await renderComponent(Button, { variant: 'accent' }, { default: () => 'Connect' });
  assert(btnAccent.html.includes('bg-cyan-accent') && btnAccent.html.includes('text-[#11151A]'), 'Button accent has cyan accent & contrast text');

  const btnDanger = await renderComponent(Button, { variant: 'danger' }, { default: () => 'Delete' });
  assert(btnDanger.html.includes('bg-status-error'), 'Button danger has bg-status-error');

  const btnGhost = await renderComponent(Button, { variant: 'ghost' }, { default: () => 'Cancel' });
  assert(btnGhost.html.includes('bg-transparent') && btnGhost.html.includes('text-text-secondary'), 'Button ghost has transparent bg & secondary text');

  // Button Sizes
  const btnSm = await renderComponent(Button, { size: 'sm' });
  assert(btnSm.html.includes('h-7') && btnSm.html.includes('text-xs'), 'Button sm size has h-7 text-xs');

  const btnLg = await renderComponent(Button, { size: 'lg' });
  assert(btnLg.html.includes('h-10') && btnLg.html.includes('text-sm'), 'Button lg size has h-10 text-sm');

  const btnIcon = await renderComponent(Button, { size: 'icon' }, { default: () => h('span', 'ICON') });
  assert(btnIcon.html.includes('h-8 w-8') && btnIcon.html.includes('ICON'), 'Button icon size has h-8 w-8 and renders icon');

  // Button States
  const btnLoading = await renderComponent(Button, { loading: true }, { default: () => 'Loading...' });
  assert(btnLoading.html.includes('disabled') && btnLoading.html.includes('animate-spin'), 'Button loading has disabled attribute & spinner icon');

  const btnDisabled = await renderComponent(Button, { disabled: true }, { default: () => 'Disabled' });
  assert(btnDisabled.html.includes('disabled') && btnDisabled.html.includes('cursor-not-allowed'), 'Button disabled has disabled attribute & cursor-not-allowed');

  const btnFull = await renderComponent(Button, { fullWidth: true });
  assert(btnFull.html.includes('w-full'), 'Button fullWidth has w-full class');

  // Prefix & Suffix slots
  const btnSlots = await renderComponent(Button, {}, {
    prefix: () => h('span', { class: 'prefix-slot' }, 'P'),
    default: () => 'Label',
    suffix: () => h('span', { class: 'suffix-slot' }, 'S'),
  });
  assert(btnSlots.html.includes('prefix-slot') && btnSlots.html.includes('suffix-slot'), 'Button renders prefix and suffix slots');

  // ============================================================================
  // SECTION 4: Input Runtime & Prop Tests
  // ============================================================================
  console.log('\n\x1b[1m▶ Section 4: Input Component Runtime Verification\x1b[0m');

  const inpDefault = await renderComponent(Input, { modelValue: 'hello world', placeholder: 'Enter text...' });
  assert(inpDefault.html.includes('value="hello world"') && inpDefault.html.includes('placeholder="Enter text..."'), 'Input renders modelValue and placeholder');
  assert(inpDefault.html.includes('type="text"'), 'Input default type is text');

  const inpPassword = await renderComponent(Input, { type: 'password', modelValue: 'secret' });
  assert(inpPassword.html.includes('type="password"') && inpPassword.html.includes('value="secret"'), 'Input custom type password works');

  const inpDisabled = await renderComponent(Input, { disabled: true });
  assert(inpDisabled.html.includes('disabled') && inpDisabled.html.includes('cursor-not-allowed'), 'Input disabled state applied to input element');

  const inpReadonly = await renderComponent(Input, { readonly: true });
  assert(inpReadonly.html.includes('readonly'), 'Input readonly state applied');

  // Error states: boolean vs string
  const inpErrorBool = await renderComponent(Input, { error: true });
  assert(inpErrorBool.html.includes('border-status-error'), 'Input error=true applies border-status-error');

  const inpErrorString = await renderComponent(Input, { error: 'Port must be between 1024 and 65535' });
  assert(inpErrorString.html.includes('Port must be between 1024 and 65535'), 'Input error string displays error message below input');

  // Sizes
  const inpSm = await renderComponent(Input, { size: 'sm' });
  assert(inpSm.html.includes('h-7'), 'Input sm has h-7 class');

  const inpLg = await renderComponent(Input, { size: 'lg' });
  assert(inpLg.html.includes('h-10'), 'Input lg has h-10 class');

  // Slots
  const inpSlots = await renderComponent(Input, {}, {
    prefix: () => h('span', { class: 'input-pre' }, '$'),
    suffix: () => h('span', { class: 'input-suf' }, '.com'),
  });
  assert(inpSlots.html.includes('input-pre') && inpSlots.html.includes('input-suf'), 'Input prefix and suffix slots render');

  // ============================================================================
  // SECTION 5: Badge Runtime & Prop Tests
  // ============================================================================
  console.log('\n\x1b[1m▶ Section 5: Badge Component Runtime Verification\x1b[0m');

  const badgeDefault = await renderComponent(Badge, {}, { default: () => 'Paper 1.20.4' });
  assert(badgeDefault.html.includes('Paper 1.20.4') && badgeDefault.html.includes('bg-elevated/70'), 'Badge default renders neutral variant');

  const badgeSuccess = await renderComponent(Badge, { variant: 'success' }, { default: () => 'Online' });
  assert(badgeSuccess.html.includes('bg-status-success-subtle') && badgeSuccess.html.includes('text-status-success'), 'Badge success variant has subtle bg & green text');

  const badgeError = await renderComponent(Badge, { variant: 'error' }, { default: () => 'Crashed' });
  assert(badgeError.html.includes('bg-status-error-subtle') && badgeError.html.includes('text-status-error'), 'Badge error variant has subtle bg & red text');

  const badgeWarning = await renderComponent(Badge, { variant: 'warning' }, { default: () => 'Outdated' });
  assert(badgeWarning.html.includes('bg-status-warning-subtle') && badgeWarning.html.includes('text-status-warning'), 'Badge warning variant has subtle bg & warning text');

  const badgeCyan = await renderComponent(Badge, { variant: 'cyan' }, { default: () => 'Fabric' });
  assert(badgeCyan.html.includes('bg-cyan-subtle') && badgeCyan.html.includes('text-cyan-accent'), 'Badge cyan variant has cyan subtle & cyan accent text');

  const badgeOrange = await renderComponent(Badge, { variant: 'orange' }, { default: () => 'Pro' });
  assert(badgeOrange.html.includes('bg-orange-subtle') && badgeOrange.html.includes('text-orange-accent'), 'Badge orange variant has orange subtle & orange accent text');

  const badgeDot = await renderComponent(Badge, { variant: 'success', dot: true }, { default: () => 'Running' });
  assert(badgeDot.html.includes('rounded-full') && badgeDot.html.includes('bg-status-success'), 'Badge dot=true renders status dot');

  // ============================================================================
  // SECTION 6: StatusDot Runtime & Prop Tests
  // ============================================================================
  console.log('\n\x1b[1m▶ Section 6: StatusDot Component Runtime Verification\x1b[0m');

  const dotOnline = await renderComponent(StatusDot, { status: 'online', label: 'Server Running' });
  assert(dotOnline.html.includes('bg-status-success') && dotOnline.html.includes('Server Running'), 'StatusDot online renders green bg and label');

  const dotSynonym = await renderComponent(StatusDot, { status: 'running' });
  assert(dotSynonym.html.includes('bg-status-success'), 'StatusDot normalizes "running" to online (green)');

  const dotStarting = await renderComponent(StatusDot, { status: 'starting' });
  assert(dotStarting.html.includes('bg-orange-accent') && dotStarting.html.includes('animate-ping'), 'StatusDot starting renders orange accent and auto-pulse');

  const dotStopped = await renderComponent(StatusDot, { status: 'stopped' });
  assert(dotStopped.html.includes('bg-status-error'), 'StatusDot stopped renders red bg');

  const dotError = await renderComponent(StatusDot, { status: 'error' });
  assert(dotError.html.includes('bg-status-error'), 'StatusDot error renders red bg');

  const dotWarning = await renderComponent(StatusDot, { status: 'warning' });
  assert(dotWarning.html.includes('bg-status-warning'), 'StatusDot warning renders warning bg');

  const dotPulse = await renderComponent(StatusDot, { status: 'online', pulse: true });
  assert(dotPulse.html.includes('animate-ping'), 'StatusDot pulse=true renders animate-ping on online status');

  const dotSizes = ['xs', 'sm', 'md', 'lg'];
  for (const size of dotSizes) {
    const dotS = await renderComponent(StatusDot, { status: 'online', size });
    assert(dotS.html.includes('rounded-full'), `StatusDot renders for size ${size}`);
  }

  // ============================================================================
  // SECTION 7: Card Runtime & Prop Tests
  // ============================================================================
  console.log('\n\x1b[1m▶ Section 7: Card Component Runtime Verification\x1b[0m');

  const cardDefault = await renderComponent(Card, { title: 'Server Properties', subtitle: 'server.properties configuration' }, {
    default: () => 'Card Body Content',
  });
  assert(cardDefault.html.includes('Server Properties') && cardDefault.html.includes('server.properties configuration'), 'Card renders title and subtitle');
  assert(cardDefault.html.includes('Card Body Content'), 'Card renders body slot');
  assert(cardDefault.html.includes('bg-card'), 'Card default background is bg-card');

  const cardElevated = await renderComponent(Card, { elevated: true });
  assert(cardElevated.html.includes('bg-elevated'), 'Card elevated=true has bg-elevated');

  const cardSelected = await renderComponent(Card, { selected: true });
  assert(cardSelected.html.includes('border-cyan-accent') && cardSelected.html.includes('ring-cyan-accent'), 'Card selected=true has cyan accent border and ring');

  const cardHoverable = await renderComponent(Card, { hoverable: true });
  assert(cardHoverable.html.includes('hover:bg-card-hover') && cardHoverable.html.includes('cursor-pointer'), 'Card hoverable=true has hover styles');

  const cardSlots = await renderComponent(Card, {}, {
    header: () => h('div', { class: 'custom-header' }, 'Header Slot'),
    actions: () => h('button', { class: 'card-action-btn' }, 'Action'),
    default: () => 'Body Content',
    footer: () => h('div', { class: 'custom-footer' }, 'Footer Slot'),
  });
  assert(cardSlots.html.includes('custom-header') && cardSlots.html.includes('card-action-btn'), 'Card header and actions slots render');
  assert(cardSlots.html.includes('custom-footer'), 'Card footer slot renders');

  // ============================================================================
  // SECTION 8: Modal Runtime & Prop Tests
  // ============================================================================
  console.log('\n\x1b[1m▶ Section 8: Modal Component Runtime Verification\x1b[0m');

  const modalClosed = await renderComponent(Modal, { modelValue: false, title: 'Hidden Modal' }, { default: () => 'Hidden Content' });
  assert(!modalClosed.teleportHtml.includes('Hidden Modal'), 'Modal modelValue=false renders nothing in teleport target');

  const modalOpen = await renderComponent(Modal, {
    modelValue: true,
    title: 'Delete World Data',
    description: 'This action is irreversible.',
    maxWidth: 'lg',
    showClose: true,
  }, {
    default: () => 'Modal Body Content',
    footer: () => h('button', { class: 'confirm-btn' }, 'Confirm Delete'),
  });

  assert(modalOpen.teleportHtml.includes('Delete World Data'), 'Modal open renders title in teleport target');
  assert(modalOpen.teleportHtml.includes('This action is irreversible.'), 'Modal open renders description');
  assert(modalOpen.teleportHtml.includes('max-w-lg'), 'Modal maxWidth=lg sets max-w-lg class');
  assert(modalOpen.teleportHtml.includes('Modal Body Content'), 'Modal renders default body slot');
  assert(modalOpen.teleportHtml.includes('confirm-btn'), 'Modal renders footer slot');
  assert(modalOpen.teleportHtml.includes('lucide-x'), 'Modal showClose=true renders close button with X icon');

  // Modal showClose: false
  const modalNoClose = await renderComponent(Modal, { modelValue: true, title: 'No Close Button', showClose: false });
  assert(!modalNoClose.teleportHtml.includes('lucide-x'), 'Modal showClose=false omits close button');

  // ============================================================================
  // SECTION 9: Tabs Runtime & Prop Tests
  // ============================================================================
  console.log('\n\x1b[1m▶ Section 9: Tabs Component Runtime Verification\x1b[0m');

  const sampleTabs = [
    { id: 'servers', label: 'Servers', count: 4 },
    { id: 'clients', label: 'Clients', count: 2 },
    { id: 'disabled_tab', label: 'Archived', disabled: true },
  ];

  const tabsLine = await renderComponent(Tabs, {
    modelValue: 'servers',
    tabs: sampleTabs,
    variant: 'line',
  });
  assert(tabsLine.html.includes('Servers') && tabsLine.html.includes('Clients'), 'Tabs line variant renders tab labels');
  assert(tabsLine.html.includes('border-cyan-accent text-cyan-accent'), 'Active tab in line variant has cyan accent border and text');
  assert(tabsLine.html.includes('>4<'), 'Tabs renders count pill for active tab');
  assert(tabsLine.html.includes('cursor-not-allowed') && tabsLine.html.includes('disabled'), 'Disabled tab in line variant has disabled attribute and cursor');

  const tabsSegmented = await renderComponent(Tabs, {
    modelValue: 'clients',
    tabs: sampleTabs,
    variant: 'segmented',
    fullWidth: true,
  });
  assert(tabsSegmented.html.includes('bg-sidebar') && tabsSegmented.html.includes('bg-elevated text-cyan-accent'), 'Segmented tabs renders container and active item elevated cyan styling');
  assert(tabsSegmented.html.includes('w-full'), 'Tabs fullWidth=true has w-full class');

  // ============================================================================
  // SECTION 10: MetricCard Runtime & Prop Tests
  // ============================================================================
  console.log('\n\x1b[1m▶ Section 10: MetricCard Component Runtime Verification\x1b[0m');

  const metricNormal = await renderComponent(MetricCard, {
    label: 'Tick Time (MSPT)',
    value: 14.2,
    unit: 'ms',
    secondaryLabel: 'TPS',
    secondaryValue: 20.0,
    status: 'normal',
  });
  assert(metricNormal.html.includes('Tick Time (MSPT)') && metricNormal.html.includes('uppercase'), 'MetricCard renders label with uppercase class');
  assert(metricNormal.html.includes('14.2') && metricNormal.html.includes('ms'), 'MetricCard renders value and unit');
  assert(metricNormal.html.includes('TPS:') && metricNormal.html.includes('20'), 'MetricCard renders secondary label and value');
  assert(metricNormal.html.includes('text-text-primary'), 'MetricCard normal status uses text-text-primary');

  const metricSuccess = await renderComponent(MetricCard, { label: 'TPS', value: 20.0, status: 'success' });
  assert(metricSuccess.html.includes('text-status-success'), 'MetricCard status=success uses text-status-success');

  const metricWarning = await renderComponent(MetricCard, { label: 'Memory', value: '88%', status: 'warning' });
  assert(metricWarning.html.includes('text-status-warning'), 'MetricCard status=warning uses text-status-warning');

  const metricError = await renderComponent(MetricCard, { label: 'MSPT', value: 65.4, unit: 'ms', status: 'error' });
  assert(metricError.html.includes('text-status-error'), 'MetricCard status=error uses text-status-error');

  const metricCyan = await renderComponent(MetricCard, { label: 'Active Tunnels', value: 3, status: 'cyan' });
  assert(metricCyan.html.includes('text-cyan-accent'), 'MetricCard status=cyan uses text-cyan-accent');

  const metricCompact = await renderComponent(MetricCard, { label: 'Players', value: 5, compact: true });
  assert(metricCompact.html.includes('p-2.5') && metricCompact.html.includes('text-lg'), 'MetricCard compact=true applies compact padding and font size');

  // ============================================================================
  // SECTION 11: Adversarial & Stress Edge Cases
  // ============================================================================
  console.log('\n\x1b[1m▶ Section 11: Adversarial & Stress Testing\x1b[0m');

  // HTML escaping in Input, Button, Card, Badge, Modal
  const xssPayload = '<script>alert("pwned")</script>';
  const btnXss = await renderComponent(Button, {}, { default: () => xssPayload });
  assert(btnXss.html.includes('&lt;script&gt;') && !btnXss.html.includes('<script>'), 'Button escapes potential HTML injection in slot');

  const inputXss = await renderComponent(Input, { modelValue: xssPayload });
  assert(inputXss.html.includes('&lt;script&gt;') || inputXss.html.includes('&quot;pwned&quot;'), 'Input escapes value attribute safely');

  // Empty tabs
  const emptyTabs = await renderComponent(Tabs, { modelValue: '', tabs: [] });
  assert(emptyTabs.html.includes('<div'), 'Tabs handles empty tabs array without crashing');

  // Unknown variant fallbacks
  const btnUnknown = await renderComponent(Button, { variant: 'unknown-nonexistent' });
  assert(btnUnknown.html.includes('bg-elevated'), 'Button falls back to secondary on unknown variant');

  const statusUnknown = await renderComponent(StatusDot, { status: 'weird-status-xyz' });
  assert(statusUnknown.html.includes('bg-status-error'), 'StatusDot falls back to stopped (red) on unknown status string');

  // Extreme numbers in MetricCard
  const metricExtreme = await renderComponent(MetricCard, { label: 'Bytes', value: 999999999999, secondaryValue: -1 });
  assert(metricExtreme.html.includes('999999999999') && metricExtreme.html.includes('-1'), 'MetricCard handles extreme numeric values');

  await server.close();

  // Summary
  console.log('\n\x1b[1m\x1b[36m======================================================================\x1b[0m');
  console.log(`\x1b[1mRESULTS: Total: ${passed + failed} | Passed: \x1b[32m${passed}\x1b[0m | Failed: \x1b[31m${failed}\x1b[0m`);
  console.log('\x1b[1m\x1b[36m======================================================================\x1b[0m\n');

  if (failed > 0) {
    console.error('\x1b[31mFailures:\x1b[0m');
    for (const f of failures) {
      console.error(` - ${f}`);
    }
    process.exit(1);
  } else {
    console.log('\x1b[32m✔ ALL EMPIRICAL VERIFICATIONS PASSED CLEANLY!\x1b[0m\n');
    process.exit(0);
  }
}

run().catch((err) => {
  console.error('\x1b[31mFatal test harness error:\x1b[0m', err);
  process.exit(1);
});
