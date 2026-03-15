//! Fractal Viewer Application
//!
//! A cross-platform 3D fractal viewer using ray marching.

use fractal_app::app::App;
use std::sync::Arc;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowId};

/// Application wrapper that handles window creation via ApplicationHandler (native only)
#[cfg(not(target_arch = "wasm32"))]
struct AppHandler {
    /// Window attributes to use when creating the window
    window_attrs: winit::window::WindowAttributes,
    /// The actual application state (created after window is available)
    app: Option<App>,
    /// Window reference needed for async initialization
    window: Option<Arc<Window>>,
}

#[cfg(not(target_arch = "wasm32"))]
impl AppHandler {
    fn new(window_attrs: winit::window::WindowAttributes) -> Self {
        Self {
            window_attrs,
            app: None,
            window: None,
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
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
            // On native, we use pollster to block on the async initialization
            #[cfg(not(target_arch = "wasm32"))]
            {
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

    // Prepare window attributes (fullscreen-like)
    let window_attrs = winit::window::WindowAttributes::default()
        .with_title("Fractal Viewer")
        .with_maximized(true);

    // Create and run the application handler
    let mut handler = AppHandler::new(window_attrs);
    event_loop
        .run_app(&mut handler)
        .expect("Event loop error");
}

#[cfg(target_arch = "wasm32")]
fn main() {
    use std::cell::RefCell;
    use std::rc::Rc;

    // WASM entry point
    std::panic::set_hook(Box::new(console_error_panic_hook::hook));
    console_log::init_with_level(log::Level::Info).expect("Failed to init logger");

    log::info!("Starting Fractal Viewer (WASM)");

    // On WASM, we need a shared state approach since we can't block on async
    // We use Rc<RefCell<>> for the app state that gets initialized asynchronously
    struct WasmAppHandler {
        window_attrs: winit::window::WindowAttributes,
        app: Rc<RefCell<Option<App>>>,
        window: Option<Arc<Window>>,
        pending_init: Rc<RefCell<bool>>,
    }

    impl ApplicationHandler for WasmAppHandler {
        fn resumed(&mut self, event_loop: &ActiveEventLoop) {
            if self.window.is_none() {
                let window = Arc::new(
                    event_loop
                        .create_window(self.window_attrs.clone())
                        .expect("Failed to create window"),
                );
                self.window = Some(window.clone());

                // On WASM, use spawn_local to handle async initialization
                if !*self.pending_init.borrow() {
                    *self.pending_init.borrow_mut() = true;
                    let app_ref = self.app.clone();
                    wasm_bindgen_futures::spawn_local(async move {
                        match App::new(window).await {
                            Ok(app) => {
                                log::info!("Application initialized successfully (WASM)");
                                *app_ref.borrow_mut() = Some(app);
                                // Hide loading indicator
                                if let Some(window) = web_sys::window() {
                                    if let Some(document) = window.document() {
                                        if let Some(loading) = document.get_element_by_id("loading") {
                                            let _ = loading.set_attribute("style", "display:none");
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                log::error!("Failed to create application: {}", e);
                            }
                        }
                    });
                }
            }
        }

        fn window_event(
            &mut self,
            event_loop: &ActiveEventLoop,
            _window_id: WindowId,
            event: WindowEvent,
        ) {
            if let Ok(mut app_opt) = self.app.try_borrow_mut() {
                if let Some(ref mut app) = *app_opt {
                    app.handle_window_event(&event, event_loop);
                }
            }
        }

        fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
            if let Some(ref window) = self.window {
                window.request_redraw();
            }
        }
    }

    // Create event loop
    let event_loop = EventLoop::new().expect("Failed to create event loop");
    event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);

    // Prepare window attributes — let winit create and append the canvas to <body>
    let window_attrs = {
        use winit::platform::web::WindowAttributesExtWebSys;

        // Get viewport size for full-screen canvas
        let (width, height) = web_sys::window()
            .map(|w| {
                let width = w.inner_width().ok().and_then(|v| v.as_f64()).unwrap_or(1280.0);
                let height = w.inner_height().ok().and_then(|v| v.as_f64()).unwrap_or(720.0);
                (width as u32, height as u32)
            })
            .unwrap_or((1280, 720));

        winit::window::WindowAttributes::default()
            .with_title("Fractal Viewer")
            .with_inner_size(winit::dpi::LogicalSize::new(width, height))
            .with_append(true)
    };

    // Create and run the WASM application handler
    let mut handler = WasmAppHandler {
        window_attrs,
        app: Rc::new(RefCell::new(None)),
        window: None,
        pending_init: Rc::new(RefCell::new(false)),
    };
    event_loop
        .run_app(&mut handler)
        .expect("Event loop error");
}
