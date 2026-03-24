---
name: add-fractal
description: Guided implementation of a new 3D fractal type
---

# /add-fractal — Add a New Fractal Type

Step-by-step guided implementation of a new 3D fractal type.

## Usage

```
/add-fractal [fractal name and description]
```

## Instructions

Before starting, read the existing implementations to understand patterns:
- `crates/fractal-core/src/fractals/mod.rs` — `FractalType` enum, `FractalParams` struct
- `crates/fractal-renderer/shaders/raymarcher.wgsl` — existing SDF functions
- `crates/fractal-renderer/shaders/sdf_common.wgsl` — shared SDF utilities
- `crates/fractal-ui/src/panels/fractal_params.rs` — per-type UI controls
- `crates/fractal-ui/src/app_settings.rs` — range definitions
- `crates/fractal-ui/src/default_app_settings.toml` — default range values

Then implement in this order:

### Step 1: Add Enum Variant

In `crates/fractal-core/src/fractals/mod.rs`:
- Add a new variant to the `FractalType` enum
- Follow the existing numbering pattern for serde serialization

### Step 2: Implement SDF in WGSL

In `crates/fractal-renderer/shaders/raymarcher.wgsl`:
- Add a new `sdf_<name>()` function following existing SDF function patterns
- Use `effective_iterations` (not `u.iterations`) for LOD compatibility
- Add the case to the `sdf()` dispatch function
- If utilities are reusable, add them to `sdf_common.wgsl` instead

Also add the SDF to `crates/fractal-renderer/shaders/sdf_volume.wgsl` for mesh export support.

### Step 3: Add UI Controls

In `crates/fractal-ui/src/panels/fractal_params.rs`:
- Add a match arm for the new fractal type
- Add sliders/controls for any fractal-specific parameters
- Read range values from `AppSettings`

### Step 4: Add Range Definitions

In `crates/fractal-ui/src/app_settings.rs`:
- Add range structs for the new fractal's parameters
- Add the ranges to the `AppSettings` struct hierarchy

In `crates/fractal-ui/src/default_app_settings.toml`:
- Add default min/max/speed/decimals for each parameter

### Step 5: Document

In `docs/DEVELOPMENT_GUIDE.md`, update the Features → Fractal Types section:
- Add the new type to the list with description
- Note any unique parameters or behaviors

### Step 6: Add Tests

- Add tests in `fractal-core` for any new math/parameter logic
- Tests should verify constraints, valid ranges, and round-trip serialization
- Do NOT write tests that only assert default values

### Step 7: Verify

Run `/check-all` to verify cross-compilation, then `/test` to run the test suite.

## Conventions

- All `FractalParams` fields use `#[serde(default)]` for backward compatibility
- SDF functions should be numerically stable and return valid distance estimates
- Use Inigo Quilez's SDF techniques (https://iquilezles.org/articles) as reference
- The uniform buffer is 512 bytes — verify the size assertion still passes after adding fields
