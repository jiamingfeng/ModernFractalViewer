//! Dual Contouring mesh extraction from SDF volume data.
//!
//! Produces a watertight quad/triangle mesh by placing one vertex per cell
//! (via QEF minimisation) and connecting vertices across sign-changing edges.
//!
//! Reference: Ju et al., "Dual Contouring of Hermite Data" (2002).

use super::qef::QefSolver;
use super::MeshData;
use std::collections::HashMap;

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


/// Estimate the SDF gradient at grid point `(gx, gy, gz)` via central
/// differences.  Returns a unit-length normal or `[0,1,0]` fallback.
fn estimate_gradient(
    grid: &[[f32; 2]],
    gx: u32,
    gy: u32,
    gz: u32,
    vx: u32,
    vy: u32,
    max_gx: u32,
    max_gy: u32,
    max_gz: u32,
) -> [f32; 3] {
    let centre = sanitize(grid[grid_index(gx, gy, gz, vx, vy)][0], 0.0);
    let s = |idx: usize| sanitize(grid[idx][0], centre);

    let xm = if gx > 0 { gx - 1 } else { gx };
    let xp = if gx < max_gx { gx + 1 } else { gx };
    let ym = if gy > 0 { gy - 1 } else { gy };
    let yp = if gy < max_gy { gy + 1 } else { gy };
    let zm = if gz > 0 { gz - 1 } else { gz };
    let zp = if gz < max_gz { gz + 1 } else { gz };

    let dfdx = s(grid_index(xp, gy, gz, vx, vy)) - s(grid_index(xm, gy, gz, vx, vy));
    let dfdy = s(grid_index(gx, yp, gz, vx, vy)) - s(grid_index(gx, ym, gz, vx, vy));
    let dfdz = s(grid_index(gx, gy, zp, vx, vy)) - s(grid_index(gx, gy, zm, vx, vy));

    let len = (dfdx * dfdx + dfdy * dfdy + dfdz * dfdz).sqrt();
    if len > 1e-10 && len.is_finite() {
        [dfdx / len, dfdy / len, dfdz / len]
    } else {
        [0.0, 1.0, 0.0]
    }
}

/// Linearly interpolate the edge crossing point and trap value between two
/// grid vertices with values `v0` and `v1` at positions `p0` and `p1`.
#[inline]
fn edge_intersection(
    iso: f32,
    p0: [f32; 3],
    p1: [f32; 3],
    v0: f32,
    v1: f32,
    trap0: f32,
    trap1: f32,
) -> ([f32; 3], f32) {
    let denom = v1 - v0;
    let t = if denom.abs() > 1e-10 {
        ((iso - v0) / denom).clamp(0.0, 1.0)
    } else {
        0.5
    };
    let pos = [
        p0[0] + t * (p1[0] - p0[0]),
        p0[1] + t * (p1[1] - p0[1]),
        p0[2] + t * (p1[2] - p0[2]),
    ];
    let trap = trap0 + t * (trap1 - trap0);
    (pos, trap)
}

// ── Main entry point ─────────────────────────────────────────────────────

