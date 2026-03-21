//! glTF 2.0 binary (.glb) export for [`MeshData`](super::MeshData).

use std::io::Write;
use std::path::Path;

use gltf_json::validation::Checked;
use gltf_json::validation::USize64;

/// Errors that can occur during GLB export.
#[derive(Debug)]
pub enum ExportError {
    /// The mesh has no vertices.
    EmptyMesh,
    /// An I/O error occurred while writing the file.
    Io(std::io::Error),
    /// Failed to serialize the glTF JSON chunk.
    Json(gltf_json::Error),
}

impl std::fmt::Display for ExportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExportError::EmptyMesh => write!(f, "Cannot export empty mesh"),
            ExportError::Io(e) => write!(f, "I/O error: {e}"),
            ExportError::Json(e) => write!(f, "JSON error: {e}"),
        }
    }
}

impl From<std::io::Error> for ExportError {
    fn from(e: std::io::Error) -> Self {
        ExportError::Io(e)
    }
}

impl From<gltf_json::Error> for ExportError {
    fn from(e: gltf_json::Error) -> Self {
        ExportError::Json(e)
    }
}

/// Export a [`super::MeshData`] as a binary glTF 2.0 (`.glb`) file.
///
/// The resulting file contains a single mesh with POSITION, NORMAL,
/// optional COLOR_0 (when `mesh.colors` is non-empty), and u32 indices.
pub fn export_glb(mesh: &super::MeshData, path: &Path) -> Result<(), ExportError> {
    if mesh.positions.is_empty() {
        return Err(ExportError::EmptyMesh);
    }

    let glb_bytes = build_glb(mesh)?;

    let mut file = std::fs::File::create(path)?;
    file.write_all(&glb_bytes)?;
    Ok(())
}

