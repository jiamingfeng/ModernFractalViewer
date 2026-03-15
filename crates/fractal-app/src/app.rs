//! Main application state and rendering loop

use std::sync::Arc;
use std::time::Instant;

use fractal_core::Camera;
use fractal_renderer::{FractalPipeline, RenderContext};
use fractal_ui::{FractalPanel, UiState};
use winit::event::{ElementState, Event, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::Window;

use crate::input::InputState;

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
        })
    }
    
    pub fn handle_event(&mut self, event: &Event<()>, elwt: &ActiveEventLoop) {
        match event {
            Event::WindowEvent { event, .. } => {
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
                    
                    WindowEvent::RedrawRequested => {
                        self.update();
                        if let Err(e) = self.render() {
                            log::error!("Render error: {}", e);
                        }
                    }
                    
                    _ => {}
                }
            }
            
            Event::AboutToWait => {
                self.window.request_redraw();
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
            let pan_speed = self.camera.zoom * 0.002;
            self.camera.pan(glam::Vec3::new(-dx * pan_speed, dy * pan_speed, 0.0));
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
    
    fn update(&mut self) {
        let now = Instant::now();
        let dt = (now - self.last_frame).as_secs_f32();
        self.last_frame = now;
        
        // Auto-rotate
        if self.ui_state.auto_rotate {
            self.camera.orbit(dt * self.ui_state.rotation_speed, 0.0);
        }
        
        // Sync camera from UI state
        self.camera.fov = self.ui_state.camera.fov;
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
        self.pipeline.update_uniforms(&self.render_ctx.queue);
        
        // Create encoder for fractal rendering
        let mut encoder = self.render_ctx.device.create_command_encoder(
            &wgpu::CommandEncoderDescriptor {
                label: Some("Fractal Encoder"),
            }
        );
        
        // Render fractal
        self.pipeline.render(&mut encoder, &view);
        
        // Submit fractal rendering commands
        self.render_ctx.queue.submit(std::iter::once(encoder.finish()));
        
        // Render egui UI with separate encoder
        // TODO: Fix egui-wgpu lifetime issue - the render() method requires 'static but
        // encoder.begin_render_pass() returns a borrow. Need to investigate proper pattern.
        // For now, egui is disabled to verify fractal rendering works.
        // if self.ui_state.show_panel {
        //     self.render_egui(&view, width, height)?;
        // }
        let _ = (width, height); // Suppress unused warning
        
        output.present();
        
        // Sync UI camera state back
        self.ui_state.camera = self.camera.clone();
        
        Ok(())
    }
    
    #[allow(dead_code)]
    fn render_egui(&mut self, _view: &wgpu::TextureView, _width: u32, _height: u32) -> Result<(), wgpu::SurfaceError> {
        // TODO: This function has a lifetime issue with egui-wgpu 0.31
        // The Renderer::render() method requires 'static lifetime for render_pass
        // but encoder.begin_render_pass() returns a borrow.
        // Need to investigate the proper integration pattern for egui-wgpu.
        // For now, return Ok to allow compilation.
        Ok(())
    }
    
}
