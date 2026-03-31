//! Main application state and rendering loop

use std::sync::Arc;

#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;
#[cfg(target_arch = "wasm32")]
use web_time::Instant;

use fractal_core::benchmark_types::{
    BenchmarkScenario, TimingMethod, compute_stats,
};
use fractal_core::{Camera, FractalParams, SavedSession};
use fractal_core::sdf::{ColorConfig, LightingConfig, RayMarchConfig};
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

/// Data captured in phase 1 of a non-blocking export, waiting for the GPU
/// staging buffer to become mapped so the volume can be read back without
/// blocking the main thread.
#[cfg(not(target_arch = "wasm32"))]
struct PendingGpuReadback {
    path: std::path::PathBuf,
    rx: std::sync::mpsc::Receiver<Result<(), wgpu::BufferAsyncError>>,
    config: fractal_core::mesh::ExportConfig,
    /// Effective bounds after boundary extension (SDF-space)
    effective_bounds_min: [f32; 3],
    effective_bounds_max: [f32; 3],
    /// Effective iso-level after adaptive computation
    effective_iso_level: f32,
    color_config: fractal_core::sdf::ColorConfig,
    lighting_config: fractal_core::sdf::LightingConfig,
    /// Android MediaStore display name. Empty on non-Android targets.
    display_name: String,
}

