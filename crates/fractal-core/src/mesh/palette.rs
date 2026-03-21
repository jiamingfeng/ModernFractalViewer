//! CPU-side palette sampling and vertex color computation.
//! Mirrors the WGSL sample_palette() and get_color() functions.

use crate::sdf::ColorConfig;

/// Catmull-Rom palette sampling, matching raymarcher.wgsl sample_palette().
pub fn sample_palette(t_raw: f32, palette: &[[f32; 4]], count: u32) -> [f32; 3] {
    if count <= 1 {
        return [palette[0][0], palette[0][1], palette[0][2]];
    }

    let t = t_raw.fract() * (count - 1) as f32;
    let i = t.floor() as u32;
    let f = t - t.floor();

    let get = |idx: u32| -> [f32; 3] {
        let idx = idx.min(count.saturating_sub(1)) as usize;
        [palette[idx][0], palette[idx][1], palette[idx][2]]
    };

    let p0 = get(if i > 0 { i - 1 } else { 0 });
    let p1 = get(i);
    let p2 = get(i + 1);
    let p3 = get((i + 2).min(count - 1));

    let f2 = f * f;
    let f3 = f2 * f;

    // Catmull-Rom basis
    let mut result = [0.0f32; 3];
    for c in 0..3 {
        result[c] = 0.5
            * ((2.0 * p1[c])
                + (-p0[c] + p2[c]) * f
                + (2.0 * p0[c] - 5.0 * p1[c] + 4.0 * p2[c] - p3[c]) * f2
                + (-p0[c] + 3.0 * p1[c] - 3.0 * p2[c] + p3[c]) * f3);
        result[c] = result[c].clamp(0.0, 1.0);
    }

    result
}

/// Compute vertex color matching the shader's get_color() function.
/// Returns RGBA with alpha = 1.0.
pub fn get_vertex_color(
    trap: f32,
    normal: [f32; 3],
    color_config: &ColorConfig,
    palette: &[[f32; 4]],
) -> [f32; 4] {
    let count = color_config.palette_count;
    let scale = color_config.palette_scale;
    let offset = color_config.palette_offset;

    let rgb = match color_config.color_mode {
        // Solid color -- first palette color
        0 => {
            let idx = 0.min(count.saturating_sub(1) as usize);
            [palette[idx][0], palette[idx][1], palette[idx][2]]
        }
        // Orbit trap -- palette lookup
        1 => {
            let t = trap * scale + offset;
            sample_palette(t, palette, count)
        }
        // Iteration-based -- approximate with trap value
        2 => {
            let t = trap * scale + offset;
            sample_palette(t, palette, count)
        }
        // Normal-based coloring
        3 => [
            normal[0] * 0.5 + 0.5,
            normal[1] * 0.5 + 0.5,
            normal[2] * 0.5 + 0.5,
        ],
        // Combined orbit trap + iteration (approximate)
        4 => {
            let t = trap * scale + offset;
            sample_palette(t, palette, count)
        }
        _ => {
            let idx = 0.min(count.saturating_sub(1) as usize);
            [palette[idx][0], palette[idx][1], palette[idx][2]]
        }
    };

    [rgb[0], rgb[1], rgb[2], 1.0]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_palette(colors: &[[f32; 3]]) -> Vec<[f32; 4]> {
        colors.iter().map(|c| [c[0], c[1], c[2], 1.0]).collect()
    }

    #[test]
    fn single_color_palette() {
        let palette = make_palette(&[[0.5, 0.3, 0.8]]);
        let result = sample_palette(0.0, &palette, 1);
        assert_eq!(result, [0.5, 0.3, 0.8]);

        let result = sample_palette(0.5, &palette, 1);
        assert_eq!(result, [0.5, 0.3, 0.8]);

        let result = sample_palette(1.0, &palette, 1);
        assert_eq!(result, [0.5, 0.3, 0.8]);
    }

    #[test]
    fn two_color_endpoints() {
        let palette = make_palette(&[[0.0, 0.0, 0.0], [1.0, 1.0, 1.0]]);

        // t=0.0 => fract(0.0)=0.0 => first color
        let result = sample_palette(0.0, &palette, 2);
        // At f=0, Catmull-Rom gives p1 exactly
        assert!((result[0] - 0.0).abs() < 1e-5, "got {:?}", result);

        // t close to 1.0 should approach the second color
        let result = sample_palette(0.999, &palette, 2);
        assert!(result[0] > 0.9, "got {:?}", result);
        assert!(result[1] > 0.9, "got {:?}", result);
        assert!(result[2] > 0.9, "got {:?}", result);
    }

    #[test]
    fn catmull_rom_differs_from_linear() {
        // With 4+ stops, Catmull-Rom should differ from linear interpolation at midpoints
        let palette = make_palette(&[
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
        ]);

        // Midpoint between second and third color (t = 0.5 => fract(0.5)*3 = 1.5)
        let result = sample_palette(0.5, &palette, 4);

        // Linear interp at midpoint between [1,0,0] and [0,1,0] would be [0.5, 0.5, 0.0]
        // Catmull-Rom should differ due to neighboring stop influence
        let linear_r = 0.5;
        let linear_g = 0.5;
        let diff_r = (result[0] - linear_r).abs();
        let diff_g = (result[1] - linear_g).abs();
        assert!(
            diff_r > 0.01 || diff_g > 0.01,
            "Catmull-Rom should differ from linear at midpoint, got {:?}",
            result
        );
    }

    #[test]
    fn wrap_behavior() {
        let palette = make_palette(&[[1.0, 0.0, 0.0], [0.0, 0.0, 1.0]]);

        // t=1.0 should wrap via fract => 0.0 => first color
        let at_zero = sample_palette(0.0, &palette, 2);
        let at_one = sample_palette(1.0, &palette, 2);
        // fract(1.0) == 0.0, so these should match
        assert!(
            (at_zero[0] - at_one[0]).abs() < 1e-5,
            "t=0 gave {:?}, t=1.0 gave {:?}",
            at_zero,
            at_one
        );

        // t=2.5 should wrap the same as t=0.5
        let at_half = sample_palette(0.5, &palette, 2);
        let at_wrap = sample_palette(2.5, &palette, 2);
        assert!(
            (at_half[0] - at_wrap[0]).abs() < 1e-5,
            "t=0.5 gave {:?}, t=2.5 gave {:?}",
            at_half,
            at_wrap
        );
    }

    #[test]
    fn all_color_modes_valid_range() {
        let palette = make_palette(&[
            [0.2, 0.4, 0.6],
            [0.8, 0.1, 0.3],
            [0.5, 0.9, 0.2],
        ]);
        let normal = [0.577, 0.577, 0.577];

        for mode in 0..=5 {
            let config = ColorConfig {
                color_mode: mode,
                palette_count: 3,
                palette_scale: 1.0,
                palette_offset: 0.0,
                ..Default::default()
            };
            let color = get_vertex_color(0.5, normal, &config, &palette);
            for ch in 0..4 {
                assert!(
                    (0.0..=1.0).contains(&color[ch]),
                    "color_mode={} channel={} value={} out of range",
                    mode,
                    ch,
                    color[ch]
                );
            }
            // Alpha should always be 1.0
            assert_eq!(color[3], 1.0, "color_mode={} alpha not 1.0", mode);
        }
    }
}
