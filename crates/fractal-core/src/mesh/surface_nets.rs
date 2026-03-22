//! Surface Nets mesh extraction using the `fast_surface_nets` crate.
//!
//! This module bridges the external crate's API with our [`MeshData`] format,
//! using the same GPU-readback grid as Marching Cubes and Dual Contouring.
//!
//! Surface Nets produces inherently smooth meshes by placing vertices at the
//! average of edge crossing points within each cell, acting as a natural
//! low-pass filter for high-frequency SDF noise — ideal for fractal surfaces.

use super::MeshData;
use fast_surface_nets::ndshape::RuntimeShape;
use fast_surface_nets::{surface_nets, SurfaceNetsBuffer};

// ── Helpers ──────────────────────────────────────────────────────────────

/// Replace NaN/Inf with a fallback value.
#[inline]
fn sanitize(v: f32, fallback: f32) -> f32 {
    if v.is_finite() { v } else { fallback }
}

/// Flat grid index matching the GPU compute shader layout:
/// `index = x + y * vx + z * vx * vy`
#[inline]
fn grid_index(x: u32, y: u32, z: u32, vx: u32, vy: u32) -> usize {
    (x + y * vx + z * vx * vy) as usize
}

// ── Main entry point ─────────────────────────────────────────────────────

/// Extract a triangle mesh from a 3D SDF volume using Surface Nets.
///
/// The function signature matches [`super::marching_cubes::extract_mesh`] and
/// [`super::dual_contouring::extract_mesh`] for drop-in interchangeability.
///
/// # Arguments
/// * `grid` - Flat array of `[distance, trap]` pairs from GPU readback
/// * `dims` - Number of *cells* per axis (vertex count = dims + 1)
/// * `bounds_min` / `bounds_max` - World-space bounding box
/// * `iso_level` - Iso-value for surface extraction (typically 0.0 for SDFs)
/// * `compute_normals` - If true, normalize the SDF-gradient normals from the
///   crate; if false, compute area-weighted face normals instead
/// * `progress` - Optional callback receiving `[0.0, 1.0]`
pub fn extract_mesh(
    grid: &[[f32; 2]],
    dims: [u32; 3],
    bounds_min: [f32; 3],
    bounds_max: [f32; 3],
    iso_level: f32,
    compute_normals: bool,
    progress: Option<&dyn Fn(f32)>,
) -> MeshData {
    let vx = dims[0] + 1;
    let vy = dims[1] + 1;
    let vz = dims[2] + 1;
    let expected = (vx as usize) * (vy as usize) * (vz as usize);

    // Validate inputs
    if dims[0] == 0 || dims[1] == 0 || dims[2] == 0 || grid.len() < expected {
        if grid.len() < expected && expected > 0 {
            eprintln!(
                "surface_nets: grid has {} samples but expected {} for dims {:?}",
                grid.len(),
                expected,
                dims
            );
        }
        if let Some(cb) = &progress {
            cb(0.0);
            cb(1.0);
        }
        return MeshData {
            positions: Vec::new(),
            normals: Vec::new(),
            colors: Vec::new(),
            indices: Vec::new(),
        };
    }

    if let Some(cb) = &progress {
        cb(0.0);
    }

    // ── Phase 1: Build SDF-only array (strip trap values) ────────────
    // The fast_surface_nets crate expects a flat `[f32]` array of signed
    // distances.  Our grid is `[distance, trap]` pairs, so we extract
    // just the distance channel.
    //
    // We also shift values by iso_level so the crate treats the zero
    // crossing as the surface (the crate uses 0.0 as the isosurface).
    let sdf: Vec<f32> = grid[..expected]
        .iter()
        .map(|&[d, _]| sanitize(d, 1.0) - iso_level)
        .collect();

    if let Some(cb) = &progress {
        cb(0.2);
    }

    // ── Phase 2: Run Surface Nets ────────────────────────────────────
    // The ndshape RuntimeShape linearizes as: x + y * sx + z * sx * sy
    // which matches our GPU layout exactly.
    let shape = RuntimeShape::<u32, 3>::new([vx, vy, vz]);
    let mut buffer = SurfaceNetsBuffer::default();

    // min = [0, 0, 0], max = [vx-1, vy-1, vz-1] (inclusive corners)
    surface_nets(
        &sdf,
        &shape,
        [0, 0, 0],
        [vx - 1, vy - 1, vz - 1],
        &mut buffer,
    );

    if let Some(cb) = &progress {
        cb(0.6);
    }

    // ── Phase 3: Transform positions from grid-space to world-space ──
    let dx = (bounds_max[0] - bounds_min[0]) / dims[0] as f32;
    let dy = (bounds_max[1] - bounds_min[1]) / dims[1] as f32;
    let dz = (bounds_max[2] - bounds_min[2]) / dims[2] as f32;

    let positions: Vec<[f32; 3]> = buffer
        .positions
        .iter()
        .map(|&[gx, gy, gz]| {
            [
                bounds_min[0] + gx * dx,
                bounds_min[1] + gy * dy,
                bounds_min[2] + gz * dz,
            ]
        })
        .collect();

    // ── Phase 4: Compute normals ─────────────────────────────────────
    let normals = if compute_normals {
        // The crate provides SDF-gradient normals via bilinear interpolation
        // of central differences — already high quality. We just need to
        // normalize them (the crate explicitly leaves them unnormalized).
        buffer
            .normals
            .iter()
            .map(|n| {
                let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
                if len > 1e-10 && len.is_finite() {
                    [n[0] / len, n[1] / len, n[2] / len]
                } else {
                    [0.0, 1.0, 0.0]
                }
            })
            .collect()
    } else {
        compute_face_normals(&positions, &buffer.indices)
    };

    if let Some(cb) = &progress {
        cb(0.8);
    }

    // ── Phase 5: Interpolate trap values for coloring ────────────────
    // For each vertex, find its grid-space position and trilinearly
    // interpolate the trap value from the 8 surrounding grid corners.
    let colors: Vec<[f32; 4]> = buffer
        .positions
        .iter()
        .map(|&[gx, gy, gz]| {
            let trap = interpolate_trap(grid, gx, gy, gz, vx, vy, vz);
            [sanitize(trap, 0.0), 0.0, 0.0, 0.0]
        })
        .collect();

    if let Some(cb) = &progress {
        cb(1.0);
    }

    MeshData {
        positions,
        normals,
        colors,
        indices: buffer.indices,
    }
}

