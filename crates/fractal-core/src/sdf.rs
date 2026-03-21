//! Signed Distance Function utilities
//!
//! This module provides helper types and constants for SDF-based rendering.
//! The actual SDF implementations are in WGSL shaders for GPU execution.

use serde::{Deserialize, Serialize};

/// Ray marching configuration
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(default)]
pub struct RayMarchConfig {
    /// Maximum number of ray marching steps
    pub max_steps: u32,
    /// Distance threshold to consider a hit
    pub epsilon: f32,
    /// Maximum ray distance before giving up
    pub max_distance: f32,
    /// Number of ambient occlusion samples
    pub ao_steps: u32,
    /// Ambient occlusion intensity
    pub ao_intensity: f32,
    /// Epsilon for normal calculation (smaller = smoother normals, reduces banding)
    pub normal_epsilon: f32,
    /// Number of samples per pixel for super-sampling (1, 2, or 4)
    pub sample_count: u32,
    /// Whether continuous Level of Detail is enabled
    /// (scales epsilon with pixel footprint at distance)
    pub lod_enabled: bool,
    /// LOD aggressiveness multiplier (1.0 = pixel-exact, >1 = more aggressive)
    pub lod_scale: f32,
}

impl Default for RayMarchConfig {
    fn default() -> Self {
        Self {
            max_steps: 128,
            epsilon: 0.001,
            max_distance: 100.0,
            ao_steps: 5,
            ao_intensity: 0.2,
            normal_epsilon: 0.0001,
            sample_count: 1,
            lod_enabled: true,
            lod_scale: 1.0,
        }
    }
}

/// Lighting configuration for rendering
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(default)]
pub struct LightingConfig {
    /// Light direction (normalized)
    pub light_dir: [f32; 3],
    /// Ambient light intensity
    pub ambient: f32,
    /// Diffuse light intensity (Blinn-Phong)
    pub diffuse: f32,
    /// Specular light intensity (Blinn-Phong)
    pub specular: f32,
    /// Specular shininess exponent (Blinn-Phong)
    pub shininess: f32,
    /// Lighting model: 0 = Blinn-Phong, 1 = PBR (Cook-Torrance GGX)
    pub lighting_model: u32,
    /// Surface roughness (PBR, 0=smooth, 1=rough)
    pub roughness: f32,
    /// Metalness (PBR, 0=dielectric, 1=metal)
    pub metallic: f32,
    /// Direct light brightness (PBR)
    pub light_intensity: f32,
    /// Shadow sharpness factor (IQ's k parameter). Higher = sharper, lower = softer.
    pub shadow_softness: f32,
}

impl Default for LightingConfig {
    fn default() -> Self {
        Self {
            light_dir: [0.577, 0.577, 0.577], // normalized (1,1,1)
            ambient: 0.1,
            diffuse: 0.8,
            specular: 0.3,
            shininess: 32.0,
            lighting_model: 1,
            roughness: 0.5,
            metallic: 0.0,
            light_intensity: 1.5,
            shadow_softness: 8.0,
        }
    }
}

/// Color configuration for fractal rendering
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(default)]
pub struct ColorConfig {
    /// Base color RGB (used for solid mode)
    pub base_color: [f32; 3],
    /// Secondary color for gradients (legacy, seeded into palette)
    pub secondary_color: [f32; 3],
    /// Background color RGB
    pub background_color: [f32; 3],
    /// Color mode (0: solid, 1: orbit trap, 2: iteration, 3: normal, 4: combined)
    pub color_mode: u32,
    /// Palette colors (up to 8 RGB stops)
    pub palette_colors: [[f32; 3]; 8],
    /// Number of active palette stops (1-8)
    pub palette_count: u32,
    /// Multiplier on trap/iteration value before palette lookup
    pub palette_scale: f32,
    /// Offset added before palette lookup
    pub palette_offset: f32,
    /// Dither strength (0.0 = off, 1.0 = normal, up to 2.0)
    pub dither_strength: f32,
    /// Index of selected palette preset (for UI tracking)
    pub palette_preset: usize,
}

impl Default for ColorConfig {
    fn default() -> Self {
        let preset = &PALETTE_PRESETS[0];
        let mut palette_colors = [[0.0; 3]; 8];
        for (i, c) in preset.colors.iter().enumerate() {
            palette_colors[i] = *c;
        }
        Self {
            base_color: preset.colors[0],
            secondary_color: preset.colors[preset.colors.len() - 1],
            background_color: [0.05, 0.05, 0.1],
            color_mode: 1, // orbit trap
            palette_colors,
            palette_count: preset.colors.len() as u32,
            palette_scale: 1.6,
            palette_offset: 0.0,
            dither_strength: 1.0,
            palette_preset: 0,
        }
    }
}