/// Extract a triangle mesh from a 3D SDF volume using Dual Contouring.
///
/// The function signature matches [`super::marching_cubes::extract_mesh`] for
/// drop-in interchangeability.
///
/// # Arguments
/// * `grid` - Flat array of `[distance, trap]` pairs
/// * `dims` - Number of *cells* per axis
/// * `bounds_min` / `bounds_max` - World-space bounding box
/// * `iso_level` - Iso-value for surface extraction
/// * `compute_normals` - If true, use SDF-gradient normals; else face normals
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
                "dual_contouring: grid has {} samples but expected {} for dims {:?}",
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

    let dx = (bounds_max[0] - bounds_min[0]) / dims[0] as f32;
    let dy = (bounds_max[1] - bounds_min[1]) / dims[1] as f32;
    let dz = (bounds_max[2] - bounds_min[2]) / dims[2] as f32;

    let max_gx = dims[0]; // = vx - 1
    let max_gy = dims[1];
    let max_gz = dims[2];

    // ── Phase A: Place one vertex per cell that has sign changes ─────

    // Map from cell coordinate to vertex index
    let mut cell_vertex: HashMap<(u32, u32, u32), u32> = HashMap::new();
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut trap_values: Vec<f32> = Vec::new();

    // 8 corner offsets: same convention as MC
    let corner_offsets: [[u32; 3]; 8] = [
        [0, 0, 0],
        [1, 0, 0],
        [1, 1, 0],
        [0, 1, 0],
        [0, 0, 1],
        [1, 0, 1],
        [1, 1, 1],
        [0, 1, 1],
    ];

    // 12 edges as pairs of corner indices
    let edges: [[usize; 2]; 12] = [
        [0, 1], [1, 2], [2, 3], [3, 0],
        [4, 5], [5, 6], [6, 7], [7, 4],
        [0, 4], [1, 5], [2, 6], [3, 7],
    ];

    for cz in 0..dims[2] {
        if let Some(cb) = &progress {
            cb(cz as f32 / dims[2] as f32 * 0.5); // 0..0.5 for phase A
        }

        for cy in 0..dims[1] {
            for cx in 0..dims[0] {
                // Read 8 corner values
                let mut corner_vals = [0.0f32; 8];
                let mut corner_trap = [0.0f32; 8];
                let mut corner_pos = [[0.0f32; 3]; 8];
                let mut corner_grid = [[0u32; 3]; 8];

                for (i, off) in corner_offsets.iter().enumerate() {
                    let gx = cx + off[0];
                    let gy = cy + off[1];
                    let gz = cz + off[2];
                    let idx = grid_index(gx, gy, gz, vx, vy);
                    corner_vals[i] = sanitize(grid[idx][0], iso_level + 1.0);
                    corner_trap[i] = sanitize(grid[idx][1], 0.0);
                    corner_pos[i] = [
                        bounds_min[0] + gx as f32 * dx,
                        bounds_min[1] + gy as f32 * dy,
                        bounds_min[2] + gz as f32 * dz,
                    ];
                    corner_grid[i] = [gx, gy, gz];
                }

                // Check edges for sign changes
                let mut qef = QefSolver::new();
                let mut trap_sum = 0.0f32;
                let mut crossing_count = 0u32;

                for &[c0, c1] in &edges {
                    let inside0 = corner_vals[c0] < iso_level;
                    let inside1 = corner_vals[c1] < iso_level;
                    if inside0 == inside1 {
                        continue; // no sign change
                    }

                    // Compute edge intersection
                    let (pt, trap) = edge_intersection(
                        iso_level,
                        corner_pos[c0],
                        corner_pos[c1],
                        corner_vals[c0],
                        corner_vals[c1],
                        corner_trap[c0],
                        corner_trap[c1],
                    );

                    // Estimate gradient at the intersection by interpolating
                    // gradients at the two endpoints
                    let g0 = corner_grid[c0];
                    let g1 = corner_grid[c1];
                    let n0 = estimate_gradient(grid, g0[0], g0[1], g0[2], vx, vy, max_gx, max_gy, max_gz);
                    let n1 = estimate_gradient(grid, g1[0], g1[1], g1[2], vx, vy, max_gx, max_gy, max_gz);

                    // Interpolate normal (same t as position)
                    let denom = corner_vals[c1] - corner_vals[c0];
                    let t = if denom.abs() > 1e-10 {
                        ((iso_level - corner_vals[c0]) / denom).clamp(0.0, 1.0)
                    } else {
                        0.5
                    };
                    let mut n = [
                        n0[0] + t * (n1[0] - n0[0]),
                        n0[1] + t * (n1[1] - n0[1]),
                        n0[2] + t * (n1[2] - n0[2]),
                    ];
                    let nlen = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
                    if nlen > 1e-10 {
                        n[0] /= nlen;
                        n[1] /= nlen;
                        n[2] /= nlen;
                    } else {
                        n = [0.0, 1.0, 0.0];
                    }

                    qef.add(n, pt);
                    trap_sum += trap;
                    crossing_count += 1;
                }

                if crossing_count == 0 {
                    continue; // Cell has no surface crossing
                }

                // Solve QEF for optimal vertex in cell
                let cell_min = corner_pos[0]; // (cx, cy, cz) corner
                let cell_max = corner_pos[6]; // (cx+1, cy+1, cz+1) corner
                let vertex = qef.solve(cell_min, cell_max);

                let vert_idx = positions.len() as u32;
                positions.push(vertex);
                trap_values.push(trap_sum / crossing_count as f32);
                cell_vertex.insert((cx, cy, cz), vert_idx);
            }
        }
    }

    // ── Phase B: Emit quads for sign-changing edges ──────────────────
    //
    // For each interior edge shared by 4 cells, if the edge has a sign
    // change, connect the 4 cell vertices into a quad.
    //
    // There are 3 families of edges (X, Y, Z-aligned).

    let mut indices: Vec<u32> = Vec::new();

    // Helper: check sign change between two grid vertices
    let sign_change = |gx0: u32, gy0: u32, gz0: u32, gx1: u32, gy1: u32, gz1: u32| -> bool {
        let i0 = grid_index(gx0, gy0, gz0, vx, vy);
        let i1 = grid_index(gx1, gy1, gz1, vx, vy);
        let v0 = sanitize(grid[i0][0], iso_level + 1.0);
        let v1 = sanitize(grid[i1][0], iso_level + 1.0);
        (v0 < iso_level) != (v1 < iso_level)
    };

    // Helper: is the first vertex inside (negative SDF)?
    let is_inside = |gx: u32, gy: u32, gz: u32| -> bool {
        let i = grid_index(gx, gy, gz, vx, vy);
        sanitize(grid[i][0], iso_level + 1.0) < iso_level
    };

    if let Some(cb) = &progress {
        cb(0.5);
    }

    // X-aligned edges: from (gx, gy, gz) to (gx+1, gy, gz)
    // Shared by cells: (gx, cy, cz) where cy ∈ {gy-1, gy}, cz ∈ {gz-1, gz}
    // Requires gy ≥ 1, gz ≥ 1, and gy < vy-1 = dims[1], gz < vz-1 = dims[2]
    let total_edge_iters = dims[0] + dims[1] + dims[2];
    let mut edge_done: usize = 0;
    for gx in 0..dims[0] {
        for gy in 1..dims[1] {
            for gz in 1..dims[2] {
                if !sign_change(gx, gy, gz, gx + 1, gy, gz) {
                    continue;
                }

                // 4 cells sharing this edge
                let c0 = (gx, gy, gz);         // cell (gx, gy, gz)
                let c1 = (gx, gy - 1, gz);     // cell (gx, gy-1, gz)
                let c2 = (gx, gy - 1, gz - 1); // cell (gx, gy-1, gz-1)
                let c3 = (gx, gy, gz - 1);     // cell (gx, gy, gz-1)

                if let (Some(&v0), Some(&v1), Some(&v2), Some(&v3)) = (
                    cell_vertex.get(&c0),
                    cell_vertex.get(&c1),
                    cell_vertex.get(&c2),
                    cell_vertex.get(&c3),
                ) {
                    // Wind quad so normal points from inside to outside
                    // For X-axis edge, the gradient is along Y-Z diagonal
                    if is_inside(gx, gy, gz) {
                        emit_quad(&mut indices, &positions, v0, v1, v2, v3);
                    } else {
                        emit_quad(&mut indices, &positions, v0, v3, v2, v1);
                    }
                }
            }
        }
        edge_done += 1;
        if let Some(cb) = &progress {
            cb(0.5 + 0.4 * (edge_done as f32 / total_edge_iters as f32));
        }
    }

    // Y-aligned edges: from (gx, gy, gz) to (gx, gy+1, gz)
    // Shared by cells: cx ∈ {gx-1, gx}, cz ∈ {gz-1, gz}
    for gy in 0..dims[1] {
        for gx in 1..dims[0] {
            for gz in 1..dims[2] {
                if !sign_change(gx, gy, gz, gx, gy + 1, gz) {
                    continue;
                }

                let c0 = (gx, gy, gz);
                let c1 = (gx, gy, gz - 1);
                let c2 = (gx - 1, gy, gz - 1);
                let c3 = (gx - 1, gy, gz);

                if let (Some(&v0), Some(&v1), Some(&v2), Some(&v3)) = (
                    cell_vertex.get(&c0),
                    cell_vertex.get(&c1),
                    cell_vertex.get(&c2),
                    cell_vertex.get(&c3),
                ) {
                    if is_inside(gx, gy, gz) {
                        emit_quad(&mut indices, &positions, v0, v1, v2, v3);
                    } else {
                        emit_quad(&mut indices, &positions, v0, v3, v2, v1);
                    }
                }
            }
        }
        edge_done += 1;
        if let Some(cb) = &progress {
            cb(0.5 + 0.4 * (edge_done as f32 / total_edge_iters as f32));
        }
    }

    // Z-aligned edges: from (gx, gy, gz) to (gx, gy, gz+1)
    // Shared by cells: cx ∈ {gx-1, gx}, cy ∈ {gy-1, gy}
    for gz in 0..dims[2] {
        for gx in 1..dims[0] {
            for gy in 1..dims[1] {
                if !sign_change(gx, gy, gz, gx, gy, gz + 1) {
                    continue;
                }

                let c0 = (gx, gy, gz);
                let c1 = (gx - 1, gy, gz);
                let c2 = (gx - 1, gy - 1, gz);
                let c3 = (gx, gy - 1, gz);

                if let (Some(&v0), Some(&v1), Some(&v2), Some(&v3)) = (
                    cell_vertex.get(&c0),
                    cell_vertex.get(&c1),
                    cell_vertex.get(&c2),
                    cell_vertex.get(&c3),
                ) {
                    if is_inside(gx, gy, gz) {
                        emit_quad(&mut indices, &positions, v0, v1, v2, v3);
                    } else {
                        emit_quad(&mut indices, &positions, v0, v3, v2, v1);
                    }
                }
            }
        }
        edge_done += 1;
        if let Some(cb) = &progress {
            cb(0.5 + 0.4 * (edge_done as f32 / total_edge_iters as f32));
        }
    }

    // ── Phase C: Compute normals and colors ──────────────────────────

    let normals = if compute_normals {
        compute_gradient_normals(&positions, grid, dims, bounds_min, dx, dy, dz, vx, vy)
    } else {
        compute_face_normals(&positions, &indices)
    };

    // Build per-vertex colours: [trap, 0, 0, 0] (same as MC for palette pipeline)
    let colors: Vec<[f32; 4]> = trap_values
        .iter()
        .map(|&t| [sanitize(t, 0.0), 0.0, 0.0, 0.0])
        .collect();

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

