// Fractal Ray Marcher Shader
// WGSL shader for rendering 3D fractals using ray marching
// Transpiled to SPIR-V (Vulkan), MSL (Metal), HLSL (DX12) by Naga

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

    // Reserved
    _reserved0: vec4<f32>,
    _reserved1: vec4<f32>,
    _reserved2: vec4<f32>,
    _reserved3: vec4<f32>,
    _reserved4: vec4<f32>,
    _reserved5: vec4<f32>,
    _reserved6: vec4<f32>,
}

@group(0) @binding(0)
var<uniform> u: Uniforms;

// ============================================================================
// VERTEX SHADER - Fullscreen Triangle
// ============================================================================

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    // Generate fullscreen triangle (3 vertices, no vertex buffer needed)
    // Technique: Use vertex_index to compute positions that cover screen
    var out: VertexOutput;
    
    // Triangle vertices: (-1,-1), (3,-1), (-1,3)
    // This covers the entire [-1,1] x [-1,1] clip space
    let x = f32(i32(vertex_index & 1u) * 4 - 1);
    let y = f32(i32(vertex_index >> 1u) * 4 - 1);
    
    out.position = vec4<f32>(x, y, 0.0, 1.0);
    out.uv = vec2<f32>((x + 1.0) * 0.5, (1.0 - y) * 0.5);
    
    return out;
}

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

