//! Fractal Viewer Application
//!
//! A cross-platform 3D fractal viewer using ray marching.

mod app;
mod input;

use app::App;

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    // Initialize logging
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .init();
    
    log::info!("Starting Fractal Viewer");
    
    // Run the application
    pollster::block_on(run());
}

#[cfg(target_arch = "wasm32")]
fn main() {
    // WASM entry point
    std::panic::set_hook(Box::new(console_error_panic_hook::hook));
    console_log::init_with_level(log::Level::Info).expect("Failed to init logger");
    
    log::info!("Starting Fractal Viewer (WASM)");
    
    wasm_bindgen_futures::spawn_local(run());
}

async fn run() {
    // Create event loop
    let event_loop = winit::event_loop::EventLoop::new()
        .expect("Failed to create event loop");
    
    event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);
    
    // Create window
    let window_attrs = winit::window::WindowAttributes::default()
        .with_title("Fractal Viewer")
        .with_inner_size(winit::dpi::LogicalSize::new(1280, 720));
    
    #[cfg(target_arch = "wasm32")]
    let window_attrs = {
        use wasm_bindgen::JsCast;
        use winit::platform::web::WindowAttributesExtWebSys;
        
        let canvas = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.get_element_by_id("canvas"))
            .and_then(|e| e.dyn_into::<web_sys::HtmlCanvasElement>().ok());
        
        if let Some(canvas) = canvas {
            window_attrs.with_canvas(Some(canvas))
        } else {
            window_attrs
        }
    };
    
    let window = std::sync::Arc::new(
        event_loop.create_window(window_attrs)
            .expect("Failed to create window")
    );
    
    // Create application
    let mut app = App::new(window.clone()).await
        .expect("Failed to create application");
    
    log::info!("Application initialized successfully");
    
    // Run event loop
    event_loop.run(move |event, elwt| {
        app.handle_event(&event, elwt);
    }).expect("Event loop error");
}