// ── Trap value interpolation ─────────────────────────────────────────────

/// Trilinearly interpolate the trap value at a continuous grid-space position.
fn interpolate_trap(
    grid: &[[f32; 2]],
    gx: f32,
    gy: f32,
    gz: f32,
    vx: u32,
    vy: u32,
    vz: u32,
) -> f32 {
    let max_x = (vx - 1) as f32;
    let max_y = (vy - 1) as f32;
    let max_z = (vz - 1) as f32;

    let fx = gx.clamp(0.0, max_x);
    let fy = gy.clamp(0.0, max_y);
    let fz = gz.clamp(0.0, max_z);

    let x0 = (fx.floor() as u32).min(vx - 2);
    let y0 = (fy.floor() as u32).min(vy - 2);
    let z0 = (fz.floor() as u32).min(vz - 2);

    let tx = fx - x0 as f32;
    let ty = fy - y0 as f32;
    let tz = fz - z0 as f32;

    // Sample trap values at 8 corners
    let t = |x: u32, y: u32, z: u32| -> f32 {
        let idx = grid_index(x, y, z, vx, vy);
        if idx < grid.len() {
            sanitize(grid[idx][1], 0.0)
        } else {
            0.0
        }
    };

    let c000 = t(x0, y0, z0);
    let c100 = t(x0 + 1, y0, z0);
    let c010 = t(x0, y0 + 1, z0);
    let c110 = t(x0 + 1, y0 + 1, z0);
    let c001 = t(x0, y0, z0 + 1);
    let c101 = t(x0 + 1, y0, z0 + 1);
    let c011 = t(x0, y0 + 1, z0 + 1);
    let c111 = t(x0 + 1, y0 + 1, z0 + 1);

    // Trilinear interpolation
    let c00 = c000 * (1.0 - tx) + c100 * tx;
    let c10 = c010 * (1.0 - tx) + c110 * tx;
    let c01 = c001 * (1.0 - tx) + c101 * tx;
    let c11 = c011 * (1.0 - tx) + c111 * tx;

    let c0 = c00 * (1.0 - ty) + c10 * ty;
    let c1 = c01 * (1.0 - ty) + c11 * ty;

    c0 * (1.0 - tz) + c1 * tz
}

