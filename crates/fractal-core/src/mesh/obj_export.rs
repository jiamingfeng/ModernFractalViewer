//! Wavefront OBJ mesh export.
//!
//! Writes a text-based `.obj` file with vertex positions, normals, and
//! triangle face definitions. Widely compatible with 3D modeling and
//! printing software.

use super::MeshData;
use std::io::Write;
use std::path::Path;

/// Errors that can occur during OBJ export.
#[derive(Debug)]
pub enum ObjExportError {
    /// The mesh contains no geometry.
    EmptyMesh,
    /// An I/O error occurred while writing.
    Io(std::io::Error),
}

impl std::fmt::Display for ObjExportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ObjExportError::EmptyMesh => write!(f, "mesh has no vertices"),
            ObjExportError::Io(e) => write!(f, "I/O error: {e}"),
        }
    }
}

impl std::error::Error for ObjExportError {}

impl From<std::io::Error> for ObjExportError {
    fn from(e: std::io::Error) -> Self {
        ObjExportError::Io(e)
    }
}

/// Export a mesh as a Wavefront OBJ file.
///
/// Writes vertex positions (`v`), vertex normals (`vn`), and triangular
/// faces (`f`) with `v//vn` indexing. OBJ uses 1-based indices.
pub fn export_obj(mesh: &MeshData, path: &Path) -> Result<(), ObjExportError> {
    if mesh.positions.is_empty() || mesh.indices.is_empty() {
        return Err(ObjExportError::EmptyMesh);
    }

    let file = std::fs::File::create(path)?;
    let mut w = std::io::BufWriter::new(file);

    writeln!(w, "# ModernFractalViewer mesh export")?;
    writeln!(w, "# Vertices: {}", mesh.positions.len())?;
    writeln!(w, "# Triangles: {}", mesh.indices.len() / 3)?;
    writeln!(w)?;

    // Vertex positions
    for p in &mesh.positions {
        writeln!(w, "v {:.6} {:.6} {:.6}", p[0], p[1], p[2])?;
    }
    writeln!(w)?;

    // Vertex normals
    let has_normals = !mesh.normals.is_empty();
    if has_normals {
        for n in &mesh.normals {
            writeln!(w, "vn {:.6} {:.6} {:.6}", n[0], n[1], n[2])?;
        }
        writeln!(w)?;
    }

    // Faces (1-based indexing)
    for tri in mesh.indices.chunks_exact(3) {
        let (i0, i1, i2) = (tri[0] + 1, tri[1] + 1, tri[2] + 1);
        if has_normals {
            writeln!(w, "f {i0}//{i0} {i1}//{i1} {i2}//{i2}")?;
        } else {
            writeln!(w, "f {i0} {i1} {i2}")?;
        }
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
            colors: vec![[1.0, 1.0, 1.0, 1.0]; 3],
            indices: vec![0, 1, 2],
        }
    }

    #[test]
    fn obj_export_roundtrip() {
        let mesh = make_triangle_mesh();
        let dir = std::env::temp_dir();
        let path = dir.join("test_export.obj");

        export_obj(&mesh, &path).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        // Count vertex lines
        let v_count = content.lines().filter(|l| l.starts_with("v ")).count();
        let vn_count = content.lines().filter(|l| l.starts_with("vn ")).count();
        let f_count = content.lines().filter(|l| l.starts_with("f ")).count();

        assert_eq!(v_count, 3);
        assert_eq!(vn_count, 3);
        assert_eq!(f_count, 1);

        // Verify 1-based indices in face line
        let face_line = content.lines().find(|l| l.starts_with("f ")).unwrap();
        assert!(face_line.contains("1//1"));

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
        let path = std::env::temp_dir().join("test_empty.obj");
        assert!(export_obj(&mesh, &path).is_err());
    }
}
