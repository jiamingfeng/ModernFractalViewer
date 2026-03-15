//! Input state management

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
}
