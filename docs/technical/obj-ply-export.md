# OBJ & PLY Export

## Overview

ModernFractalViewer supports three mesh export formats: glTF Binary (.glb), Wavefront OBJ (.obj), and Stanford PLY (.ply). This article covers the OBJ and PLY implementations. Both are simple, widely-supported formats that complement glTF's PBR material system with broader tool compatibility.

| Format | Encoding | Colors | Normals | Best For |
|--------|----------|--------|---------|----------|
| glTF Binary (.glb) | Binary | Vertex RGBA | Yes | Viewers, game engines, PBR rendering |
| Wavefront OBJ (.obj) | Text | No | Yes | Universal compatibility, text editing |
| Stanford PLY (.ply) | Binary LE | Vertex RGBA | Yes | 3D printing, point cloud tools |

## Wavefront OBJ (.obj)

### Format Anatomy

OBJ is a text-based format with a simple line-oriented structure:

```
# Comment line
v  x y z          # Vertex position
vn nx ny nz       # Vertex normal
f  v1//n1 v2//n2 v3//n3   # Face (vertex//normal indices)
```

Key characteristics:
- **1-based indexing** -- the first vertex is index 1, not 0
- **Text format** -- human-readable, easy to debug, but larger files
- **No color support** -- OBJ has no standard for per-vertex colors (MTL materials are separate)
- **v//vn notation** -- vertex and normal share the same index (no texture coordinates)

### Code Walkthrough

**File:** `crates/fractal-core/src/mesh/obj_export.rs`

```rust
pub fn export_obj(mesh: &MeshData, path: &Path) -> Result<(), ObjExportError> {
    if mesh.positions.is_empty() || mesh.indices.is_empty() {
        return Err(ObjExportError::EmptyMesh);
    }

    let file = std::fs::File::create(path)?;
    let mut w = std::io::BufWriter::new(file);

    // Header comments
    writeln!(w, "# ModernFractalViewer mesh export")?;
    writeln!(w, "# Vertices: {}", mesh.positions.len())?;
    writeln!(w, "# Triangles: {}", mesh.indices.len() / 3)?;

    // Vertex positions
    for p in &mesh.positions {
        writeln!(w, "v {:.6} {:.6} {:.6}", p[0], p[1], p[2])?;
    }

    // Vertex normals
    if !mesh.normals.is_empty() {
        for n in &mesh.normals {
            writeln!(w, "vn {:.6} {:.6} {:.6}", n[0], n[1], n[2])?;
        }
    }

    // Faces (1-based indexing)
    for tri in mesh.indices.chunks_exact(3) {
        let (i0, i1, i2) = (tri[0] + 1, tri[1] + 1, tri[2] + 1);
        writeln!(w, "f {i0}//{i0} {i1}//{i1} {i2}//{i2}")?;
    }

    w.flush()?;
    Ok(())
}
```

#### Design Decisions

- **BufWriter:** Wrapping `File` in `BufWriter` is critical. Without it, each `writeln!` call triggers a syscall, making export of large meshes (100K+ triangles) extremely slow. `BufWriter` batches writes into 8KB chunks.

- **6 decimal places:** `{:.6}` provides ~0.001 mm precision at 1-meter scale, which is more than sufficient for 3D printing and visualization.

- **`v//vn` indexing:** Since vertex and normal indices are always the same (one normal per vertex), we use `v//vn` format (skip texture coordinate). This is the simplest face format OBJ supports.

- **1-based index conversion:** `tri[0] + 1` converts from Rust's 0-based to OBJ's 1-based indexing. A common source of off-by-one bugs in OBJ exporters.

### Error Handling

```rust
pub enum ObjExportError {
    EmptyMesh,        // No geometry to export
    Io(std::io::Error), // File creation/write failure
}
```

Empty mesh is detected early to provide a clear error message rather than creating an empty file.

## Stanford PLY (.ply)

### Format Anatomy

PLY has an ASCII header followed by binary data. Our implementation uses **binary little-endian** for compact files and fast writing:

```
ply
format binary_little_endian 1.0
comment ModernFractalViewer mesh export
element vertex <count>
property float x
property float y
property float z
property float nx
property float ny
property float nz
property uchar red
property uchar green
property uchar blue
property uchar alpha
element face <count>
property list uchar int vertex_indices
end_header
<binary vertex data>
<binary face data>
```

Key characteristics:
- **ASCII header, binary body** -- header is human-readable for inspection
- **Per-vertex RGBA colors** -- unlike OBJ, PLY natively supports vertex colors
- **0-based indexing** -- matches Rust's internal representation (no conversion needed)
- **Binary little-endian** -- matches x86/ARM architectures natively

