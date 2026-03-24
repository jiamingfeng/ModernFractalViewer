//! Marching cubes mesh extraction from SDF volume data.
//!
//! Takes a 3D grid of SDF samples (distance + trap value) and produces
//! a triangle mesh representing the iso-surface.

use super::mc_tables::{CORNER_OFFSETS, EDGE_TABLE, EDGE_VERTICES, TRI_TABLE};
use super::MeshData;
use std::collections::HashMap;

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

/// Map a local edge index (0–11) within cell `(cx, cy, cz)` to a canonical
/// representation `(grid_vertex_x, grid_vertex_y, grid_vertex_z, axis)`.
///
/// Each edge is identified by the lower grid vertex it starts from and the
/// axis direction it runs along (0=X, 1=Y, 2=Z). This ensures that shared
/// edges between adjacent cells map to the same canonical key.
#[inline]
fn edge_canonical(cx: u32, cy: u32, cz: u32, edge: usize) -> (u32, u32, u32, u8) {
    // Edge definitions: (corner0, corner1) from EDGE_VERTICES.
    // Each edge runs along one axis from one grid vertex to another.
    // We identify it by the "lower" vertex and the axis.
    match edge {
        0  => (cx,     cy,     cz,     0), // edge 0→1, X-axis at (cx, cy, cz)
        1  => (cx + 1, cy,     cz,     1), // edge 1→2, Y-axis at (cx+1, cy, cz)
        2  => (cx,     cy + 1, cz,     0), // edge 3→2, X-axis at (cx, cy+1, cz)
        3  => (cx,     cy,     cz,     1), // edge 0→3, Y-axis at (cx, cy, cz)
        4  => (cx,     cy,     cz + 1, 0), // edge 4→5, X-axis at (cx, cy, cz+1)
        5  => (cx + 1, cy,     cz + 1, 1), // edge 5→6, Y-axis at (cx+1, cy, cz+1)
        6  => (cx,     cy + 1, cz + 1, 0), // edge 7→6, X-axis at (cx, cy+1, cz+1)
        7  => (cx,     cy,     cz + 1, 1), // edge 4→7, Y-axis at (cx, cy, cz+1)
        8  => (cx,     cy,     cz,     2), // edge 0→4, Z-axis at (cx, cy, cz)
        9  => (cx + 1, cy,     cz,     2), // edge 1→5, Z-axis at (cx+1, cy, cz)
        10 => (cx + 1, cy + 1, cz,     2), // edge 2→6, Z-axis at (cx+1, cy+1, cz)
        11 => (cx,     cy + 1, cz,     2), // edge 3→7, Z-axis at (cx, cy+1, cz)
        _  => unreachable!(),
    }
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

    // Vertex deduplication: shared vertices at cube edges are identified by
    // their canonical key (grid_vertex_x, grid_vertex_y, grid_vertex_z, axis).
    // This typically reduces vertex count by ~50%.
    let mut edge_vertex_map: HashMap<(u32, u32, u32, u8), u32> = HashMap::new();

    // Iterate over all cells
    for cz in 0..dims[2] {
        // Report progress per z-slice
        if let Some(cb) = &progress {
            cb(cz as f32 / dims[2] as f32);
        }

        // Clean up edge vertices from z-planes that are no longer needed.
        // Edges at z < cz cannot be referenced by any future cell.
        if cz > 0 {
            edge_vertex_map.retain(|&(_, _, gz, _), _| gz >= cz);
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

                // For each intersected edge, compute or reuse the shared vertex.
                // edge_indices[e] holds the global vertex index for edge e.
                let mut edge_indices = [0u32; 12];
                for e in 0..12 {
                    if edge_flags & (1 << e) != 0 {
                        let key = edge_canonical(cx, cy, cz, e);
                        edge_indices[e] = *edge_vertex_map
                            .entry(key)
                            .or_insert_with(|| {
                                let [c0, c1] = EDGE_VERTICES[e];
                                let (pos, trap) = interpolate_vertex(
                                    iso_level,
                                    corner_pos[c0],
                                    corner_pos[c1],
                                    corner_vals[c0],
                                    corner_vals[c1],
                                );
                                let idx = positions.len() as u32;
                                positions.push(pos);
                                colors.push([trap, 0.0, 0.0, 0.0]);
                                idx
                            });
                    }
                }

                // Emit triangles from TRI_TABLE.
                //
                // The standard Bourke TRI_TABLE winds triangles so the
                // cross-product normal points toward the *inside* (negative
                // SDF region).  To get outward-facing normals in a
                // right-handed / CCW front-face convention (as glTF expects)
                // we reverse the winding by emitting vertices in 0-2-1 order.
                let tri_row = &TRI_TABLE[cube_index as usize];
                let mut t = 0;
                while t < 16 {
                    if tri_row[t] < 0 {
                        break;
                    }

                    let i0 = edge_indices[tri_row[t] as usize];
                    let i1 = edge_indices[tri_row[t + 1] as usize];
                    let i2 = edge_indices[tri_row[t + 2] as usize];

                    // Reversed winding: 0, 2, 1  (instead of 0, 1, 2)
                    indices.push(i0);
                    indices.push(i2);
                    indices.push(i1);

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

            // Central differences with boundary clamping.
            // Use two-sided differences when possible; one-sided at boundaries.
            let xm = if gx > 0 { gx - 1 } else { gx };
            let xp = if gx < max_gx { gx + 1 } else { gx };
            let ym = if gy > 0 { gy - 1 } else { gy };
            let yp = if gy < max_gy { gy + 1 } else { gy };
            let zm = if gz > 0 { gz - 1 } else { gz };
            let zp = if gz < max_gz { gz + 1 } else { gz };

            // Sample grid distance values.  For the fallback when a grid
            // sample is NaN/Inf we use the *centre* sample so the gradient
            // contribution in that axis collapses to zero instead of a
            // bogus large value.
            let centre = sanitize(grid[grid_index(gx, gy, gz, vx, vy)][0], 0.0);
            let s = |idx: usize| sanitize(grid[idx][0], centre);

            let dfdx = s(grid_index(xp, gy, gz, vx, vy))
                - s(grid_index(xm, gy, gz, vx, vy));
            let dfdy = s(grid_index(gx, yp, gz, vx, vy))
                - s(grid_index(gx, ym, gz, vx, vy));
            let dfdz = s(grid_index(gx, gy, zp, vx, vy))
                - s(grid_index(gx, gy, zm, vx, vy));

            // The SDF gradient points *outward* (direction of increasing
            // distance), which is the desired normal direction.
            let len = (dfdx * dfdx + dfdy * dfdy + dfdz * dfdz).sqrt();
            if len > 1e-10 && len.is_finite() {
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

        // For a sphere SDF centred at origin, gradient normals should point
        // away from origin: dot(normal, position) > 0 for most vertices.
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
            ratio > 0.95,
            "only {:.1}% of gradient normals point outward (expected >95%)",
            ratio * 100.0
        );
    }

    #[test]
    fn face_normals_point_outward() {
        let res = 16;
        let grid = make_sphere_grid(res, [-1.0; 3], [1.0; 3], 0.5);
        // Use compute_normals = false to get face normals from cross product
        let mesh = extract_mesh(
            &grid,
            [res, res, res],
            [-1.0; 3],
            [1.0; 3],
            0.0,
            false,
            None,
        );

        // For a sphere at origin, the face normal derived from the winding
        // order should also point outward: dot(normal, centroid) > 0.
        let mut outward_count = 0usize;
        let total_tris = mesh.indices.len() / 3;
        for tri in mesh.indices.chunks_exact(3) {
            let i0 = tri[0] as usize;
            let i1 = tri[1] as usize;
            let i2 = tri[2] as usize;
            let p0 = mesh.positions[i0];
            let p1 = mesh.positions[i1];
            let p2 = mesh.positions[i2];
            // Triangle centroid
            let cx = (p0[0] + p1[0] + p2[0]) / 3.0;
            let cy = (p0[1] + p1[1] + p2[1]) / 3.0;
            let cz = (p0[2] + p1[2] + p2[2]) / 3.0;
            let n = mesh.normals[i0]; // face normal assigned to all 3 verts
            let dot = cx * n[0] + cy * n[1] + cz * n[2];
            if dot > 0.0 {
                outward_count += 1;
            }
        }
        let ratio = outward_count as f32 / total_tris as f32;
        assert!(
            ratio > 0.95,
            "only {:.1}% of face normals point outward (expected >95%)",
            ratio * 100.0
        );
    }

    #[test]
    fn winding_consistent_with_normals() {
        // Verify that the cross product of (v1-v0) × (v2-v0) for each
        // non-degenerate triangle agrees with the assigned face normal direction.
        let res = 16;
        let grid = make_sphere_grid(res, [-1.0; 3], [1.0; 3], 0.5);
        let mesh = extract_mesh(
            &grid,
            [res, res, res],
            [-1.0; 3],
            [1.0; 3],
            0.0,
            false,
            None,
        );

        let mut agree = 0usize;
        let mut tested = 0usize;
        for tri in mesh.indices.chunks_exact(3) {
            let p0 = mesh.positions[tri[0] as usize];
            let p1 = mesh.positions[tri[1] as usize];
            let p2 = mesh.positions[tri[2] as usize];

            let e1 = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
            let e2 = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];
            let cross = [
                e1[1] * e2[2] - e1[2] * e2[1],
                e1[2] * e2[0] - e1[0] * e2[2],
                e1[0] * e2[1] - e1[1] * e2[0],
            ];
            let cross_len = (cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2]).sqrt();

            // Skip degenerate triangles (near-zero area)
            if cross_len < 1e-10 {
                continue;
            }
            tested += 1;

            let n = mesh.normals[tri[0] as usize];
            let dot = cross[0] * n[0] + cross[1] * n[1] + cross[2] * n[2];
            if dot > 0.0 {
                agree += 1;
            }
        }
        assert!(tested > 0, "no non-degenerate triangles found");
        let ratio = agree as f32 / tested as f32;
        assert!(
            ratio > 0.99,
            "only {:.1}% of non-degenerate triangles have winding matching normal (expected >99%)",
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

    #[test]
    fn vertex_deduplication_reduces_count() {
        // With vertex sharing, MC should produce significantly fewer vertices
        // than 3 × triangle_count (the non-deduplicated case).
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

        let tri_count = mesh.indices.len() / 3;
        assert!(tri_count > 0, "should have triangles");

        // Without dedup, vertex count would equal 3 × tri_count.
        // With dedup, vertex count should be significantly less.
        let max_without_dedup = tri_count * 3;
        assert!(
            mesh.positions.len() < max_without_dedup,
            "vertex dedup should reduce count: {} vertices vs {} max without dedup",
            mesh.positions.len(),
            max_without_dedup,
        );

        // Typically expect ~50% reduction; verify at least 30% reduction
        let reduction = 1.0 - (mesh.positions.len() as f32 / max_without_dedup as f32);
        assert!(
            reduction > 0.3,
            "expected >30% vertex reduction, got {:.1}%",
            reduction * 100.0,
        );
    }
}
