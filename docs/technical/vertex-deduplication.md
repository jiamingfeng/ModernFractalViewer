# Vertex Deduplication for Marching Cubes

## Problem

In a naive Marching Cubes implementation, each cube processes its 12 edges independently and creates new vertices for every edge that crosses the iso-surface. Since adjacent cubes share edges, the same vertex gets created multiple times.

Consider a cube edge shared by 4 neighboring cells. Without deduplication, the same interpolated vertex is created 4 times -- once per cell. For a typical sphere SDF at resolution 16, this means:

- **Without dedup:** ~2,400 vertices (3 per triangle)
- **With dedup:** ~800 vertices (shared across triangles)

That's a **67% reduction** in vertex count, which directly reduces memory usage, file size, and downstream processing time (smoothing, decimation, export).

## Solution: Canonical Edge Keys

Each edge in the Marching Cubes grid can be uniquely identified by **the grid vertex it starts from** and **the axis it runs along**. Two adjacent cells that share an edge will compute the same canonical key for that edge, allowing us to detect the duplicate.

### The Cube Edge Geometry

A Marching Cubes cell at grid position `(cx, cy, cz)` has 8 corners and 12 edges. Each edge connects two corners and runs along one of 3 axes:

```
Axis 0 (X): edges 0, 2, 4, 6  -- run in the X direction
Axis 1 (Y): edges 1, 3, 5, 7  -- run in the Y direction
Axis 2 (Z): edges 8, 9, 10, 11 -- run in the Z direction
```

The canonical key is a tuple `(grid_x, grid_y, grid_z, axis)` representing the "lower" grid vertex and the axis direction:

| Edge | Corners | Axis | Canonical Key |
|------|---------|------|---------------|
| 0    | 0 -> 1  | X    | `(cx, cy, cz, 0)` |
| 1    | 1 -> 2  | Y    | `(cx+1, cy, cz, 1)` |
| 2    | 3 -> 2  | X    | `(cx, cy+1, cz, 0)` |
| 3    | 0 -> 3  | Y    | `(cx, cy, cz, 1)` |
| 4    | 4 -> 5  | X    | `(cx, cy, cz+1, 0)` |
| 5    | 5 -> 6  | Y    | `(cx+1, cy, cz+1, 1)` |
| 6    | 7 -> 6  | X    | `(cx, cy+1, cz+1, 0)` |
| 7    | 4 -> 7  | Y    | `(cx, cy, cz+1, 1)` |
| 8    | 0 -> 4  | Z    | `(cx, cy, cz, 2)` |
| 9    | 1 -> 5  | Z    | `(cx+1, cy, cz, 2)` |
| 10   | 2 -> 6  | Z    | `(cx+1, cy+1, cz, 2)` |
| 11   | 3 -> 7  | Z    | `(cx, cy+1, cz, 2)` |

### Why This Works

Cell `(cx, cy, cz)` edge 1 runs from corner 1 to corner 2 along the Y axis, starting at grid vertex `(cx+1, cy, cz)`. The neighboring cell `(cx+1, cy, cz)` has this same edge as its edge 3, running from corner 0 to corner 3 along Y, starting at... `(cx+1, cy, cz)`. Same key, same edge.

Every shared edge between two cells maps to the same canonical key, regardless of which cell processes it first.

## Code Walkthrough

**File:** `crates/fractal-core/src/mesh/marching_cubes.rs`

### The Canonical Key Function

```rust
fn edge_canonical(cx: u32, cy: u32, cz: u32, edge: usize) -> (u32, u32, u32, u8) {
    match edge {
        0  => (cx,     cy,     cz,     0), // X-axis at (cx, cy, cz)
        1  => (cx + 1, cy,     cz,     1), // Y-axis at (cx+1, cy, cz)
        2  => (cx,     cy + 1, cz,     0), // X-axis at (cx, cy+1, cz)
        3  => (cx,     cy,     cz,     1), // Y-axis at (cx, cy, cz)
        4  => (cx,     cy,     cz + 1, 0), // X-axis at (cx, cy, cz+1)
        5  => (cx + 1, cy,     cz + 1, 1), // Y-axis at (cx+1, cy, cz+1)
        6  => (cx,     cy + 1, cz + 1, 0), // X-axis at (cx, cy+1, cz+1)
        7  => (cx,     cy,     cz + 1, 1), // Y-axis at (cx, cy, cz+1)
        8  => (cx,     cy,     cz,     2), // Z-axis at (cx, cy, cz)
        9  => (cx + 1, cy,     cz,     2), // Z-axis at (cx+1, cy, cz)
        10 => (cx + 1, cy + 1, cz,     2), // Z-axis at (cx+1, cy+1, cz)
        11 => (cx,     cy + 1, cz,     2), // Z-axis at (cx, cy+1, cz)
        _  => unreachable!(),
    }
}
```

### HashMap Lookup

The main extraction loop uses a `HashMap<(u32, u32, u32, u8), u32>` mapping canonical edge keys to vertex indices:

```rust
let mut edge_vertex_map: HashMap<(u32, u32, u32, u8), u32> = HashMap::new();

// For each intersected edge in the current cell:
for e in 0..12 {
    if edge_flags & (1 << e) != 0 {
        let key = edge_canonical(cx, cy, cz, e);
        edge_indices[e] = *edge_vertex_map
            .entry(key)
            .or_insert_with(|| {
                // First time seeing this edge -- interpolate and create vertex
                let [c0, c1] = EDGE_VERTICES[e];
                let (pos, trap) = interpolate_vertex(...);
                let idx = positions.len() as u32;
                positions.push(pos);
                colors.push([trap, 0.0, 0.0, 0.0]);
                idx
            });
    }
}
```

The `entry().or_insert_with()` pattern is key:
- **First cell** to encounter this edge: inserts a new vertex, returns its index
- **Subsequent cells** sharing this edge: returns the existing vertex index (no new vertex created)

### Rolling Cleanup

The HashMap would grow unboundedly as the algorithm processes z-slices. Since edges at `z < current_z` can never be referenced by future cells, we prune them after each z-slice:

```rust
if cz > 0 {
    edge_vertex_map.retain(|&(_, _, gz, _), _| gz >= cz);
}
```

This keeps the HashMap size proportional to one z-slice rather than the entire volume, which matters at high resolutions.

### Triangle Emission

Instead of pushing 3 new vertices per triangle, the triangle emission code uses the shared vertex indices:

```rust
let tri_row = &TRI_TABLE[cube_index as usize];
let mut t = 0;
while t < 16 {
    if tri_row[t] < 0 { break; }
    let i0 = edge_indices[tri_row[t] as usize];
    let i1 = edge_indices[tri_row[t + 1] as usize];
    let i2 = edge_indices[tri_row[t + 2] as usize];
    // Reversed winding for outward-facing normals
    indices.push(i0);
    indices.push(i2);
    indices.push(i1);
    t += 3;
}
```

## Impact

| Metric | Without Dedup | With Dedup | Reduction |
|--------|--------------|------------|-----------|
| Vertices (sphere, res=16) | ~2,400 | ~800 | ~67% |
| Memory | 3x indices | shared | ~50-67% |
| Smoothing time | O(V) per iter | O(V/3) per iter | ~67% |
| File size | larger | smaller | ~40-60% |

The deduplication also improves mesh quality for downstream operations:
- **Smoothing** requires vertex adjacency -- shared vertices create proper neighbor relationships
- **Decimation** needs edge connectivity -- shared vertices ensure edges are correctly identified
- **Normal computation** via face averaging requires vertices to know all their adjacent faces