// Mandelbulb SDF
// Based on the formula by Daniel White and Paul Nylander
fn sdf_mandelbulb(pos: vec3<f32>) -> vec2<f32> {
    var z = pos;
    var dr = 1.0;
    var r = 0.0;
    var trap = 1e10; // Orbit trap for coloring
    
    let power = u.power;
    let iterations = u.iterations;
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
    let iterations = u.iterations;
    var trap = 1e10;
    
    // Start with a box
    var d = sdf_box(z, vec3<f32>(1.0));
    var s = 1.0;
    
    for (var i = 0u; i < iterations; i = i + 1u) {
        // Fold space
        let a = z * s % 2.0 - 1.0;
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
    
    let iterations = u.iterations;
    
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
    let iterations = u.iterations;
    
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
    let iterations = u.iterations;
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
    let iterations = u.iterations;
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

// Helper: Box SDF
fn sdf_box(p: vec3<f32>, b: vec3<f32>) -> f32 {
    let q = abs(p) - b;
    return length(max(q, vec3<f32>(0.0))) + min(max(q.x, max(q.y, q.z)), 0.0);
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

// ============================================================================
// RAY MARCHING
// ============================================================================

struct RayMarchResult {
    hit: bool,
    distance: f32,
    steps: u32,
    trap: f32,
}

fn ray_march(ro: vec3<f32>, rd: vec3<f32>) -> RayMarchResult {
    var result: RayMarchResult;
    result.hit = false;
    result.distance = 0.0;
    result.steps = 0u;
    result.trap = 0.0;
    
    var t = 0.0;
    let max_steps = u.max_steps;
    let epsilon = u.epsilon;
    let max_distance = u.max_distance;
    
    for (var i = 0u; i < max_steps; i = i + 1u) {
        let pos = ro + rd * t;
        let res = map(pos);
        let d = res.x;
        
        if (d < epsilon) {
            result.hit = true;
            result.distance = t;
            result.steps = i;
            result.trap = res.y;
            return result;
        }
        
        if (t > max_distance) {
            break;
        }
        
        t = t + d * 0.9; // Slight relaxation for stability
        result.steps = i;
    }
    
    result.distance = t;
    return result;
}

// ============================================================================
// NORMAL CALCULATION
// ============================================================================

fn calc_normal(pos: vec3<f32>) -> vec3<f32> {
    let e = vec2<f32>(u.epsilon, 0.0);
    return normalize(vec3<f32>(
        map(pos + e.xyy).x - map(pos - e.xyy).x,
        map(pos + e.yxy).x - map(pos - e.yxy).x,
        map(pos + e.yyx).x - map(pos - e.yyx).x
    ));
}

// ============================================================================
// AMBIENT OCCLUSION
// ============================================================================

fn calc_ao(pos: vec3<f32>, nor: vec3<f32>) -> f32 {
    var occ = 0.0;
    var sca = 1.0;
    let ao_steps = u.ao_steps;
    
    for (var i = 0u; i < ao_steps; i = i + 1u) {
        let h = 0.01 + 0.12 * f32(i) / f32(ao_steps);
        let d = map(pos + h * nor).x;
        occ = occ + (h - d) * sca;
        sca = sca * 0.95;
    }
    
    return clamp(1.0 - u.ao_intensity * occ, 0.0, 1.0);
}

// ============================================================================
// SOFT SHADOWS
// ============================================================================

fn calc_shadow(ro: vec3<f32>, rd: vec3<f32>) -> f32 {
    var res = 1.0;
    var t = 0.01;
    
    for (var i = 0u; i < 32u; i = i + 1u) {
        let h = map(ro + rd * t).x;
        res = min(res, 8.0 * h / t);
        t = t + clamp(h, 0.02, 0.1);
        if (res < 0.001 || t > 5.0) {
            break;
        }
    }
    
    return clamp(res, 0.0, 1.0);
}

// ============================================================================
// DITHERING (eliminates 8-bit color banding)
// ============================================================================

// Integer hash for screen-space dithering (Wang hash variant)
fn dither_hash(p: vec2<u32>, frame: u32) -> f32 {
    var x = p.x + p.y * 1597u + frame * 3571u;
    x = (x ^ (x >> 16u)) * 0x45d9f3bu;
    x = (x ^ (x >> 16u)) * 0x45d9f3bu;
    x = x ^ (x >> 16u);
    return f32(x) / 4294967295.0;
}

// Triangular-distribution dither: zero-mean, lower variance than uniform
fn triangular_dither(pixel: vec2<u32>, frame: u32) -> f32 {
    let r0 = dither_hash(pixel, frame);
    let r1 = dither_hash(pixel, frame + 1000000u);
    return (r0 + r1) - 1.0; // range [-1, 1], triangular distribution
}

// ============================================================================
// PALETTE SAMPLING
// ============================================================================

fn get_palette_color(index: u32) -> vec3<f32> {
    let i = min(index, max(u.palette_count, 1u) - 1u);
    switch i {
        case 0u: { return u.palette_0.xyz; }
        case 1u: { return u.palette_1.xyz; }
        case 2u: { return u.palette_2.xyz; }
        case 3u: { return u.palette_3.xyz; }
        case 4u: { return u.palette_4.xyz; }
        case 5u: { return u.palette_5.xyz; }
        case 6u: { return u.palette_6.xyz; }
        case 7u: { return u.palette_7.xyz; }
        default: { return u.palette_0.xyz; }
    }
}

fn sample_palette(t_raw: f32) -> vec3<f32> {
    let count = u.palette_count;
    if (count <= 1u) {
        return u.palette_0.xyz;
    }

    // fract wraps for cyclic palettes; multiply by (count-1) to span all stops
    let t = fract(t_raw) * f32(count - 1u);
    let i = u32(floor(t));
    let f = t - floor(t);

    // Smoothstep for perceptually smoother blending
    let sf = f * f * (3.0 - 2.0 * f);

    let c0 = get_palette_color(i);
    let c1 = get_palette_color(i + 1u);
    return mix(c0, c1, sf);
}

// ============================================================================
// COLORING
// ============================================================================

fn get_color(trap: f32, nor: vec3<f32>, steps: u32) -> vec3<f32> {
    switch u.color_mode {
        // Solid color — first palette color
        case 0u: {
            return get_palette_color(0u);
        }
        // Orbit trap — palette lookup
        case 1u: {
            let t = trap * u.palette_scale + u.palette_offset;
            return sample_palette(t);
        }
        // Iteration-based — palette lookup
        case 2u: {
            let t = f32(steps) / f32(u.max_steps) * u.palette_scale + u.palette_offset;
            return sample_palette(t);
        }
        // Normal-based coloring
        case 3u: {
            return nor * 0.5 + 0.5;
        }
        // Combined orbit trap + iteration
        case 4u: {
            let trap_t = trap * u.palette_scale + u.palette_offset;
            let iter_t = f32(steps) / f32(u.max_steps);
            let t = mix(trap_t, iter_t, 0.5);
            return sample_palette(t);
        }
        default: {
            return get_palette_color(0u);
        }
    }
}

// ============================================================================
// CAMERA
// ============================================================================

fn get_ray_direction(uv: vec2<f32>) -> vec3<f32> {
    let cam_pos = u.camera_pos.xyz;
    let cam_target = u.camera_target.xyz;
    let cam_up = normalize(u.camera_up.xyz);
    
    let forward = normalize(cam_target - cam_pos);
    let right = normalize(cross(forward, cam_up));
    let up = cross(right, forward);
    
    let fov_factor = tan(u.camera_fov * 0.5);
    let aspect = u.aspect_ratio;
    
    // Convert UV from [0,1] to [-1,1]
    let ndc = uv * 2.0 - 1.0;
    
    // Apply aspect ratio and FOV
    let ray_dir = normalize(
        forward + 
        ndc.x * right * fov_factor * aspect + 
        ndc.y * up * fov_factor
    );
    
    return ray_dir;
}

// ============================================================================
// FRAGMENT SHADER
// ============================================================================

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let uv = in.uv;

    // Camera setup
    let ro = u.camera_pos.xyz;
    let rd = get_ray_direction(uv);

    // Ray march
    let result = ray_march(ro, rd);

    var col: vec3<f32>;

    if (!result.hit) {
        // Background gradient
        let bg = u.background_color.xyz;
        let grad = 0.5 + 0.5 * rd.y;
        col = bg * grad;
    } else {
        // Hit point and normal
        let pos = ro + rd * result.distance;
        let nor = calc_normal(pos);

        // Surface color from palette
        col = get_color(result.trap, nor, result.steps);

        // Lighting
        let light_dir = normalize(u.light_dir.xyz);

        // Diffuse
        let diff = max(dot(nor, light_dir), 0.0);

        // Specular (Blinn-Phong)
        let half_vec = normalize(light_dir - rd);
        let spec = pow(max(dot(nor, half_vec), 0.0), u.shininess);

        // Ambient occlusion
        let ao = calc_ao(pos, nor);

        // Soft shadow
        let shadow = calc_shadow(pos + nor * u.epsilon * 2.0, light_dir);

        // Combine lighting
        col = col * (
            u.ambient * ao +
            u.diffuse * diff * shadow +
            u.specular * spec * shadow
        );

        // Tone mapping (simple Reinhard)
        col = col / (col + vec3<f32>(1.0));

        // Gamma correction
        col = pow(col, vec3<f32>(1.0 / 2.2));
    }

    // Dithering — eliminates 8-bit banding on both background and surface
    let pixel = vec2<u32>(u32(in.position.x), u32(in.position.y));
    let dither = triangular_dither(pixel, u.frame_count) * u.dither_strength;
    col = col + vec3<f32>(dither / 255.0);

    return vec4<f32>(col, 1.0);
}
