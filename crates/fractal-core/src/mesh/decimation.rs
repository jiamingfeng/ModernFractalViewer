//! Mesh decimation using Quadric Error Metrics (QEM).
//!
//! Implements the Garland & Heckbert (1997) edge-collapse algorithm to
//! reduce triangle count while preserving shape quality. Each vertex
//! accumulates a 4×4 symmetric error matrix (quadric); edges are collapsed
//! in order of increasing error cost via a priority queue.

use super::MeshData;
use std::collections::{BinaryHeap, HashSet};

/// A 4×4 symmetric matrix stored as 10 unique values (upper triangle).
///
/// Layout: `[a00, a01, a02, a03, a11, a12, a13, a22, a23, a33]`
#[derive(Debug, Clone, Copy)]
struct Quadric([f64; 10]);

impl Quadric {
    fn zero() -> Self {
        Quadric([0.0; 10])
    }

    /// Build a quadric from a plane equation `ax + by + cz + d = 0`.
    fn from_plane(a: f64, b: f64, c: f64, d: f64) -> Self {
        Quadric([
            a * a, a * b, a * c, a * d,
            b * b, b * c, b * d,
            c * c, c * d,
            d * d,
        ])
    }

    fn add(&self, other: &Quadric) -> Quadric {
        let mut r = [0.0; 10];
        for i in 0..10 {
            r[i] = self.0[i] + other.0[i];
        }
        Quadric(r)
    }

    /// Evaluate the quadric error for a point `[x, y, z]`.
    fn evaluate(&self, v: [f64; 3]) -> f64 {
        let q = &self.0;
        let x = v[0];
        let y = v[1];
        let z = v[2];

        // v^T Q v where Q is symmetric 4×4 (homogeneous, w=1)
        q[0] * x * x + 2.0 * q[1] * x * y + 2.0 * q[2] * x * z + 2.0 * q[3] * x
            + q[4] * y * y + 2.0 * q[5] * y * z + 2.0 * q[6] * y
            + q[7] * z * z + 2.0 * q[8] * z
            + q[9]
    }

    /// Try to find the optimal vertex position that minimizes the quadric.
    ///
    /// Solves the 3×3 linear system from the gradient of `v^T Q v`.
    /// Returns `None` if the system is singular.
    fn optimal_position(&self) -> Option<[f64; 3]> {
        let q = &self.0;
        // 3×3 system: [[a00,a01,a02],[a01,a11,a12],[a02,a12,a22]] * [x,y,z] = -[a03,a13,a23]
        let a = [[q[0], q[1], q[2]], [q[1], q[4], q[5]], [q[2], q[5], q[7]]];
        let b = [-q[3], -q[6], -q[8]];

        // Cramer's rule
        let det = a[0][0] * (a[1][1] * a[2][2] - a[1][2] * a[2][1])
            - a[0][1] * (a[1][0] * a[2][2] - a[1][2] * a[2][0])
            + a[0][2] * (a[1][0] * a[2][1] - a[1][1] * a[2][0]);

        if det.abs() < 1e-12 {
            return None;
        }

        let inv_det = 1.0 / det;
        let x = (b[0] * (a[1][1] * a[2][2] - a[1][2] * a[2][1])
            - a[0][1] * (b[1] * a[2][2] - a[1][2] * b[2])
            + a[0][2] * (b[1] * a[2][1] - a[1][1] * b[2]))
            * inv_det;
        let y = (a[0][0] * (b[1] * a[2][2] - a[1][2] * b[2])
            - b[0] * (a[1][0] * a[2][2] - a[1][2] * a[2][0])
            + a[0][2] * (a[1][0] * b[2] - b[1] * a[2][0]))
            * inv_det;
        let z = (a[0][0] * (a[1][1] * b[2] - b[1] * a[2][1])
            - a[0][1] * (a[1][0] * b[2] - b[1] * a[2][0])
            + b[0] * (a[1][0] * a[2][1] - a[1][1] * a[2][0]))
            * inv_det;

        if x.is_finite() && y.is_finite() && z.is_finite() {
            Some([x, y, z])
        } else {
            None
        }
    }
}

