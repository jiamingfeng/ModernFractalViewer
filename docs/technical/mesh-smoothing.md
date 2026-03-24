# Mesh Smoothing

## Problem

Fractal SDFs produce inherently noisy surfaces. The chaotic, self-similar geometry of fractals like the Mandelbulb means that even at high resolution, extracted meshes have high-frequency bumps and jagged edges that don't correspond to meaningful surface detail -- they're sampling artifacts from the discrete voxel grid.

Smoothing reduces this noise while preserving the overall shape. Two methods are implemented: **Laplacian** (simple but causes shrinkage) and **Taubin** (volume-preserving).

## Laplacian Smoothing

### Algorithm

For each vertex, compute the average position of its neighbors and move toward it:

```
v_new = v + lambda * (avg(neighbors) - v)
```

where `lambda` is the smoothing strength in `(0, 1]`.

This is equivalent to applying a low-pass filter to the mesh surface. Each iteration removes the highest-frequency component of the surface detail.

### The Shrinkage Problem

Laplacian smoothing has a well-known flaw: it causes **mesh shrinkage**. Every vertex moves toward its neighbors' centroid, which means convex regions shrink and the overall volume decreases with each iteration.

For a cube mesh after 5 iterations with `lambda = 0.5`, the bounding box volume decreases noticeably. The cube becomes a rounded blob that's smaller than the original.

This is unacceptable for fractal meshes where maintaining the overall scale is important for 3D printing or visualization accuracy.

## Taubin Smoothing (Volume-Preserving)

### Algorithm

Taubin (1995) solved the shrinkage problem by alternating two smoothing passes per iteration:

1. **Shrink pass:** Apply Laplacian with positive `lambda` (moves vertices inward)
2. **Inflate pass:** Apply Laplacian with negative `mu` (moves vertices outward)

The key insight is choosing `|mu| > |lambda|` so the inflate step slightly over-compensates for the shrinkage, keeping the mesh volume stable.

Standard formula:

```
mu = -(lambda + 0.01)
```

For `lambda = 0.5`:
- Shrink pass: `lambda = +0.5`
- Inflate pass: `mu = -0.51`

The inflate step is 2% stronger than the shrink step, which is just enough to counteract the cumulative shrinkage without causing volume growth.

### Why 0.01?

The offset `0.01` is a heuristic from Taubin's original paper. Too small (e.g., `0.001`) and shrinkage still accumulates over many iterations. Too large (e.g., `0.1`) and the mesh inflates, developing outward bumps. The value `0.01` provides stable behavior across 1-10 iterations for typical mesh densities.

## Code Walkthrough

**File:** `crates/fractal-core/src/mesh/smoothing.rs`

### Adjacency Building

Before smoothing, we need to know each vertex's neighbors. This is built from the triangle index buffer:

```rust
fn build_adjacency(vertex_count: usize, indices: &[u32]) -> Vec<Vec<usize>> {
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); vertex_count];
    for tri in indices.chunks_exact(3) {
        let a = tri[0] as usize;
        let b = tri[1] as usize;
        let c = tri[2] as usize;
        for &(i, j) in &[(a, b), (b, c), (c, a)] {
            if !adj[i].contains(&j) { adj[i].push(j); }
            if !adj[j].contains(&i) { adj[j].push(i); }
        }
    }
    adj
}
```

Each triangle contributes 3 edges. The `contains()` check prevents duplicate neighbor entries. For typical mesh vertex valences (5-7 neighbors), the linear scan is faster than a HashSet.

### The Smooth Step

A single Laplacian pass that can be used with either positive (shrink) or negative (inflate) lambda:

```rust
fn smooth_step(positions: &mut [[f32; 3]], adj: &[Vec<usize>], lambda: f32) {
    let old: Vec<[f32; 3]> = positions.to_vec();
    for (i, neighbors) in adj.iter().enumerate() {
        if neighbors.is_empty() { continue; }
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
        positions[i][0] = old[i][0] + lambda * (avg[0] - old[i][0]);
        positions[i][1] = old[i][1] + lambda * (avg[1] - old[i][1]);
        positions[i][2] = old[i][2] + lambda * (avg[2] - old[i][2]);
    }
}
```

The `old` snapshot is critical -- all vertices read from the *previous* state and write to the *current* state. Without this, vertices processed later in the loop would be influenced by already-smoothed earlier vertices, creating asymmetric results.

### Laplacian Smooth

```rust
pub fn laplacian_smooth(mesh: &mut MeshData, iterations: u32, lambda: f32) {
    if iterations == 0 || mesh.positions.is_empty() { return; }
    let adj = build_adjacency(mesh.positions.len(), &mesh.indices);
    for _ in 0..iterations {
        smooth_step(&mut mesh.positions, &adj, lambda);
    }
}
```

### Taubin Smooth

```rust
pub fn taubin_smooth(mesh: &mut MeshData, iterations: u32, lambda: f32) {
    if iterations == 0 || mesh.positions.is_empty() { return; }
    let mu = -(lambda + 0.01);
    let adj = build_adjacency(mesh.positions.len(), &mesh.indices);
    for _ in 0..iterations {
        smooth_step(&mut mesh.positions, &adj, lambda);  // shrink
        smooth_step(&mut mesh.positions, &adj, mu);       // inflate
    }
}
```

Each Taubin iteration performs **two** `smooth_step` calls, so it's approximately 2x the cost of Laplacian per iteration.

## Configuration Tradeoffs

### Iterations

| Iterations | Effect | Export Time Impact |
|------------|--------|--------------------|
| 1-3        | Subtle smoothing, removes voxel artifacts | Fast (negligible) |
| 4-5        | Moderate smoothing, visually clean | Noticeable on large meshes |
| 6-10       | Heavy smoothing, may lose fine detail | Significantly slower |

The time complexity is `O(iterations * vertices)` per smoothing pass. At resolution 256 with ~200K vertices, each iteration takes a few milliseconds. At resolution 512 with ~1M vertices, iterations become noticeable.

### Lambda

| Lambda | Effect |
|--------|--------|
| 0.1-0.3 | Subtle -- preserves detail, minimal noise reduction |
| 0.4-0.6 | Balanced -- good noise reduction, retains major features |
| 0.7-1.0 | Aggressive -- strong smoothing, may soften sharp edges |

Lambda does **not** affect export time (same number of operations per iteration regardless of lambda value). The default `lambda = 0.5` provides a good balance.

### Method Choice

- **None:** No smoothing. Use when the SDF is already clean or when preserving every detail matters.
- **Laplacian:** Simple, fast, but shrinks the mesh. Acceptable for meshes where slight size reduction doesn't matter.
- **Taubin:** Recommended for most use cases. Same smoothing quality as Laplacian without the volume loss. Default choice.

## Properties

Smoothing does **not** modify:
- Triangle connectivity (indices remain unchanged)
- Vertex count
- Normals (these should be recomputed after smoothing if needed)
- Colors

Only vertex positions are updated.