// ── Quad emission ────────────────────────────────────────────────────────

/// Emits a quad as two triangles, splitting along the shorter diagonal for
/// better triangle quality.
fn emit_quad(indices: &mut Vec<u32>, positions: &[[f32; 3]], v0: u32, v1: u32, v2: u32, v3: u32) {
    let p0 = positions[v0 as usize];
    let p1 = positions[v1 as usize];
    let p2 = positions[v2 as usize];
    let p3 = positions[v3 as usize];

    // Choose the shorter diagonal for better triangle shape
    let d02 = dist_sq(p0, p2);
    let d13 = dist_sq(p1, p3);

    if d02 <= d13 {
        // Split along 0-2 diagonal
        indices.extend_from_slice(&[v0, v1, v2, v0, v2, v3]);
    } else {
        // Split along 1-3 diagonal
        indices.extend_from_slice(&[v0, v1, v3, v1, v2, v3]);
    }
}

#[inline]
fn dist_sq(a: [f32; 3], b: [f32; 3]) -> f32 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    let dz = a[2] - b[2];
    dx * dx + dy * dy + dz * dz
}

// ── Normal computation ───────────────────────────────────────────────────

/// Compute smooth normals from SDF gradient via central differences.
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
    let max_gx = dims[0];
    let max_gy = dims[1];
    let max_gz = dims[2];

    positions
        .iter()
        .map(|pos| {
            let fx = ((pos[0] - bounds_min[0]) * inv_dx).clamp(0.0, max_gx as f32);
            let fy = ((pos[1] - bounds_min[1]) * inv_dy).clamp(0.0, max_gy as f32);
            let fz = ((pos[2] - bounds_min[2]) * inv_dz).clamp(0.0, max_gz as f32);

            let gx = (fx.round() as u32).min(max_gx);
            let gy = (fy.round() as u32).min(max_gy);
            let gz = (fz.round() as u32).min(max_gz);

            estimate_gradient(grid, gx, gy, gz, vx, vy, max_gx, max_gy, max_gz)
        })
        .collect()
}

