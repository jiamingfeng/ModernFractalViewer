//! Thumbnail capture via offscreen rendering
//!
//! Renders the current fractal to a small offscreen texture and reads
//! it back to CPU memory for PNG encoding and session saving.

use wgpu;

/// Captures a thumbnail by rendering to an offscreen texture and
/// reading the result back to CPU memory.
pub struct ThumbnailCapture {
    texture: wgpu::Texture,
    texture_view: wgpu::TextureView,
    staging_buffer: wgpu::Buffer,
    width: u32,
    height: u32,
    padded_bytes_per_row: u32,
    format: wgpu::TextureFormat,
}

impl ThumbnailCapture {
    /// Create a new thumbnail capture target.
    ///
    /// `width` and `height` control the thumbnail resolution.
    /// `format` must match the render pipeline's target format.
    pub fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        width: u32,
        height: u32,
    ) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Thumbnail Texture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });

        let texture_view = texture.create_view(&Default::default());

        // Compute row alignment for wgpu buffer-texture copies.
        // COPY_BYTES_PER_ROW_ALIGNMENT = 256
        let bytes_per_row = width * 4; // RGBA / BGRA = 4 bytes per pixel
        let padded_bytes_per_row =
            (bytes_per_row + wgpu::COPY_BYTES_PER_ROW_ALIGNMENT - 1)
                & !(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT - 1);

        let buffer_size = (padded_bytes_per_row * height) as u64;

        let staging_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Thumbnail Staging Buffer"),
            size: buffer_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        Self {
            texture,
            texture_view,
            staging_buffer,
            width,
            height,
            padded_bytes_per_row,
            format,
        }
    }

    /// The texture view to render into.
    pub fn view(&self) -> &wgpu::TextureView {
        &self.texture_view
    }

    /// Width of the thumbnail in pixels.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Height of the thumbnail in pixels.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Add a copy command from the rendered texture to the staging buffer.
    /// Call this after the render pass, before submitting the encoder.
    pub fn copy_to_buffer(&self, encoder: &mut wgpu::CommandEncoder) {
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &self.staging_buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(self.padded_bytes_per_row),
                    rows_per_image: Some(self.height),
                },
            },
            wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
        );
    }

    /// Read pixels back from the staging buffer (blocking).
    ///
    /// Returns RGBA8 pixel data (`width * height * 4` bytes) regardless
    /// of the surface format (handles BGRA → RGBA conversion).
    ///
    /// Must be called after the command encoder containing `copy_to_buffer`
    /// has been submitted.
    pub fn read_pixels(&self, device: &wgpu::Device) -> Vec<u8> {
        let buffer_slice = self.staging_buffer.slice(..);

        // Map the buffer (request CPU access)
        let (tx, rx) = std::sync::mpsc::channel();
        buffer_slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });

        // Wait for the GPU to finish and the mapping to complete
        device.poll(wgpu::Maintain::Wait);
        rx.recv()
            .expect("map_async callback dropped")
            .expect("buffer mapping failed");

        let data = buffer_slice.get_mapped_range();
        let bytes_per_row = self.width * 4;
        let padded = self.padded_bytes_per_row;

        // Strip row padding and convert BGRA → RGBA if needed
        let is_bgra = matches!(
            self.format,
            wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
        );

        let mut pixels = Vec::with_capacity((bytes_per_row * self.height) as usize);
        for row in 0..self.height {
            let offset = (row * padded) as usize;
            let row_data = &data[offset..offset + bytes_per_row as usize];
            if is_bgra {
                // Swap B and R channels: BGRA → RGBA
                for chunk in row_data.chunks_exact(4) {
                    pixels.extend_from_slice(&[chunk[2], chunk[1], chunk[0], chunk[3]]);
                }
            } else {
                pixels.extend_from_slice(row_data);
            }
        }

        drop(data);
        self.staging_buffer.unmap();

        pixels
    }
}
