//! Fractal rendering pipeline

use crate::context::RenderContext;
use crate::uniforms::Uniforms;
use bytemuck;
use wgpu::{self, util::DeviceExt};

/// The embedded shader source, baked into the binary at compile time.
const EMBEDDED_SHADER: &str = include_str!("../shaders/raymarcher.wgsl");

/// The main rendering pipeline for fractal ray marching
pub struct FractalPipeline {
    pub render_pipeline: wgpu::RenderPipeline,
    pub uniform_buffer: wgpu::Buffer,
    pub uniform_bind_group: wgpu::BindGroup,
    pub uniforms: Uniforms,
    /// Cached format for pipeline recreation during hot-reload.
    format: wgpu::TextureFormat,
}

impl FractalPipeline {
    /// Create a new fractal rendering pipeline
    ///
    /// # How WGSL works with Vulkan
    ///
    /// wgpu uses the Naga shader compiler to translate WGSL to native formats:
    /// - Vulkan: WGSL → SPIR-V (at runtime, cached by driver)
    /// - Metal: WGSL → MSL
    /// - DirectX 12: WGSL → HLSL → DXIL
    /// - WebGPU: WGSL passes through directly
    ///
    /// This translation happens once at shader creation time, not per-frame.
    /// Performance is identical to hand-written SPIR-V after initial compilation.
    pub fn new(ctx: &RenderContext) -> Self {
        Self::create(&ctx.device, ctx.format)
    }

    /// Create a pipeline without a window/surface (for headless testing).
    pub fn new_headless(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        Self::create(device, format)
    }

    /// Resolve the shader source. With `hot-reload` feature, tries loading from
    /// disk first and falls back to the embedded source.
    fn resolve_shader_source() -> String {
        #[cfg(feature = "hot-reload")]
        {
            if let Some(path) = Self::shader_path() {
                if let Ok(source) = std::fs::read_to_string(&path) {
                    log::info!("Hot-reload: loaded shader from {}", path.display());
                    return source;
                }
            }
        }
        EMBEDDED_SHADER.to_string()
    }

    /// Returns the on-disk shader path for hot-reload.
    #[cfg(feature = "hot-reload")]
    pub fn shader_path() -> Option<std::path::PathBuf> {
        // Try relative to the workspace root (common during `cargo run`)
        let candidates = [
            std::path::PathBuf::from("crates/fractal-renderer/shaders/raymarcher.wgsl"),
            std::path::PathBuf::from("shaders/raymarcher.wgsl"),
        ];
        for candidate in &candidates {
            if candidate.exists() {
                return Some(candidate.clone());
            }
        }
        // Try relative to the executable
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                let path = dir.join("shaders/raymarcher.wgsl");
                if path.exists() {
                    return Some(path);
                }
            }
        }
        None
    }

    fn create(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let wgsl_source = Self::resolve_shader_source();

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Fractal Shader"),
            source: wgpu::ShaderSource::Wgsl(wgsl_source.into()),
        });

        // Create uniform buffer
        let uniforms = Uniforms::new();
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Uniform Buffer"),
            contents: bytemuck::cast_slice(&[uniforms]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // Create bind group layout
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Uniform Bind Group Layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        // Create bind group
        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Uniform Bind Group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        // Create pipeline layout
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Fractal Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        // Create render pipeline
        let render_pipeline = Self::create_render_pipeline(device, format, &shader, &pipeline_layout);

        Self {
            render_pipeline,
            uniform_buffer,
            uniform_bind_group,
            uniforms,
            format,
        }
    }

    fn create_render_pipeline(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        shader: &wgpu::ShaderModule,
        pipeline_layout: &wgpu::PipelineLayout,
    ) -> wgpu::RenderPipeline {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Fractal Render Pipeline"),
            layout: Some(pipeline_layout),
            vertex: wgpu::VertexState {
                module: shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        })
    }

    /// Attempt to reload the shader from new WGSL source.
    /// On success, replaces the render pipeline. On failure, returns the error
    /// and the old pipeline continues rendering.
    pub fn reload_shader(
        &mut self,
        device: &wgpu::Device,
        wgsl_source: &str,
    ) -> Result<(), String> {
        // Use error scope to catch shader compilation errors
        device.push_error_scope(wgpu::ErrorFilter::Validation);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Fractal Shader (Hot Reload)"),
            source: wgpu::ShaderSource::Wgsl(wgsl_source.into()),
        });

        let error = pollster::block_on(device.pop_error_scope());
        if let Some(err) = error {
            return Err(format!("Shader compile error: {err}"));
        }

        // Rebuild bind group layout + pipeline layout + render pipeline
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Uniform Bind Group Layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Fractal Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        self.render_pipeline = Self::create_render_pipeline(device, self.format, &shader, &pipeline_layout);
        log::info!("Shader hot-reloaded successfully");
        Ok(())
    }

    /// Update uniforms and upload to GPU
    pub fn update_uniforms(&mut self, queue: &wgpu::Queue) {
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[self.uniforms]));
    }

    /// Render the fractal to the given view
    pub fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
    ) {
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Fractal Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        render_pass.set_pipeline(&self.render_pipeline);
        render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
        render_pass.draw(0..3, 0..1); // Fullscreen triangle
    }
}
