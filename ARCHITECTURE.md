# Conduit Architecture & Implementation Rules

## Dynamic Version & Environment Resolution (No Hardcoding)

To maintain robustness and prevent runtime crashes or version mismatches:
1. **Never Hardcode Version IDs, Forge Numbers, or Library Paths**:
   - Detect installed loaders and game versions dynamically by parsing `versions/` or reading the instance's version manifest (`.minecraft/versions/*/neoforge-*.json` or `*.json`).
   - Extract version arguments (`--fml.neoForgeVersion`, `--fml.fmlVersion`, `--fml.neoFormVersion`) directly from the detected version JSON instead of hardcoding strings like `"21.1.187"` or `"4.0.41"`.
   - Resolve module paths and classpath jars dynamically from the libraries described in the version JSON.
2. **Dynamic Worlds & Instances Discovery**:
   - Worlds in `.minecraft/saves` and local servers must be enumerated dynamically from disk, reading level names and last modified times.
   - User-selectable Mod Loaders (`NeoForge`, `Fabric`, `Vanilla`, `Forge`, `Quilt`) and Minecraft Versions (`1.21.1`, `1.21.4`, `1.20.1`, etc.) must be dynamically queried or selectable from options rather than locked to a single static configuration.
3. **Mod Management Architecture**:
   - Individual mods can be enabled or disabled dynamically (e.g. renaming `.jar` <-> `.jar.disabled`).
   - Mod metadata (name, version, icon, mod_id, authors) must be extracted dynamically from `neoforge.mods.toml`, `fabric.mod.json`, or jar assets (`logo.png`/`icon.png`).
   - Mod versions must be switchable via Modrinth API version queries for the user's selected loader and Minecraft version.
4. **Server & Client Process Isolation**:
   - Server worlds must default to independent world storage (e.g. `instance/server/world`), or if linking to singleplayer saves, ensure locks (`session.lock`) are checked and processes are cleaned before launch.
   - Always verify and terminate orphaned background processes before restarting services.

# Frontend Engineering & Code Generation Directives

## 1. Zero-Stub & Completeness Mandate
- **No Truncation or Placeholders:** Never use comments like `// TODO`, `// implement here`, `/* add styles */`, `// ... rest of code`, or placeholder functions. Deliver the complete, runnable file from start to finish.
- **No Dead UI Elements:** Every button, link, form, input, and modal must be fully wired to active state or handlers. If an external backend or endpoint is not yet available, write realistic client-side mock handlers with state updates, realistic delays (`setTimeout`), and simulated edge cases (success, loading, error).
- **Self-Contained Mock Data:** If rendering lists, feeds, tables, or charts, include comprehensive mock datasets inline (with proper IDs, edge-case strings, and timestamps) so the component renders populated immediately.

## 2. State & Reactivity Rigor
- **Immutable State Updates:** Never mutate state in place (e.g., avoid `array.push()`, `object.key = value`). Always return fresh references using spread operators or pure transformation methods.
- **Hook & Lifecycle Correctness:**
  - In React, ensure all referenced variables are present in hook dependency arrays (`useEffect`, `useCallback`, `useMemo`).
  - Never trigger unconditional state dispatches inside render bodies; guard state updates against infinite render loops.
  - Clean up event listeners, intervals, and observers inside return functions.
- **Predictable Asynchrony:** Handle async race conditions using AbortController or cancellation flags to prevent stale state overwrites.

## 3. UI, CSS, and Layout Constraints
- **Layout Safety & Collision Prevention:**
  - Avoid fragile fixed dimensions (`width: 500px`). Default to flexible wrappers (`w-full`, `max-w-*`, `min-h-*`).
  - Explicitly guard against text overflow on dynamic strings using `break-words`, `truncate`, or `line-clamp-*`.
  - Prevent clipped dropdowns, tooltips, and flyout menus by ensuring parent containers do not have accidental `overflow: hidden` when rendering absolute/fixed elements.
- **Layering & Hitboxes:** Never apply `z-index` indiscriminately. Ensure transparent overlays do not block interaction on underlying elements (use `pointer-events-none` where applicable).
- **Interactive State Feedback:** Every interactive element must include visual feedback for `:hover`, `:focus-visible`, `:active`, and `disabled` states. Disabled buttons must explicitly disable their underlying `onClick` handlers.

## 4. Syntax & Type Safety
- **Strict AST Integrity:** Every opening tag (`<div>`, `<Fragment>`, etc.) must have a corresponding closing tag. Verify closing delimiters before finishing output.
- **Safe Property Access:** Use optional chaining (`data?.items?.[0]`) and nullish coalescing (`??`) to avoid runtime crashes from missing nested properties.
- **Complete Imports:** Include all necessary component, icon, hook, and utility imports explicitly at the top of the file. Do not assume globals exist.

## UI Microcopy & Text Density Constraints
- **Zero Redundant Helper Text:** Never output descriptive captions, instructional subtitles, or explanatory paragraphs beneath labels if the label or placeholder already conveys the intent (e.g., no "Enter your email address to receive updates" under an "Email" input).
- **No Decorative Intro/Outro Cards:** Strip out filler welcome text, patronizing onboarding messages, empty status cards, and unnecessary "Tip:" boxes unless explicitly requested.
- **Strict Label Economy:** Use concise, 1–3 word action-oriented labels ("Deploy", "Save", "API Key") instead of conversational phrases ("Click here to deploy your project").
- **Clean Empty States:** If a container has no data, show a minimal, single-line indicator or simple placeholder rather than large illustrative graphics with multi-sentence guidance.
- **Contextual Tooltips Over Static Text:** Move secondary, rarely needed clarifications out of the main layout into subtle, on-demand hover tooltips (`title` attribute or tooltip primitive) rather than cluttering the screen with permanent muted body copy.

## Anti-Hallucination & Copy Precision Directives
- **Zero Fabricated Constraints:** Never invent validation rules, file limits, character caps, encryption claims, propagation delays, or technical requirements in UI helper text unless they are explicitly declared in the data schema or codebase specifications.
- **Strict Format Neutrality:** If an input does not have a hardcoded validator provided in the prompt, write NO format guidance beneath it. Never guess password complexity, date formats, or phone syntax.
- **Literal Placeholders Only:** Placeholders must show neutral, minimal examples (e.g., `user@example.com`, `https://example.com`) rather than instructional sentences disguised as placeholders (e.g., avoid `Please enter your primary corporate email address`).
- **Eliminate Faux-Technical Badges:** Do not inject decorative badges, security disclaimers, or status chips (e.g., "AES-256", "Synced", "Auto-saved") unless those states are wired to active, verified boolean flags in the component's state machine.
