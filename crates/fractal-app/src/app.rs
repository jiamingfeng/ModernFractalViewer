//! Main application state and rendering loop

use std::sync::Arc;

#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;
#[cfg(target_arch = "wasm32")]
use web_time::Instant;

use fractal_core::{Camera, SavedSession};
use fractal_renderer::{FractalPipeline, RenderContext, ThumbnailCapture};
use fractal_ui::{FractalPanel, UiState};
use winit::event::{ElementState, MouseButton, MouseScrollDelta, Touch, TouchPhase, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::Window;

use crate::input::InputState;
use crate::session_manager::SessionManager;

/// Main application state
pub struct App {
    window: Arc<Window>,
    render_ctx: RenderContext,
    pipeline: FractalPipeline,
    egui_ctx: egui::Context,
    egui_state: egui_winit::State,
    egui_renderer: egui_wgpu::Renderer,
    ui_state: UiState,
    camera: Camera,
    input: InputState,
    start_time: Instant,
    last_frame: Instant,
    vsync_prev: bool,
    /// Tracks whether the surface has been synced with the actual window size.
    /// On WASM, the initial inner_size() can be stale when the canvas hasn't
    /// been laid out yet, so we force a resize on the first RedrawRequested.
    needs_initial_configure: bool,
    session_manager: Option<SessionManager>,
    thumbnail_capture: ThumbnailCapture,
}

impl App {
    pub async fn new(window: Arc<Window>) -> Result<Self, Box<dyn std::error::Error>> {
        // Initialize wgpu render context
        let render_ctx = RenderContext::new(window.clone()).await?;
        
        // Create fractal rendering pipeline
        let pipeline = FractalPipeline::new(&render_ctx);
        
        // Initialize egui
        let egui_ctx = egui::Context::default();
        let egui_state = egui_winit::State::new(
            egui_ctx.clone(),
            egui::ViewportId::ROOT,
            &window,
            Some(window.scale_factor() as f32),
            None,
            None,
        );
        
        let egui_renderer = egui_wgpu::Renderer::new(
            &render_ctx.device,
            render_ctx.format,
            None,
            1,
            false,
        );
        
        // Initialize state
        let ui_state = UiState::default();
        let camera = Camera::default();

        // Session save/load
        let session_manager = match SessionManager::new() {
            Ok(mgr) => Some(mgr),
            Err(e) => {
                log::warn!("Session manager unavailable: {e}");
                None
            }
        };

        let thumbnail_capture = ThumbnailCapture::new(
            &render_ctx.device,
            render_ctx.format,
            320,
            180,
        );

        Ok(Self {
            window,
            render_ctx,
            pipeline,
            egui_ctx,
            egui_state,
            egui_renderer,
            ui_state,
            camera,
            input: InputState::default(),
            start_time: Instant::now(),
            last_frame: Instant::now(),
            vsync_prev: true,
            needs_initial_configure: true,
            session_manager,
            thumbnail_capture,
        })
    }
    
    /// Handle window events using the new winit 0.30+ ApplicationHandler pattern
    pub fn handle_window_event(&mut self, event: &WindowEvent, elwt: &ActiveEventLoop) {
        // Let egui handle events first
        let egui_response = self.egui_state.on_window_event(&self.window, event);

        if egui_response.consumed {
            return;
        }

        match event {
            WindowEvent::CloseRequested => {
                log::info!("Close requested");
                elwt.exit();
            }

            WindowEvent::Resized(size) => {
                if size.width > 0 && size.height > 0 {
                    self.render_ctx.resize(size.width, size.height);
                    log::info!("Resized to {}x{}", size.width, size.height);
                }
            }

            WindowEvent::KeyboardInput { event, .. } => {
                self.handle_keyboard(event);
            }

            WindowEvent::MouseInput { state, button, .. } => {
                self.handle_mouse_button(*button, *state);
            }

            WindowEvent::CursorMoved { position, .. } => {
                self.handle_mouse_move(position.x as f32, position.y as f32);
            }

            WindowEvent::MouseWheel { delta, .. } => {
                self.handle_scroll(delta);
            }

            WindowEvent::Touch(touch) => {
                self.handle_touch(touch);
            }

            WindowEvent::RedrawRequested => {
                // On the first redraw (especially WASM), sync the surface size
                // with the actual window/canvas dimensions. The initial size
                // from window.inner_size() during construction may be stale
                // because the canvas hadn't been laid out in the DOM yet.
                if self.needs_initial_configure {
                    self.needs_initial_configure = false;
                    let size = self.window.inner_size();
                    if size.width > 0 && size.height > 0 {
                        self.render_ctx.resize(size.width, size.height);
                        log::info!("Initial surface configure: {}x{}", size.width, size.height);
                    }
                }

                self.update();
                if let Err(e) = self.render() {
                    log::error!("Render error: {}", e);
                }
            }

            _ => {}
        }
    }
    
    fn handle_keyboard(&mut self, event: &winit::event::KeyEvent) {
        if event.state == ElementState::Pressed {
            match event.physical_key {
                PhysicalKey::Code(KeyCode::Escape) => {
                    self.ui_state.show_panel = !self.ui_state.show_panel;
                }
                PhysicalKey::Code(KeyCode::KeyR) => {
                    self.camera.reset();
                    log::info!("Camera reset");
                }
                PhysicalKey::Code(KeyCode::Space) => {
                    self.ui_state.auto_rotate = !self.ui_state.auto_rotate;
                }
                _ => {}
            }
        }
    }
    
    fn handle_mouse_button(&mut self, button: MouseButton, state: ElementState) {
        match button {
            MouseButton::Left => {
                self.input.left_mouse_down = state == ElementState::Pressed;
            }
            MouseButton::Right => {
                self.input.right_mouse_down = state == ElementState::Pressed;
            }
            MouseButton::Middle => {
                self.input.middle_mouse_down = state == ElementState::Pressed;
            }
            _ => {}
        }
    }
    
    fn handle_mouse_move(&mut self, x: f32, y: f32) {
        let dx = x - self.input.mouse_pos.0;
        let dy = y - self.input.mouse_pos.1;
        
        if self.input.left_mouse_down {
            // Orbit camera
            self.camera.orbit(dx * 0.005, -dy * 0.005);
        }
        
        if self.input.right_mouse_down {
            // Pan camera
            let pan_speed = self.camera.distance * 0.002;
            self.camera.pan(glam::Vec3::new(-dx * pan_speed, -dy * pan_speed, 0.0));
        }
        
        self.input.mouse_pos = (x, y);
    }
    
    fn handle_scroll(&mut self, delta: &MouseScrollDelta) {
        let scroll = match delta {
            MouseScrollDelta::LineDelta(_, y) => *y,
            MouseScrollDelta::PixelDelta(pos) => pos.y as f32 * 0.01,
        };
        
        let zoom_factor = 1.0 - scroll * 0.1;
        self.camera.zoom_by(zoom_factor);
    }

    fn handle_touch(&mut self, touch: &Touch) {
        use crate::input::TouchPoint;

        let id = touch.id;
        let pos = TouchPoint {
            x: touch.location.x as f32,
            y: touch.location.y as f32,
        };

        match touch.phase {
            TouchPhase::Started => {
                self.input.touches.insert(id, pos);
                // Reset pinch state when touch count changes
                self.input.prev_pinch_distance = None;
                self.input.prev_pinch_midpoint = None;
            }

            TouchPhase::Moved => {
                let touch_count = self.input.touches.len();

                if touch_count == 1 {
                    // Single finger drag → orbit camera (negate dx for natural direction)
                    if let Some(prev) = self.input.touches.get(&id) {
                        let dx = pos.x - prev.x;
                        let dy = pos.y - prev.y;
                        self.camera.orbit(-dx * 0.005, -dy * 0.005);
                    }
                    self.input.touches.insert(id, pos);
                } else if touch_count == 2 {
                    // Update this finger's position
                    self.input.touches.insert(id, pos);

                    // Get both touch points
                    let points: Vec<&TouchPoint> = self.input.touches.values().collect();
                    if points.len() == 2 {
                        let a = *points[0];
                        let b = *points[1];

                        let dist = InputState::pinch_distance(&a, &b);
                        let mid = InputState::pinch_midpoint(&a, &b);

                        // Pinch-to-zoom
                        if let Some(prev_dist) = self.input.prev_pinch_distance {
                            if prev_dist > 1.0 {
                                let scale = prev_dist / dist;
                                self.camera.zoom_by(scale);
                            }
                        }

                        // Two-finger pan
                        if let Some(prev_mid) = self.input.prev_pinch_midpoint {
                            let dx = mid.0 - prev_mid.0;
                            let dy = mid.1 - prev_mid.1;
                            let pan_speed = self.camera.distance * 0.002;
                            self.camera.pan(glam::Vec3::new(
                                -dx * pan_speed,
                                -dy * pan_speed,
                                0.0,
                            ));
                        }

                        self.input.prev_pinch_distance = Some(dist);
                        self.input.prev_pinch_midpoint = Some(mid);
                    }
                } else {
                    // 3+ fingers: just track positions
                    self.input.touches.insert(id, pos);
                }
            }

            TouchPhase::Ended | TouchPhase::Cancelled => {
                self.input.touches.remove(&id);
                // Reset pinch state when touch count changes
                self.input.prev_pinch_distance = None;
                self.input.prev_pinch_midpoint = None;
            }
        }
    }
    
    fn update(&mut self) {
        let now = Instant::now();
        let dt = (now - self.last_frame).as_secs_f32();
        self.last_frame = now;

        // Auto-rotate
        if self.ui_state.auto_rotate {
            self.camera.orbit(dt * self.ui_state.rotation_speed, 0.0);
        }

        // Apply present mode change when vsync is toggled
        if self.ui_state.vsync != self.vsync_prev {
            self.vsync_prev = self.ui_state.vsync;
            let mode = if self.ui_state.vsync {
                wgpu::PresentMode::AutoVsync
            } else {
                wgpu::PresentMode::AutoNoVsync
            };
            self.render_ctx.set_present_mode(mode);
        }

        // Push current camera state to UI for display/sliders
        self.ui_state.camera = self.camera.clone();
    }
    
    fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        let output = self.render_ctx.surface.get_current_texture()?;
        let view = output.texture.create_view(&Default::default());
        
        // Update uniforms
        let time = self.start_time.elapsed().as_secs_f32();
        let (width, height) = self.render_ctx.size();
        
        self.pipeline.uniforms.update_camera(&self.camera, self.render_ctx.aspect_ratio());
        self.pipeline.uniforms.update_resolution(width, height);
        self.pipeline.uniforms.update_time(time);
        self.pipeline.uniforms.update_fractal(&self.ui_state.fractal_params);
        self.pipeline.uniforms.update_ray_march(&self.ui_state.ray_march_config);
        self.pipeline.uniforms.update_lighting(&self.ui_state.lighting_config);
        self.pipeline.uniforms.update_color(&self.ui_state.color_config);
        self.pipeline.uniforms.frame_count = self.pipeline.uniforms.frame_count.wrapping_add(1);
        self.pipeline.update_uniforms(&self.render_ctx.queue);
        
        // Create encoder for fractal rendering
        let mut encoder = self.render_ctx.device.create_command_encoder(
            &wgpu::CommandEncoderDescriptor {
                label: Some("Fractal Encoder"),
            }
        );
        
        // Render fractal
        self.pipeline.render(&mut encoder, &view);
        
        // Render egui UI on top of the fractal.
        // Always run egui so the toggle button is visible even when the panel is collapsed.
        {
            let raw_input = self.egui_state.take_egui_input(&self.window);
            let full_output = self.egui_ctx.run(raw_input, |ctx| {
                FractalPanel::show(ctx, &mut self.ui_state);

                // Debug overlay
                if self.ui_state.show_debug {
                    egui::Window::new("Debug")
                        .anchor(egui::Align2::RIGHT_TOP, [-10.0, 10.0])
                        .show(ctx, |ui| {
                            ui.label(format!("FPS: {:.1}", 1.0 / (self.last_frame.elapsed().as_secs_f32() + 0.001)));
                            ui.label(format!("Camera: ({:.2}, {:.2}, {:.2})",
                                self.camera.position.x,
                                self.camera.position.y,
                                self.camera.position.z));
                            ui.label(format!("Zoom: {:.4}x", 1.0 / self.camera.distance));
                        });
                }
            });
            
            self.egui_state.handle_platform_output(&self.window, full_output.platform_output);
            
            let clipped_primitives = self.egui_ctx.tessellate(
                full_output.shapes,
                self.egui_ctx.pixels_per_point(),
            );
            
            // Upload egui textures
            for (id, delta) in &full_output.textures_delta.set {
                self.egui_renderer.update_texture(
                    &self.render_ctx.device,
                    &self.render_ctx.queue,
                    *id,
                    delta,
                );
            }
            
            let screen_descriptor = egui_wgpu::ScreenDescriptor {
                size_in_pixels: [width, height],
                pixels_per_point: self.window.scale_factor() as f32,
            };
            
            self.egui_renderer.update_buffers(
                &self.render_ctx.device,
                &self.render_ctx.queue,
                &mut encoder,
                &clipped_primitives,
                &screen_descriptor,
            );
            
            // Render egui - use forget_lifetime() to convert render pass to 'static as required by egui-wgpu
            {
                let render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Egui Render Pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load, // Don't clear, overlay on top
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                
                // SAFETY: egui-wgpu requires 'static lifetime for the render pass.
                // forget_lifetime() is the official way to convert a render pass to 'static.
                // The render pass internally keeps all referenced resources alive.
                let mut render_pass = render_pass.forget_lifetime();
                
                self.egui_renderer.render(
                    &mut render_pass,
                    &clipped_primitives,
                    &screen_descriptor,
                );
            } // render_pass is dropped here before encoder.finish()
            
            // Free egui textures
            for id in &full_output.textures_delta.free {
                self.egui_renderer.free_texture(id);
            }
        }
        
        // Submit commands
        self.render_ctx.queue.submit(std::iter::once(encoder.finish()));
        output.present();
        
        // Sync UI camera changes back to app camera
        // This picks up reset, view buttons, slider changes from the egui UI
        self.camera = self.ui_state.camera.clone();

        // Handle session save/load/delete requests from the UI
        self.handle_session_requests();

        Ok(())
    }

    fn handle_session_requests(&mut self) {
        let Some(ref _session_manager) = self.session_manager else {
            return;
        };

        // Refresh session list if needed
        if self.ui_state.sessions_dirty {
            self.ui_state.sessions_dirty = false;
            self.refresh_session_slots();
        }

        // Handle save
        if self.ui_state.pending_save {
            self.ui_state.pending_save = false;
            self.save_session();
        }

        // Handle load
        if let Some(id) = self.ui_state.pending_load.take() {
            self.load_session(&id);
        }

        // Handle delete
        if let Some(id) = self.ui_state.pending_delete.take() {
            if let Some(ref mgr) = self.session_manager {
                if let Err(e) = mgr.delete(&id) {
                    log::error!("Failed to delete session: {e}");
                }
            }
            self.ui_state.sessions_dirty = true;
        }
    }

    fn save_session(&mut self) {
        // Capture thumbnail: render fractal at thumbnail resolution
        let thumb_w = self.thumbnail_capture.width();
        let thumb_h = self.thumbnail_capture.height();

        // Temporarily set resolution to thumbnail size
        self.pipeline.uniforms.update_resolution(thumb_w, thumb_h);
        self.pipeline.update_uniforms(&self.render_ctx.queue);

        // Render to offscreen texture + copy to staging buffer
        let mut encoder = self.render_ctx.device.create_command_encoder(
            &wgpu::CommandEncoderDescriptor {
                label: Some("Thumbnail Encoder"),
            },
        );
        self.pipeline.render(&mut encoder, self.thumbnail_capture.view());
        self.thumbnail_capture.copy_to_buffer(&mut encoder);
        self.render_ctx.queue.submit(std::iter::once(encoder.finish()));

        // Read pixels back (blocking)
        let pixels = self.thumbnail_capture.read_pixels(&self.render_ctx.device);

        // Restore original resolution
        let (width, height) = self.render_ctx.size();
        self.pipeline.uniforms.update_resolution(width, height);
        self.pipeline.update_uniforms(&self.render_ctx.queue);

        // Encode as PNG
        let thumbnail_base64 = encode_thumbnail_png(&pixels, thumb_w, thumb_h);

        // Build session
        let name = if self.ui_state.save_name.trim().is_empty() {
            self.ui_state.fractal_params.fractal_type.name().to_string()
        } else {
            self.ui_state.save_name.trim().to_string()
        };

        let session = SavedSession {
            version: "1".to_string(),
            timestamp: SessionManager::timestamp_iso8601(),
            name,
            fractal_type_name: self.ui_state.fractal_params.fractal_type.name().to_string(),
            thumbnail_base64,
            thumbnail_width: thumb_w,
            thumbnail_height: thumb_h,
            fractal_params: self.ui_state.fractal_params,
            ray_march_config: self.ui_state.ray_march_config,
            lighting_config: self.ui_state.lighting_config,
            color_config: self.ui_state.color_config,
            camera: self.camera.clone(),
        };

        if let Some(ref mgr) = self.session_manager {
            match mgr.save(&session) {
                Ok(id) => {
                    log::info!("Session saved as {id}");
                    self.ui_state.save_name.clear();
                }
                Err(e) => log::error!("Failed to save session: {e}"),
            }
        }
        self.ui_state.sessions_dirty = true;
    }

    fn load_session(&mut self, id: &str) {
        let Some(ref mgr) = self.session_manager else {
            return;
        };
        match mgr.load(id) {
            Ok(session) => {
                self.ui_state.fractal_params = session.fractal_params;
                self.ui_state.ray_march_config = session.ray_march_config;
                self.ui_state.lighting_config = session.lighting_config;
                self.ui_state.color_config = session.color_config;
                self.ui_state.camera = session.camera.clone();
                self.camera = session.camera;
                log::info!("Loaded session '{}'", session.name);
            }
            Err(e) => log::error!("Failed to load session {id}: {e}"),
        }
    }

    fn refresh_session_slots(&mut self) {
        let Some(ref mgr) = self.session_manager else {
            return;
        };
        match mgr.list_sessions() {
            Ok(sessions) => {
                self.ui_state.session_slots = sessions
                    .into_iter()
                    .map(|(id, session)| {
                        // Decode thumbnail from base64 and upload as egui texture
                        let thumbnail = decode_thumbnail_to_egui(
                            &self.egui_ctx,
                            &id,
                            &session.thumbnail_base64,
                            session.thumbnail_width,
                            session.thumbnail_height,
                        );
                        fractal_ui::SessionSlotDisplay {
                            id,
                            name: session.name,
                            timestamp: session.timestamp,
                            fractal_type_name: session.fractal_type_name,
                            thumbnail,
                        }
                    })
                    .collect();
            }
            Err(e) => log::error!("Failed to list sessions: {e}"),
        }
    }
}

