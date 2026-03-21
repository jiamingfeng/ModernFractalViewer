//! Input state management

use std::collections::HashMap;

/// Tracks a single touch point
#[derive(Debug, Clone, Copy)]
pub struct TouchPoint {
    pub x: f32,
    pub y: f32,
}

/// Tracks input state for camera controls
#[derive(Debug, Clone, Default)]
pub struct InputState {
    /// Current mouse position
    pub mouse_pos: (f32, f32),
    /// Left mouse button down
    pub left_mouse_down: bool,
    /// Right mouse button down
    pub right_mouse_down: bool,
    /// Middle mouse button down
    pub middle_mouse_down: bool,

    /// L key held (light direction control mode)
    pub l_key_down: bool,

    /// Active touch points by finger id
    pub touches: HashMap<u64, TouchPoint>,
    /// Previous distance between two fingers (for pinch-to-zoom)
    pub prev_pinch_distance: Option<f32>,
    /// Previous midpoint between two fingers (for two-finger pan)
    pub prev_pinch_midpoint: Option<(f32, f32)>,
}

impl InputState {
    /// Calculate the distance between two touch points
    pub fn pinch_distance(a: &TouchPoint, b: &TouchPoint) -> f32 {
        let dx = a.x - b.x;
        let dy = a.y - b.y;
        (dx * dx + dy * dy).sqrt()
    }

    /// Calculate the midpoint between two touch points
    pub fn pinch_midpoint(a: &TouchPoint, b: &TouchPoint) -> (f32, f32) {
        ((a.x + b.x) * 0.5, (a.y + b.y) * 0.5)
    }
}
