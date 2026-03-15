//! Fractal Renderer Library
//!
//! This crate handles all GPU rendering using wgpu, including:
//! - Ray marching pipeline for fractal SDFs
//! - Shader management
//! - GPU uniform buffers

pub mod context;
pub mod pipeline;
pub mod uniforms;

pub use context::RenderContext;
pub use pipeline::FractalPipeline;
pub use uniforms::Uniforms;
