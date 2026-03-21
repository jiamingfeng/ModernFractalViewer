// SDF Volume Compute Pipeline
//
// Note: Add `pub mod compute;` to lib.rs to expose this module.

use bytemuck::{Pod, Zeroable};
use wgpu;
use wgpu::util::DeviceExt;

/// GPU-side parameters for the SDF volume sampling compute shader.
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct VolumeParams {
    bounds_min: [f32; 3],
    _pad0: f32,
    bounds_max: [f32; 3],
    _pad1: f32,
    grid_size: [u32; 3],
    _pad2: u32,
}

pub struct SdfVolumeCompute {
    compute_pipeline: wgpu::ComputePipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    volume_params_buffer: wgpu::Buffer,
    volume_buffer: wgpu::Buffer,
    staging_buffer: wgpu::Buffer,
    current_total_vertices: u64,
}

impl SdfVolumeCompute {
    /// Creates a new SDF volume compute pipeline.
    ///
    /// `uniform_buffer` is the existing Uniforms buffer (group 0, binding 0)
    /// used by the SDF functions.
    pub fn new(device: &wgpu::Device, _uniform_buffer: &wgpu::Buffer) -> Self {
        // Load and concatenate shader sources: sdf_common.wgsl + sdf_volume.wgsl
        let common_source = crate::pipeline::sdf_common_source();
        let volume_source = include_str!("../shaders/sdf_volume.wgsl");
        let full_source = format!("{common_source}\n{volume_source}");

        let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("SDF Volume Compute Shader"),
            source: wgpu::ShaderSource::Wgsl(full_source.into()),
        });

        // Bind group layout: 3 bindings for compute visibility
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("SDF Volume Bind Group Layout"),
            entries: &[
                // binding 0: Uniforms (SDF params)
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // binding 1: VolumeParams
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // binding 2: volume output (storage, read_write)
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("SDF Volume Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let compute_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("SDF Volume Compute Pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader_module,
            entry_point: Some("sample_volume"),
            compilation_options: Default::default(),
            cache: None,
        });

        // Default grid: 64^3 cells → 65^3 vertices
        let default_resolution: u32 = 64;
        let default_total_vertices = (default_resolution as u64 + 1).pow(3);
        let volume_byte_size = default_total_vertices * 8; // vec2<f32> = 8 bytes

        let volume_params_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Volume Params Buffer"),
            contents: bytemuck::bytes_of(&VolumeParams {
                bounds_min: [-1.0, -1.0, -1.0],
                _pad0: 0.0,
                bounds_max: [1.0, 1.0, 1.0],
                _pad1: 0.0,
                grid_size: [default_resolution, default_resolution, default_resolution],
                _pad2: 0,
            }),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let volume_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Volume Output Buffer"),
            size: volume_byte_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let staging_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Volume Staging Buffer"),
            size: volume_byte_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        Self {
            compute_pipeline,
            bind_group_layout,
            volume_params_buffer,
            volume_buffer,
            staging_buffer,
            current_total_vertices: default_total_vertices,
        }
    }

    /// Dispatches the SDF volume sampling compute shader.
    ///
    /// Samples the SDF on a 3D grid of `(resolution+1)^3` vertices spanning
    /// `[bounds_min, bounds_max]`.
    pub fn dispatch(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        uniform_buffer: &wgpu::Buffer,
        bounds_min: [f32; 3],
        bounds_max: [f32; 3],
        resolution: u32,
    ) {
        let total_vertices = (resolution as u64 + 1).pow(3);
        let volume_byte_size = total_vertices * 8; // vec2<f32> = 8 bytes

        // Recreate buffers if the grid size changed
        if total_vertices != self.current_total_vertices {
            self.volume_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Volume Output Buffer"),
                size: volume_byte_size,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            });

            self.staging_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Volume Staging Buffer"),
                size: volume_byte_size,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });

            self.current_total_vertices = total_vertices;
        }

        // Update volume params
        let params = VolumeParams {
            bounds_min,
            _pad0: 0.0,
            bounds_max,
            _pad1: 0.0,
            grid_size: [resolution, resolution, resolution],
            _pad2: 0,
        };
        queue.write_buffer(&self.volume_params_buffer, 0, bytemuck::bytes_of(&params));

        // Create bind group
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("SDF Volume Bind Group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: self.volume_params_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: self.volume_buffer.as_entire_binding(),
                },
            ],
        });

        // Dispatch compute shader
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("SDF Volume Compute Encoder"),
        });

        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("SDF Volume Compute Pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.compute_pipeline);
            pass.set_bind_group(0, &bind_group, &[]);

            // Dispatch workgroups: ceil((resolution+1) / 4) per dimension
            let vertex_count = resolution + 1;
            let workgroups = (vertex_count + 3) / 4;
            pass.dispatch_workgroups(workgroups, workgroups, workgroups);
        }

        // Copy volume buffer to staging buffer for CPU readback
        encoder.copy_buffer_to_buffer(
            &self.volume_buffer,
            0,
            &self.staging_buffer,
            0,
            volume_byte_size,
        );

        queue.submit(std::iter::once(encoder.finish()));
    }

    /// Reads the volume data back from the GPU.
    ///
    /// Returns a `Vec<[f32; 2]>` where each element is `[distance, trap]`
    /// for each grid vertex in linear order (x + y*vx + z*vx*vy).
    pub fn read_volume(&self, device: &wgpu::Device) -> Vec<[f32; 2]> {
        let buffer_slice = self.staging_buffer.slice(..);

        // Set up async mapping with a channel (same pattern as thumbnail.rs)
        let (tx, rx) = std::sync::mpsc::channel();
        buffer_slice.map_async(wgpu::MapMode::Read, move |result| {
            tx.send(result).unwrap();
        });
        device.poll(wgpu::Maintain::Wait);

        rx.recv()
            .expect("Failed to receive map result")
            .expect("Failed to map staging buffer");

        let data = buffer_slice.get_mapped_range();
        let result: &[[f32; 2]] = bytemuck::cast_slice(&data);
        let output = result.to_vec();

        drop(data);
        self.staging_buffer.unmap();

        output
    }
}
