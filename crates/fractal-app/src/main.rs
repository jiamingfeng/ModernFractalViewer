//! Fractal Viewer Application
//!
//! A cross-platform 3D fractal viewer using ray marching.

mod app;
mod input;

use app::App;
use std::sync::Arc;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowId};

/// Application wrapper that handles window creation via ApplicationHandler
struct AppHandler {
    /// Window attributes to use when creating the window
    window_attrs: winit::window::WindowAttributes,
    /// The actual application state (created after window is available)
    app: Option<App>,
    /// Window reference needed for async initialization
    window: Option<Arc<Window>>,
}

impl AppHandler {
    fn new(window_attrs: winit::window::WindowAttributes) -> Self {
        Self {
            window_attrs,
            app: None,
            window: None,
        }
    }
}

impl ApplicationHandler for AppHandler {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // Create window using ActiveEventLoop (the non-deprecated way)
        if self.window.is_none() {
            let window = Arc::new(
                event_loop
                    .create_window(self.window_attrs.clone())
                    .expect("Failed to create window"),
            );
            self.window = Some(window.clone());

            // Initialize the application with the window
            // We use pollster to block on the async initialization
            match pollster::block_on(App::new(window)) {
                Ok(app) => {
                    log::info!("Application initialized successfully");
                    self.app = Some(app);
                }
                Err(e) => {
                    log::error!("Failed to create application: {}", e);
                    event_loop.exit();
                }
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        if let Some(ref mut app) = self.app {
            app.handle_window_event(&event, event_loop);
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(ref window) = self.window {
            window.request_redraw();
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    // Initialize logging
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    log::info!("Starting Fractal Viewer");

    // Create event loop
    let event_loop = EventLoop::new().expect("Failed to create event loop");
    event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);

    // Prepare window attributes
    let window_attrs = winit::window::WindowAttributes::default()
        .with_title("Fractal Viewer")
        .with_inner_size(winit::dpi::LogicalSize::new(1280, 720));

    // Create and run the application handler
    let mut handler = AppHandler::new(window_attrs);
    event_loop
        .run_app(&mut handler)
        .expect("Event loop error");
}

#[cfg(target_arch = "wasm32")]
fn main() {
    // WASM entry point
    std::panic::set_hook(Box::new(console_error_panic_hook::hook));
    console_log::init_with_level(log::Level::Info).expect("Failed to init logger");

    log::info!("Starting Fractal Viewer (WASM)");

    // Create event loop
    let event_loop = EventLoop::new().expect("Failed to create event loop");
    event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);

    // Prepare window attributes with canvas
    let window_attrs = {
        use wasm_bindgen::JsCast;
        use winit::platform::web::WindowAttributesExtWebSys;

        let base_attrs = winit::window::WindowAttributes::default()
            .with_title("Fractal Viewer")
            .with_inner_size(winit::dpi::LogicalSize::new(1280, 720));

        let canvas = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.get_element_by_id("canvas"))
            .and_then(|e| e.dyn_into::<web_sys::HtmlCanvasElement>().ok());

        if let Some(canvas) = canvas {
            base_attrs.with_canvas(Some(canvas))
        } else {
            base_attrs
        }
    };

    // Create and run the application handler
    let mut handler = AppHandler::new(window_attrs);
    event_loop
        .run_app(&mut handler)
        .expect("Event loop error");
}
