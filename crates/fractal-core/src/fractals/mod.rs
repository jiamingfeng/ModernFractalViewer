//! Fractal type definitions and parameters
//!
//! This module defines the different types of fractals supported
//! and their configurable parameters.

mod mandelbulb;
mod menger;

pub use mandelbulb::MandelbulbParams;
pub use menger::MengerParams;

use serde::{Deserialize, Serialize};

/// Enumeration of supported fractal types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[repr(u32)]
pub enum FractalType {
    #[default]
    Mandelbulb = 0,
    Menger = 1,
    Julia3D = 2,
    Mandelbox = 3,
    Sierpinski = 4,
    Apollonian = 5,
}

impl FractalType {
    /// Get all available fractal types
    pub fn all() -> &'static [FractalType] {
        &[
            FractalType::Mandelbulb,
            FractalType::Menger,
            FractalType::Julia3D,
            FractalType::Mandelbox,
            FractalType::Sierpinski,
            FractalType::Apollonian,
        ]
    }

    /// Get the display name of this fractal type
    pub fn name(&self) -> &'static str {
        match self {
            FractalType::Mandelbulb => "Mandelbulb",
            FractalType::Menger => "Menger Sponge",
            FractalType::Julia3D => "Julia 3D",
            FractalType::Mandelbox => "Mandelbox",
            FractalType::Sierpinski => "Sierpinski",
            FractalType::Apollonian => "Apollonian",
        }
    }
}

/// Unified fractal parameters that can be sent to the GPU
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(default)]
pub struct FractalParams {
    /// Type of fractal to render
    pub fractal_type: FractalType,
    /// Power parameter (used by Mandelbulb, Mandelbox)
    pub power: f32,
    /// Number of iterations
    pub iterations: u32,
    /// Bailout radius
    pub bailout: f32,
    /// Scale parameter (used by Menger, Mandelbox, Sierpinski)
    pub scale: f32,
    /// Fold limit (used by Mandelbox)
    pub fold_limit: f32,
    /// Minimum radius squared (used by Mandelbox)
    pub min_radius_sq: f32,
    /// Julia constant (x, y, z, w for quaternion)
    pub julia_c: [f32; 4],
}

impl Default for FractalParams {
    fn default() -> Self {
        Self {
            fractal_type: FractalType::Mandelbulb,
            power: 8.0,
            iterations: 12,
            bailout: 2.0,
            scale: 2.0,
            fold_limit: 1.0,
            min_radius_sq: 0.25,
            julia_c: [0.0, 0.0, 0.0, 0.0],
        }
    }
}

impl FractalParams {
    /// Create Mandelbulb parameters
    pub fn mandelbulb(power: f32, iterations: u32) -> Self {
        Self {
            fractal_type: FractalType::Mandelbulb,
            power,
            iterations,
            bailout: 2.0,
            ..Default::default()
        }
    }

    /// Create Menger Sponge parameters
    pub fn menger(iterations: u32) -> Self {
        Self {
            fractal_type: FractalType::Menger,
            iterations,
            scale: 3.0,
            ..Default::default()
        }
    }

    /// Create default parameters for a given fractal type
    pub fn for_type(fractal_type: FractalType) -> Self {
        match fractal_type {
            FractalType::Mandelbulb => Self::mandelbulb(8.0, 12),
            FractalType::Menger => Self::menger(4),
            FractalType::Julia3D => Self {
                fractal_type: FractalType::Julia3D,
                iterations: 11,
                bailout: 4.0,
                julia_c: [-0.8, 0.156, 0.0, -0.1],
                ..Default::default()
            },
            FractalType::Mandelbox => Self {
                fractal_type: FractalType::Mandelbox,
                scale: 2.0,
                fold_limit: 1.0,
                min_radius_sq: 0.25,
                iterations: 15,
                ..Default::default()
            },
            FractalType::Sierpinski => Self {
                fractal_type: FractalType::Sierpinski,
                scale: 2.0,
                iterations: 12,
                ..Default::default()
            },
            FractalType::Apollonian => Self {
                fractal_type: FractalType::Apollonian,
                iterations: 8,
                ..Default::default()
            },
        }
    }
}