/// A named palette preset
pub struct PalettePreset {
    pub name: &'static str,
    pub colors: &'static [[f32; 3]],
}

pub const PALETTE_PRESETS: &[PalettePreset] = &[
    PalettePreset {
        name: "Inferno",
        colors: &[
            [0.0, 0.0, 0.04],
            [0.28, 0.06, 0.38],
            [0.72, 0.16, 0.29],
            [0.99, 0.56, 0.13],
            [0.98, 0.99, 0.64],
        ],
    },
    PalettePreset {
        name: "Ocean",
        colors: &[
            [0.0, 0.05, 0.15],
            [0.0, 0.2, 0.5],
            [0.0, 0.5, 0.7],
            [0.3, 0.8, 0.8],
            [0.9, 0.95, 1.0],
        ],
    },
    PalettePreset {
        name: "Sunset",
        colors: &[
            [0.1, 0.0, 0.2],
            [0.5, 0.0, 0.4],
            [0.9, 0.2, 0.2],
            [1.0, 0.6, 0.1],
            [1.0, 0.95, 0.5],
        ],
    },
    PalettePreset {
        name: "Magma",
        colors: &[
            [0.0, 0.0, 0.02],
            [0.27, 0.05, 0.35],
            [0.65, 0.14, 0.35],
            [0.97, 0.41, 0.22],
            [0.99, 0.82, 0.53],
        ],
    },
    PalettePreset {
        name: "Viridis",
        colors: &[
            [0.27, 0.0, 0.33],
            [0.28, 0.23, 0.51],
            [0.13, 0.44, 0.51],
            [0.37, 0.65, 0.37],
            [0.99, 0.91, 0.15],
        ],
    },
    PalettePreset {
        name: "Classic",
        colors: &[
            [0.8, 0.3, 0.1],
            [0.1, 0.4, 0.8],
        ],
    },
    PalettePreset {
        name: "Fire",
        colors: &[
            [0.0, 0.0, 0.0],
            [0.5, 0.0, 0.0],
            [1.0, 0.3, 0.0],
            [1.0, 0.7, 0.0],
            [1.0, 1.0, 0.5],
            [1.0, 1.0, 1.0],
        ],
    },
    PalettePreset {
        name: "Ice",
        colors: &[
            [0.0, 0.0, 0.1],
            [0.1, 0.2, 0.5],
            [0.3, 0.5, 0.8],
            [0.6, 0.8, 1.0],
            [0.9, 0.95, 1.0],
        ],
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_palette_presets_count() {
        assert_eq!(PALETTE_PRESETS.len(), 8);
    }

    #[test]
    fn test_palette_presets_have_names() {
        for preset in PALETTE_PRESETS {
            assert!(!preset.name.is_empty());
        }
    }

    #[test]
    fn test_palette_presets_colors_in_range() {
        for preset in PALETTE_PRESETS {
            for color in preset.colors {
                for &channel in color {
                    assert!(
                        (0.0..=1.0).contains(&channel),
                        "Preset '{}' has out-of-range color value: {}",
                        preset.name,
                        channel
                    );
                }
            }
        }
    }

    #[test]
    fn test_palette_presets_min_colors() {
        for preset in PALETTE_PRESETS {
            assert!(
                preset.colors.len() >= 2,
                "Preset '{}' has fewer than 2 colors",
                preset.name
            );
        }
    }

    #[test]
    fn test_palette_presets_max_colors() {
        for preset in PALETTE_PRESETS {
            assert!(
                preset.colors.len() <= 8,
                "Preset '{}' has more than 8 colors",
                preset.name
            );
        }
    }

    #[test]
    fn test_serde_roundtrip_configs() {
        let rm = RayMarchConfig::default();
        let json = serde_json::to_string(&rm).unwrap();
        let rm2: RayMarchConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(rm.max_steps, rm2.max_steps);
        assert_eq!(rm.epsilon, rm2.epsilon);

        let lc = LightingConfig::default();
        let json = serde_json::to_string(&lc).unwrap();
        let lc2: LightingConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(lc.ambient, lc2.ambient);

        let cc = ColorConfig::default();
        let json = serde_json::to_string(&cc).unwrap();
        let cc2: ColorConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(cc.color_mode, cc2.color_mode);
        assert_eq!(cc.palette_scale, cc2.palette_scale);
    }
}