/// Build the complete GLB byte buffer in memory.
fn build_glb(mesh: &super::MeshData) -> Result<Vec<u8>, ExportError> {
    let has_colors = !mesh.colors.is_empty();
    let vertex_count = mesh.positions.len();
    let index_count = mesh.indices.len();

    // ── Build binary buffer ────────────────────────────────────────────
    let positions_bytes = vertex_count * 12; // 3 * f32
    let normals_bytes = vertex_count * 12;
    let colors_bytes = if has_colors { vertex_count * 16 } else { 0 }; // 4 * f32
    let indices_bytes = index_count * 4; // u32

    let total_bin = positions_bytes + normals_bytes + colors_bytes + indices_bytes;
    let mut bin = Vec::with_capacity(total_bin);

    // Positions
    for p in &mesh.positions {
        for &v in p {
            bin.extend_from_slice(&v.to_le_bytes());
        }
    }
    // Normals
    for n in &mesh.normals {
        for &v in n {
            bin.extend_from_slice(&v.to_le_bytes());
        }
    }
    // Colors
    if has_colors {
        for c in &mesh.colors {
            for &v in c {
                bin.extend_from_slice(&v.to_le_bytes());
            }
        }
    }
    // Indices
    for &i in &mesh.indices {
        bin.extend_from_slice(&i.to_le_bytes());
    }

    debug_assert_eq!(bin.len(), total_bin);

    // ── Byte offsets for buffer views ──────────────────────────────────
    let positions_offset = 0usize;
    let normals_offset = positions_bytes;
    let colors_offset = normals_offset + normals_bytes;
    let indices_offset = if has_colors {
        colors_offset + colors_bytes
    } else {
        normals_offset + normals_bytes
    };

    // ── Compute position min/max ───────────────────────────────────────
    let (pos_min, pos_max) = compute_bounding_box(&mesh.positions);

    // ── Build glTF JSON structure ──────────────────────────────────────
    let mut buffer_views = Vec::new();
    let mut accessors = Vec::new();
    let mut bv_index = 0u32;
    let mut acc_index = 0u32;

    let extras: gltf_json::Extras = Default::default();

    // Buffer view 0: positions
    buffer_views.push(gltf_json::buffer::View {
        buffer: gltf_json::Index::new(0),
        byte_length: USize64(positions_bytes as u64),
        byte_offset: Some(USize64(positions_offset as u64)),
        byte_stride: Some(gltf_json::buffer::Stride(12)),
        target: Some(Checked::Valid(gltf_json::buffer::Target::ArrayBuffer)),
        extensions: None,
        extras: extras.clone(),
    });
    let positions_bv = bv_index;
    bv_index += 1;

    // Buffer view 1: normals
    buffer_views.push(gltf_json::buffer::View {
        buffer: gltf_json::Index::new(0),
        byte_length: USize64(normals_bytes as u64),
        byte_offset: Some(USize64(normals_offset as u64)),
        byte_stride: Some(gltf_json::buffer::Stride(12)),
        target: Some(Checked::Valid(gltf_json::buffer::Target::ArrayBuffer)),
        extensions: None,
        extras: extras.clone(),
    });
    let normals_bv = bv_index;
    bv_index += 1;

    // Buffer view (optional): colors
    let colors_bv = if has_colors {
        buffer_views.push(gltf_json::buffer::View {
            buffer: gltf_json::Index::new(0),
            byte_length: USize64(colors_bytes as u64),
            byte_offset: Some(USize64(colors_offset as u64)),
            byte_stride: Some(gltf_json::buffer::Stride(16)),
            target: Some(Checked::Valid(gltf_json::buffer::Target::ArrayBuffer)),
            extensions: None,
            extras: extras.clone(),
        });
        let idx = bv_index;
        bv_index += 1;
        Some(idx)
    } else {
        None
    };

    // Buffer view: indices (no stride for index buffers)
    buffer_views.push(gltf_json::buffer::View {
        buffer: gltf_json::Index::new(0),
        byte_length: USize64(indices_bytes as u64),
        byte_offset: Some(USize64(indices_offset as u64)),
        byte_stride: None,
        target: Some(Checked::Valid(
            gltf_json::buffer::Target::ElementArrayBuffer,
        )),
        extensions: None,
        extras: extras.clone(),
    });
    let indices_bv = bv_index;

    // Accessor 0: POSITION (with min/max bounding box)
    accessors.push(gltf_json::Accessor {
        buffer_view: Some(gltf_json::Index::new(positions_bv)),
        byte_offset: Some(USize64(0)),
        count: USize64(vertex_count as u64),
        component_type: Checked::Valid(gltf_json::accessor::GenericComponentType(
            gltf_json::accessor::ComponentType::F32,
        )),
        type_: Checked::Valid(gltf_json::accessor::Type::Vec3),
        min: Some(gltf_json::Value::from(vec![
            gltf_json::Value::from(pos_min[0]),
            gltf_json::Value::from(pos_min[1]),
            gltf_json::Value::from(pos_min[2]),
        ])),
        max: Some(gltf_json::Value::from(vec![
            gltf_json::Value::from(pos_max[0]),
            gltf_json::Value::from(pos_max[1]),
            gltf_json::Value::from(pos_max[2]),
        ])),
        normalized: false,
        sparse: None,
        extensions: None,
        extras: extras.clone(),
    });
    let position_acc = acc_index;
    acc_index += 1;

    // Accessor 1: NORMAL
    accessors.push(gltf_json::Accessor {
        buffer_view: Some(gltf_json::Index::new(normals_bv)),
        byte_offset: Some(USize64(0)),
        count: USize64(vertex_count as u64),
        component_type: Checked::Valid(gltf_json::accessor::GenericComponentType(
            gltf_json::accessor::ComponentType::F32,
        )),
        type_: Checked::Valid(gltf_json::accessor::Type::Vec3),
        min: None,
        max: None,
        normalized: false,
        sparse: None,
        extensions: None,
        extras: extras.clone(),
    });
    let normal_acc = acc_index;
    acc_index += 1;

    // Accessor (optional): COLOR_0
    let color_acc = if has_colors {
        accessors.push(gltf_json::Accessor {
            buffer_view: Some(gltf_json::Index::new(colors_bv.unwrap())),
            byte_offset: Some(USize64(0)),
            count: USize64(vertex_count as u64),
            component_type: Checked::Valid(gltf_json::accessor::GenericComponentType(
                gltf_json::accessor::ComponentType::F32,
            )),
            type_: Checked::Valid(gltf_json::accessor::Type::Vec4),
            min: None,
            max: None,
            normalized: false,
            sparse: None,
            extensions: None,
            extras: extras.clone(),
        });
        let idx = acc_index;
        acc_index += 1;
        Some(idx)
    } else {
        None
    };

    // Accessor: indices (SCALAR / U32)
    accessors.push(gltf_json::Accessor {
        buffer_view: Some(gltf_json::Index::new(indices_bv)),
        byte_offset: Some(USize64(0)),
        count: USize64(index_count as u64),
        component_type: Checked::Valid(gltf_json::accessor::GenericComponentType(
            gltf_json::accessor::ComponentType::U32,
        )),
        type_: Checked::Valid(gltf_json::accessor::Type::Scalar),
        min: None,
        max: None,
        normalized: false,
        sparse: None,
        extensions: None,
        extras: extras.clone(),
    });
    let indices_acc = acc_index;
    let _ = acc_index; // final value unused

    // ── Primitive attributes map ───────────────────────────────────────
    let mut attributes = std::collections::BTreeMap::new();
    attributes.insert(
        Checked::Valid(gltf_json::mesh::Semantic::Positions),
        gltf_json::Index::new(position_acc),
    );
    attributes.insert(
        Checked::Valid(gltf_json::mesh::Semantic::Normals),
        gltf_json::Index::new(normal_acc),
    );
    if let Some(c_acc) = color_acc {
        attributes.insert(
            Checked::Valid(gltf_json::mesh::Semantic::Colors(0)),
            gltf_json::Index::new(c_acc),
        );
    }

    let primitive = gltf_json::mesh::Primitive {
        attributes,
        indices: Some(gltf_json::Index::new(indices_acc)),
        mode: Checked::Valid(gltf_json::mesh::Mode::Triangles),
        material: None,
        targets: None,
        extensions: None,
        extras: extras.clone(),
    };

    let gltf_mesh = gltf_json::Mesh {
        primitives: vec![primitive],
        weights: None,
        extensions: None,
        extras: extras.clone(),
    };

    let node = gltf_json::Node {
        mesh: Some(gltf_json::Index::new(0)),
        ..Default::default()
    };

    let scene = gltf_json::Scene {
        nodes: vec![gltf_json::Index::new(0)],
        extensions: None,
        extras: extras.clone(),
    };

    let root = gltf_json::Root {
        asset: gltf_json::Asset {
            version: "2.0".to_string(),
            generator: Some("fractal-core".to_string()),
            copyright: None,
            min_version: None,
            extensions: None,
            extras: extras.clone(),
        },
        scene: Some(gltf_json::Index::new(0)),
        scenes: vec![scene],
        nodes: vec![node],
        meshes: vec![gltf_mesh],
        accessors,
        buffer_views,
        buffers: vec![gltf_json::Buffer {
            byte_length: USize64(total_bin as u64),
            uri: None,
            extensions: None,
            extras: extras.clone(),
        }],
        ..Default::default()
    };

    // ── Serialize JSON ─────────────────────────────────────────────────
    let mut json_bytes = gltf_json::serialize::to_vec(&root)?;

    // Pad JSON to 4-byte alignment with spaces (0x20)
    while json_bytes.len() % 4 != 0 {
        json_bytes.push(b' ');
    }

    // Pad BIN to 4-byte alignment with zeros
    while bin.len() % 4 != 0 {
        bin.push(0);
    }

    // ── Assemble GLB ───────────────────────────────────────────────────
    let json_chunk_len = json_bytes.len() as u32;
    let bin_chunk_len = bin.len() as u32;

    // Total = 12 (header) + 8 (json chunk header) + json + 8 (bin chunk header) + bin
    let total_length = 12 + 8 + json_chunk_len + 8 + bin_chunk_len;

    let mut glb = Vec::with_capacity(total_length as usize);

    // GLB Header (12 bytes)
    glb.extend_from_slice(&0x46546C67u32.to_le_bytes()); // magic: "glTF"
    glb.extend_from_slice(&2u32.to_le_bytes()); // version: 2
    glb.extend_from_slice(&total_length.to_le_bytes()); // total length

    // JSON chunk header (8 bytes)
    glb.extend_from_slice(&json_chunk_len.to_le_bytes()); // chunk length
    glb.extend_from_slice(&0x4E4F534Au32.to_le_bytes()); // chunk type: JSON

    // JSON chunk data
    glb.extend_from_slice(&json_bytes);

    // BIN chunk header (8 bytes)
    glb.extend_from_slice(&bin_chunk_len.to_le_bytes()); // chunk length
    glb.extend_from_slice(&0x004E4942u32.to_le_bytes()); // chunk type: BIN

    // BIN chunk data
    glb.extend_from_slice(&bin);

    debug_assert_eq!(glb.len(), total_length as usize);

    Ok(glb)
}

