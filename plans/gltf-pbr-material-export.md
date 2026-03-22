# glTF PBR Material Export Plan

## Problem

The current glTF export includes only per-vertex `COLOR_0` attributes with no material. This means:
1. **High-frequency vertex colors** from fractal trap values look noisy — every vertex gets a different palette color based on its local SDF trap value, creating a chaotic appearance
2. **No lighting properties** — glTF viewers use a default material (usually white, unlit or basic), so the mesh looks flat/plastic
3. **The app's PBR/Blinn-Phong lighting is lost** — the user carefully tunes roughness, metallic, ambient etc. in the viewer, but none of it transfers to the exported mesh

## Current State

### Lighting Config Available at Export Time
From [`LightingConfig`](crates/fractal-core/src/sdf.rs:52):
- `lighting_model: u32` — 0 = Blinn-Phong, 1 = PBR Cook-Torrance GGX
- `roughness: f32` — 0..1
- `metallic: f32` — 0..1
- `ambient: f32`
- `diffuse: f32`
- `specular: f32`
- `shininess: f32`

### Color Config Available at Export Time
From [`ColorConfig`](crates/fractal-core/src/sdf.rs:95):
- `base_color: [f32; 3]`
- `palette_colors: [[f32; 3]; 8]`
- `palette_count: u32`
- `color_mode: u32` — 0: solid, 1: orbit trap, 2: iteration, 3: normal, 4: combined

### Current glTF Export
[`gltf_export.rs`](crates/fractal-core/src/mesh/gltf_export.rs) writes:
- POSITION accessor
- NORMAL accessor
- COLOR_0 accessor (per-vertex RGBA float)
- Indices
- **No material** — the primitive has `material: None`

## Solution: Add glTF PBR Material

glTF 2.0 natively supports PBR Metallic-Roughness materials. We should export a material that maps our lighting config directly to the glTF material model.

### What glTF Materials Support

```
Material
├── pbrMetallicRoughness
│   ├── baseColorFactor: [r, g, b, a]     ← overall tint
│   ├── baseColorTexture                   ← optional texture
│   ├── metallicFactor: f32                ← 0..1
│   └── roughnessFactor: f32               ← 0..1
├── emissiveFactor: [r, g, b]
├── alphaMode: OPAQUE/MASK/BLEND
└── doubleSided: bool
```

### Mapping Strategy

| App Config | glTF Material Field |
|-----------|-------------------|
| `lighting_config.roughness` | `roughnessFactor` |
| `lighting_config.metallic` | `metallicFactor` |
| `color_config.base_color` | `baseColorFactor` when color_mode=0 (solid) |
| Per-vertex palette colors | Keep `COLOR_0` attribute — glTF multiplies baseColorFactor × COLOR_0 |
| `lighting_config.ambient` | `emissiveFactor` (subtle ambient glow) |

**Key insight:** glTF spec says `baseColorFactor × COLOR_0(vertex)` = final base color. So we can:
- Set `baseColorFactor = [1,1,1,1]` to let vertex colors pass through unchanged
- Or set `baseColorFactor` to the base_color and reduce vertex color saturation
- The material's `metallicFactor` and `roughnessFactor` give the mesh physical appearance

### For Blinn-Phong Lighting Model
When `lighting_model == 0`, convert to approximate PBR:
- `roughnessFactor = 1.0 - (shininess / 128.0).clamp(0.0, 0.95)` — higher shininess → lower roughness
- `metallicFactor = specular.clamp(0.0, 1.0)` — specular intensity maps roughly to metallic
- `baseColorFactor = base_color` with alpha 1.0

### For PBR Lighting Model
When `lighting_model == 1`, direct mapping:
- `roughnessFactor = roughness`
- `metallicFactor = metallic`
- `baseColorFactor = [1, 1, 1, 1]` (let vertex colors define base color)

## Implementation Plan

### 1. Add Material Config to Export Pipeline

Add a new struct to carry material properties through the export:

```rust
/// Material properties for glTF export, derived from the app's lighting config.
pub struct ExportMaterial {
    pub base_color_factor: [f32; 4],
    pub metallic_factor: f32,
    pub roughness_factor: f32,
    pub emissive_factor: [f32; 3],
    pub double_sided: bool,
}
```

### 2. Update `export_glb` Signature

Change [`export_glb()`](crates/fractal-core/src/mesh/gltf_export.rs:46) to accept an optional material:

```rust
pub fn export_glb(
    mesh: &MeshData,
    material: Option<&ExportMaterial>,
    path: &Path,
) -> Result<(), ExportError>
```

### 3. Build glTF Material JSON

In [`build_glb()`](crates/fractal-core/src/mesh/gltf_export.rs:59), add material creation:
- Create `gltf_json::Material` with `pbr_metallic_roughness` populated
- Set the primitive's `material: Some(Index::new(0))`
- Add the material to `root.materials`

### 4. Update Call Site in `app.rs`

In [`spawn_export_thread()`](crates/fractal-app/src/app.rs:1448), construct `ExportMaterial` from `LightingConfig` + `ColorConfig` and pass it to `export_glb()`.

### 5. Smooth Vertex Colors (Optional Enhancement)

For high-frequency fractal coloring, apply Laplacian color smoothing similar to the normal smoothing we added for DC — this would reduce the noisy per-vertex color variation.

## Files Changed

| File | Change |
|------|--------|
| [`crates/fractal-core/src/mesh/mod.rs`](crates/fractal-core/src/mesh/mod.rs) | Add `ExportMaterial` struct |
| [`crates/fractal-core/src/mesh/gltf_export.rs`](crates/fractal-core/src/mesh/gltf_export.rs) | Add material to glTF output, update `export_glb` signature |
| [`crates/fractal-app/src/app.rs`](crates/fractal-app/src/app.rs) | Construct `ExportMaterial` from lighting/color config, pass to export |

No new dependencies needed — `gltf-json` already has full material support.