/// Encode RGBA pixels as PNG and return as base64 string.
#[cfg(not(target_arch = "wasm32"))]
fn encode_thumbnail_png(pixels: &[u8], width: u32, height: u32) -> String {
    use base64::Engine;
    use image::{ImageBuffer, RgbaImage};

    let img: RgbaImage = ImageBuffer::from_raw(width, height, pixels.to_vec())
        .unwrap_or_else(|| ImageBuffer::new(width, height));

    let mut png_bytes = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut png_bytes);
    img.write_to(&mut cursor, image::ImageFormat::Png)
        .unwrap_or_else(|e| log::error!("PNG encode failed: {e}"));

    base64::engine::general_purpose::STANDARD.encode(&png_bytes)
}

/// Stub for WASM — PNG encoding not available (image crate is native-only).
#[cfg(target_arch = "wasm32")]
fn encode_thumbnail_png(_pixels: &[u8], _width: u32, _height: u32) -> String {
    String::new()
}

/// Stub for WASM — image decoding not available.
#[cfg(target_arch = "wasm32")]
fn decode_thumbnail_to_egui(
    _ctx: &egui::Context,
    _id: &str,
    _base64_data: &str,
    _width: u32,
    _height: u32,
) -> Option<egui::TextureHandle> {
    None
}

/// Decode a base64 PNG thumbnail and upload as an egui texture.
#[cfg(not(target_arch = "wasm32"))]
fn decode_thumbnail_to_egui(
    ctx: &egui::Context,
    id: &str,
    base64_data: &str,
    _width: u32,
    _height: u32,
) -> Option<egui::TextureHandle> {
    use base64::Engine;

    if base64_data.is_empty() {
        return None;
    }

    let png_bytes = base64::engine::general_purpose::STANDARD
        .decode(base64_data)
        .ok()?;

    // Decode PNG to RGBA pixels
    let img = image::load_from_memory_with_format(&png_bytes, image::ImageFormat::Png)
        .ok()?
        .into_rgba8();

    let size = [img.width() as usize, img.height() as usize];
    let pixels: Vec<egui::Color32> = img
        .pixels()
        .map(|p| egui::Color32::from_rgba_unmultiplied(p[0], p[1], p[2], p[3]))
        .collect();

    let color_image = egui::ColorImage { size, pixels };
    Some(ctx.load_texture(
        format!("thumb_{id}"),
        color_image,
        egui::TextureOptions::LINEAR,
    ))
}