/// Compute the axis-aligned bounding box of a set of positions.
fn compute_bounding_box(positions: &[[f32; 3]]) -> ([f32; 3], [f32; 3]) {
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];

    for p in positions {
        for i in 0..3 {
            if p[i] < min[i] {
                min[i] = p[i];
            }
            if p[i] > max[i] {
                max[i] = p[i];
            }
        }
    }

    (min, max)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::MeshData;

    /// Build a simple triangle mesh for testing.
    fn triangle_mesh() -> MeshData {
        MeshData {
            positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            normals: vec![[0.0, 0.0, 1.0], [0.0, 0.0, 1.0], [0.0, 0.0, 1.0]],
            colors: vec![],
            indices: vec![0, 1, 2],
        }
    }

    /// Build a simple triangle mesh with per-vertex colors.
    fn triangle_mesh_with_colors() -> MeshData {
        MeshData {
            positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            normals: vec![[0.0, 0.0, 1.0], [0.0, 0.0, 1.0], [0.0, 0.0, 1.0]],
            colors: vec![
                [1.0, 0.0, 0.0, 1.0],
                [0.0, 1.0, 0.0, 1.0],
                [0.0, 0.0, 1.0, 1.0],
            ],
            indices: vec![0, 1, 2],
        }
    }

    /// Helper to extract the JSON string from a GLB byte buffer.
    fn extract_json(bytes: &[u8]) -> &str {
        let json_len =
            u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]) as usize;
        let json_bytes = &bytes[20..20 + json_len];
        std::str::from_utf8(json_bytes).expect("GLB JSON chunk is not valid UTF-8")
    }

    #[test]
    fn test_valid_glb_output() {
        let mesh = triangle_mesh();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.glb");

        export_glb(&mesh, &path).unwrap();

        let bytes = std::fs::read(&path).unwrap();

        // GLB magic: "glTF" = 0x46546C67
        assert_eq!(
            u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
            0x46546C67,
            "GLB magic mismatch"
        );

        // GLB version: 2
        assert_eq!(
            u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]),
            2,
            "GLB version mismatch"
        );

        // Total length matches file size
        let total_length =
            u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]) as usize;
        assert_eq!(total_length, bytes.len(), "GLB total length mismatch");
    }

    #[test]
    fn test_empty_mesh_error() {
        let mesh = MeshData {
            positions: vec![],
            normals: vec![],
            colors: vec![],
            indices: vec![],
        };
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty.glb");

        let result = export_glb(&mesh, &path);
        assert!(result.is_err());
        assert!(
            matches!(result.unwrap_err(), ExportError::EmptyMesh),
            "Expected EmptyMesh error"
        );
    }

    #[test]
    fn test_color_attribute_present() {
        let mesh = triangle_mesh_with_colors();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("colored.glb");

        export_glb(&mesh, &path).unwrap();

        let bytes = std::fs::read(&path).unwrap();
        let json_str = extract_json(&bytes);

        assert!(
            json_str.contains("COLOR_0"),
            "JSON should contain COLOR_0 attribute, got: {json_str}"
        );
    }

    #[test]
    fn test_glb_alignment() {
        // Test with both colored and uncolored meshes
        for mesh in [triangle_mesh(), triangle_mesh_with_colors()] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("aligned.glb");

            export_glb(&mesh, &path).unwrap();

            let bytes = std::fs::read(&path).unwrap();
            assert_eq!(
                bytes.len() % 4,
                0,
                "GLB file size must be 4-byte aligned, got {} bytes",
                bytes.len()
            );
        }
    }

    #[test]
    fn test_bounding_box_in_accessor() {
        let mesh = MeshData {
            positions: vec![
                [-1.0, -2.0, -3.0],
                [4.0, 5.0, 6.0],
                [0.0, 0.0, 0.0],
            ],
            normals: vec![[0.0, 0.0, 1.0]; 3],
            colors: vec![],
            indices: vec![0, 1, 2],
        };
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bbox.glb");

        export_glb(&mesh, &path).unwrap();

        let bytes = std::fs::read(&path).unwrap();
        let json_str = extract_json(&bytes);

        // Parse the JSON and verify min/max on the position accessor
        let root: serde_json::Value = serde_json::from_str(json_str.trim()).unwrap();
        let pos_accessor = &root["accessors"][0];

        let min = pos_accessor["min"].as_array().unwrap();
        let max = pos_accessor["max"].as_array().unwrap();

        assert_eq!(min[0].as_f64().unwrap(), -1.0);
        assert_eq!(min[1].as_f64().unwrap(), -2.0);
        assert_eq!(min[2].as_f64().unwrap(), -3.0);
        assert_eq!(max[0].as_f64().unwrap(), 4.0);
        assert_eq!(max[1].as_f64().unwrap(), 5.0);
        assert_eq!(max[2].as_f64().unwrap(), 6.0);
    }

    #[test]
    fn test_no_color_attribute_when_empty() {
        let mesh = triangle_mesh(); // no colors
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("no_color.glb");

        export_glb(&mesh, &path).unwrap();

        let bytes = std::fs::read(&path).unwrap();
        let json_str = extract_json(&bytes);

        assert!(
            !json_str.contains("COLOR_0"),
            "JSON should NOT contain COLOR_0 when colors are empty"
        );
    }
}
