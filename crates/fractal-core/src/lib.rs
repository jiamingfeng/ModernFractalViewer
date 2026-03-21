//! Fractal Core Library
//! 
//! This crate contains the core mathematical types and fractal definitions
//! used by the fractal viewer application.

pub mod camera;
pub mod fractals;
pub mod mesh;
pub mod sdf;
pub mod session;

pub use camera::Camera;
pub use fractals::{FractalParams, FractalType};
pub use session::SavedSession;
