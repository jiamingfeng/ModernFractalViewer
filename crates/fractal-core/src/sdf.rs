//! Signed Distance Function utilities
//!
//! This module provides helper types and constants for SDF-based rendering.
//! The actual SDF implementations are in WGSL shaders for GPU execution.

use serde::{Deserialize, Serialize};

/// Ray marching configuration
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
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
        }
    }
}

/// Lighting configuration for rendering
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct LightingConfig {
    /// Light direction (normalized)
    pub light_dir: [f32; 3],
    /// Ambient light intensity
    pub ambient: f32,
    /// Diffuse light intensity
    pub diffuse: f32,
    /// Specular light intensity
    pub specular: f32,
    /// Specular shininess exponent
    pub shininess: f32,
}

impl Default for LightingConfig {
    fn default() -> Self {
        Self {
            light_dir: [0.577, 0.577, 0.577], // normalized (1,1,1)
            ambient: 0.1,
            diffuse: 0.8,
            specular: 0.3,
            shininess: 32.0,
        }
    }
}

/// Color configuration for fractal rendering
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
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
    #[serde(default)]
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
