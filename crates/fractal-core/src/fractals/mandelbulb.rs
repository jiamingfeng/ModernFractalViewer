//! Mandelbulb fractal parameters

use serde::{Deserialize, Serialize};

/// Parameters specific to Mandelbulb rendering
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct MandelbulbParams {
    /// Power of the Mandelbulb (classic is 8)
    pub power: f32,
    /// Maximum iterations before bailout
    pub iterations: u32,
    /// Bailout radius
    pub bailout: f32,
}

impl Default for MandelbulbParams {
    fn default() -> Self {
        Self {
            power: 8.0,
            iterations: 12,
            bailout: 2.0,
        }
    }
}

impl MandelbulbParams {
    /// Create a classic Mandelbulb (power 8)
    pub fn classic() -> Self {
        Self::default()
    }

    /// Create a Mandelbulb with custom power
    pub fn with_power(power: f32) -> Self {
        Self {
            power,
            ..Default::default()
        }
    }
}
