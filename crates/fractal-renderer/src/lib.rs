//! Fractal Renderer Library
//!
//! This crate handles all GPU rendering using wgpu, including:
//! - Ray marching pipeline for fractal SDFs
//! - Shader management
//! - GPU uniform buffers

pub mod benchmark;
pub mod compute;
pub mod context;
pub mod pipeline;
pub mod thumbnail;
pub mod uniforms;

pub use context::RenderContext;
pub use pipeline::FractalPipeline;
pub use thumbnail::ThumbnailCapture;
pub use uniforms::Uniforms;
