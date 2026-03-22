//! Marching cubes mesh extraction from SDF volume data.
//!
//! Takes a 3D grid of SDF samples (distance + trap value) and produces
//! a triangle mesh representing the iso-surface.

use super::mc_tables::{CORNER_OFFSETS, EDGE_TABLE, EDGE_VERTICES, TRI_TABLE};
use super::MeshData;

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

/// Linearly interpolate the surface crossing point along an edge.
///
/// Returns `(position, trap)` at the iso-surface crossing.
/// Handles NaN/Inf in grid values by falling back to the midpoint.
#[inline]
fn interpolate_vertex(
    iso: f32,
    p0: [f32; 3],
    p1: [f32; 3],
    v0: [f32; 2], // [distance, trap]
    v1: [f32; 2],
) -> ([f32; 3], f32) {
    let d0 = sanitize(v0[0], iso);
    let d1 = sanitize(v1[0], iso);
    let denom = d1 - d0;
    // Use !(...) pattern so NaN falls through to the else (midpoint) branch
    let t = if !(denom.abs() < 1e-10) {
        ((iso - d0) / denom).clamp(0.0, 1.0)
    } else {
        0.5
    };
    let pos = [
        p0[0] + t * (p1[0] - p0[0]),
        p0[1] + t * (p1[1] - p0[1]),
        p0[2] + t * (p1[2] - p0[2]),
    ];
    let trap = sanitize(
        sanitize(v0[1], 0.0) + t * (sanitize(v1[1], 0.0) - sanitize(v0[1], 0.0)),
        0.0,
    );
    (pos, trap)
}

