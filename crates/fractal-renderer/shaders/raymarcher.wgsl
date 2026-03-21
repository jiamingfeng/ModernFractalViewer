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

    // Reserved (76 bytes at offset 436)
    _res_a: f32,
    _res_b: f32,
    _res_c: f32,
    _reserved1: vec4<f32>,
    _reserved2: vec4<f32>,
    _reserved3: vec4<f32>,
    _reserved4: vec4<f32>,
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
    
    var t = u.near_clip;
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
    let e = vec2<f32>(u.normal_epsilon, 0.0);
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
    // Improved soft shadows with inner penumbra (IQ, nurof3n).
    // Allows the ray to penetrate the SDF for smooth contact shadows.
    // w = shadow_softness (higher = softer, lower = harder).
    var res = 1.0;
    var t = 0.01;
    let w = u.shadow_softness;

    for (var i = 0u; i < 64u; i = i + 1u) {
        let h = map(ro + t * rd).x;
        res = min(res, h / (w * t));
        t = t + clamp(h, 0.005, 0.50);
        if (res < -1.0 || t > 20.0) {
            break;
        }
    }

    res = max(res, -1.0);
    // Smooth remap from [-1, 1] to [0, 1]
    return 0.25 * (1.0 + res) * (1.0 + res) * (2.0 - res);
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

    // Catmull-Rom spline: uses 4 control points for C1-continuous interpolation
    // across stop boundaries, much smoother than pairwise smoothstep
    let p0 = get_palette_color(select(0u, i - 1u, i > 0u));
    let p1 = get_palette_color(i);
    let p2 = get_palette_color(i + 1u);
    let p3 = get_palette_color(min(i + 2u, count - 1u));

    let f2 = f * f;
    let f3 = f2 * f;

    // Catmull-Rom basis: 0.5 * [(-p0+3p1-3p2+p3)t^3 + (2p0-5p1+4p2-p3)t^2 + (-p0+p2)t + 2p1]
    let result = 0.5 * (
        (2.0 * p1) +
        (-p0 + p2) * f +
        (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * f2 +
        (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * f3
    );

    // Clamp to [0,1] since Catmull-Rom can overshoot
    return clamp(result, vec3<f32>(0.0), vec3<f32>(1.0));
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

// Render a single sample for a given UV coordinate
// ============================================================================
// PBR LIGHTING (Cook-Torrance GGX)
// ============================================================================

fn shade_pbr(
    albedo: vec3<f32>,
    nor: vec3<f32>,
    light_dir: vec3<f32>,
    view_dir: vec3<f32>,
    ao: f32,
    shadow: f32,
) -> vec3<f32> {
    let half_vec = normalize(light_dir + view_dir);
    let NdotL = max(dot(nor, light_dir), 0.0);
    let NdotV = max(dot(nor, view_dir), 0.001);
    let NdotH = max(dot(nor, half_vec), 0.0);
    let VdotH = max(dot(view_dir, half_vec), 0.0);

    let roughness = clamp(u.roughness, 0.04, 1.0);
    let metallic = u.metallic;

    // GGX Normal Distribution Function (Trowbridge-Reitz)
    let a = roughness * roughness;
    let a2 = a * a;
    let denom_d = NdotH * NdotH * (a2 - 1.0) + 1.0;
    let D = a2 / (3.14159265 * denom_d * denom_d);

    // Schlick Fresnel approximation
    let F0 = mix(vec3<f32>(0.04), albedo, metallic);
    let F = F0 + (1.0 - F0) * pow(1.0 - VdotH, 5.0);

    // Smith GGX Geometry Function (Schlick-Beckmann approximation)
    let k = (roughness + 1.0) * (roughness + 1.0) / 8.0;
    let G1_L = NdotL / (NdotL * (1.0 - k) + k);
    let G1_V = NdotV / (NdotV * (1.0 - k) + k);
    let G = G1_L * G1_V;

    // Cook-Torrance specular BRDF
    let specular_brdf = (D * F * G) / max(4.0 * NdotL * NdotV, 0.001);

    // Energy-conserving Lambert diffuse
    let kD = (vec3<f32>(1.0) - F) * (1.0 - metallic);
    let diffuse = kD * albedo / 3.14159265;

    // Combine: ambient + (diffuse + specular) * direct light
    return u.ambient * albedo * ao
        + (diffuse + specular_brdf) * NdotL * shadow * u.light_intensity;
}

// ============================================================================
// FRAGMENT SHADER — render a single sample
// ============================================================================

fn render_sample(uv: vec2<f32>) -> vec3<f32> {
    let ro = u.camera_pos.xyz;
    let rd = get_ray_direction(uv);
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

        // Shared lighting setup
        let light_dir = normalize(u.light_dir.xyz);
        let view_dir = -rd;
        let ao = calc_ao(pos, nor);
        let shadow = calc_shadow(pos + nor * u.epsilon * 2.0, light_dir);

        if (u.lighting_model == 1u) {
            // PBR Lighting (Cook-Torrance GGX microfacet BRDF)
            col = shade_pbr(col, nor, light_dir, view_dir, ao, shadow);
        } else {
            // Blinn-Phong Lighting (default)
            let diff = max(dot(nor, light_dir), 0.0);
            let half_vec = normalize(light_dir + view_dir);
            let spec = pow(max(dot(nor, half_vec), 0.0), u.shininess);

            col = col * (
                u.ambient * ao +
                u.diffuse * diff * shadow +
                u.specular * spec * shadow
            );
        }

        // Tone mapping (simple Reinhard)
        col = col / (col + vec3<f32>(1.0));

        // Gamma correction
        col = pow(col, vec3<f32>(1.0 / 2.2));
    }

    return col;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let pixel_size = 1.0 / u.resolution;
    var col = vec3<f32>(0.0);

    if (u.sample_count <= 1u) {
        // Fast path: single sample, no overhead
        col = render_sample(in.uv);
    } else if (u.sample_count == 2u) {
        // 2x: diagonal offsets
        col += render_sample(in.uv + vec2<f32>(-0.25, -0.25) * pixel_size);
        col += render_sample(in.uv + vec2<f32>( 0.25,  0.25) * pixel_size);
        col *= 0.5;
    } else {
        // 4x: Rotated Grid Super-Sampling (RGSS)
        col += render_sample(in.uv + vec2<f32>(-0.375, -0.125) * pixel_size);
        col += render_sample(in.uv + vec2<f32>( 0.125, -0.375) * pixel_size);
        col += render_sample(in.uv + vec2<f32>( 0.375,  0.125) * pixel_size);
        col += render_sample(in.uv + vec2<f32>(-0.125,  0.375) * pixel_size);
        col *= 0.25;
    }

    // Per-channel dithering — independent noise per RGB channel eliminates
    // 8-bit banding on dark monochromatic surfaces (e.g. deep purples/blues)
    let pixel = vec2<u32>(u32(in.position.x), u32(in.position.y));
    let dr = triangular_dither(pixel, u.frame_count) * u.dither_strength;
    let dg = triangular_dither(pixel, u.frame_count + 7919u) * u.dither_strength;
    let db = triangular_dither(pixel, u.frame_count + 15887u) * u.dither_strength;
    col = col + vec3<f32>(dr, dg, db) / 255.0;

    return vec4<f32>(col, 1.0);
}
