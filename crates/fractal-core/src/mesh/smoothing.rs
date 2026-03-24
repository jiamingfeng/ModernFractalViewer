//! Mesh position smoothing algorithms.
//!
//! Smoothing is applied as a post-processing step after mesh extraction
//! to reduce high-frequency noise from chaotic fractal SDFs.

use super::MeshData;

/// Build a vertex adjacency list from triangle indices.
///
/// Returns a `Vec<Vec<usize>>` where `adj[i]` is the list of vertex indices
/// that share an edge with vertex `i`.
fn build_adjacency(vertex_count: usize, indices: &[u32]) -> Vec<Vec<usize>> {
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); vertex_count];
    for tri in indices.chunks_exact(3) {
        let a = tri[0] as usize;
        let b = tri[1] as usize;
        let c = tri[2] as usize;
        // Add each edge pair (deduplicate by checking contains — small neighbor lists)
        for &(i, j) in &[(a, b), (b, c), (c, a)] {
            if !adj[i].contains(&j) {
                adj[i].push(j);
            }
            if !adj[j].contains(&i) {
                adj[j].push(i);
            }
        }
    }
    adj
}

/// One iteration of Laplacian smoothing on positions.
///
/// Moves each vertex toward the average of its neighbors by `lambda`.
/// `lambda` in `(0, 1]` — higher values smooth more aggressively.
fn smooth_step(positions: &mut [[f32; 3]], adj: &[Vec<usize>], lambda: f32) {
    // Compute target positions first (read old, write new)
    let old: Vec<[f32; 3]> = positions.to_vec();
    for (i, neighbors) in adj.iter().enumerate() {
        if neighbors.is_empty() {
            continue;
        }
        let inv_n = 1.0 / neighbors.len() as f32;
        let mut avg = [0.0f32; 3];
        for &j in neighbors {
            avg[0] += old[j][0];
            avg[1] += old[j][1];
            avg[2] += old[j][2];
        }
        avg[0] *= inv_n;
        avg[1] *= inv_n;
        avg[2] *= inv_n;

        // Move toward average by lambda
        positions[i][0] = old[i][0] + lambda * (avg[0] - old[i][0]);
        positions[i][1] = old[i][1] + lambda * (avg[1] - old[i][1]);
        positions[i][2] = old[i][2] + lambda * (avg[2] - old[i][2]);
    }
}

/// Apply Laplacian smoothing to mesh vertex positions.
///
/// Simple averaging that reduces noise but causes slight mesh shrinkage
/// proportional to the number of iterations and lambda value.
pub fn laplacian_smooth(mesh: &mut MeshData, iterations: u32, lambda: f32) {
    if iterations == 0 || mesh.positions.is_empty() {
        return;
    }
    let adj = build_adjacency(mesh.positions.len(), &mesh.indices);
    for _ in 0..iterations {
        smooth_step(&mut mesh.positions, &adj, lambda);
    }
}