/// Extract a triangle mesh from a 3D SDF volume using Marching Cubes.
///
/// # Arguments
/// * `grid` - Flat array of `[distance, trap]` pairs, length = `(dims[0]+1)*(dims[1]+1)*(dims[2]+1)`
/// * `dims` - Number of *cells* per axis `[nx, ny, nz]`; vertex count per axis is `dims[i]+1`
/// * `bounds_min` - World-space minimum corner of the sampling volume
/// * `bounds_max` - World-space maximum corner of the sampling volume
/// * `iso_level` - Iso-value for surface extraction (0.0 for standard SDFs)
/// * `compute_normals` - If true, estimate normals from SDF gradient via central differences
/// * `progress` - Optional callback receiving progress in `[0.0, 1.0]`
pub fn extract_mesh(
    grid: &[[f32; 2]],
    dims: [u32; 3],
    bounds_min: [f32; 3],
    bounds_max: [f32; 3],
    iso_level: f32,
    compute_normals: bool,
    progress: Option<&dyn Fn(f32)>,
) -> MeshData {
    // Vertex counts per axis
    let vx = dims[0] + 1;
    let vy = dims[1] + 1;
    let vz = dims[2] + 1;
    let expected = (vx as usize) * (vy as usize) * (vz as usize);

    // Validate inputs
    if dims[0] == 0 || dims[1] == 0 || dims[2] == 0 || grid.len() < expected {
        if grid.len() < expected && expected > 0 {
            eprintln!(
                "marching_cubes: grid has {} samples but expected {} for dims {:?}",
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

    // Cell sizes
    let dx = (bounds_max[0] - bounds_min[0]) / dims[0] as f32;
    let dy = (bounds_max[1] - bounds_min[1]) / dims[1] as f32;
    let dz = (bounds_max[2] - bounds_min[2]) / dims[2] as f32;

    // Pre-allocate output vectors (rough estimate: ~2 triangles per surface cell)
    let estimated_tris = (dims[0] as usize) * (dims[1] as usize) * 2;
    let mut positions: Vec<[f32; 3]> = Vec::with_capacity(estimated_tris * 3);
    let mut colors: Vec<[f32; 4]> = Vec::with_capacity(estimated_tris * 3);
    let mut indices: Vec<u32> = Vec::with_capacity(estimated_tris * 3);

    // Iterate over all cells
    for cz in 0..dims[2] {
        // Report progress per z-slice
        if let Some(cb) = &progress {
            cb(cz as f32 / dims[2] as f32);
        }

        for cy in 0..dims[1] {
            for cx in 0..dims[0] {
                // Read 8 corner values and compute cube_index
                let mut corner_vals = [[0.0f32; 2]; 8];
                let mut corner_pos = [[0.0f32; 3]; 8];
                let mut cube_index: u8 = 0;

                for (i, offset) in CORNER_OFFSETS.iter().enumerate() {
                    let gx = cx + offset[0];
                    let gy = cy + offset[1];
                    let gz = cz + offset[2];
                    let idx = grid_index(gx, gy, gz, vx, vy);
                    let raw = grid[idx];
                    // Sanitize NaN/Inf from GPU data: treat non-finite distance
                    // as "far outside" (positive) so it won't create spurious
                    // surface crossings.
                    corner_vals[i] = [
                        sanitize(raw[0], iso_level + 1.0),
                        sanitize(raw[1], 0.0),
                    ];
                    corner_pos[i] = [
                        bounds_min[0] + gx as f32 * dx,
                        bounds_min[1] + gy as f32 * dy,
                        bounds_min[2] + gz as f32 * dz,
                    ];
                    if corner_vals[i][0] < iso_level {
                        cube_index |= 1 << i;
                    }
                }

                // Skip if no edges intersected
                let edge_flags = EDGE_TABLE[cube_index as usize];
                if edge_flags == 0 {
                    continue;
                }

                // Compute interpolated vertices for intersected edges
                let mut edge_verts = [([0.0f32; 3], 0.0f32); 12];
                for e in 0..12 {
                    if edge_flags & (1 << e) != 0 {
                        let [c0, c1] = EDGE_VERTICES[e];
                        edge_verts[e] = interpolate_vertex(
                            iso_level,
                            corner_pos[c0],
                            corner_pos[c1],
                            corner_vals[c0],
                            corner_vals[c1],
                        );
                    }
                }

                // Emit triangles from TRI_TABLE
                let tri_row = &TRI_TABLE[cube_index as usize];
                let mut t = 0;
                while t < 16 {
                    if tri_row[t] < 0 {
                        break;
                    }
                    let base_idx = positions.len() as u32;

                    for k in 0..3 {
                        let edge_idx = tri_row[t + k] as usize;
                        let (pos, trap) = edge_verts[edge_idx];
                        positions.push(pos);
                        colors.push([trap, 0.0, 0.0, 0.0]);
                    }

                    // Wind triangles consistently
                    indices.push(base_idx);
                    indices.push(base_idx + 1);
                    indices.push(base_idx + 2);

                    t += 3;
                }
            }
        }
    }

    // Compute normals
    let normals = if compute_normals {
        compute_gradient_normals(&positions, grid, dims, bounds_min, dx, dy, dz, vx, vy)
    } else {
        compute_face_normals(&positions, &indices)
    };

    if let Some(cb) = &progress {
        cb(1.0);
    }

    MeshData {
        positions,
        normals,
        colors,
        indices,
    }
}

/// Compute smooth normals from SDF gradient via central differences on the grid.
///
/// For each vertex, find the nearest grid point and sample 6 neighbours to
/// estimate the gradient direction, then normalize.
fn compute_gradient_normals(
    positions: &[[f32; 3]],
    grid: &[[f32; 2]],
    dims: [u32; 3],
    bounds_min: [f32; 3],
    dx: f32,
    dy: f32,
    dz: f32,
    vx: u32,
    vy: u32,
) -> Vec<[f32; 3]> {
    let inv_dx = 1.0 / dx;
    let inv_dy = 1.0 / dy;
    let inv_dz = 1.0 / dz;
    let max_gx = dims[0]; // max valid grid x index = vx - 1 = dims[0]
    let max_gy = dims[1];
    let max_gz = dims[2];

    positions
        .iter()
        .map(|pos| {
            // Map world position to continuous grid coordinates
            let fx = ((pos[0] - bounds_min[0]) * inv_dx).clamp(0.0, max_gx as f32);
            let fy = ((pos[1] - bounds_min[1]) * inv_dy).clamp(0.0, max_gy as f32);
            let fz = ((pos[2] - bounds_min[2]) * inv_dz).clamp(0.0, max_gz as f32);

            // Nearest grid point
            let gx = (fx.round() as u32).min(max_gx);
            let gy = (fy.round() as u32).min(max_gy);
            let gz = (fz.round() as u32).min(max_gz);

            // Central differences with boundary clamping
            let xm = if gx > 0 { gx - 1 } else { gx };
            let xp = if gx < max_gx { gx + 1 } else { gx };
            let ym = if gy > 0 { gy - 1 } else { gy };
            let yp = if gy < max_gy { gy + 1 } else { gy };
            let zm = if gz > 0 { gz - 1 } else { gz };
            let zp = if gz < max_gz { gz + 1 } else { gz };

            let s = |idx: usize| sanitize(grid[idx][0], 0.0);
            let dfdx = s(grid_index(xp, gy, gz, vx, vy))
                - s(grid_index(xm, gy, gz, vx, vy));
            let dfdy = s(grid_index(gx, yp, gz, vx, vy))
                - s(grid_index(gx, ym, gz, vx, vy));
            let dfdz = s(grid_index(gx, gy, zp, vx, vy))
                - s(grid_index(gx, gy, zm, vx, vy));

            // Normalize — use !(...) so NaN falls to fallback
            let len = (dfdx * dfdx + dfdy * dfdy + dfdz * dfdz).sqrt();
            if !(len <= 1e-10) && len.is_finite() {
                [dfdx / len, dfdy / len, dfdz / len]
            } else {
                [0.0, 1.0, 0.0] // fallback up vector
            }
        })
        .collect()
}

/// Compute flat face normals from triangle cross products.
fn compute_face_normals(positions: &[[f32; 3]], indices: &[u32]) -> Vec<[f32; 3]> {
    let mut normals = vec![[0.0f32; 3]; positions.len()];

    for tri in indices.chunks_exact(3) {
        let i0 = tri[0] as usize;
        let i1 = tri[1] as usize;
        let i2 = tri[2] as usize;

        let p0 = positions[i0];
        let p1 = positions[i1];
        let p2 = positions[i2];

        // Edge vectors
        let e1 = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
        let e2 = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];

        // Cross product
        let nx = e1[1] * e2[2] - e1[2] * e2[1];
        let ny = e1[2] * e2[0] - e1[0] * e2[2];
        let nz = e1[0] * e2[1] - e1[1] * e2[0];

        let len = (nx * nx + ny * ny + nz * nz).sqrt();
        let n = if len > 1e-10 {
            [nx / len, ny / len, nz / len]
        } else {
            [0.0, 1.0, 0.0]
        };

        // Assign same face normal to all three vertices
        normals[i0] = n;
        normals[i1] = n;
        normals[i2] = n;
    }

    normals
}

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
        let mesh = extract_mesh(
            &grid,
            [res, res, res],
            [-1.0; 3],
            [1.0; 3],
            0.0,
            true,
            None,
        );

        // Mesh should be non-empty
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

        // All positions should be within bounds (with some tolerance for interpolation)
        for pos in &mesh.positions {
            for c in 0..3 {
                assert!(
                    pos[c] >= -1.1 && pos[c] <= 1.1,
                    "position component {:.3} out of bounds",
                    pos[c]
                );
            }
        }

        // Normals should be approximately unit length
        for n in &mesh.normals {
            let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            assert!(
                (len - 1.0).abs() < 0.01,
                "normal length {:.4} not unit",
                len
            );
        }
    }

    #[test]
    fn all_positive_produces_empty() {
        // All values > 0 (fully outside surface)
        let grid = vec![[1.0f32, 0.0]; 27];
        let mesh = extract_mesh(&grid, [2, 2, 2], [-1.0; 3], [1.0; 3], 0.0, true, None);
        assert!(mesh.positions.is_empty());
        assert!(mesh.indices.is_empty());
    }

    #[test]
    fn all_negative_produces_empty() {
        // All values < 0 (fully inside surface)
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
    fn trap_values_interpolated() {
        let res = 16;
        let grid = make_sphere_grid(res, [-1.0; 3], [1.0; 3], 0.5);
        let mesh = extract_mesh(
            &grid,
            [res, res, res],
            [-1.0; 3],
            [1.0; 3],
            0.0,
            true,
            None,
        );

        // At the surface, trap = abs(distance) should be close to 0
        // since we're extracting at iso=0
        let mut max_trap = 0.0f32;
        for color in &mesh.colors {
            max_trap = max_trap.max(color[0]);
        }
        // Cell diagonal at res=16: sqrt(3) * (2.0/16) ≈ 0.217
        // Trap values should be small but not necessarily zero due to grid resolution
        assert!(
            max_trap < 0.25,
            "max trap value {:.4} too large (expected near 0 at surface)",
            max_trap
        );
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
        let _ = extract_mesh(
            &grid,
            [res, res, res],
            [-1.0; 3],
            [1.0; 3],
            0.0,
            true,
            Some(&cb),
        );

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

        // Check monotonicity
        for w in vals.windows(2) {
            assert!(
                w[1] >= w[0],
                "progress not monotonic: {} -> {}",
                w[0],
                w[1]
            );
        }
    }

    #[test]
    fn normals_point_outward() {
        let res = 16;
        let grid = make_sphere_grid(res, [-1.0; 3], [1.0; 3], 0.5);
        let mesh = extract_mesh(
            &grid,
            [res, res, res],
            [-1.0; 3],
            [1.0; 3],
            0.0,
            true,
            None,
        );

        // For a sphere SDF centred at origin, normals should point away from
        // origin: dot(normal, position) > 0 for most vertices.
        let mut outward_count = 0;
        let total = mesh.positions.len();
        for i in 0..total {
            let p = mesh.positions[i];
            let n = mesh.normals[i];
            let dot = p[0] * n[0] + p[1] * n[1] + p[2] * n[2];
            if dot > 0.0 {
                outward_count += 1;
            }
        }
        let ratio = outward_count as f32 / total as f32;
        assert!(
            ratio > 0.90,
            "only {:.1}% of normals point outward (expected >90%)",
            ratio * 100.0
        );
    }

    #[test]
    fn no_nan_in_output() {
        let res = 16;
        let grid = make_sphere_grid(res, [-1.0; 3], [1.0; 3], 0.5);
        let mesh = extract_mesh(
            &grid,
            [res, res, res],
            [-1.0; 3],
            [1.0; 3],
            0.0,
            true,
            None,
        );

        for (i, pos) in mesh.positions.iter().enumerate() {
            assert!(
                pos[0].is_finite() && pos[1].is_finite() && pos[2].is_finite(),
                "NaN/Inf in position[{}]: {:?}",
                i,
                pos
            );
        }
        for (i, n) in mesh.normals.iter().enumerate() {
            assert!(
                n[0].is_finite() && n[1].is_finite() && n[2].is_finite(),
                "NaN/Inf in normal[{}]: {:?}",
                i,
                n
            );
        }
        for (i, c) in mesh.colors.iter().enumerate() {
            assert!(
                c[0].is_finite() && c[1].is_finite() && c[2].is_finite() && c[3].is_finite(),
                "NaN/Inf in color[{}]: {:?}",
                i,
                c
            );
        }
    }

    #[test]
    fn nan_in_grid_produces_no_nan_output() {
        // Start with a valid sphere grid, then inject NaN at some points
        let res = 8;
        let mut grid = make_sphere_grid(res, [-1.0; 3], [1.0; 3], 0.5);

        // Inject NaN at ~10% of grid points
        for i in (0..grid.len()).step_by(10) {
            grid[i] = [f32::NAN, f32::NAN];
        }
        // Also inject Inf
        for i in (5..grid.len()).step_by(17) {
            grid[i] = [f32::INFINITY, f32::NEG_INFINITY];
        }

        let mesh = extract_mesh(
            &grid,
            [res, res, res],
            [-1.0; 3],
            [1.0; 3],
            0.0,
            true,
            None,
        );

        // Verify no NaN/Inf in any output
        for (i, pos) in mesh.positions.iter().enumerate() {
            assert!(
                pos[0].is_finite() && pos[1].is_finite() && pos[2].is_finite(),
                "NaN/Inf in position[{}]: {:?}",
                i,
                pos
            );
        }
        for (i, n) in mesh.normals.iter().enumerate() {
            assert!(
                n[0].is_finite() && n[1].is_finite() && n[2].is_finite(),
                "NaN/Inf in normal[{}]: {:?}",
                i,
                n
            );
        }
        for (i, c) in mesh.colors.iter().enumerate() {
            assert!(
                c[0].is_finite() && c[1].is_finite() && c[2].is_finite() && c[3].is_finite(),
                "NaN/Inf in color[{}]: {:?}",
                i,
                c
            );
        }
    }
}
