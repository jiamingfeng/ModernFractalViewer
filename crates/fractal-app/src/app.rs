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

/// Minimum time (seconds) the splash screen is displayed.
const SPLASH_MIN_DURATION_SECS: f32 = 2.0;

/// Splash screen state, present during the first few frames of rendering.
struct SplashState {
    /// Background image texture (fractal render)
    background: egui::TextureHandle,
    /// Background image aspect ratio (width / height)
    bg_aspect: f32,
    /// Small app icon for bottom-right corner
    icon: egui::TextureHandle,
    /// Current loading status message
    status: String,
}

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
    /// Splash screen state (Some during loading, None after transition)
    splash: Option<SplashState>,
    /// Cached data directory path for config/session saves
    data_dir: Option<std::path::PathBuf>,
    /// Number of frames rendered (used to manage splash lifecycle)
    rendered_frames: u32,
    /// Hot-reload file watcher (only present with `hot-reload` feature)
    #[cfg(feature = "hot-reload")]
    hot_reloader: Option<crate::hot_reload::HotReloader>,
    /// Shared log buffer for in-app log window
    log_entries: crate::log_capture::LogBuffer,
}

impl App {
    pub async fn new(
        window: Arc<Window>,
        _data_dir_override: Option<std::path::PathBuf>,
        log_entries: crate::log_capture::LogBuffer,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        // Initialize wgpu render context
        let render_ctx = RenderContext::new(window.clone()).await?;

        // Immediately clear the surface to black to replace the OS-default white
        // window background. This runs before pipeline/egui setup so it's as early
        // as possible after the GPU is ready.
        {
            if let Ok(output) = render_ctx.surface.get_current_texture() {
                let view = output.texture.create_view(&Default::default());
                let mut encoder = render_ctx.device.create_command_encoder(
                    &wgpu::CommandEncoderDescriptor { label: Some("Initial Clear") },
                );
                {
                    let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("Black Clear"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &view,
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
                }
                render_ctx.queue.submit(std::iter::once(encoder.finish()));
                output.present();
            }
        }

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
        let mut ui_state = UiState::default();
        ui_state.version_info = format!("{} ({})", env!("APP_VERSION"), env!("APP_COMMIT"));

        // Determine data directory and load control ranges
        let data_dir = {
            #[cfg(target_os = "android")]
            { _data_dir_override.clone() }
            #[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
            { dirs::data_dir().map(|d| d.join("ModernFractalViewer")) }
            #[cfg(target_arch = "wasm32")]
            { None::<std::path::PathBuf> }
        };

        // Load control ranges from config file (or defaults)
        {
            #[cfg(target_arch = "wasm32")]
            { ui_state.settings = crate::config_manager::load_settings_wasm(); }
            #[cfg(not(target_arch = "wasm32"))]
            {
                if let Some(ref dir) = data_dir {
                    ui_state.settings = crate::config_manager::load_settings(dir);
                }
            }
        }

        let mut camera = Camera::default();

        // Session save/load
        let session_manager = {
            #[cfg(target_os = "android")]
            {
                match _data_dir_override {
                    Some(dir) => {
                        let saves_dir = dir.join("saves");
                        match SessionManager::new_with_dir(saves_dir) {
                            Ok(mgr) => Some(mgr),
                            Err(e) => {
                                log::warn!("Session manager unavailable: {e}");
                                None
                            }
                        }
                    }
                    None => {
                        log::warn!("Session manager unavailable: no data directory on Android");
                        None
                    }
                }
            }
            #[cfg(not(target_os = "android"))]
            {
                match SessionManager::new() {
                    Ok(mgr) => Some(mgr),
                    Err(e) => {
                        log::warn!("Session manager unavailable: {e}");
                        None
                    }
                }
            }
        };

        // Auto-load last session if the setting is enabled
        if ui_state.settings.auto_load_last_session {
        if let Some(ref mgr) = session_manager {
            if let Ok(session) = mgr.load("__last_session") {
                ui_state.fractal_params = session.fractal_params;
                ui_state.ray_march_config = session.ray_march_config;
                ui_state.lighting_config = session.lighting_config;
                ui_state.color_config = session.color_config;
                camera = session.camera;
                log::info!("Restored last session");
            }
        }
        }

        // Initialize hot-reloader (only with hot-reload feature)
        #[cfg(feature = "hot-reload")]
        let hot_reloader = {
            use fractal_renderer::FractalPipeline;
            let shader_path = FractalPipeline::shader_path();
            let config_path = data_dir.as_ref().map(|d| d.join("settings.toml"));
            Some(crate::hot_reload::HotReloader::new(shader_path, config_path))
        };

        let thumbnail_capture = ThumbnailCapture::new(
            &render_ctx.device,
            render_ctx.format,
            320,
            180,
        );

        // Load splash screen textures (native only; WASM uses HTML loading indicator)
        #[cfg(not(target_arch = "wasm32"))]
        let splash = {
            let splash_bytes = include_bytes!("../assets/splash.png");
            let splash_img = image::load_from_memory(splash_bytes)
                .expect("Failed to decode splash image")
                .into_rgba8();
            let splash_size = [splash_img.width() as usize, splash_img.height() as usize];
            let bg_aspect = splash_size[0] as f32 / splash_size[1] as f32;
            let splash_pixels: Vec<egui::Color32> = splash_img
                .pixels()
                .map(|p| egui::Color32::from_rgba_unmultiplied(p[0], p[1], p[2], p[3]))
                .collect();
            let background = egui_ctx.load_texture(
                "splash_bg",
                egui::ColorImage { size: splash_size, pixels: splash_pixels },
                egui::TextureOptions::LINEAR,
            );

            let icon_bytes = include_bytes!("../assets/icon.png");
            let icon_img = image::load_from_memory(icon_bytes)
                .expect("Failed to decode icon image")
                .into_rgba8();
            let icon_size = [icon_img.width() as usize, icon_img.height() as usize];
            let icon_pixels: Vec<egui::Color32> = icon_img
                .pixels()
                .map(|p| egui::Color32::from_rgba_unmultiplied(p[0], p[1], p[2], p[3]))
                .collect();
            let icon = egui_ctx.load_texture(
                "splash_icon",
                egui::ColorImage { size: icon_size, pixels: icon_pixels },
                egui::TextureOptions::LINEAR,
            );

            Some(SplashState {
                background,
                bg_aspect,
                icon,
                status: "Loading...".to_string(),
            })
        };
        #[cfg(target_arch = "wasm32")]
        let splash: Option<SplashState> = None;

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
            data_dir,
            splash,
            rendered_frames: 0,
            #[cfg(feature = "hot-reload")]
            hot_reloader,
            log_entries,
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
                self.save_last_session();
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

        // Hot-reload: check for shader/config file changes
        #[cfg(feature = "hot-reload")]
        self.poll_hot_reload();
    }

    #[cfg(feature = "hot-reload")]
    fn poll_hot_reload(&mut self) {
        use crate::hot_reload::HotReloadEvent;

        let reloader = match self.hot_reloader.as_mut() {
            Some(r) => r,
            None => return,
        };

        match reloader.poll() {
            HotReloadEvent::ShaderChanged => {
                if let Some(source) = reloader.read_shader() {
                    match self.pipeline.reload_shader(&self.render_ctx.device, &source) {
                        Ok(()) => {
                            reloader.shader_error = None;
                        }
                        Err(e) => {
                            log::error!("Hot-reload shader error: {e}");
                            reloader.shader_error = Some(e);
                        }
                    }
                }
            }
            HotReloadEvent::ConfigChanged => {
                if let Some(toml_str) = reloader.read_config() {
                    match toml::from_str::<fractal_ui::AppSettings>(&toml_str) {
                        Ok(ranges) => {
                            self.ui_state.settings = ranges;
                            log::info!("Hot-reloaded control settings from config file");
                        }
                        Err(e) => {
                            log::warn!("Config hot-reload parse error: {e}");
                        }
                    }
                }
            }
            HotReloadEvent::None => {}
        }
    }

    fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        let output = self.render_ctx.surface.get_current_texture()?;
        let view = output.texture.create_view(&Default::default());
        let (width, height) = self.render_ctx.size();

        let mut encoder = self.render_ctx.device.create_command_encoder(
            &wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            }
        );

