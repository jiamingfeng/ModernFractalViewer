# Mesh Decimation (Quadric Error Metrics)

## Problem

Mesh extraction from SDF volumes produces high triangle counts -- a 256-resolution grid can easily generate 500K+ triangles. This is too many for:

- **3D printing** slicers that struggle with large meshes
- **Real-time rendering** in external viewers
- **File size** -- a 500K-triangle GLB can be 20+ MB

We need to reduce triangle count while preserving shape quality. Naive approaches (e.g., random edge removal) destroy important features. We need a method that knows *which* edges matter.

## Algorithm: Garland & Heckbert (1997)

The **Quadric Error Metrics (QEM)** algorithm assigns each vertex an error metric that measures how far it is from its original surrounding planes. Edges are collapsed in order of increasing error -- cheap collapses first, expensive ones (at sharp features) last.

### Phase 1: Quadric Initialization

For each triangle face, compute the plane equation `ax + by + cz + d = 0` from the face normal and a vertex. From this plane, build a 4x4 symmetric matrix (the "quadric"):

```
        [ a*a  a*b  a*c  a*d ]
Q_p =   [ a*b  b*b  b*c  b*d ]
        [ a*c  b*c  c*c  c*d ]
        [ a*d  b*d  c*d  d*d ]
```

Each vertex's quadric is the **sum** of the quadrics from all faces touching that vertex:

```
Q_v = sum of Q_p for all faces containing v
```

Since the matrix is symmetric, only 10 unique values need to be stored (upper triangle):

```rust
struct Quadric([f64; 10]);
// Layout: [a00, a01, a02, a03, a11, a12, a13, a22, a23, a33]
```

The quadric error for placing a vertex at position `v = [x, y, z]` is:

```
error(v) = v^T * Q * v    (with homogeneous w=1)
```

This expands to:

```rust
q[0]*x*x + 2.0*q[1]*x*y + 2.0*q[2]*x*z + 2.0*q[3]*x
+ q[4]*y*y + 2.0*q[5]*y*z + 2.0*q[6]*y
+ q[7]*z*z + 2.0*q[8]*z
+ q[9]
```

**File:** `crates/fractal-core/src/mesh/decimation.rs`, `Quadric::evaluate()` (line 41)

### Phase 2: Priority Queue Construction

For each unique edge `(v0, v1)`, compute:

1. **Combined quadric**: `Q_combined = Q_v0 + Q_v1`
2. **Optimal position**: Solve `dQ/dv = 0` -- a 3x3 linear system
3. **Cost**: Evaluate `Q_combined` at the optimal position

The optimal position minimizes error for the merged vertex. If the 3x3 system is singular (degenerate geometry), fall back to the edge midpoint.

```rust
fn optimal_position(&self) -> Option<[f64; 3]> {
    // 3x3 system via Cramer's rule
    let det = a[0][0] * (a[1][1] * a[2][2] - a[1][2] * a[2][1])
        - a[0][1] * (a[1][0] * a[2][2] - a[1][2] * a[2][0])
        + a[0][2] * (a[1][0] * a[2][1] - a[1][1] * a[2][0]);
    if det.abs() < 1e-12 { return None; }
    // ... solve via Cramer's rule
}
```

All edges go into a min-heap (lowest cost first):

```rust
let mut heap = BinaryHeap::with_capacity(edges.len());
for &(v0, v1) in &edges {
    heap.push(compute_collapse(v0, v1, &quadrics, &positions, &generation));
}
```

**File:** `crates/fractal-core/src/mesh/decimation.rs`, lines 222-249

### Phase 3: Collapse Loop

Pop the lowest-cost edge from the heap. Merge vertex `v1` into `v0`:

1. Move `v0` to the optimal position
2. Add `v1`'s quadric to `v0`'s quadric
3. Mark `v1` as collapsed into `v0`
4. Update all faces that referenced `v1` to reference `v0`
5. Remove degenerate faces (where two or more vertices became identical)
6. Re-insert affected edges into the heap with updated costs

```rust
// Perform collapse: merge v1 into v0
positions[v0] = collapse.target;
quadrics[v0] = quadrics[v0].add(&quadrics[v1]);
generation[v0] += 1;
collapsed_into[v1] = Some(v0);
```

The loop continues until the live face count reaches the target:

```rust
while live_faces > target_faces {
    let collapse = match heap.pop() { ... };
    // ... validate, collapse, update
}
```

### Phase 4: Lazy Deletion

When an edge is collapsed, all neighboring edges become stale (their costs were computed with old vertex positions). Rather than removing and reinserting every affected entry, we use **generation counters**:

- Each vertex has a generation number, incremented on every collapse
- Each heap entry stores the generation of both vertices at creation time
- When popping, if the stored generations don't match current ones, the entry is stale -- skip it

```rust
if generation[v0] != collapse.gen0 || generation[v1] != collapse.gen1 {
    continue; // stale entry
}
```

Additionally, collapsed vertices form chains (A -> B -> C). A `resolve()` function follows the chain to find the ultimate target:

```rust
fn resolve(collapsed_into: &[Option<usize>], mut v: usize) -> usize {
    while let Some(target) = collapsed_into[v] {
        v = target;
    }
    v
}
```

### Phase 5: Compaction

After all collapses, the vertex and face arrays have holes (collapsed vertices, degenerate faces). The final step builds compact arrays:

1. Iterate all vertices; skip those that were collapsed or have no remaining faces
2. Build a remap table: `old_index -> new_index`
3. Copy positions (from f64 back to f32), normals, and colors for surviving vertices
4. Rebuild the index buffer using the remap table, skipping degenerate faces

```rust
let mut vertex_remap: Vec<Option<u32>> = vec![None; vertex_count];
for (old_i, remap) in vertex_remap.iter_mut().enumerate() {
    if collapsed_into[old_i].is_some() { continue; }
    if vertex_faces[old_i].is_empty() { continue; }
    *remap = Some(new_positions.len() as u32);
    new_positions.push([...]);
}
```

## Precision: Why f64?

The quadric math involves summing many plane equations. With f32, accumulated error can cause:
- Spurious negative costs (the quadric becomes non-positive-definite)
- Cramer's rule produces NaN from near-zero determinants
- Optimal positions jump to wildly wrong locations

Using f64 internally and converting back to f32 only at compaction eliminates these issues with negligible performance cost (the bottleneck is the heap operations, not the arithmetic).

## API

```rust
pub fn decimate(
    mesh: &mut MeshData,
    target_ratio: f32,        // 0.01-1.0: fraction of triangles to keep
    progress: Option<&dyn Fn(f32)>,
)
```

- `target_ratio = 0.5` keeps ~50% of triangles
- `target_ratio = 0.1` keeps ~10% (aggressive simplification)
- `target_ratio >= 0.999` is a no-op

**File:** `crates/fractal-core/src/mesh/decimation.rs`

## Visual Intuition

Think of the quadric as a "cost field" around each vertex. A vertex at the intersection of two flat planes has a low-cost valley along the edge line. The QEM naturally preserves:

- **Sharp edges** -- high cost to collapse because the surrounding planes diverge
- **Flat regions** -- low cost to collapse because neighboring planes are nearly coplanar
- **Fine details** -- intermediate cost, collapsed only when the budget demands it

The result is that flat regions simplify aggressively while sharp features and fine details are preserved as long as the triangle budget allows.
