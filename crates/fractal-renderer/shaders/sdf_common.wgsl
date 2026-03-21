// Shared SDF definitions for fractal rendering and mesh export
// This file is concatenated with raymarcher.wgsl (render) or sdf_volume.wgsl (compute)
// by the Rust shader loader. Do NOT add entry points here.

// ============================================================================
// UNIFORMS
// ============================================================================

struct Uniforms {
    // Camera
    camera_pos: vec4<f32>,
    camera_target: vec4<f32>,
    camera_up: vec4<f32>,
    camera_fov: f32,
    aspect_ratio: f32,
    _pad1: vec2<f32>,

    // Resolution and time
    resolution: vec2<f32>,
    time: f32,
    _pad2: f32,

    // Fractal parameters
    fractal_type: u32,
    power: f32,
    iterations: u32,
    bailout: f32,

    scale: f32,
    fold_limit: f32,
    min_radius_sq: f32,
    _pad3: f32,

    julia_c: vec4<f32>,

    // Ray marching config
    max_steps: u32,
    epsilon: f32,
    max_distance: f32,
    ao_steps: u32,
    ao_intensity: f32,
    _pad4a: f32,
    _pad4b: f32,
    _pad4c: f32,

    // Lighting
    light_dir: vec4<f32>,
    ambient: f32,
    diffuse: f32,
    specular: f32,
    shininess: f32,

    // Colors
    base_color: vec4<f32>,
    secondary_color: vec4<f32>,
    background_color: vec4<f32>,
    color_mode: u32,
    palette_count: u32,
    palette_scale: f32,
    palette_offset: f32,

    // Palette (8 color stops)
    palette_0: vec4<f32>,
    palette_1: vec4<f32>,
    palette_2: vec4<f32>,
    palette_3: vec4<f32>,
    palette_4: vec4<f32>,
    palette_5: vec4<f32>,
    palette_6: vec4<f32>,
    palette_7: vec4<f32>,

    // Dithering
    frame_count: u32,
    dither_strength: f32,
    _pad6: vec2<f32>,

    // Rendering extras
    normal_epsilon: f32,
    sample_count: u32,
    near_clip: f32,
    _pad7: f32,

    // PBR / lighting model (20 bytes at offset 416)
    lighting_model: u32,       // 0 = Blinn-Phong, 1 = PBR
    roughness: f32,
    metallic: f32,
    light_intensity: f32,
    shadow_softness: f32,

    // LOD (8 bytes at offset 436)
    lod_enabled: u32,
    lod_scale: f32,

    // Reserved (68 bytes at offset 444)
    _res_c: f32,
    _reserved1: vec4<f32>,
    _reserved2: vec4<f32>,
    _reserved3: vec4<f32>,
    _reserved4: vec4<f32>,
}

@group(0) @binding(0)
var<uniform> u: Uniforms;

// LOD-controlled iteration count for SDF evaluation.
// Set by ray_march() and render_sample() based on pixel footprint.
var<private> effective_iterations: u32;

// ============================================================================
// DOUBLE-SINGLE PRECISION EMULATION (for deep zoom)
// ============================================================================

// Double-single representation: value = hi + lo
// Provides ~14 digits of precision using two f32 values
struct DS {
    hi: f32,
    lo: f32,
}

fn ds_from_f32(a: f32) -> DS {
    return DS(a, 0.0);
}

// Knuth's TwoSum algorithm for error-free addition
fn ds_add(a: DS, b: DS) -> DS {
    let s = a.hi + b.hi;
    let v = s - a.hi;
    let e = (a.hi - (s - v)) + (b.hi - v);
    let lo = e + a.lo + b.lo;
    let hi = s + lo;
    return DS(hi, lo - (hi - s));
}

fn ds_sub(a: DS, b: DS) -> DS {
    return ds_add(a, DS(-b.hi, -b.lo));
}