/// Compute face normals from triangle cross products, accumulated per vertex.
fn compute_face_normals(positions: &[[f32; 3]], indices: &[u32]) -> Vec<[f32; 3]> {
    let mut normals = vec![[0.0f32; 3]; positions.len()];

    for tri in indices.chunks_exact(3) {
        let i0 = tri[0] as usize;
        let i1 = tri[1] as usize;
        let i2 = tri[2] as usize;

        let p0 = positions[i0];
        let p1 = positions[i1];
        let p2 = positions[i2];

        let e1 = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
        let e2 = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];

        // Cross product (area-weighted normal)
        let nx = e1[1] * e2[2] - e1[2] * e2[1];
        let ny = e1[2] * e2[0] - e1[0] * e2[2];
        let nz = e1[0] * e2[1] - e1[1] * e2[0];

        // Accumulate (area-weighted) onto all 3 vertices for smooth shading
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

    /// Create a box SDF grid for sharp-feature testing.
    fn make_box_grid(
        resolution: u32,
        bounds_min: [f32; 3],
        bounds_max: [f32; 3],
        half_size: f32,
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
                    // Box SDF
                    let qx = x.abs() - half_size;
                    let qy = y.abs() - half_size;
                    let qz = z.abs() - half_size;
                    let outside = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2) + qz.max(0.0).powi(2)).sqrt();
                    let inside = qx.max(qy).max(qz).min(0.0);
                    let dist = outside + inside;
                    let idx = grid_index(gx, gy, gz, vx, vy);
                    grid[idx] = [dist, dist.abs()];
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
    fn box_sdf_produces_mesh() {
        let res = 16;
        let grid = make_box_grid(res, [-1.0; 3], [1.0; 3], 0.4);
        let mesh = extract_mesh(&grid, [res; 3], [-1.0; 3], [1.0; 3], 0.0, true, None);

        assert!(!mesh.positions.is_empty(), "box mesh should be non-empty");
        assert!(!mesh.indices.is_empty());
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
            ratio > 0.90,
            "only {:.1}% of normals point outward (expected >90%)",
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
        for (i, c) in mesh.colors.iter().enumerate() {
            assert!(
                c[0].is_finite() && c[1].is_finite() && c[2].is_finite() && c[3].is_finite(),
                "NaN/Inf in color[{i}]: {c:?}"
            );
        }
    }

    #[test]
    fn nan_in_grid_produces_no_nan_output() {
        let res = 8;
        let mut grid = make_sphere_grid(res, [-1.0; 3], [1.0; 3], 0.5);
        for i in (0..grid.len()).step_by(10) {
            grid[i] = [f32::NAN, f32::NAN];
        }
        for i in (5..grid.len()).step_by(17) {
            grid[i] = [f32::INFINITY, f32::NEG_INFINITY];
        }

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
    fn dc_fewer_vertices_than_mc() {
        // DC should produce fewer vertices than MC for the same grid,
        // because DC shares vertices between adjacent quads.
        let res = 12;
        let grid = make_sphere_grid(res, [-1.0; 3], [1.0; 3], 0.5);

        let dc_mesh = extract_mesh(&grid, [res; 3], [-1.0; 3], [1.0; 3], 0.0, true, None);
        let mc_mesh = super::super::marching_cubes::extract_mesh(
            &grid,
            [res; 3],
            [-1.0; 3],
            [1.0; 3],
            0.0,
            true,
            None,
        );

        // DC should have significantly fewer vertices due to vertex sharing
        assert!(
            dc_mesh.positions.len() < mc_mesh.positions.len(),
            "DC vertices ({}) should be fewer than MC vertices ({})",
            dc_mesh.positions.len(),
            mc_mesh.positions.len()
        );
    }

    #[test]
    fn face_normals_point_outward() {
        let res = 16;
        let grid = make_sphere_grid(res, [-1.0; 3], [1.0; 3], 0.5);
        let mesh = extract_mesh(&grid, [res; 3], [-1.0; 3], [1.0; 3], 0.0, false, None);

        let mut outward = 0usize;
        let total_tris = mesh.indices.len() / 3;
        for tri in mesh.indices.chunks_exact(3) {
            let p0 = mesh.positions[tri[0] as usize];
            let p1 = mesh.positions[tri[1] as usize];
            let p2 = mesh.positions[tri[2] as usize];
            let cx = (p0[0] + p1[0] + p2[0]) / 3.0;
            let cy = (p0[1] + p1[1] + p2[1]) / 3.0;
            let cz = (p0[2] + p1[2] + p2[2]) / 3.0;
            let n = mesh.normals[tri[0] as usize];
            let dot = cx * n[0] + cy * n[1] + cz * n[2];
            if dot > 0.0 {
                outward += 1;
            }
        }
        let ratio = outward as f32 / total_tris as f32;
        assert!(
            ratio > 0.90,
            "only {:.1}% of face normals point outward (expected >90%)",
            ratio * 100.0
        );
    }
}
