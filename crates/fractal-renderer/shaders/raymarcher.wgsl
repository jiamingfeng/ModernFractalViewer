// Fractal Ray Marcher Shader
// WGSL shader for rendering 3D fractals using ray marching
// Transpiled to SPIR-V (Vulkan), MSL (Metal), HLSL (DX12) by Naga
//
// NOTE: This file is prepended with sdf_common.wgsl at load time, which
// provides the Uniforms struct, SDF functions, and map() dispatcher.

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

    // Continuous LOD: pixel angular size determines minimum resolvable detail.
    // When lod_enabled == 0, lod_factor is 0.0 so adaptive_epsilon == epsilon.
    // See: iquilezles.org/articles/raymarchingdf
    let fov_factor = tan(u.camera_fov * 0.5);
    let pixel_angular_size = 2.0 * fov_factor / u.resolution.y;
    let lod_factor = f32(u.lod_enabled) * u.lod_scale * pixel_angular_size;

    effective_iterations = u.iterations;

    for (var i = 0u; i < max_steps; i = i + 1u) {
        // LOD iteration reduction: fewer SDF iterations when pixel footprint is
        // large relative to epsilon. Each removed iteration halves the detail
        // frequency, filtering sub-pixel chaos from the SDF itself.
        if (u.lod_enabled != 0u) {
            let pixel_footprint = t * pixel_angular_size;
            let lod_ratio = pixel_footprint * u.lod_scale / epsilon;
            let reduce = u32(clamp(log2(max(1.0, lod_ratio)), 0.0, f32(u.iterations) - 3.0));
            effective_iterations = u.iterations - reduce;
        }

        let pos = ro + rd * t;
        let res = map(pos);
        let d = res.x;

        // Adaptive epsilon: grows linearly with distance when LOD is enabled
        let adaptive_epsilon = epsilon + t * lod_factor;

        if (d < adaptive_epsilon) {
            result.hit = true;
            result.distance = t;
            result.steps = i;
            result.trap = res.y;
            return result;
        }

        if (t > max_distance) {
            break;
        }

        // Minimum step size prevents micro-stepping through noisy SDF regions
        let min_step = select(0.0, adaptive_epsilon * 0.2, u.lod_enabled != 0u);
        t = t + max(d * 0.9, min_step);
        result.steps = i;
    }

    result.distance = t;
    return result;
}

// ============================================================================
// NORMAL CALCULATION
// ============================================================================

fn calc_normal(pos: vec3<f32>, t: f32) -> vec3<f32> {
    // Tetrahedron technique (4 SDF evals instead of 6) with distance-proportional
    // epsilon for LoD filtering. Prevents banding at far distances and aliasing
    // at near distances. See: iquilezles.org/articles/normalsSDF
    // When LOD is enabled, clamp h to at least the pixel footprint AND half the
    // adaptive hit epsilon, so normals never resolve finer than the hit detection.
    let pixel_h = 2.0 * tan(u.camera_fov * 0.5) / u.resolution.y * t;
    let adaptive_eps = u.epsilon + t * f32(u.lod_enabled) * u.lod_scale
                       * 2.0 * tan(u.camera_fov * 0.5) / u.resolution.y;
    let lod_h = select(0.0, max(pixel_h, adaptive_eps * 0.5), u.lod_enabled != 0u);
    let h = max(max(u.normal_epsilon * t, lod_h), 1e-7);
    let k = vec2<f32>(1.0, -1.0);
    return normalize(
        k.xyy * map(pos + k.xyy * h).x +
        k.yyx * map(pos + k.yyx * h).x +
        k.yxy * map(pos + k.yxy * h).x +
        k.xxx * map(pos + k.xxx * h).x
    );
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
    // See: iquilezles.org/articles/rmshadows
    // k = shadow_softness: higher = sharper/harder shadows, lower = softer.
    // Typical range: 2 (very soft) to 32+ (sharp).
    var res = 1.0;
    var t = 0.01;
    let k = u.shadow_softness;

    for (var i = 0u; i < 64u; i = i + 1u) {
        let h = map(ro + t * rd).x;
        res = min(res, h * k / t);
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
    effective_iterations = u.iterations;
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
        // Set LOD iterations at hit distance for normal/AO/shadow evaluation
        if (u.lod_enabled != 0u) {
            let pixel_angular_size = 2.0 * tan(u.camera_fov * 0.5) / u.resolution.y;
            let pixel_footprint = result.distance * pixel_angular_size;
            let lod_ratio = pixel_footprint * u.lod_scale / u.epsilon;
            let reduce = u32(clamp(log2(max(1.0, lod_ratio)), 0.0, f32(u.iterations) - 3.0));
            effective_iterations = u.iterations - reduce;
        }

        // Hit point and normal
        let pos = ro + rd * result.distance;
        let nor = calc_normal(pos, result.distance);

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