// ── Normal computation ───────────────────────────────────────────────────

/// Compute face normals from triangle cross products, accumulated per vertex.
fn compute_face_normals(positions: &[[f32; 3]], indices: &[u32]) -> Vec<[f32; 3]> {
    let mut normals = vec![[0.0f32; 3]; positions.len()];

    for tri in indices.chunks_exact(3) {
        let i0 = tri[0] as usize;
        let i1 = tri[1] as usize;
        let i2 = tri[2] as usize;

        if i0 >= positions.len() || i1 >= positions.len() || i2 >= positions.len() {
            continue;
        }

        let p0 = positions[i0];
        let p1 = positions[i1];
        let p2 = positions[i2];

        let e1 = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
        let e2 = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];

        // Cross product (area-weighted normal)
        let nx = e1[1] * e2[2] - e1[2] * e2[1];
        let ny = e1[2] * e2[0] - e1[0] * e2[2];
        let nz = e1[0] * e2[1] - e1[1] * e2[0];

        for &idx in &[i0, i1, i2] {
            normals[idx][0] += nx;
            normals[idx][1] += ny;
            normals[idx][2] += nz;
        }
    }

    // Normalise
    for n in &mut normals {
        let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        if len > 1e-10 {
            n[0] /= len;
            n[1] /= len;
            n[2] /= len;
        } else {
            *n = [0.0, 1.0, 0.0];
        }
    }

    normals
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Create a sphere SDF grid: `|p| - radius` with trap = abs(distance).
    fn make_sphere_grid(
        resolution: u32,
        bounds_min: [f32; 3],
        bounds_max: [f32; 3],
        radius: f32,
    ) -> Vec<[f32; 2]> {
        let vx = resolution + 1;
        let vy = resolution + 1;
        let vz = resolution + 1;
        let dx = (bounds_max[0] - bounds_min[0]) / resolution as f32;
        let dy = (bounds_max[1] - bounds_min[1]) / resolution as f32;
        let dz = (bounds_max[2] - bounds_min[2]) / resolution as f32;

        let mut grid = vec![[0.0f32; 2]; (vx * vy * vz) as usize];
        for gz in 0..vz {
            for gy in 0..vy {
                for gx in 0..vx {
                    let x = bounds_min[0] + gx as f32 * dx;
                    let y = bounds_min[1] + gy as f32 * dy;
                    let z = bounds_min[2] + gz as f32 * dz;
                    let dist = (x * x + y * y + z * z).sqrt() - radius;
                    let trap = dist.abs();
                    let idx = grid_index(gx, gy, gz, vx, vy);
                    grid[idx] = [dist, trap];
                }
            }
        }
        grid
    }

    #[test]
    fn sphere_sdf_produces_mesh() {
        let res = 16;
        let grid = make_sphere_grid(res, [-1.0; 3], [1.0; 3], 0.5);
        let mesh = extract_mesh(&grid, [res; 3], [-1.0; 3], [1.0; 3], 0.0, true, None);

        assert!(!mesh.positions.is_empty(), "positions should not be empty");
        assert!(!mesh.indices.is_empty(), "indices should not be empty");
        assert_eq!(mesh.normals.len(), mesh.positions.len());
        assert_eq!(mesh.colors.len(), mesh.positions.len());

        // All indices should be valid
        for &idx in &mesh.indices {
            assert!(
                (idx as usize) < mesh.positions.len(),
                "index {} out of bounds (len={})",
                idx,
                mesh.positions.len()
            );
        }
    }

    #[test]
    fn all_positive_produces_empty() {
        let grid = vec![[1.0f32, 0.0]; 27];
        let mesh = extract_mesh(&grid, [2, 2, 2], [-1.0; 3], [1.0; 3], 0.0, true, None);
        assert!(mesh.positions.is_empty());
        assert!(mesh.indices.is_empty());
    }

    #[test]
    fn all_negative_produces_empty() {
        let grid = vec![[-1.0f32, 0.5]; 27];
        let mesh = extract_mesh(&grid, [2, 2, 2], [-1.0; 3], [1.0; 3], 0.0, true, None);
        assert!(mesh.positions.is_empty());
        assert!(mesh.indices.is_empty());
    }

    #[test]
    fn empty_grid_returns_empty_mesh() {
        let mesh = extract_mesh(&[], [0, 0, 0], [-1.0; 3], [1.0; 3], 0.0, true, None);
        assert!(mesh.positions.is_empty());
        assert!(mesh.indices.is_empty());
    }

    #[test]
    fn normals_point_outward() {
        let res = 16;
        let grid = make_sphere_grid(res, [-1.0; 3], [1.0; 3], 0.5);
        let mesh = extract_mesh(&grid, [res; 3], [-1.0; 3], [1.0; 3], 0.0, true, None);

        let mut outward = 0;
        let total = mesh.positions.len();
        for i in 0..total {
            let p = mesh.positions[i];
            let n = mesh.normals[i];
            let dot = p[0] * n[0] + p[1] * n[1] + p[2] * n[2];
            if dot > 0.0 {
                outward += 1;
            }
        }
        let ratio = outward as f32 / total as f32;
        assert!(
            ratio > 0.80,
            "only {:.1}% of normals point outward (expected >80%)",
            ratio * 100.0
        );
    }

    #[test]
    fn no_nan_in_output() {
        let res = 16;
        let grid = make_sphere_grid(res, [-1.0; 3], [1.0; 3], 0.5);
        let mesh = extract_mesh(&grid, [res; 3], [-1.0; 3], [1.0; 3], 0.0, true, None);

        for (i, pos) in mesh.positions.iter().enumerate() {
            assert!(
                pos[0].is_finite() && pos[1].is_finite() && pos[2].is_finite(),
                "NaN/Inf in position[{i}]: {pos:?}"
            );
        }
        for (i, n) in mesh.normals.iter().enumerate() {
            assert!(
                n[0].is_finite() && n[1].is_finite() && n[2].is_finite(),
                "NaN/Inf in normal[{i}]: {n:?}"
            );
        }
    }

    #[test]
    fn vertices_inside_bounds() {
        let res = 12;
        let grid = make_sphere_grid(res, [-1.0; 3], [1.0; 3], 0.5);
        let mesh = extract_mesh(&grid, [res; 3], [-1.0; 3], [1.0; 3], 0.0, true, None);

        for (i, pos) in mesh.positions.iter().enumerate() {
            for c in 0..3 {
                assert!(
                    pos[c] >= -1.01 && pos[c] <= 1.01,
                    "position[{i}][{c}] = {:.4} out of bounds",
                    pos[c]
                );
            }
        }
    }

    #[test]
    fn progress_callback_monotonic() {
        use std::sync::Mutex;

        let values = Mutex::new(Vec::<f32>::new());
        let cb = |p: f32| {
            values.lock().unwrap().push(p);
        };

        let res = 8;
        let grid = make_sphere_grid(res, [-1.0; 3], [1.0; 3], 0.5);
        let _ = extract_mesh(&grid, [res; 3], [-1.0; 3], [1.0; 3], 0.0, true, Some(&cb));

        let vals = values.lock().unwrap();
        assert!(vals.len() >= 2, "should have at least start + end callbacks");
        assert!(
            (*vals.first().unwrap() - 0.0).abs() < 1e-6,
            "first progress should be 0.0"
        );
        assert!(
            (*vals.last().unwrap() - 1.0).abs() < 1e-6,
            "last progress should be 1.0"
        );

        for w in vals.windows(2) {
            assert!(w[1] >= w[0], "progress not monotonic: {} -> {}", w[0], w[1]);
        }
    }

    #[test]
    fn colors_have_trap_values() {
        let res = 12;
        let grid = make_sphere_grid(res, [-1.0; 3], [1.0; 3], 0.5);
        let mesh = extract_mesh(&grid, [res; 3], [-1.0; 3], [1.0; 3], 0.0, true, None);

        // At least some vertices should have non-zero trap values
        let non_zero = mesh.colors.iter().filter(|c| c[0].abs() > 1e-6).count();
        assert!(
            non_zero > 0,
            "expected some non-zero trap values in colors"
        );
    }
}
