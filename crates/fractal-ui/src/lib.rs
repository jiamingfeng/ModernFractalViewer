//! Fractal UI Library
//!
//! This crate provides egui-based UI components for the fractal viewer.

pub mod app_settings;
pub mod panels;
pub mod state;

pub use app_settings::AppSettings;
pub use panels::FractalPanel;
pub use state::{SessionSlotDisplay, UiState};