/// A candidate edge collapse in the priority queue.
#[derive(Debug, Clone)]
struct EdgeCollapse {
    /// Error cost of this collapse (lower = better).
    cost: f64,
    /// Vertex indices forming the edge.
    v0: usize,
    v1: usize,
    /// Optimal position for the merged vertex.
    target: [f64; 3],
    /// Generation counter for v0 at the time this entry was created.
    gen0: u32,
    /// Generation counter for v1 at the time this entry was created.
    gen1: u32,
}

impl PartialEq for EdgeCollapse {
    fn eq(&self, other: &Self) -> bool {
        self.cost == other.cost
    }
}

impl Eq for EdgeCollapse {}

impl PartialOrd for EdgeCollapse {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for EdgeCollapse {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Min-heap: reverse ordering so lowest cost is popped first
        other
            .cost
            .partial_cmp(&self.cost)
            .unwrap_or(std::cmp::Ordering::Equal)
    }
}

/// Simplify a mesh by collapsing edges using Quadric Error Metrics.
///
/// `target_ratio` is the fraction of original triangles to keep (0.01–1.0).
/// The optional `progress` callback receives values in `[0.0, 1.0]`.
pub fn decimate(mesh: &mut MeshData, target_ratio: f32, progress: Option<&dyn Fn(f32)>) {
    let target_ratio = target_ratio.clamp(0.01, 1.0);
    if mesh.indices.is_empty() || target_ratio >= 0.999 {
        return;
    }

    let vertex_count = mesh.positions.len();
    let initial_face_count = mesh.indices.len() / 3;
    let target_faces = ((initial_face_count as f32 * target_ratio).ceil() as usize).max(1);

    // Convert positions to f64 for precision
    let mut positions: Vec<[f64; 3]> = mesh
        .positions
        .iter()
        .map(|p| [p[0] as f64, p[1] as f64, p[2] as f64])
        .collect();

    // Build face list and edge adjacency
    let mut faces: Vec<[usize; 3]> = mesh
        .indices
        .chunks_exact(3)
        .map(|t| [t[0] as usize, t[1] as usize, t[2] as usize])
        .collect();

    // Track which faces reference each vertex
    let mut vertex_faces: Vec<HashSet<usize>> = vec![HashSet::new(); vertex_count];
    for (fi, face) in faces.iter().enumerate() {
        for &vi in face {
            vertex_faces[vi].insert(fi);
        }
    }

    // Compute initial quadrics from face planes
    let mut quadrics: Vec<Quadric> = vec![Quadric::zero(); vertex_count];
    for face in &faces {
        let p0 = positions[face[0]];
        let p1 = positions[face[1]];
        let p2 = positions[face[2]];

        let e1 = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
        let e2 = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];
        let nx = e1[1] * e2[2] - e1[2] * e2[1];
        let ny = e1[2] * e2[0] - e1[0] * e2[2];
        let nz = e1[0] * e2[1] - e1[1] * e2[0];
        let len = (nx * nx + ny * ny + nz * nz).sqrt();
        if len < 1e-15 {
            continue;
        }
        let (a, b, c) = (nx / len, ny / len, nz / len);
        let d = -(a * p0[0] + b * p0[1] + c * p0[2]);
        let plane_q = Quadric::from_plane(a, b, c, d);

        for &vi in face {
            quadrics[vi] = quadrics[vi].add(&plane_q);
        }
    }

    // Collect unique edges
    let mut edges: HashSet<(usize, usize)> = HashSet::new();
    for face in &faces {
        let mut add_edge = |a: usize, b: usize| {
            let key = if a < b { (a, b) } else { (b, a) };
            edges.insert(key);
        };
        add_edge(face[0], face[1]);
        add_edge(face[1], face[2]);
        add_edge(face[2], face[0]);
    }

    // Generation counters for lazy deletion
    let mut generation: Vec<u32> = vec![0; vertex_count];
    // Track collapsed vertices (merged into another)
    let mut collapsed_into: Vec<Option<usize>> = vec![None; vertex_count];

    // Resolve collapse chain to find the ultimate target
    fn resolve(collapsed_into: &[Option<usize>], mut v: usize) -> usize {
        while let Some(target) = collapsed_into[v] {
            v = target;
        }
        v
    }

    // Compute collapse cost for an edge
    let compute_collapse = |v0: usize, v1: usize, quadrics: &[Quadric], positions: &[[f64; 3]], generation: &[u32]| -> EdgeCollapse {
        let combined = quadrics[v0].add(&quadrics[v1]);
        let (target, cost) = if let Some(opt) = combined.optimal_position() {
            (opt, combined.evaluate(opt))
        } else {
            // Fallback: midpoint
            let mid = [
                (positions[v0][0] + positions[v1][0]) * 0.5,
                (positions[v0][1] + positions[v1][1]) * 0.5,
                (positions[v0][2] + positions[v1][2]) * 0.5,
            ];
            (mid, combined.evaluate(mid))
        };
        EdgeCollapse {
            cost: cost.max(0.0),
            v0,
            v1,
            target,
            gen0: generation[v0],
            gen1: generation[v1],
        }
    };

    // Build initial priority queue
    let mut heap = BinaryHeap::with_capacity(edges.len());
    for &(v0, v1) in &edges {
        heap.push(compute_collapse(v0, v1, &quadrics, &positions, &generation));
    }

    // Track living face count
    let mut live_faces = initial_face_count;
    let faces_to_remove = initial_face_count.saturating_sub(target_faces);
    let mut faces_removed = 0usize;

    // Collapse loop
    while live_faces > target_faces {
        let collapse = match heap.pop() {
            Some(c) => c,
            None => break,
        };

        // Lazy deletion: skip stale entries
        let v0 = resolve(&collapsed_into, collapse.v0);
        let v1 = resolve(&collapsed_into, collapse.v1);
        if v0 == v1 {
            continue;
        }
        if generation[v0] != collapse.gen0 || generation[v1] != collapse.gen1 {
            continue;
        }

        // Perform collapse: merge v1 into v0
        positions[v0] = collapse.target;
        quadrics[v0] = quadrics[v0].add(&quadrics[v1]);
        generation[v0] += 1;
        collapsed_into[v1] = Some(v0);

        // Update faces: replace v1 with v0, remove degenerate faces
        let v1_faces: Vec<usize> = vertex_faces[v1].iter().copied().collect();
        for fi in v1_faces {
            vertex_faces[v1].remove(&fi);

            // Replace v1 with v0 in the face
            for vi in faces[fi].iter_mut() {
                if *vi == v1 {
                    *vi = v0;
                }
            }

            // Check if face became degenerate (two or more identical vertices)
            let f = faces[fi];
            if f[0] == f[1] || f[1] == f[2] || f[0] == f[2] {
                // Remove this face from all its vertices
                for &vi in &faces[fi] {
                    vertex_faces[vi].remove(&fi);
                }
                live_faces -= 1;
                faces_removed += 1;
            } else {
                vertex_faces[v0].insert(fi);
            }
        }

        // Re-insert affected edges
        let neighbors: HashSet<usize> = vertex_faces[v0]
            .iter()
            .flat_map(|&fi| faces[fi].iter().copied())
            .filter(|&vi| vi != v0 && collapsed_into[vi].is_none())
            .collect();

        for &nb in &neighbors {
            heap.push(compute_collapse(v0, nb, &quadrics, &positions, &generation));
        }

        if let Some(cb) = &progress {
            if faces_to_remove > 0 {
                cb(faces_removed as f32 / faces_to_remove as f32);
            }
        }
    }

    // Compact: build new vertex/face arrays excluding collapsed data
    let mut vertex_remap: Vec<Option<u32>> = vec![None; vertex_count];
    let mut new_positions: Vec<[f32; 3]> = Vec::new();
    let mut new_normals: Vec<[f32; 3]> = Vec::new();
    let mut new_colors: Vec<[f32; 4]> = Vec::new();

    for (old_i, remap) in vertex_remap.iter_mut().enumerate() {
        if collapsed_into[old_i].is_some() {
            continue; // Vertex was collapsed away
        }
        if vertex_faces[old_i].is_empty() {
            continue; // No remaining faces reference this vertex
        }
        let new_i = new_positions.len() as u32;
        *remap = Some(new_i);
        new_positions.push([
            positions[old_i][0] as f32,
            positions[old_i][1] as f32,
            positions[old_i][2] as f32,
        ]);
        if old_i < mesh.normals.len() {
            new_normals.push(mesh.normals[old_i]);
        }
        if old_i < mesh.colors.len() {
            new_colors.push(mesh.colors[old_i]);
        }
    }

    let mut new_indices: Vec<u32> = Vec::new();
    for face in faces.iter() {
        // Skip degenerate faces
        if face[0] == face[1] || face[1] == face[2] || face[0] == face[2] {
            continue;
        }
        // Skip faces whose vertices were all removed
        let i0 = vertex_remap[face[0]];
        let i1 = vertex_remap[face[1]];
        let i2 = vertex_remap[face[2]];
        if let (Some(a), Some(b), Some(c)) = (i0, i1, i2) {
            // Skip degenerate (all same index)
            if a != b && b != c && a != c {
                new_indices.push(a);
                new_indices.push(b);
                new_indices.push(c);
            }
        }
    }

    mesh.positions = new_positions;
    mesh.normals = new_normals;
    mesh.colors = new_colors;
    mesh.indices = new_indices;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Create an octahedron mesh (6 vertices, 8 triangles).
    fn make_octahedron() -> MeshData {
        let positions = vec![
            [0.0, 1.0, 0.0],   // top
            [1.0, 0.0, 0.0],   // +x
            [0.0, 0.0, 1.0],   // +z
            [-1.0, 0.0, 0.0],  // -x
            [0.0, 0.0, -1.0],  // -z
            [0.0, -1.0, 0.0],  // bottom
        ];
        #[rustfmt::skip]
        let indices = vec![
            0,1,2, 0,2,3, 0,3,4, 0,4,1, // top 4 faces
            5,2,1, 5,3,2, 5,4,3, 5,1,4, // bottom 4 faces
        ];
        let n = positions.len();
        MeshData {
            positions,
            normals: vec![[0.0, 1.0, 0.0]; n],
            colors: vec![[1.0, 1.0, 1.0, 1.0]; n],
            indices,
        }
    }

    #[test]
    fn decimate_reduces_triangle_count() {
        let mut mesh = make_octahedron();
        let orig_tris = mesh.indices.len() / 3;
        assert_eq!(orig_tris, 8);

        decimate(&mut mesh, 0.5, None);

        let new_tris = mesh.indices.len() / 3;
        assert!(
            new_tris < orig_tris,
            "decimation should reduce triangles: {orig_tris} -> {new_tris}"
        );
    }

    #[test]
    fn decimate_ratio_one_noop() {
        let mut mesh = make_octahedron();
        let orig = mesh.positions.clone();
        let orig_indices = mesh.indices.clone();

        decimate(&mut mesh, 1.0, None);

        assert_eq!(mesh.positions.len(), orig.len());
        assert_eq!(mesh.indices.len(), orig_indices.len());
    }

    #[test]
    fn decimate_preserves_valid_indices() {
        let mut mesh = make_octahedron();
        decimate(&mut mesh, 0.5, None);

        for &idx in &mesh.indices {
            assert!(
                (idx as usize) < mesh.positions.len(),
                "index {idx} out of bounds (vertex count = {})",
                mesh.positions.len()
            );
        }
    }

    #[test]
    fn decimate_no_degenerate_triangles() {
        let mut mesh = make_octahedron();
        decimate(&mut mesh, 0.5, None);

        for tri in mesh.indices.chunks_exact(3) {
            assert_ne!(tri[0], tri[1], "degenerate triangle: {tri:?}");
            assert_ne!(tri[1], tri[2], "degenerate triangle: {tri:?}");
            assert_ne!(tri[0], tri[2], "degenerate triangle: {tri:?}");
        }
    }

    #[test]
    fn decimate_handles_small_mesh() {
        // Single triangle — should not crash
        let mut mesh = MeshData {
            positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            normals: vec![[0.0, 0.0, 1.0]; 3],
            colors: vec![[1.0, 1.0, 1.0, 1.0]; 3],
            indices: vec![0, 1, 2],
        };
        decimate(&mut mesh, 0.5, None);
        // Should not crash; may or may not reduce (single triangle is minimal)
    }

    #[test]
    fn decimate_empty_mesh_noop() {
        let mut mesh = MeshData {
            positions: vec![],
            normals: vec![],
            colors: vec![],
            indices: vec![],
        };
        decimate(&mut mesh, 0.5, None);
        assert!(mesh.positions.is_empty());
    }
}
