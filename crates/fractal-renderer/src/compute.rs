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
    /// First Z-layer index in this slab (0 when the volume fits in one pass).
    z_offset: u32,
    /// Number of Z-layers in this slab.
    slab_z_count: u32,
    _pad3: u32,
    _pad4: u32,
    _pad5: u32,
}

pub struct SdfVolumeCompute {
    compute_pipeline: wgpu::ComputePipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    volume_params_buffer: wgpu::Buffer,
    volume_buffer: wgpu::Buffer,
    staging_buffer: wgpu::Buffer,
    /// Current slab capacity in elements (for buffer reuse).
    current_slab_elements: u64,
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

        // Default grid: 64^3 cells → 65^3 vertices (fits in a single slab)
        let default_resolution: u32 = 64;
        let default_elements = (default_resolution as u64 + 1).pow(3);
        let volume_byte_size = default_elements * 8; // vec2<f32> = 8 bytes

        let volume_params_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Volume Params Buffer"),
            contents: bytemuck::bytes_of(&VolumeParams {
                bounds_min: [-1.0, -1.0, -1.0],
                _pad0: 0.0,
                bounds_max: [1.0, 1.0, 1.0],
                _pad1: 0.0,
                grid_size: [default_resolution, default_resolution, default_resolution],
                z_offset: 0,
                slab_z_count: default_resolution + 1,
                _pad3: 0,
                _pad4: 0,
                _pad5: 0,
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
            current_slab_elements: default_elements,
        }
    }

    /// Ensure the GPU storage + staging buffers are at least `needed_elements` large.
    fn ensure_buffer_capacity(&mut self, device: &wgpu::Device, needed_elements: u64) {
        if needed_elements <= self.current_slab_elements {
            return;
        }
        let byte_size = needed_elements * 8;

        self.volume_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Volume Output Buffer"),
            size: byte_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        self.staging_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Volume Staging Buffer"),
            size: byte_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        self.current_slab_elements = needed_elements;
    }

    /// Dispatches the SDF volume sampling compute shader, automatically
    /// splitting into Z-slabs when the volume exceeds the per-binding limit.
    ///
    /// This is a **blocking** convenience method that returns the full volume.
    /// For the non-blocking path, use [`dispatch_slabs`] + [`initiate_map_async`]
    /// + [`try_read_volume`].
    pub fn dispatch_and_read(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        uniform_buffer: &wgpu::Buffer,
        bounds_min: [f32; 3],
        bounds_max: [f32; 3],
        resolution: u32,
    ) -> Vec<[f32; 2]> {
        let vx = resolution as u64 + 1;
        let vy = resolution as u64 + 1;
        let vz = resolution as u64 + 1;
        let total_elements = vx * vy * vz;

        let max_binding = device.limits().max_storage_buffer_binding_size as u64;
        let max_elements_per_slab = max_binding / 8; // 8 bytes per vec2<f32>
        let elements_per_layer = vx * vy; // one Z-layer

        // How many Z layers fit in one slab?
        let max_z_per_slab = (max_elements_per_slab / elements_per_layer).max(1) as u32;

        let mut output = Vec::with_capacity(total_elements as usize);

        let mut z_cursor: u32 = 0;
        while z_cursor < vz as u32 {
            let slab_z = (vz as u32 - z_cursor).min(max_z_per_slab);
            let slab_elements = elements_per_layer * slab_z as u64;

            self.ensure_buffer_capacity(device, slab_elements);
            self.dispatch_slab(device, queue, uniform_buffer, bounds_min, bounds_max, resolution, z_cursor, slab_z);

            // Blocking read
            let data = self.read_slab_blocking(device, slab_elements);
            output.extend_from_slice(&data);

            z_cursor += slab_z;
        }

        output
    }

