//! Marching cubes mesh extraction from SDF volume data.
//!
//! Takes a 3D grid of SDF samples (distance + trap value) and produces
//! a triangle mesh representing the iso-surface.
//!
//! The full lookup-table implementation is pending. This stub provides the
//! correct public API so the rest of the codebase compiles and can be tested.

use super::MeshData;

/// Extract a triangle mesh from a 3D SDF volume using Marching Cubes.
///
/// # Arguments
/// * `grid` - Flat array of `[distance, trap]` pairs, length = `(dims[0]+1)*(dims[1]+1)*(dims[2]+1)`
/// * `dims` - Number of *cells* per axis `[nx, ny, nz]`; vertex count per axis is `dims[i]+1`
/// * `bounds_min` - World-space minimum corner of the sampling volume
/// * `bounds_max` - World-space maximum corner of the sampling volume
/// * `iso_level` - Iso-value for surface extraction (0.0 for standard SDFs)
/// * `compute_normals` - If true, estimate normals from SDF gradient via central differences
/// * `progress` - Optional callback receiving progress in `[0.0, 1.0]`
pub fn extract_mesh(
    grid: &[[f32; 2]],
    dims: [u32; 3],
    bounds_min: [f32; 3],
    bounds_max: [f32; 3],
    iso_level: f32,
    compute_normals: bool,
    progress: Option<&dyn Fn(f32)>,
) -> MeshData {
    let _iso_level = iso_level;
    let _compute_normals = compute_normals;

    let vx = (dims[0] + 1) as usize;
    let vy = (dims[1] + 1) as usize;
    let vz = (dims[2] + 1) as usize;
    let expected = vx * vy * vz;

    if grid.len() < expected {
        eprintln!(
            "marching_cubes: grid has {} samples but expected {} for dims {:?}",
            grid.len(),
            expected,
            dims
        );
    }

    if let Some(cb) = &progress {
        cb(0.0);
    }

    // TODO: Full Marching Cubes implementation with edge/triangle lookup tables.
    // For now, return an empty mesh so the rest of the pipeline compiles and runs.
    eprintln!(
        "marching_cubes::extract_mesh is a stub — returning empty mesh. \
         Grid {}x{}x{}, bounds [{:.2},{:.2},{:.2}]→[{:.2},{:.2},{:.2}]",
        dims[0], dims[1], dims[2],
        bounds_min[0], bounds_min[1], bounds_min[2],
        bounds_max[0], bounds_max[1], bounds_max[2],
    );

    if let Some(cb) = &progress {
        cb(1.0);
    }

    MeshData {
        positions: Vec::new(),
        normals: Vec::new(),
        colors: Vec::new(),
        indices: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_grid_returns_empty_mesh() {
        let mesh = extract_mesh(&[], [0, 0, 0], [-1.0; 3], [1.0; 3], 0.0, true, None);
        assert!(mesh.positions.is_empty());
        assert!(mesh.indices.is_empty());
    }

    #[test]
    fn stub_returns_empty_mesh() {
        // 2x2x2 cells → 3x3x3 = 27 vertices, all positive (outside surface)
        let grid = vec![[1.0f32, 0.0]; 27];
        let mesh = extract_mesh(&grid, [2, 2, 2], [-1.0; 3], [1.0; 3], 0.0, true, None);
        // Stub always returns empty — this test documents current behavior
        assert!(mesh.positions.is_empty());
    }

    #[test]
    fn progress_callback_fires() {
        use std::sync::atomic::{AtomicU32, Ordering};
        let count = AtomicU32::new(0);
        let cb = |_p: f32| {
            count.fetch_add(1, Ordering::Relaxed);
        };
        let grid = vec![[1.0f32, 0.0]; 27];
        let _ = extract_mesh(&grid, [2, 2, 2], [-1.0; 3], [1.0; 3], 0.0, true, Some(&cb));
        assert!(count.load(Ordering::Relaxed) >= 2, "should fire at least start + end");
    }
}
