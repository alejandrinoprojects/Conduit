# 5. Skin Anvil — Skin Painting Mechanics
**Repository:** [spencerfrost/skin-anvil](https://github.com/spencerfrost/skin-anvil)
**Tech Stack:** TypeScript, HTML5 Canvas, WebGL

## Core Concepts & Architecture
Skin Anvil provides dual-layer 64x64 pixel editing for Minecraft player skins, supporting both Steve (4px arm) and Alex (3px arm) geometries.

### Key Architectural Patterns
1. **Dual-Layer Texture Unwrapping:**
   - Base layer (inner skin) and overlay (outer jacket, hat, sleeves, pants).
2. **Brush & Eyedropper Mechanics:**
   - Real-time color quantization, opacity handling, and symmetry mirroring.
3. **Export & Upload:**
   - Converts 64x64 canvas to raw PNG buffers for Mojang API skin upload.

## Conduit Implementation Roadmap
- [x] Player profile and skin preview interface
- [ ] 2D pixel-grid texture brush and palette picker in Player & Skins section