        if self.splash.is_some() {
            // Frame 1: warm up the fractal pipeline by issuing a hidden draw.
            // The driver JIT-compiles the shader on first use; the splash paints over it.
            if self.rendered_frames == 1 {
                self.pipeline.uniforms.update_camera(&self.camera, self.render_ctx.aspect_ratio());
                self.pipeline.uniforms.update_resolution(width, height);
                self.pipeline.uniforms.update_time(0.0);
                self.pipeline.uniforms.update_fractal(&self.ui_state.fractal_params);
                self.pipeline.uniforms.update_ray_march(&self.ui_state.ray_march_config);
                self.pipeline.uniforms.update_lighting(&self.ui_state.lighting_config);
                self.pipeline.uniforms.update_color(&self.ui_state.color_config);
                self.pipeline.update_uniforms(&self.render_ctx.queue);
                self.pipeline.render(&mut encoder, &view);
                if let Some(ref mut splash) = self.splash {
                    splash.status = "Compiling shaders...".to_string();
                }
            }

            self.render_splash_frame(&mut encoder, &view);

            // Transition out of splash after minimum display time
            if self.start_time.elapsed().as_secs_f32() >= SPLASH_MIN_DURATION_SECS {
                self.splash = None;
                self.window.set_maximized(true);
                self.window.set_resizable(true);
                log::info!("Splash screen dismissed");
            }
        } else {
            // === Normal fractal + egui rendering ===
            let time = self.start_time.elapsed().as_secs_f32();

            self.pipeline.uniforms.update_camera(&self.camera, self.render_ctx.aspect_ratio());
            self.pipeline.uniforms.update_resolution(width, height);
            self.pipeline.uniforms.update_time(time);
            self.pipeline.uniforms.update_fractal(&self.ui_state.fractal_params);
            self.pipeline.uniforms.update_ray_march(&self.ui_state.ray_march_config);
            self.pipeline.uniforms.update_lighting(&self.ui_state.lighting_config);
            self.pipeline.uniforms.update_color(&self.ui_state.color_config);
            self.pipeline.uniforms.frame_count = self.pipeline.uniforms.frame_count.wrapping_add(1);
            self.pipeline.update_uniforms(&self.render_ctx.queue);

            // Render fractal
            self.pipeline.render(&mut encoder, &view);

            // Render egui UI on top of the fractal
            {
                let raw_input = self.egui_state.take_egui_input(&self.window);
                let full_output = self.egui_ctx.run(raw_input, |ctx| {
                    FractalPanel::show(ctx, &mut self.ui_state);

                    // Debug overlay
                    if self.ui_state.show_debug {
                        egui::Window::new("Debug")
                            .anchor(egui::Align2::RIGHT_TOP, [-10.0, 10.0])
                            .show(ctx, |ui| {
                                ui.label(format!("Version: {}", self.ui_state.version_info));
                                ui.label(format!("FPS: {:.1}", 1.0 / (self.last_frame.elapsed().as_secs_f32() + 0.001)));
                                ui.label(format!("Camera: ({:.2}, {:.2}, {:.2})",
                                    self.camera.position.x,
                                    self.camera.position.y,
                                    self.camera.position.z));
                                ui.label(format!("Zoom: {:.4}x", 1.0 / self.camera.distance));
                            });
                    }

                    // Log window
                    if self.ui_state.show_logs {
                        egui::Window::new("Logs")
                            .default_size([600.0, 350.0])
                            .resizable(true)
                            .show(ctx, |ui| {
                                // Filter bar
                                ui.horizontal(|ui| {
                                    ui.label("Filter:");
                                    ui.text_edit_singleline(&mut self.ui_state.log_filter_text);
                                    if ui.button("Clear").clicked() {
                                        if let Ok(mut buf) = self.log_entries.lock() {
                                            buf.clear();
                                        }
                                    }
                                });
                                ui.horizontal(|ui| {
                                    ui.toggle_value(&mut self.ui_state.log_show_info, "INFO");
                                    ui.toggle_value(&mut self.ui_state.log_show_warn, "WARN");
                                    ui.toggle_value(&mut self.ui_state.log_show_error, "ERROR");
                                });
                                ui.separator();

                                // Log entries
                                let filter_text = self.ui_state.log_filter_text.to_lowercase();
                                egui::ScrollArea::vertical()
                                    .stick_to_bottom(true)
                                    .show(ui, |ui| {
                                        if let Ok(entries) = self.log_entries.lock() {
                                            for entry in entries.iter() {
                                                // Level filter
                                                let show = match entry.level {
                                                    log::Level::Error => self.ui_state.log_show_error,
                                                    log::Level::Warn => self.ui_state.log_show_warn,
                                                    log::Level::Info => self.ui_state.log_show_info,
                                                    _ => false,
                                                };
                                                if !show {
                                                    continue;
                                                }

                                                let formatted = entry.formatted();

                                                // Text filter
                                                if !filter_text.is_empty()
                                                    && !formatted.to_lowercase().contains(&filter_text)
                                                {
                                                    continue;
                                                }

                                                let color = match entry.level {
                                                    log::Level::Error => egui::Color32::from_rgb(255, 80, 80),
                                                    log::Level::Warn => egui::Color32::from_rgb(255, 200, 50),
                                                    _ => egui::Color32::from_gray(200),
                                                };
                                                ui.colored_label(color, &formatted);
                                            }
                                        }
                                    });
                            });
                    }
                });

                self.egui_state.handle_platform_output(&self.window, full_output.platform_output);

                let clipped_primitives = self.egui_ctx.tessellate(
                    full_output.shapes,
                    self.egui_ctx.pixels_per_point(),
                );

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

                {
                    let render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("Egui Render Pass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &view,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Load,
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        depth_stencil_attachment: None,
                        timestamp_writes: None,
                        occlusion_query_set: None,
                    });

                    let mut render_pass = render_pass.forget_lifetime();
                    self.egui_renderer.render(
                        &mut render_pass,
                        &clipped_primitives,
                        &screen_descriptor,
                    );
                }

                for id in &full_output.textures_delta.free {
                    self.egui_renderer.free_texture(id);
                }
            }
        }

        // Submit and present
        self.render_ctx.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        self.rendered_frames = self.rendered_frames.saturating_add(1);

        // Only process app logic after splash is dismissed
        if self.splash.is_none() {
            self.camera = self.ui_state.camera.clone();
            self.handle_session_requests();
            self.save_settings_if_dirty();

            // Handle "open config file" request from UI
            #[cfg(not(target_arch = "wasm32"))]
            if self.ui_state.open_config_requested {
                self.ui_state.open_config_requested = false;
                self.open_config_file();
            }
        }

        Ok(())
    }

    /// Render the splash screen: background image + bottom overlay with text and icon.
    fn render_splash_frame(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
    ) {
        let (width, height) = self.render_ctx.size();

        // Clear to black on every splash frame. Each swapchain texture may be
        // uninitialized (white) on first use, and the swapchain has multiple buffers.
        {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Splash Clear"),
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
        }

        // Build egui splash UI
        let raw_input = self.egui_state.take_egui_input(&self.window);
        let splash = self.splash.as_ref().unwrap();
        let bg_tex_id = splash.background.id();
        let bg_aspect = splash.bg_aspect;
        let icon_tex_id = splash.icon.id();
        let status = splash.status.clone();
        let version_info = self.ui_state.version_info.clone();

        let full_output = self.egui_ctx.run(raw_input, |ctx| {
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE.fill(egui::Color32::BLACK))
                .show(ctx, |ui| {
                    let rect = ui.max_rect();

                    // 1) Background image — scale to cover
                    let rect_aspect = rect.width() / rect.height();
                    let (img_w, img_h) = if rect_aspect > bg_aspect {
                        (rect.width(), rect.width() / bg_aspect)
                    } else {
                        (rect.height() * bg_aspect, rect.height())
                    };
                    let img_rect = egui::Rect::from_center_size(
                        rect.center(),
                        egui::vec2(img_w, img_h),
                    );
                    ui.painter().image(
                        bg_tex_id,
                        img_rect,
                        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                        egui::Color32::WHITE,
                    );

                    // 2) Semi-transparent dark gradient at bottom for text readability
                    let gradient_height = 100.0;
                    let gradient_rect = egui::Rect::from_min_max(
                        egui::pos2(rect.left(), rect.bottom() - gradient_height),
                        rect.right_bottom(),
                    );
                    ui.painter().rect_filled(
                        gradient_rect,
                        0.0,
                        egui::Color32::from_black_alpha(180),
                    );

                    // 3) Bottom-left: App name, version, status
                    let margin = 16.0;
                    let text_bottom = rect.bottom() - margin;
                    let text_left = rect.left() + margin;

                    ui.painter().text(
                        egui::pos2(text_left, text_bottom - 50.0),
                        egui::Align2::LEFT_BOTTOM,
                        "Modern Fractal Viewer",
                        egui::FontId::proportional(20.0),
                        egui::Color32::WHITE,
                    );

                    ui.painter().text(
                        egui::pos2(text_left, text_bottom - 28.0),
                        egui::Align2::LEFT_BOTTOM,
                        &version_info,
                        egui::FontId::proportional(13.0),
                        egui::Color32::from_gray(180),
                    );

                    ui.painter().text(
                        egui::pos2(text_left, text_bottom - 8.0),
                        egui::Align2::LEFT_BOTTOM,
                        &status,
                        egui::FontId::proportional(13.0),
                        egui::Color32::from_gray(140),
                    );

                    // 4) Bottom-right: icon + copyright
                    let right_margin = rect.right() - margin;
                    let icon_size = 40.0;
                    let icon_rect = egui::Rect::from_min_size(
                        egui::pos2(right_margin - icon_size, text_bottom - 62.0),
                        egui::vec2(icon_size, icon_size),
                    );
                    ui.painter().image(
                        icon_tex_id,
                        icon_rect,
                        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                        egui::Color32::WHITE,
                    );

                    ui.painter().text(
                        egui::pos2(right_margin, text_bottom - 8.0),
                        egui::Align2::RIGHT_BOTTOM,
                        "Copyright \u{00A9} 2025 jiamingfeng. MIT",
                        egui::FontId::proportional(11.0),
                        egui::Color32::from_gray(140),
                    );
                });
        });

        // Tessellate and render egui
        self.egui_state.handle_platform_output(&self.window, full_output.platform_output);
        let clipped = self.egui_ctx.tessellate(
            full_output.shapes,
            self.egui_ctx.pixels_per_point(),
        );

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
            encoder,
            &clipped,
            &screen_descriptor,
        );

        {
            let render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Splash Egui Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            let mut render_pass = render_pass.forget_lifetime();
            self.egui_renderer.render(&mut render_pass, &clipped, &screen_descriptor);
        }

        for id in &full_output.textures_delta.free {
            self.egui_renderer.free_texture(id);
        }
    }

    fn save_settings_if_dirty(&mut self) {
        if !self.ui_state.settings_dirty {
            return;
        }
        self.ui_state.settings_dirty = false;
        #[cfg(target_arch = "wasm32")]
        {
            if let Err(e) = crate::config_manager::save_settings_wasm(&self.ui_state.settings) {
                log::error!("Failed to save control ranges: {e}");
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            if let Some(ref dir) = self.data_dir {
                if let Err(e) = crate::config_manager::save_settings(dir, &self.ui_state.settings) {
                    log::error!("Failed to save control ranges: {e}");
                }
            }
        }
    }

    /// Save current state to the reserved "__last_session" slot (called on app exit).
    fn save_last_session(&self) {
        let Some(ref mgr) = self.session_manager else {
            return;
        };
        let session = SavedSession {
            version: "1".to_string(),
            timestamp: SessionManager::timestamp_iso8601(),
            name: "Last Session".to_string(),
            fractal_type_name: self.ui_state.fractal_params.fractal_type.name().to_string(),
            thumbnail_base64: String::new(), // skip thumbnail for speed on exit
            thumbnail_width: 0,
            thumbnail_height: 0,
            fractal_params: self.ui_state.fractal_params,
            ray_march_config: self.ui_state.ray_march_config,
            lighting_config: self.ui_state.lighting_config,
            color_config: self.ui_state.color_config,
            camera: self.camera.clone(),
        };
        match mgr.save_overwrite("__last_session", &session) {
            Ok(()) => log::info!("Saved last session"),
            Err(e) => log::error!("Failed to save last session: {e}"),
        }
    }

    /// Open the control settings TOML config file in the OS default editor.
    #[cfg(not(target_arch = "wasm32"))]
    fn open_config_file(&mut self) {
        let Some(ref dir) = self.data_dir else {
            log::warn!("No data directory available");
            return;
        };
        let path = dir.join("settings.toml");

        // Ensure file exists before opening
        if !path.exists() {
            self.ui_state.settings_dirty = true;
            self.save_settings_if_dirty();
        }

        log::info!("Opening config file: {}", path.display());
        if let Err(e) = std::process::Command::new("cmd")
            .args(["/C", "start", "", &path.display().to_string()])
            .spawn()
        {
            // Fallback for non-Windows
            if let Err(e2) = std::process::Command::new("xdg-open")
                .arg(&path)
                .spawn()
            {
                if let Err(e3) = std::process::Command::new("open")
                    .arg(&path)
                    .spawn()
                {
                    log::error!("Failed to open config file: {e}, {e2}, {e3}");
                }
            }
        }
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

        // Handle save new
        if self.ui_state.pending_save {
            self.ui_state.pending_save = false;
            self.save_session(None);
        }

        // Handle overwrite existing
        if let Some(id) = self.ui_state.pending_overwrite.take() {
            self.save_session(Some(&id));
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

    /// Save the current session. If `overwrite_id` is `Some`, overwrites that slot;
    /// otherwise creates a new slot.
    fn save_session(&mut self, overwrite_id: Option<&str>) {
        // Capture thumbnail (native only — WASM can't do blocking GPU readback)
        #[cfg(not(target_arch = "wasm32"))]
        let (thumbnail_base64, thumb_w, thumb_h) = {
            let tw = self.thumbnail_capture.width();
            let th = self.thumbnail_capture.height();

            // Temporarily set resolution to thumbnail size
            self.pipeline.uniforms.update_resolution(tw, th);
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

            // Read pixels back (blocking — safe on native, would deadlock on WASM)
            let pixels = self.thumbnail_capture.read_pixels(&self.render_ctx.device);

            // Restore original resolution
            let (width, height) = self.render_ctx.size();
            self.pipeline.uniforms.update_resolution(width, height);
            self.pipeline.update_uniforms(&self.render_ctx.queue);

            (encode_thumbnail_png(&pixels, tw, th), tw, th)
        };
        #[cfg(target_arch = "wasm32")]
        let (thumbnail_base64, thumb_w, thumb_h) = (String::new(), 0u32, 0u32);

        // Auto-generate name from fractal type + short timestamp
        let timestamp = SessionManager::timestamp_iso8601();
        let short_ts = &timestamp[..10]; // YYYY-MM-DD
        let name = format!(
            "{} {}",
            self.ui_state.fractal_params.fractal_type.name(),
            short_ts
        );

        let session = SavedSession {
            version: "1".to_string(),
            timestamp,
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
            let result = if let Some(id) = overwrite_id {
                mgr.save_overwrite(id, &session).map(|()| id.to_string())
            } else {
                mgr.save(&session)
            };
            match result {
                Ok(id) => log::info!("Session saved as {id}"),
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

/// Decode a base64 PNG thumbnail and upload as an egui texture.
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
