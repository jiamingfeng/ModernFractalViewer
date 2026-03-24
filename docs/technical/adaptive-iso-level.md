# Adaptive Iso-Level

## Problem

Signed Distance Field (SDF) mesh extraction works by finding the **iso-surface** -- the set of points where `SDF(p) = iso_level`. For a standard SDF, `iso_level = 0` defines the exact surface boundary.

At high resolutions (e.g., 512 cells per axis), voxels are small enough to capture fine details. But at coarse resolutions (e.g., 64 or 128), thin features can fall entirely *between* grid sample points. When every sample on both sides of a thin arm reports a small positive distance, the marching cubes algorithm sees no sign change and produces no triangles. The feature vanishes.

This is especially problematic for fractal SDFs, where structures like Mandelbulb tendrils or Sierpinski filaments can be thinner than a single voxel at moderate resolutions.

## Solution

Instead of using a fixed `iso_level = 0`, we scale the iso-level proportionally to the voxel size:

```
iso_level = factor * voxel_diagonal
```

where:
- **`factor`** is a user-configurable multiplier (default 0.1)
- **`voxel_diagonal`** is the 3D diagonal of one voxel cell

By "inflating" the iso-surface outward, we ensure that thin features -- even those narrower than a voxel -- still produce sign changes in neighboring samples. The mesh extraction algorithm then captures them as slightly thickened geometry rather than missing them entirely.

## The Math

Given a grid of `N` cells per axis over a bounding box from `bounds_min` to `bounds_max`:

```
voxel_size[i] = (bounds_max[i] - bounds_min[i]) / N

voxel_diagonal = sqrt(voxel_size[0]^2 + voxel_size[1]^2 + voxel_size[2]^2)

effective_iso = factor * voxel_diagonal
```

For a cubic bounding box of side length `L`:

```
voxel_size = L / N
voxel_diagonal = voxel_size * sqrt(3)
effective_iso = factor * L * sqrt(3) / N
```

### Scaling Table

For a 300 cm bounding box ([-150, 150] cm, the Mandelbulb default) with `factor = 0.1`:

| Resolution | Voxel Size (cm) | Voxel Diagonal (cm) | Effective Iso (cm) |
|------------|-----------------|---------------------|-------------------|
| 64         | 4.69            | 8.12                | 0.812             |
| 128        | 2.34            | 4.06                | 0.406             |
| 256        | 1.17            | 2.03                | 0.203             |
| 512        | 0.586           | 1.01                | 0.101             |

At resolution 64, the iso-surface inflates by ~0.8 cm -- enough to catch features that would otherwise vanish. At resolution 512, the inflation is only ~0.1 cm -- negligible on a 300 cm model.

## Code Walkthrough

The adaptive iso-level computation lives in the `start_export()` method:

**File:** `crates/fractal-app/src/app.rs`

```rust
// Compute effective iso-level (adaptive scales with voxel size)
let effective_iso = if config.adaptive_iso {
    let voxel_sizes = [
        (sdf_bounds_max[0] - sdf_bounds_min[0]) / config.resolution as f32,
        (sdf_bounds_max[1] - sdf_bounds_min[1]) / config.resolution as f32,
        (sdf_bounds_max[2] - sdf_bounds_min[2]) / config.resolution as f32,
    ];
    let voxel_diag = (voxel_sizes[0] * voxel_sizes[0]
        + voxel_sizes[1] * voxel_sizes[1]
        + voxel_sizes[2] * voxel_sizes[2])
    .sqrt();
    config.adaptive_iso_factor * voxel_diag
} else {
    config.iso_level
};
```

1. Compute per-axis voxel sizes from the bounding box and resolution
2. Compute the 3D diagonal via Euclidean distance
3. Multiply by the user-configurable factor
4. Fall back to the manual `iso_level` when adaptive mode is disabled

## Boundary Extension

Adaptive iso-level pairs with **boundary extension** to prevent edge clipping. When the iso-surface is inflated, geometry can extend slightly beyond the original sampling volume. Boundary extension compensates by expanding the sampling bounds:

```rust
if config.boundary_extension {
    for i in 0..3 {
        let voxel = (sdf_bounds_max[i] - sdf_bounds_min[i]) / config.resolution as f32;
        let ext = voxel + effective_iso;
        sdf_bounds_min[i] -= ext;
        sdf_bounds_max[i] += ext;
    }
}
```

Each axis is expanded by `voxel_size + effective_iso` on both sides:
- **One voxel** accounts for the standard MC cell boundary
- **Plus the iso-level** accounts for the inflated surface

## Configuration

The UI exposes two controls in the Export Mesh panel:

- **"Adaptive iso-level" checkbox** -- toggles between adaptive and manual modes (default: on)
- **"Factor" drag value** -- the multiplier `factor` in `[0.01, 0.5]` (default: 0.1)

When adaptive mode is off, the user can set `iso_level` directly via the "Iso level" drag value.

**File:** `crates/fractal-core/src/mesh/mod.rs` (`ExportConfig` struct)

```rust
pub adaptive_iso: bool,           // default: true
pub adaptive_iso_factor: f32,     // default: 0.1
pub iso_level: f32,               // default: 0.0 (used when adaptive is off)
pub boundary_extension: bool,     // default: true
```

## Tradeoffs

| Factor | Effect |
|--------|--------|
| Too low (< 0.05) | Thin features may still vanish at coarse resolutions |
| Sweet spot (0.08-0.15) | Good balance between feature capture and surface accuracy |
| Too high (> 0.3) | Surface inflates visibly; fine concavities fill in |

The default factor of 0.1 works well across fractal types. Users working with very thin structures (e.g., Apollonian gaskets) may benefit from increasing it to 0.15-0.2.