### Vertex Data Layout

Each vertex is stored as a contiguous block of bytes:

```
| x (f32 LE) | y (f32 LE) | z (f32 LE) |    -- 12 bytes position
| nx (f32 LE) | ny (f32 LE) | nz (f32 LE) |  -- 12 bytes normal
| R (u8) | G (u8) | B (u8) | A (u8) |        -- 4 bytes color
```

Total: **28 bytes per vertex** (with normals and colors).

### Face Data Layout

Each face is:

```
| 3 (u8) | i0 (i32 LE) | i1 (i32 LE) | i2 (i32 LE) |
```

Total: **13 bytes per face** (1 byte count + 3 x 4 byte indices).

### Code Walkthrough

**File:** `crates/fractal-core/src/mesh/ply_export.rs`

```rust
pub fn export_ply(mesh: &MeshData, path: &Path) -> Result<(), PlyExportError> {
    // ... validation ...

    let file = std::fs::File::create(path)?;
    let mut w = std::io::BufWriter::new(file);

    // ASCII header
    writeln!(w, "ply")?;
    writeln!(w, "format binary_little_endian 1.0")?;
    // ... property declarations ...
    writeln!(w, "end_header")?;

    // Binary vertex data
    for i in 0..vertex_count {
        let p = mesh.positions[i];
        w.write_all(&p[0].to_le_bytes())?;
        w.write_all(&p[1].to_le_bytes())?;
        w.write_all(&p[2].to_le_bytes())?;

        if has_normals {
            let n = mesh.normals[i];
            w.write_all(&n[0].to_le_bytes())?;
            w.write_all(&n[1].to_le_bytes())?;
            w.write_all(&n[2].to_le_bytes())?;
        }

        if has_colors {
            let c = mesh.colors[i];
            w.write_all(&[(c[0] * 255.0) as u8])?;  // float [0,1] -> u8 [0,255]
            w.write_all(&[(c[1] * 255.0) as u8])?;
            w.write_all(&[(c[2] * 255.0) as u8])?;
            w.write_all(&[(c[3] * 255.0) as u8])?;
        }
    }

    // Binary face data
    for tri in mesh.indices.chunks_exact(3) {
        w.write_all(&[3u8])?;  // 3 vertices per face
        w.write_all(&(tri[0] as i32).to_le_bytes())?;
        w.write_all(&(tri[1] as i32).to_le_bytes())?;
        w.write_all(&(tri[2] as i32).to_le_bytes())?;
    }

    w.flush()?;
    Ok(())
}
```

#### Design Decisions

- **Color conversion:** Internal colors are `[f32; 4]` in `[0.0, 1.0]` range. PLY expects `uchar` in `[0, 255]`. The conversion `(c * 255.0) as u8` is a simple truncation (not rounding), which is standard practice.

- **`to_le_bytes()`:** Rust's `to_le_bytes()` guarantees little-endian byte order regardless of the host architecture, ensuring correct PLY files on big-endian systems.

- **Conditional properties:** Normals and colors are only written if the mesh actually has them. The header adapts accordingly, so PLY readers know exactly which properties to expect.

- **Face count byte:** The `3u8` before each face tells the PLY reader how many vertices follow. While all our faces are triangles, this is the PLY standard for supporting mixed polygon meshes.

- **`i32` indices:** PLY specifies `int` (4-byte signed) for face indices. We cast from `u32` to `i32`, which is safe for meshes up to ~2 billion vertices.

### File Size Comparison

For a 100K-vertex, 200K-triangle mesh:

| Format | Size | Notes |
|--------|------|-------|
| OBJ | ~15 MB | Text format, 6-decimal precision |
| PLY | ~5.4 MB | Binary (28 bytes/vertex + 13 bytes/face) |
| GLB | ~4.8 MB | Binary + PBR material metadata |

PLY is ~3x smaller than OBJ for the same data. GLB is slightly smaller due to glTF's more compact binary layout.

## Choosing a Format

- **glTF Binary (.glb):** Best for visualization. Supports PBR materials, vertex colors, and is the standard for web/game engines. Use when importing into Blender, Three.js, or similar.

- **Wavefront OBJ (.obj):** Best for universal compatibility. Every 3D tool can read OBJ. Use when you need maximum portability or want to inspect/edit the mesh as text.

- **Stanford PLY (.ply):** Best for 3D printing and scientific visualization. Compact binary format with native color support. Preferred by MeshLab, CloudCompare, and many slicers.