    /// Dispatches a single slab covering `slab_z_count` Z-layers starting at
    /// `z_offset`.  Returns the number of elements in this slab.
    ///
    /// After calling this, use [`initiate_map_async`] + [`try_read_volume`]
    /// (or [`read_slab_blocking`]) to get the data.
    pub fn dispatch_slab(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        uniform_buffer: &wgpu::Buffer,
        bounds_min: [f32; 3],
        bounds_max: [f32; 3],
        resolution: u32,
        z_offset: u32,
        slab_z_count: u32,
    ) -> u64 {
        let vx = resolution as u64 + 1;
        let vy = resolution as u64 + 1;
        let slab_elements = vx * vy * slab_z_count as u64;
        let slab_byte_size = slab_elements * 8;

        self.ensure_buffer_capacity(device, slab_elements);

        // Update volume params
        let params = VolumeParams {
            bounds_min,
            _pad0: 0.0,
            bounds_max,
            _pad1: 0.0,
            grid_size: [resolution, resolution, resolution],
            z_offset,
            slab_z_count,
            _pad3: 0,
            _pad4: 0,
            _pad5: 0,
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

            // Dispatch workgroups: ceil(dim / 4) per dimension
            let wg_x = (resolution + 1 + 3) / 4;
            let wg_y = (resolution + 1 + 3) / 4;
            let wg_z = (slab_z_count + 3) / 4;
            pass.dispatch_workgroups(wg_x, wg_y, wg_z);
        }

        // Copy volume buffer to staging buffer for CPU readback
        encoder.copy_buffer_to_buffer(
            &self.volume_buffer,
            0,
            &self.staging_buffer,
            0,
            slab_byte_size,
        );

        queue.submit(std::iter::once(encoder.finish()));

        slab_elements
    }

    /// Blocking read of the staging buffer (for use after `dispatch_slab`).
    fn read_slab_blocking(&self, device: &wgpu::Device, slab_elements: u64) -> Vec<[f32; 2]> {
        let byte_size = slab_elements * 8;
        let buffer_slice = self.staging_buffer.slice(..byte_size);

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

    // ── Non-blocking API (single-slab, used by the async export path) ────

    /// Dispatches the SDF volume compute for a single slab and returns
    /// configuration needed for the async readback path.
    ///
    /// If the full volume fits in one slab, this is all you need.
    /// For multi-slab volumes, returns `Err` with a message — the caller
    /// should use [`dispatch_and_read`] on a background thread instead.
    pub fn dispatch_single_or_err(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        uniform_buffer: &wgpu::Buffer,
        bounds_min: [f32; 3],
        bounds_max: [f32; 3],
        resolution: u32,
    ) -> Result<u64, SlabInfo> {
        let vx = resolution as u64 + 1;
        let vy = resolution as u64 + 1;
        let vz = resolution as u64 + 1;
        let total_elements = vx * vy * vz;
        let max_binding = device.limits().max_storage_buffer_binding_size as u64;
        let max_elements = max_binding / 8;

        if total_elements <= max_elements {
            // Fits in one slab — use the async path
            let slab_elements = self.dispatch_slab(
                device, queue, uniform_buffer,
                bounds_min, bounds_max, resolution, 0, vz as u32,
            );
            Ok(slab_elements)
        } else {
            // Needs multiple slabs — return info for the blocking multi-slab path
            let elements_per_layer = vx * vy;
            let max_z_per_slab = (max_elements / elements_per_layer).max(1) as u32;
            Err(SlabInfo {
                total_z: vz as u32,
                max_z_per_slab,
            })
        }
    }

    /// Begins the async buffer map without blocking.
    ///
    /// Call this once after a single-slab dispatch, then poll each frame with
    /// [`try_read_volume`] until the data is ready.
    pub fn initiate_map_async(&self) -> std::sync::mpsc::Receiver<Result<(), wgpu::BufferAsyncError>> {
        let buffer_slice = self.staging_buffer.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        buffer_slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
        rx
    }

    /// Non-blocking attempt to read the volume once the async map completes.
    ///
    /// `device.poll(Maintain::Poll)` should be called each frame before this
    /// to nudge the GPU.  Returns `Some(data)` if the map is ready, or `None`
    /// if still pending.
    pub fn try_read_volume(
        &self,
        rx: &std::sync::mpsc::Receiver<Result<(), wgpu::BufferAsyncError>>,
    ) -> Option<Vec<[f32; 2]>> {
        match rx.try_recv() {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                log::error!("Failed to map staging buffer: {e:?}");
                return None;
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => return None, // still pending
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                log::error!("Map-async channel disconnected");
                return None;
            }
        }

        let buffer_slice = self.staging_buffer.slice(..);
        let data = buffer_slice.get_mapped_range();
        let result: &[[f32; 2]] = bytemuck::cast_slice(&data);
        let output = result.to_vec();

        drop(data);
        self.staging_buffer.unmap();

        Some(output)
    }
}

/// Info about a multi-slab volume that doesn't fit in a single GPU binding.
pub struct SlabInfo {
    /// Total number of Z-layers (resolution + 1).
    pub total_z: u32,
    /// Maximum Z-layers per slab given the binding limit.
    pub max_z_per_slab: u32,
}
