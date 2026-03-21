//! Fractal UI Library
//!
//! This crate provides egui-based UI components for the fractal viewer.

pub mod control_ranges;
pub mod panels;
pub mod state;

pub use control_ranges::UiControlRanges;
pub use panels::FractalPanel;
pub use state::{SessionSlotDisplay, UiState};