/// In-app benchmark orchestration state.
struct InAppBenchmarkState {
    /// Scenarios to run
    scenarios: Vec<BenchmarkScenario>,
    /// Index of the current scenario
    current_scenario: usize,
    /// Frames rendered for the current scenario
    frames_done: u32,
    /// Frames to render per scenario
    frames_per_scenario: u32,
    /// Warm-up frames remaining for the current scenario
    warmup_remaining: u32,
    /// Warm-up frames per scenario
    warmup_per_scenario: u32,
    /// Collected frame times for the current scenario (ms)
    current_times: Vec<f64>,
    /// Saved user state to restore after benchmark
    saved_fractal_params: FractalParams,
    saved_camera: Camera,
    saved_ray_march: RayMarchConfig,
    saved_color: ColorConfig,
    saved_lighting: LightingConfig,
    saved_vsync: bool,
}

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
    /// GPU compute pipeline for SDF volume sampling (mesh export)
    #[cfg(not(target_arch = "wasm32"))]
    sdf_compute: Option<fractal_renderer::compute::SdfVolumeCompute>,
    /// Background export thread handle
    #[cfg(not(target_arch = "wasm32"))]
    export_thread: Option<std::thread::JoinHandle<Result<std::path::PathBuf, String>>>,
    /// Shared export progress (written by worker thread, read by UI)
    #[cfg(not(target_arch = "wasm32"))]
    export_progress: std::sync::Arc<std::sync::Mutex<f32>>,
    /// Pending GPU readback for non-blocking export (path + receiver + export params)
    #[cfg(not(target_arch = "wasm32"))]
    pending_gpu_readback: Option<PendingGpuReadback>,
    /// In-app benchmark orchestration state
    benchmark_state: Option<InAppBenchmarkState>,
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
            let shader_paths = FractalPipeline::shader_paths();
            let config_path = data_dir.as_ref().map(|d| d.join("settings.toml"));
            Some(crate::hot_reload::HotReloader::new(shader_paths, config_path))
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
            #[cfg(not(target_arch = "wasm32"))]
            sdf_compute: None,
            #[cfg(not(target_arch = "wasm32"))]
            export_thread: None,
            #[cfg(not(target_arch = "wasm32"))]
            export_progress: std::sync::Arc::new(std::sync::Mutex::new(0.0)),
            #[cfg(not(target_arch = "wasm32"))]
            pending_gpu_readback: None,
            benchmark_state: None,
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

                // Skip rendering when minimized (0x0 surface)
                let size = self.window.inner_size();
                if size.width == 0 || size.height == 0 {
                    return;
                }

                self.update();
                if let Err(e) = self.render() {
                    match e {
                        wgpu::SurfaceError::Outdated | wgpu::SurfaceError::Lost => {
                            let size = self.window.inner_size();
                            if size.width > 0 && size.height > 0 {
                                self.render_ctx.resize(size.width, size.height);
                            }
                        }
                        _ => log::error!("Render error: {}", e),
                    }
                }
            }

            _ => {}
        }
    }
    
    fn handle_keyboard(&mut self, event: &winit::event::KeyEvent) {
        // Track L key for light direction control
        if let PhysicalKey::Code(KeyCode::KeyL) = event.physical_key {
            self.input.l_key_down = event.state == ElementState::Pressed;
        }

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

        if self.input.l_key_down {
            // Light direction control: orbit light on unit sphere
            self.orbit_light(dx * 0.005, -dy * 0.005);
        } else if self.input.left_mouse_down {
            // Orbit camera
            self.camera.orbit(dx * 0.005, -dy * 0.005);
        }

        if self.input.right_mouse_down && !self.input.l_key_down {
            // Pan camera
            let pan_speed = self.camera.distance * 0.002;
            self.camera.pan(glam::Vec3::new(-dx * pan_speed, -dy * pan_speed, 0.0));
        }

        self.input.mouse_pos = (x, y);
    }

    /// Orbit the light direction on a unit sphere by azimuth/elevation deltas.
    fn orbit_light(&mut self, d_azimuth: f32, d_elevation: f32) {
        let dir = &self.ui_state.lighting_config.light_dir;
        // Convert current direction to spherical coordinates
        let r = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt().max(0.001);
        let mut elevation = (dir[1] / r).asin();
        let mut azimuth = dir[2].atan2(dir[0]);

        azimuth += d_azimuth;
        elevation = (elevation + d_elevation).clamp(
            -std::f32::consts::FRAC_PI_2 + 0.01,
            std::f32::consts::FRAC_PI_2 - 0.01,
        );

        // Convert back to Cartesian (normalized)
        let cos_el = elevation.cos();
        self.ui_state.lighting_config.light_dir = [
            cos_el * azimuth.cos(),
            elevation.sin(),
            cos_el * azimuth.sin(),
        ];
    }
    
    /// Draw a small XYZ coordinate axes gizmo in the bottom-left corner.
    fn draw_axes_gizmo(ctx: &egui::Context) {
        let screen = ctx.screen_rect();
        let margin = 50.0;
        let center = egui::pos2(screen.right() - margin, screen.bottom() - margin);
        let axis_len = 30.0;

        egui::Area::new(egui::Id::new("axes_gizmo"))
            .fixed_pos(egui::pos2(0.0, 0.0))
            .order(egui::Order::Background)
            .interactable(false)
            .show(ctx, |ui| {
                let painter = ui.painter();

                // Background circle
                painter.circle_filled(center, axis_len + 8.0, egui::Color32::from_black_alpha(60));

                // X axis (Red) → right
                let x_end = egui::pos2(center.x + axis_len, center.y);
                painter.line_segment([center, x_end], egui::Stroke::new(2.0, egui::Color32::from_rgb(220, 50, 50)));
                painter.text(egui::pos2(x_end.x + 4.0, x_end.y), egui::Align2::LEFT_CENTER, "X",
                    egui::FontId::proportional(10.0), egui::Color32::from_rgb(220, 50, 50));

                // Y axis (Green) → up
                let y_end = egui::pos2(center.x, center.y - axis_len);
                painter.line_segment([center, y_end], egui::Stroke::new(2.0, egui::Color32::from_rgb(50, 200, 50)));
                painter.text(egui::pos2(y_end.x, y_end.y - 6.0), egui::Align2::CENTER_BOTTOM, "Y",
                    egui::FontId::proportional(10.0), egui::Color32::from_rgb(50, 200, 50));

                // Z axis (Blue) → diagonal (depth hint)
                let z_end = egui::pos2(center.x - axis_len * 0.6, center.y + axis_len * 0.4);
                painter.line_segment([center, z_end], egui::Stroke::new(2.0, egui::Color32::from_rgb(50, 80, 220)));
                painter.text(egui::pos2(z_end.x - 4.0, z_end.y), egui::Align2::RIGHT_CENTER, "Z",
                    egui::FontId::proportional(10.0), egui::Color32::from_rgb(50, 80, 220));
            });
    }

    /// Draw a 2D light direction gizmo overlay via egui.
    fn draw_light_gizmo(ctx: &egui::Context, light_dir: &[f32; 3]) {
        let screen = ctx.screen_rect();
        let center = egui::pos2(screen.center().x, screen.center().y);
        let radius = 80.0_f32;

        // Light gizmo draws behind the UI panel
        egui::Area::new(egui::Id::new("light_gizmo"))
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .order(egui::Order::Background)
            .show(ctx, |ui| {
                let painter = ui.painter();

                // Semi-transparent background circle
                painter.circle_filled(center, radius + 5.0, egui::Color32::from_black_alpha(80));

                // Equator ring (horizontal circle = ellipse)
                painter.circle_stroke(
                    center,
                    radius,
                    egui::Stroke::new(1.0, egui::Color32::from_gray(80)),
                );

                // Upper hemisphere arc
                let dome_center = egui::pos2(center.x, center.y);
                painter.circle_stroke(
                    egui::pos2(dome_center.x, dome_center.y - radius * 0.1),
                    radius * 0.7,
                    egui::Stroke::new(0.5, egui::Color32::from_gray(60)),
                );

                // Axis lines (RGB = XYZ)
                let axis_len = radius * 0.85;
                // X axis (Red) — right
                painter.line_segment(
                    [center, egui::pos2(center.x + axis_len, center.y)],
                    egui::Stroke::new(1.5, egui::Color32::from_rgb(200, 50, 50)),
                );
                // Y axis (Green) — up
                painter.line_segment(
                    [center, egui::pos2(center.x, center.y - axis_len)],
                    egui::Stroke::new(1.5, egui::Color32::from_rgb(50, 200, 50)),
                );
                // Z axis (Blue) — into screen (projected as diagonal)
                painter.line_segment(
                    [center, egui::pos2(center.x - axis_len * 0.5, center.y + axis_len * 0.35)],
                    egui::Stroke::new(1.5, egui::Color32::from_rgb(50, 80, 200)),
                );

                // Light direction arrow (projected to 2D: X→right, Y→up, Z→depth hint)
                let lx = light_dir[0];
                let ly = light_dir[1];
                let lz = light_dir[2];
                let arrow_end = egui::pos2(
                    center.x + (lx - lz * 0.35) * radius * 0.8,
                    center.y - (ly + lz * 0.15) * radius * 0.8,
                );

                // Arrow shaft
                painter.line_segment(
                    [center, arrow_end],
                    egui::Stroke::new(3.0, egui::Color32::YELLOW),
                );
                // Arrow tip
                painter.circle_filled(arrow_end, 5.0, egui::Color32::YELLOW);

                // Label
                painter.text(
                    egui::pos2(center.x, center.y + radius + 15.0),
                    egui::Align2::CENTER_TOP,
                    "Light Direction (L + drag)",
                    egui::FontId::proportional(12.0),
                    egui::Color32::from_gray(180),
                );
            });
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
                                ui.label(format!("Camera: ({:.2}, {:.2}, {:.2}) cm",
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

                    // Coordinate axes gizmo (always visible, bottom-left)
                    Self::draw_axes_gizmo(ctx);

                    // Light direction gizmo (shown when L key is held)
                    self.ui_state.light_control_active = self.input.l_key_down;
                    if self.ui_state.light_control_active {
                        Self::draw_light_gizmo(ctx, &self.ui_state.lighting_config.light_dir);
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
            self.handle_benchmark_requests();
            if !self.ui_state.benchmark_running {
                self.handle_session_requests();
                #[cfg(not(target_arch = "wasm32"))]
                self.handle_export_requests();
            }
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
    fn save_last_session(&mut self) {
        let timestamp = SessionManager::timestamp_iso8601();
        let short_ts = &timestamp[..10]; // YYYY-MM-DD
        let name = format!("Last Session {short_ts}");
        self.save_session_with_name(Some("__last_session"), &name);
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

    fn handle_benchmark_requests(&mut self) {
        // Start a new benchmark run if requested
        if self.ui_state.pending_benchmark && self.benchmark_state.is_none() {
            self.ui_state.pending_benchmark = false;
            self.ui_state.benchmark_running = true;
            self.ui_state.benchmark_results.clear();
            self.ui_state.benchmark_frame_times.clear();
            self.ui_state.benchmark_progress = 0.0;

            // Use a quick matrix: 1 fractal type (current) × 1 resolution (512×512)
            // × 4 color modes × 2 lighting models = 8 scenarios.
            // The full 192-scenario matrix is for the CLI; in-app uses a lighter set.
            let ft = self.ui_state.fractal_params.fractal_type;
            let params = fractal_core::FractalParams::for_type(ft);
            let camera = Camera::default();
            let color_modes: &[u32] = &[1, 2, 3, 4];
            let lighting_models: &[u32] = &[0, 1];

            let mut scenarios = Vec::new();
            for &cm in color_modes {
                for &lm in lighting_models {
                    let mut cc = ColorConfig::default();
                    cc.color_mode = cm;
                    let mut lc = LightingConfig::default();
                    lc.lighting_model = lm;
                    let cm_name = match cm {
                        1 => "orbit-trap", 2 => "iteration", 3 => "normal", _ => "combined",
                    };
                    let lm_name = if lm == 0 { "Blinn-Phong" } else { "PBR" };
                    scenarios.push(BenchmarkScenario {
                        name: format!("{} @ 512x512 / {} / {}", ft.name(), cm_name, lm_name),
                        fractal_params: params,
                        camera: camera.clone(),
                        width: 512,
                        height: 512,
                        ray_march_config: RayMarchConfig::default(),
                        color_config: cc,
                        lighting_config: lc,
                    });
                }
            }

            // Save user state
            let saved = InAppBenchmarkState {
                scenarios,
                current_scenario: 0,
                frames_done: 0,
                frames_per_scenario: 30,
                warmup_remaining: 5,
                warmup_per_scenario: 5,
                current_times: Vec::new(),
                saved_fractal_params: self.ui_state.fractal_params,
                saved_camera: self.camera.clone(),
                saved_ray_march: self.ui_state.ray_march_config,
                saved_color: self.ui_state.color_config,
                saved_lighting: self.ui_state.lighting_config,
                saved_vsync: self.ui_state.vsync,
            };

            // Disable VSync for accurate timing
            if self.ui_state.vsync {
                self.ui_state.vsync = false;
                self.vsync_prev = false;
                self.render_ctx.set_present_mode(wgpu::PresentMode::AutoNoVsync);
            }

            self.benchmark_state = Some(saved);
            self.apply_benchmark_scenario();
            return;
        }

        // Stop if requested
        if self.ui_state.benchmark_stop_requested {
            self.ui_state.benchmark_stop_requested = false;
            self.finish_benchmark();
            return;
        }

        // Advance running benchmark
        if let Some(ref mut bench) = self.benchmark_state {
            if bench.current_scenario >= bench.scenarios.len() {
                // All done
                self.finish_benchmark();
                return;
            }

            if bench.warmup_remaining > 0 {
                // Still warming up — just count the frame
                bench.warmup_remaining -= 1;
                return;
            }

            // Record frame time (the time between last_frame and now covers
            // the submit+present of the previous frame, which includes GPU work)
            let frame_time_ms = self.last_frame.elapsed().as_secs_f64() * 1000.0;
            bench.current_times.push(frame_time_ms);
            bench.frames_done += 1;

            // Update live graph
            self.ui_state.benchmark_frame_times.push(frame_time_ms);
            // Keep last 200 entries
            if self.ui_state.benchmark_frame_times.len() > 200 {
                self.ui_state.benchmark_frame_times.remove(0);
            }

            if bench.frames_done >= bench.frames_per_scenario {
                // Finish this scenario
                let scenario_name = bench.scenarios[bench.current_scenario].name.clone();
                let mut times = std::mem::take(&mut bench.current_times);
                let result = compute_stats(&scenario_name, TimingMethod::CpuPollWait, &mut times);
                self.ui_state.benchmark_results.push(result);

                // Move to next scenario
                bench.current_scenario += 1;
                bench.frames_done = 0;
                bench.warmup_remaining = bench.warmup_per_scenario;

                let total = bench.scenarios.len();
                let done = bench.current_scenario;
                self.ui_state.benchmark_progress = done as f32 / total as f32;

                if done < total {
                    self.apply_benchmark_scenario();
                }
            }
        }
    }

    fn apply_benchmark_scenario(&mut self) {
        if let Some(ref bench) = self.benchmark_state {
            if bench.current_scenario < bench.scenarios.len() {
                let s = &bench.scenarios[bench.current_scenario];
                self.ui_state.fractal_params = s.fractal_params;
                self.ui_state.ray_march_config = s.ray_march_config;
                self.ui_state.color_config = s.color_config;
                self.ui_state.lighting_config = s.lighting_config;
                self.camera = s.camera.clone();
                self.ui_state.camera = s.camera.clone();
                self.ui_state.benchmark_current_scenario = s.name.clone();
            }
        }
    }

    fn finish_benchmark(&mut self) {
        if let Some(bench) = self.benchmark_state.take() {
            // Restore user state
            self.ui_state.fractal_params = bench.saved_fractal_params;
            self.camera = bench.saved_camera.clone();
            self.ui_state.camera = bench.saved_camera;
            self.ui_state.ray_march_config = bench.saved_ray_march;
            self.ui_state.color_config = bench.saved_color;
            self.ui_state.lighting_config = bench.saved_lighting;

            // Restore VSync
            if bench.saved_vsync != self.ui_state.vsync {
                self.ui_state.vsync = bench.saved_vsync;
                self.vsync_prev = bench.saved_vsync;
                let mode = if bench.saved_vsync {
                    wgpu::PresentMode::AutoVsync
                } else {
                    wgpu::PresentMode::AutoNoVsync
                };
                self.render_ctx.set_present_mode(mode);
            }
        }
        self.ui_state.benchmark_running = false;
        self.ui_state.benchmark_progress = 1.0;
        self.ui_state.benchmark_current_scenario = String::from("Done");
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

    /// Handle mesh export requests from the UI.
    ///
    /// The export pipeline is split into three non-blocking phases so the
    /// viewport stays responsive throughout:
    ///
    /// 1. **start_export** — opens file dialog, dispatches GPU compute, and
    ///    initiates an async buffer map (no blocking `device.poll(Wait)`).
    /// 2. **poll GPU readback** — each frame we do a lightweight
    ///    `device.poll(Poll)` and check if the staging buffer is mapped.
    /// 3. **background thread** — once the grid data is available, a CPU
    ///    thread runs Marching Cubes + glTF export while the UI keeps running.
    #[cfg(not(target_arch = "wasm32"))]
    fn handle_export_requests(&mut self) {
        // ── Phase 3: poll running background thread ─────────────────────
        if let Some(ref handle) = self.export_thread {
            if handle.is_finished() {
                let handle = self.export_thread.take().unwrap();
                match handle.join() {
                    Ok(Ok(path)) => {
                        self.ui_state.export_status =
                            Some(format!("Exported to {}", path.display()));
                        log::info!("Mesh exported to {}", path.display());
                    }
                    Ok(Err(e)) => {
                        self.ui_state.export_status = Some(format!("Export failed: {e}"));
                        log::error!("Mesh export failed: {e}");
                    }
                    Err(_) => {
                        self.ui_state.export_status =
                            Some("Export failed: thread panicked".to_string());
                    }
                }
                self.ui_state.export_in_progress = false;
                self.ui_state.export_progress = None;
            } else {
                // Update progress from shared state
                if let Ok(p) = self.export_progress.lock() {
                    self.ui_state.export_progress = Some(*p);
                }
            }
        }

        // ── Phase 2: poll pending GPU readback ──────────────────────────
        if self.pending_gpu_readback.is_some() {
            // Nudge the GPU without blocking
            self.render_ctx.device.poll(wgpu::Maintain::Poll);

            let compute = self.sdf_compute.as_ref().unwrap();
            let ready = {
                let pending = self.pending_gpu_readback.as_ref().unwrap();
                compute.try_read_volume(&pending.rx)
            };

            if let Some(grid) = ready {
                // Take ownership of the pending state
                let pending = self.pending_gpu_readback.take().unwrap();
                self.spawn_export_thread(grid, pending);
            }
        }

        // ── Phase 1: start new export ───────────────────────────────────
        if self.ui_state.pending_export {
            self.ui_state.pending_export = false;
            if self.export_thread.is_some() || self.pending_gpu_readback.is_some() {
                return; // Already running
            }
            self.start_export();
        }
    }

    /// Phase 1: dispatches GPU compute and initiates async buffer map
    /// (non-blocking — the file dialog is the only synchronous part on desktop).
    #[cfg(not(target_arch = "wasm32"))]
    fn start_export(&mut self) {
        let fmt = self.ui_state.export_config.export_format;
        let _fractal_name = self.ui_state.fractal_params.fractal_type.name();

        // Obtain the destination path: file dialog on desktop, temp file on Android.
        let path = {
            #[cfg(not(target_os = "android"))]
            {
                // Open file dialog (synchronous, but user-driven)
                let path = rfd::FileDialog::new()
                    .set_file_name(fmt.default_filename(_fractal_name))
                    .add_filter(fmt.filter_label(), &[fmt.extension()])
                    .save_file();
                let Some(p) = path else {
                    return; // User cancelled
                };
                p
            }
            #[cfg(target_os = "android")]
            {
                // Export to a temp file first; android_export copies it to Downloads afterwards.
                let dir = match self.data_dir.as_deref() {
                    Some(d) => d,
                    None => {
                        self.ui_state.export_status =
                            Some("Export failed: no data directory".to_string());
                        self.ui_state.export_in_progress = false;
                        return;
                    }
                };
                dir.join("temp_export").with_extension(fmt.extension())
            }
        };

        // Capture display name for Android MediaStore call.
        #[cfg(target_os = "android")]
        let android_display_name = {
            let custom = self.ui_state.export_filename.trim().to_string();
            if custom.is_empty() {
                fmt.default_filename(self.ui_state.fractal_params.fractal_type.name())
            } else {
                custom
            }
        };
        #[cfg(not(target_os = "android"))]
        let android_display_name = String::new();

        // Initialize compute pipeline lazily
        if self.sdf_compute.is_none() {
            self.sdf_compute = Some(fractal_renderer::compute::SdfVolumeCompute::new(
                &self.render_ctx.device,
                &self.pipeline.uniform_buffer,
            ));
        }

        // Update uniforms so the GPU has the latest fractal params
        self.pipeline.update_uniforms(&self.render_ctx.queue);

        // GPU phase: dispatch compute shader
        let config = &self.ui_state.export_config;
        let compute = self.sdf_compute.as_mut().unwrap();

        // Convert bounds from cm (UI units) to SDF-space (÷100).
        // The SDF functions operate in their original coordinate space (~±1.5 for
        // Mandelbulb), so the GPU must sample at those positions.
        let mut sdf_bounds_min = [
            config.bounds_min[0] / 100.0,
            config.bounds_min[1] / 100.0,
            config.bounds_min[2] / 100.0,
        ];
        let mut sdf_bounds_max = [
            config.bounds_max[0] / 100.0,
            config.bounds_max[1] / 100.0,
            config.bounds_max[2] / 100.0,
        ];

        // Compute effective iso-level (adaptive scales with voxel size)
        let effective_iso = if config.adaptive_iso {
            let voxel_sizes = [
                (sdf_bounds_max[0] - sdf_bounds_min[0]) / config.resolution as f32,
                (sdf_bounds_max[1] - sdf_bounds_min[1]) / config.resolution as f32,
                (sdf_bounds_max[2] - sdf_bounds_min[2]) / config.resolution as f32,
            ];
            let voxel_diag = (voxel_sizes[0] * voxel_sizes[0]
                + voxel_sizes[1] * voxel_sizes[1]
                + voxel_sizes[2] * voxel_sizes[2])
            .sqrt();
            config.adaptive_iso_factor * voxel_diag
        } else {
            config.iso_level
        };

        // Extend bounds by one voxel + iso-level to capture edge features
        if config.boundary_extension {
            for i in 0..3 {
                let voxel = (sdf_bounds_max[i] - sdf_bounds_min[i]) / config.resolution as f32;
                let ext = voxel + effective_iso;
                sdf_bounds_min[i] -= ext;
                sdf_bounds_max[i] += ext;
            }
        }

        self.ui_state.export_in_progress = true;
        #[cfg(target_os = "android")]
        { self.ui_state.export_status = Some("Exporting to Downloads...".to_string()); }
        #[cfg(not(target_os = "android"))]
        { self.ui_state.export_status = Some("Sampling SDF volume on GPU...".to_string()); }

        match compute.dispatch_single_or_err(
            &self.render_ctx.device,
            &self.render_ctx.queue,
            &self.pipeline.uniform_buffer,
            sdf_bounds_min,
            sdf_bounds_max,
            config.resolution,
        ) {
            Ok(_slab_elements) => {
                // Single slab — use non-blocking async readback
                let rx = compute.initiate_map_async();
                self.pending_gpu_readback = Some(PendingGpuReadback {
                    path,
                    rx,
                    config: config.clone(),
                    effective_bounds_min: sdf_bounds_min,
                    effective_bounds_max: sdf_bounds_max,
                    effective_iso_level: effective_iso,
                    color_config: self.ui_state.color_config.clone(),
                    lighting_config: self.ui_state.lighting_config.clone(),
                    display_name: android_display_name.clone(),
                });
            }
            Err(_slab_info) => {
                // Multi-slab — do blocking slab-by-slab GPU dispatch + readback,
                // then spawn the mesh extraction thread.
                log::info!(
                    "Volume too large for single GPU dispatch; using multi-slab readback \
                     (resolution {})",
                    config.resolution,
                );
                let grid = compute.dispatch_and_read(
                    &self.render_ctx.device,
                    &self.render_ctx.queue,
                    &self.pipeline.uniform_buffer,
                    sdf_bounds_min,
                    sdf_bounds_max,
                    config.resolution,
                );
                // Create a dummy PendingGpuReadback with a pre-completed channel
                let (tx, rx) = std::sync::mpsc::channel();
                let _ = tx.send(Ok(()));
                let pending = PendingGpuReadback {
                    path,
                    rx,
                    config: config.clone(),
                    effective_bounds_min: sdf_bounds_min,
                    effective_bounds_max: sdf_bounds_max,
                    effective_iso_level: effective_iso,
                    color_config: self.ui_state.color_config.clone(),
                    lighting_config: self.ui_state.lighting_config.clone(),
                    display_name: android_display_name,
                };
                self.spawn_export_thread(grid, pending);
            }
        }
    }

    /// Phase 3: spawns a background thread for mesh extraction + glTF export.
    #[cfg(not(target_arch = "wasm32"))]
    fn spawn_export_thread(&mut self, grid: Vec<[f32; 2]>, pending: PendingGpuReadback) {
        let PendingGpuReadback {
            path,
            config,
            effective_bounds_min: bounds_min,
            effective_bounds_max: bounds_max,
            effective_iso_level: iso_level,
            color_config,
            lighting_config,
            display_name: _display_name,
            ..
        } = pending;
        #[cfg(target_os = "android")]
        let display_name = _display_name;
        let method = config.method;
        let resolution = config.resolution;
        let compute_normals = config.compute_normals;
        let _fractal_type_name = self.ui_state.fractal_params.fractal_type.name().to_string();

        let progress = self.export_progress.clone();

        // Reset progress
        if let Ok(mut p) = progress.lock() {
            *p = 0.0;
        }
        let method_name = method.to_string();
        self.ui_state.export_status = Some(format!("Generating mesh ({method_name})..."));

        // CPU phase: background thread for mesh extraction + export
        self.export_thread = Some(std::thread::spawn(move || {
            use fractal_core::mesh::{
                dual_contouring, marching_cubes, surface_nets, gltf_export,
                obj_export, ply_export, palette, smoothing, decimation,
                ExportFormat, ExportMaterial, MeshMethod, SmoothMethod,
            };

            let progress_cb = {
                let progress = progress.clone();
                move |p: f32| {
                    // Scale mesh extraction progress to 0..0.6
                    if let Ok(mut val) = progress.lock() {
                        *val = p * 0.6;
                    }
                }
            };

            // Extract mesh using the selected method
            let mut mesh = match method {
                MeshMethod::DualContouring => dual_contouring::extract_mesh(
                    &grid,
                    [resolution, resolution, resolution],
                    bounds_min,
                    bounds_max,
                    iso_level,
                    compute_normals,
                    Some(&progress_cb),
                ),
                MeshMethod::MarchingCubes => marching_cubes::extract_mesh(
                    &grid,
                    [resolution, resolution, resolution],
                    bounds_min,
                    bounds_max,
                    iso_level,
                    compute_normals,
                    Some(&progress_cb),
                ),
                MeshMethod::SurfaceNets => surface_nets::extract_mesh(
                    &grid,
                    [resolution, resolution, resolution],
                    bounds_min,
                    bounds_max,
                    iso_level,
                    compute_normals,
                    Some(&progress_cb),
                ),
            };

            // Apply mesh smoothing (progress 0.6 → 0.7)
            if config.smooth_iterations > 0 {
                match config.smooth_method {
                    SmoothMethod::Laplacian => {
                        smoothing::laplacian_smooth(
                            &mut mesh,
                            config.smooth_iterations,
                            config.smooth_lambda,
                        );
                    }
                    SmoothMethod::Taubin => {
                        smoothing::taubin_smooth(
                            &mut mesh,
                            config.smooth_iterations,
                            config.smooth_lambda,
                        );
                    }
                    SmoothMethod::None => {}
                }
            }
            if let Ok(mut p) = progress.lock() {
                *p = 0.7;
            }

            // Apply mesh decimation (progress 0.7 → 0.8)
            if config.decimate && config.decimate_target_ratio < 0.999 {
                let dec_progress = {
                    let progress = progress.clone();
                    move |dp: f32| {
                        if let Ok(mut val) = progress.lock() {
                            *val = 0.7 + dp * 0.1;
                        }
                    }
                };
                decimation::decimate(&mut mesh, config.decimate_target_ratio, Some(&dec_progress));
            }
            if let Ok(mut p) = progress.lock() {
                *p = 0.8;
            }

            // Compute vertex colors from trap values (progress 0.8 → 0.9)
            let palette_rgba: Vec<[f32; 4]> = color_config
                .palette_colors
                .iter()
                .map(|c| [c[0], c[1], c[2], 1.0])
                .collect();
            let vertex_count = mesh.positions.len();
            let mut final_colors = Vec::with_capacity(vertex_count);
            for i in 0..vertex_count {
                // mesh.colors stores [trap, 0, 0, 0] from extraction
                let trap = if i < mesh.colors.len() { mesh.colors[i][0] } else { 0.0 };
                let normal = if i < mesh.normals.len() {
                    mesh.normals[i]
                } else {
                    [0.0, 1.0, 0.0]
                };
                final_colors.push(palette::get_vertex_color(
                    trap,
                    normal,
                    &color_config,
                    &palette_rgba,
                ));
            }
            mesh.colors = final_colors;

            if let Ok(mut p) = progress.lock() {
                *p = 0.9;
            }

            // Build PBR material from the lighting/color config
            let export_material = ExportMaterial::from_lighting(&lighting_config, &color_config);

            // Export in the selected format (progress 0.9 → 1.0)
            match config.export_format {
                ExportFormat::Glb => {
                    gltf_export::export_glb(&mesh, Some(&export_material), &path)
                        .map_err(|e| e.to_string())?;
                }
                ExportFormat::Obj => {
                    obj_export::export_obj(&mesh, &path)
                        .map_err(|e| e.to_string())?;
                }
                ExportFormat::Ply => {
                    ply_export::export_ply(&mesh, &path)
                        .map_err(|e| e.to_string())?;
                }
            }

            if let Ok(mut p) = progress.lock() {
                *p = 1.0;
            }

            // On Android: copy the temp file to Downloads via MediaStore, then remove the temp.
            #[cfg(target_os = "android")]
            {
                let mime = config.export_format.mime_type();
                return match crate::android_export::export_to_downloads(&path, &display_name, mime) {
                    Ok(public_path) => {
                        let _ = std::fs::remove_file(&path);
                        Ok(std::path::PathBuf::from(public_path))
                    }
                    Err(e) => {
                        let _ = std::fs::remove_file(&path);
                        Err(format!("Export to Downloads failed: {e}"))
                    }
                };
            }

            #[cfg(not(target_os = "android"))]
            Ok(path)
        }));
    }

    /// Save the current session. If `overwrite_id` is `Some`, overwrites that slot;
    /// otherwise creates a new slot.
    fn save_session(&mut self, overwrite_id: Option<&str>) {
        let timestamp = SessionManager::timestamp_iso8601();
        let short_ts = &timestamp[..10];
        let name = format!(
            "{} {}",
            self.ui_state.fractal_params.fractal_type.name(),
            short_ts
        );
        self.save_session_with_name(overwrite_id, &name);
    }

    fn save_session_with_name(&mut self, overwrite_id: Option<&str>, name: &str) {
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

        let timestamp = SessionManager::timestamp_iso8601();

        let session = SavedSession {
            version: "1".to_string(),
            timestamp,
            name: name.to_string(),
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
