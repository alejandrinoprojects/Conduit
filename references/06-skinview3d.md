# 6. Skinview3D — 3D Skin Preview
**Repository:** [bs-community/skinview3d](https://github.com/bs-community/skinview3d)
**Tech Stack:** Three.js, WebGL, TypeScript

## Core Concepts & Architecture
Skinview3D is the de facto standard Three.js viewer for Minecraft skins and capes.

### Key Architectural Patterns
1. **Skeletal Animation:**
   - Walking, running, flying, and idle animation loops with configurable animation speed.
2. **Dynamic Cape & Elytra Physics:**
   - Real-time cape flapping and curved elytra meshes.
3. **Orbit Controls & Lighting:**
   - Mouse rotation, zoom, pan, and directional Minecraft ambient lighting.

## Conduit Implementation Roadmap
- [x] Three.js WebGL canvas integration in Player profile tab
- [x] Steve / Alex model toggle with live skin preview