/// Apply Taubin smoothing to mesh vertex positions.
///
/// Alternates a positive lambda step (shrink) with a negative mu step (inflate),
/// which preserves mesh volume much better than plain Laplacian smoothing.
/// Each "iteration" consists of one lambda + one mu pass.
///
/// Standard Taubin formula: `mu = -(lambda + 0.01)` (slightly larger magnitude
/// than lambda to counteract shrinkage).
pub fn taubin_smooth(mesh: &mut MeshData, iterations: u32, lambda: f32) {
    if iterations == 0 || mesh.positions.is_empty() {
        return;
    }
    let mu = -(lambda + 0.01);
    let adj = build_adjacency(mesh.positions.len(), &mesh.indices);
    for _ in 0..iterations {
        smooth_step(&mut mesh.positions, &adj, lambda);
        smooth_step(&mut mesh.positions, &adj, mu);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Create a simple unit cube mesh for testing (8 vertices, 12 triangles).
    fn make_cube_mesh() -> MeshData {
        let positions = vec![
            [-1.0, -1.0, -1.0], [1.0, -1.0, -1.0], [1.0, 1.0, -1.0], [-1.0, 1.0, -1.0],
            [-1.0, -1.0,  1.0], [1.0, -1.0,  1.0], [1.0, 1.0,  1.0], [-1.0, 1.0,  1.0],
        ];
        #[rustfmt::skip]
        let indices = vec![
            0,1,2, 0,2,3, // front
            4,6,5, 4,7,6, // back
            0,4,5, 0,5,1, // bottom
            2,6,7, 2,7,3, // top
            0,3,7, 0,7,4, // left
            1,5,6, 1,6,2, // right
        ];
        let n = positions.len();
        MeshData {
            positions,
            normals: vec![[0.0, 1.0, 0.0]; n],
            colors: vec![[1.0, 1.0, 1.0, 1.0]; n],
            indices,
        }
    }

    fn bounding_box(positions: &[[f32; 3]]) -> ([f32; 3], [f32; 3]) {
        let mut bmin = [f32::MAX; 3];
        let mut bmax = [f32::MIN; 3];
        for p in positions {
            for i in 0..3 {
                bmin[i] = bmin[i].min(p[i]);
                bmax[i] = bmax[i].max(p[i]);
            }
        }
        (bmin, bmax)
    }

    fn volume(bmin: [f32; 3], bmax: [f32; 3]) -> f32 {
        (bmax[0] - bmin[0]) * (bmax[1] - bmin[1]) * (bmax[2] - bmin[2])
    }

    #[test]
    fn zero_iterations_noop() {
        let mut mesh = make_cube_mesh();
        let orig = mesh.positions.clone();
        laplacian_smooth(&mut mesh, 0, 0.5);
        assert_eq!(mesh.positions, orig);

        let mut mesh2 = make_cube_mesh();
        taubin_smooth(&mut mesh2, 0, 0.5);
        assert_eq!(mesh2.positions, orig);
    }

    #[test]
    fn laplacian_shrinks_mesh() {
        let mut mesh = make_cube_mesh();
        let (bmin_before, bmax_before) = bounding_box(&mesh.positions);
        let vol_before = volume(bmin_before, bmax_before);

        laplacian_smooth(&mut mesh, 5, 0.5);

        let (bmin_after, bmax_after) = bounding_box(&mesh.positions);
        let vol_after = volume(bmin_after, bmax_after);

        // Laplacian smoothing should shrink the bounding box
        assert!(
            vol_after < vol_before,
            "Laplacian should shrink: before={vol_before}, after={vol_after}"
        );
    }

    #[test]
    fn taubin_preserves_volume_better_than_laplacian() {
        let mut mesh_lap = make_cube_mesh();
        let mut mesh_tau = make_cube_mesh();
        let (bmin_orig, bmax_orig) = bounding_box(&mesh_lap.positions);
        let vol_orig = volume(bmin_orig, bmax_orig);

        laplacian_smooth(&mut mesh_lap, 5, 0.5);
        taubin_smooth(&mut mesh_tau, 5, 0.5);

        let vol_lap = volume(
            bounding_box(&mesh_lap.positions).0,
            bounding_box(&mesh_lap.positions).1,
        );
        let vol_tau = volume(
            bounding_box(&mesh_tau.positions).0,
            bounding_box(&mesh_tau.positions).1,
        );

        // Taubin should preserve volume better (closer to original)
        let shrink_lap = (vol_orig - vol_lap).abs();
        let shrink_tau = (vol_orig - vol_tau).abs();
        assert!(
            shrink_tau < shrink_lap,
            "Taubin should preserve volume better: lap_shrink={shrink_lap}, tau_shrink={shrink_tau}"
        );
    }

    #[test]
    fn smoothing_preserves_connectivity() {
        let mut mesh = make_cube_mesh();
        let orig_indices = mesh.indices.clone();
        let orig_count = mesh.positions.len();

        taubin_smooth(&mut mesh, 3, 0.5);

        // Indices and vertex count should be unchanged
        assert_eq!(mesh.indices, orig_indices);
        assert_eq!(mesh.positions.len(), orig_count);
    }

    #[test]
    fn empty_mesh_noop() {
        let mut mesh = MeshData {
            positions: vec![],
            normals: vec![],
            colors: vec![],
            indices: vec![],
        };
        laplacian_smooth(&mut mesh, 5, 0.5);
        taubin_smooth(&mut mesh, 5, 0.5);
        assert!(mesh.positions.is_empty());
    }
}