// Veltkamp splitting for accurate multiplication
fn ds_mul(a: DS, b: DS) -> DS {
    let p = a.hi * b.hi;
    // Use fma for better precision if available
    let e = a.hi * b.hi - p; // Error term (approximated)
    let lo = e + a.hi * b.lo + a.lo * b.hi;
    let hi = p + lo;
    return DS(hi, lo - (hi - p));
}

// ============================================================================
// SIGNED DISTANCE FUNCTIONS (SDFs)
// ============================================================================

// Helper: Box SDF
fn sdf_box(p: vec3<f32>, b: vec3<f32>) -> f32 {
    let q = abs(p) - b;
    return length(max(q, vec3<f32>(0.0))) + min(max(q.x, max(q.y, q.z)), 0.0);
}

// Mandelbulb SDF
// Based on the formula by Daniel White and Paul Nylander
fn sdf_mandelbulb(pos: vec3<f32>) -> vec2<f32> {
    var z = pos;
    var dr = 1.0;
    var r = 0.0;
    var trap = 1e10; // Orbit trap for coloring

    let power = u.power;
    let iterations = effective_iterations;
    let bailout = u.bailout;

    for (var i = 0u; i < iterations; i = i + 1u) {
        r = length(z);
        if (r > bailout) {
            break;
        }

        // Orbit trap (distance to origin)
        trap = min(trap, r);

        // Convert to polar coordinates
        let theta = acos(z.z / r);
        let phi = atan2(z.y, z.x);

        // Scale and rotate
        dr = pow(r, power - 1.0) * power * dr + 1.0;

        // Scale
        let zr = pow(r, power);
        let new_theta = theta * power;
        let new_phi = phi * power;

        // Convert back to Cartesian
        z = zr * vec3<f32>(
            sin(new_theta) * cos(new_phi),
            sin(new_phi) * sin(new_theta),
            cos(new_theta)
        );
        z = z + pos;
    }

    // Distance estimation
    let dist = 0.5 * log(r) * r / dr;
    return vec2<f32>(dist, trap);
}

// Menger Sponge SDF
fn sdf_menger(pos: vec3<f32>) -> vec2<f32> {
    var z = pos;
    let iterations = effective_iterations;
    var trap = 1e10;

    // Start with a box
    var d = sdf_box(z, vec3<f32>(1.0));
    var s = 1.0;

    for (var i = 0u; i < iterations; i = i + 1u) {
        // Fold space
        let a = (z * s % 2.0 + 2.0) % 2.0 - 1.0;
        s = s * 3.0;
        let r = abs(1.0 - 3.0 * abs(a));

        trap = min(trap, length(r));

        // Cross
        let da = max(r.x, r.y);
        let db = max(r.y, r.z);
        let dc = max(r.z, r.x);
        let c = (min(da, min(db, dc)) - 1.0) / s;
        d = max(d, c);
    }

    return vec2<f32>(d, trap);
}

// Julia 3D (Quaternion Julia set)
fn sdf_julia(pos: vec3<f32>) -> vec2<f32> {
    var z = vec4<f32>(pos, 0.0);
    let c = u.julia_c;
    var dz = 1.0;
    var trap = 1e10;

    let iterations = effective_iterations;

    for (var i = 0u; i < iterations; i = i + 1u) {
        // Update running derivative: dz = 2 * |z| * dz
        dz = 2.0 * length(z) * dz;

        // Quaternion multiplication: z = z^2 + c
        let temp = z;
        z = vec4<f32>(
            temp.x * temp.x - temp.y * temp.y - temp.z * temp.z - temp.w * temp.w,
            2.0 * temp.x * temp.y,
            2.0 * temp.x * temp.z,
            2.0 * temp.x * temp.w
        ) + c;

        let m2 = dot(z, z);
        trap = min(trap, m2);

        if (m2 > 256.0) {
            break;
        }
    }

    // Distance estimation: d = 0.5 * |z| * log(|z|) / |dz|
    let r = length(z);
    let d = 0.5 * r * log(r) / max(dz, 1e-10);
    return vec2<f32>(d, trap);
}

