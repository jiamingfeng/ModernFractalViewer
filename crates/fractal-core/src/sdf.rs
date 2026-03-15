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
}

impl Default for RayMarchConfig {
    fn default() -> Self {
        Self {
            max_steps: 128,
            epsilon: 0.001,
            max_distance: 100.0,
            ao_steps: 5,
            ao_intensity: 0.2,
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
    /// Base color RGB
    pub base_color: [f32; 3],
    /// Secondary color for gradients
    pub secondary_color: [f32; 3],
    /// Background color RGB
    pub background_color: [f32; 3],
    /// Color mode (0: solid, 1: orbit trap, 2: iteration-based)
    pub color_mode: u32,
}

impl Default for ColorConfig {
    fn default() -> Self {
        Self {
            base_color: [0.8, 0.3, 0.1],
            secondary_color: [0.1, 0.4, 0.8],
            background_color: [0.05, 0.05, 0.1],
            color_mode: 1, // orbit trap
        }
    }
}
