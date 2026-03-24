---
name: add-uniform
description: Guided workflow for adding a new GPU uniform field (Rust + WGSL)
---

# /add-uniform — Add a New GPU Uniform Field

Guided workflow for the error-prone task of adding a new field to the GPU uniform buffer. This requires synchronized changes in both Rust and WGSL with correct byte alignment.

## Usage

```
/add-uniform [field name, type, and purpose]
```

## Instructions

Before starting, read both files to understand the current layout:
- `crates/fractal-renderer/src/uniforms.rs` — Rust `Uniforms` struct with byte offset comments
- `crates/fractal-renderer/shaders/raymarcher.wgsl` — matching WGSL struct at binding 0, group 0

### Step 1: Determine Field Type and Alignment

Choose the field type and note its alignment:
- `f32` → 4 bytes, 4-byte aligned
- `u32` / `i32` → 4 bytes, 4-byte aligned
- `vec2<f32>` / `[f32; 2]` → 8 bytes, 8-byte aligned
- `vec4<f32>` / `[f32; 4]` → 16 bytes, 16-byte aligned

**CRITICAL: Never use `vec3` or `[f32; 3]`** — WGSL `vec3` occupies 16 bytes but Rust `[f32; 3]` occupies 12 bytes, causing layout mismatch. Use individual `f32` fields with explicit `_padN` padding instead.

### Step 2: Add Field to Rust Struct

In `crates/fractal-renderer/src/uniforms.rs`:
1. Add the field at the end of the struct (before any trailing padding)
2. Add explicit `_padN: f32` fields if needed for alignment
3. Add a byte offset comment matching the existing style: `// offset: NNN`
4. Ensure the total struct size remains exactly 512 bytes (or update the compile-time assertion)

### Step 3: Add Field to WGSL Struct

In `crates/fractal-renderer/shaders/raymarcher.wgsl`:
1. Add the matching field to the uniform struct at the same position
2. Use the corresponding WGSL type (`f32`, `u32`, `vec2<f32>`, `vec4<f32>`)
3. Add padding fields if needed to match the Rust layout

### Step 4: Update the Appropriate `update_*()` Method

In `crates/fractal-renderer/src/uniforms.rs`:
- Add the field assignment in the relevant `update_*()` method (e.g., `update_fractal()`, `update_camera()`, `update_ray_march()`, `update_color()`, `update_lighting()`)
- The source data comes from the corresponding config struct in `fractal-core`

### Step 5: Update Source Config Struct (if needed)

If the uniform maps to a new user-facing parameter, add the field to the appropriate config struct in `fractal-core/src/`:
- `FractalParams` for fractal parameters
- `RayMarchConfig` for rendering parameters
- `ColorConfig` for color/palette parameters
- `LightingConfig` for lighting parameters
- Use `#[serde(default)]` for backward compatibility

### Step 6: Verify

1. Run `cargo test -p fractal-renderer` — the compile-time size assertion will catch layout errors
2. Run `cargo check --workspace` — verify everything compiles
3. If adding a visual parameter, run the app and verify the shader reads the value correctly

## Alignment Quick Reference

| Offset mod | Next `f32` | Next `vec2` | Next `vec4` |
|------------|-----------|-------------|-------------|
| 0          | OK        | OK          | OK          |
| 4          | OK        | pad 4       | pad 12      |
| 8          | OK        | OK          | pad 8       |
| 12         | OK        | pad 4       | pad 4       |

## Notes

- The palette array (8 x `vec4<f32>` = 128 bytes) takes a large chunk of the 512-byte buffer
- Current byte offsets are documented as comments in `uniforms.rs` — always update these
- If 512 bytes is insufficient, the size can be increased but both the Rust assertion and any GPU buffer allocation code must be updated
