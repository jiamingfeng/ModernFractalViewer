//! Stanford PLY mesh export (binary little-endian).
//!
//! Writes a `.ply` file with vertex positions, normals, per-vertex RGBA
//! colors, and triangle face definitions. Binary format for smaller files
//! and faster loading in 3D printing / modeling software.

use super::MeshData;
use std::io::Write;
use std::path::Path;

/// Errors that can occur during PLY export.
#[derive(Debug)]
pub enum PlyExportError {
    /// The mesh contains no geometry.
    EmptyMesh,
    /// An I/O error occurred while writing.
    Io(std::io::Error),
}

impl std::fmt::Display for PlyExportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PlyExportError::EmptyMesh => write!(f, "mesh has no vertices"),
            PlyExportError::Io(e) => write!(f, "I/O error: {e}"),
        }
    }
}

impl std::error::Error for PlyExportError {}

impl From<std::io::Error> for PlyExportError {
    fn from(e: std::io::Error) -> Self {
        PlyExportError::Io(e)
    }
}

/// Export a mesh as a binary little-endian PLY file.
///
/// Vertex properties: `x y z nx ny nz red green blue alpha` (float + uchar).
/// Face property: `vertex_indices` as a list of 3 `int` values.
pub fn export_ply(mesh: &MeshData, path: &Path) -> Result<(), PlyExportError> {
    if mesh.positions.is_empty() || mesh.indices.is_empty() {
        return Err(PlyExportError::EmptyMesh);
    }

    let vertex_count = mesh.positions.len();
    let face_count = mesh.indices.len() / 3;
    let has_normals = mesh.normals.len() == vertex_count;
    let has_colors = mesh.colors.len() == vertex_count;

    let file = std::fs::File::create(path)?;
    let mut w = std::io::BufWriter::new(file);

    // PLY header (ASCII)
    writeln!(w, "ply")?;
    writeln!(w, "format binary_little_endian 1.0")?;
    writeln!(w, "comment ModernFractalViewer mesh export")?;
    writeln!(w, "element vertex {vertex_count}")?;
    writeln!(w, "property float x")?;
    writeln!(w, "property float y")?;
    writeln!(w, "property float z")?;
    if has_normals {
        writeln!(w, "property float nx")?;
        writeln!(w, "property float ny")?;
        writeln!(w, "property float nz")?;
    }
    if has_colors {
        writeln!(w, "property uchar red")?;
        writeln!(w, "property uchar green")?;
        writeln!(w, "property uchar blue")?;
        writeln!(w, "property uchar alpha")?;
    }
    writeln!(w, "element face {face_count}")?;
    writeln!(w, "property list uchar int vertex_indices")?;
    writeln!(w, "end_header")?;

    // Vertex data (binary)
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
            w.write_all(&[(c[0] * 255.0) as u8])?;
            w.write_all(&[(c[1] * 255.0) as u8])?;
            w.write_all(&[(c[2] * 255.0) as u8])?;
            w.write_all(&[(c[3] * 255.0) as u8])?;
        }
    }

    // Face data (binary): each face = 1 byte count (3) + 3 × i32 indices
    for tri in mesh.indices.chunks_exact(3) {
        w.write_all(&[3u8])?; // vertex count per face
        w.write_all(&(tri[0] as i32).to_le_bytes())?;
        w.write_all(&(tri[1] as i32).to_le_bytes())?;
        w.write_all(&(tri[2] as i32).to_le_bytes())?;
    }

    w.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_triangle_mesh() -> MeshData {
        MeshData {
            positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            normals: vec![[0.0, 0.0, 1.0]; 3],
            colors: vec![[1.0, 0.0, 0.5, 1.0]; 3],
            indices: vec![0, 1, 2],
        }
    }

    #[test]
    fn ply_export_creates_valid_file() {
        let mesh = make_triangle_mesh();
        let path = std::env::temp_dir().join("test_export.ply");

        export_ply(&mesh, &path).unwrap();

        let data = std::fs::read(&path).unwrap();
        let content = String::from_utf8_lossy(&data);

        // Header should be valid PLY
        assert!(content.starts_with("ply\n"));
        assert!(content.contains("format binary_little_endian 1.0"));
        assert!(content.contains("element vertex 3"));
        assert!(content.contains("element face 1"));
        assert!(content.contains("end_header"));

        // File should have content after header
        let header_end = data
            .windows(11)
            .position(|w| w == b"end_header\n")
            .unwrap()
            + 11;
        let binary_data = &data[header_end..];

        // 3 vertices × (3 floats pos + 3 floats normal + 4 bytes color) = 3 × (12+12+4) = 84
        // 1 face × (1 byte count + 3 × 4 bytes index) = 13
        assert_eq!(binary_data.len(), 84 + 13);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn empty_mesh_returns_error() {
        let mesh = MeshData {
            positions: vec![],
            normals: vec![],
            colors: vec![],
            indices: vec![],
        };
        let path = std::env::temp_dir().join("test_empty.ply");
        assert!(export_ply(&mesh, &path).is_err());
    }
}