// Mandelbox SDF
fn sdf_mandelbox(pos: vec3<f32>) -> vec2<f32> {
    var z = pos;
    var dr = 1.0;
    var trap = 1e10;

    let scale = u.scale;
    let fold_limit = u.fold_limit;
    let min_radius_sq = u.min_radius_sq;
    let iterations = effective_iterations;

    for (var i = 0u; i < iterations; i = i + 1u) {
        // Box fold
        z = clamp(z, vec3<f32>(-fold_limit), vec3<f32>(fold_limit)) * 2.0 - z;

        // Sphere fold
        let r2 = dot(z, z);
        trap = min(trap, r2);

        if (r2 < min_radius_sq) {
            let t = 1.0 / min_radius_sq;
            z = z * t;
            dr = dr * t;
        } else if (r2 < 1.0) {
            let t = 1.0 / r2;
            z = z * t;
            dr = dr * t;
        }

        z = scale * z + pos;
        dr = dr * abs(scale) + 1.0;
    }

    let dist = length(z) / abs(dr);
    return vec2<f32>(dist, sqrt(trap));
}

// Sierpinski Tetrahedron SDF
fn sdf_sierpinski(pos: vec3<f32>) -> vec2<f32> {
    var z = pos;
    let iterations = effective_iterations;
    let scale = u.scale;
    var trap = 1e10;

    // Vertices of a tetrahedron
    let a1 = vec3<f32>(1.0, 1.0, 1.0);
    let a2 = vec3<f32>(-1.0, -1.0, 1.0);
    let a3 = vec3<f32>(1.0, -1.0, -1.0);
    let a4 = vec3<f32>(-1.0, 1.0, -1.0);

    var n = 0u;
    for (var i = 0u; i < iterations; i = i + 1u) {
        // Fold towards each vertex
        var c = a1;
        var dist = length(z - a1);
        var d = length(z - a2);
        if (d < dist) { c = a2; dist = d; }
        d = length(z - a3);
        if (d < dist) { c = a3; dist = d; }
        d = length(z - a4);
        if (d < dist) { c = a4; }

        trap = min(trap, length(z));
        z = scale * z - c * (scale - 1.0);
        n = n + 1u;
    }

    let final_dist = (length(z) - 2.0) * pow(scale, -f32(n));
    return vec2<f32>(final_dist, trap);
}

// Apollonian Gasket SDF (sphere packing fractal)
fn sdf_apollonian(pos: vec3<f32>) -> vec2<f32> {
    var z = pos;
    let iterations = effective_iterations;
    var trap = 1e10;
    var s = 1.0;

    for (var i = 0u; i < iterations; i = i + 1u) {
        // Fold to positive octant
        z = abs(z);

        // Sort coordinates
        if (z.x < z.y) { z = z.yxz; }
        if (z.x < z.z) { z = z.zyx; }
        if (z.y < z.z) { z = z.xzy; }

        trap = min(trap, length(z));

        // Scale and translate
        z = z * 3.0 - vec3<f32>(2.0, 2.0, 2.0);
        if (z.z < -1.0) { z.z = z.z + 2.0; }

        s = s * 3.0;
    }

    let dist = (length(z) - 0.5) / s;
    return vec2<f32>(dist, trap);
}

// ============================================================================
// MAIN SDF DISPATCHER
// ============================================================================

fn map(pos: vec3<f32>) -> vec2<f32> {
    // Returns vec2(distance, trap value for coloring)
    switch u.fractal_type {
        case 0u: { return sdf_mandelbulb(pos); }
        case 1u: { return sdf_menger(pos); }
        case 2u: { return sdf_julia(pos); }
        case 3u: { return sdf_mandelbox(pos); }
        case 4u: { return sdf_sierpinski(pos); }
        case 5u: { return sdf_apollonian(pos); }
        default: { return sdf_mandelbulb(pos); }
    }
}
