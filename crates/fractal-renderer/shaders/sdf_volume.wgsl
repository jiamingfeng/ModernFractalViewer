// SDF Volume Sampling Compute Shader
//
// This file is prepended with sdf_common.wgsl at load time, which provides:
//   - The `Uniforms` struct bound at group(0), binding(0) as `var<uniform> u`
//   - The `effective_iterations: u32` private variable
//   - The `map(pos: vec3<f32>) -> vec2<f32>` function (returns distance, trap)

struct VolumeParams {
    bounds_min: vec3<f32>,
    _pad0: f32,
    bounds_max: vec3<f32>,
    _pad1: f32,
    grid_size: vec3<u32>,
    // z_offset: first Z-layer in this slab (for chunked dispatch)
    z_offset: u32,
    // slab_z_count: number of Z-layers in this slab
    slab_z_count: u32,
    _pad3: u32,
    _pad4: u32,
    _pad5: u32,
}

@group(0) @binding(1) var<uniform> volume_params: VolumeParams;
@group(0) @binding(2) var<storage, read_write> volume: array<vec2<f32>>;

@compute @workgroup_size(4, 4, 4)
fn sample_volume(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let vertex_count = volume_params.grid_size + vec3<u32>(1u, 1u, 1u);

    // Guard against out-of-bounds invocations
    if (global_id.x >= vertex_count.x || global_id.y >= vertex_count.y || global_id.z >= volume_params.slab_z_count) {
        return;
    }

    // Actual Z index in the full grid
    let global_z = global_id.z + volume_params.z_offset;
    if (global_z >= vertex_count.z) {
        return;
    }

    // Compute normalized coordinates [0, 1] across the full grid vertices
    let t = vec3<f32>(
        f32(global_id.x) / f32(volume_params.grid_size.x),
        f32(global_id.y) / f32(volume_params.grid_size.y),
        f32(global_z) / f32(volume_params.grid_size.z),
    );

    // Map to world-space position within the bounding box
    let pos = mix(volume_params.bounds_min, volume_params.bounds_max, t);

    // Set iteration count from uniforms and evaluate the SDF
    effective_iterations = u.iterations;
    let result = map(pos);

    // Store at slab-local linear index: x + y * vx + local_z * vx * vy
    let index = global_id.x
        + global_id.y * vertex_count.x
        + global_id.z * vertex_count.x * vertex_count.y;

    volume[index] = result;
}
