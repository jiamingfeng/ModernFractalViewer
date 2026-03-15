//! Menger Sponge fractal parameters

use serde::{Deserialize, Serialize};

/// Parameters specific to Menger Sponge rendering
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct MengerParams {
    /// Number of recursive iterations
    pub iterations: u32,
    /// Scale factor for each iteration
    pub scale: f32,
}

impl Default for MengerParams {
    fn default() -> Self {
        Self {
            iterations: 4,
            scale: 3.0,
        }
    }
}

impl MengerParams {
    /// Create a classic Menger Sponge
    pub fn classic() -> Self {
        Self::default()
    }

    /// Create with custom iteration count
    pub fn with_iterations(iterations: u32) -> Self {
        Self {
            iterations,
            ..Default::default()
        }
    }
}
