//! Fractal App Library
//!
//! Shared code between native binary, WASM, and Android targets.

pub mod app;
pub mod input;
pub mod session_manager;

// Android entry point
#[cfg(target_os = "android")]
use winit::platform::android::activity::AndroidApp;

#[cfg(target_os = "android")]
#[no_mangle]
fn android_main(android_app: AndroidApp) {
    use std::sync::Arc;
    use winit::application::ApplicationHandler;
    use winit::event::WindowEvent;
    use winit::event_loop::{ActiveEventLoop, EventLoop};
    use winit::platform::android::EventLoopBuilderExtAndroid;
    use winit::window::{Window, WindowId};

    use app::App;

    android_logger::init_once(
        android_logger::Config::default().with_max_level(log::LevelFilter::Info),
    );

    log::info!("Starting Fractal Viewer (Android)");

    struct AndroidAppHandler {
        app: Option<App>,
        window: Option<Arc<Window>>,
    }

    impl ApplicationHandler for AndroidAppHandler {
        fn resumed(&mut self, event_loop: &ActiveEventLoop) {
            if self.window.is_none() {
                let window = Arc::new(
                    event_loop
                        .create_window(
                            winit::window::WindowAttributes::default()
                                .with_title("Modern Fractal Viewer"),
                        )
                        .expect("Failed to create window"),
                );
                self.window = Some(window.clone());

                match pollster::block_on(App::new(window)) {
                    Ok(app) => {
                        log::info!("Application initialized successfully (Android)");
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

    let event_loop = EventLoop::builder()
        .with_android_app(android_app)
        .build()
        .expect("Failed to create event loop");
    event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);

    let mut handler = AndroidAppHandler {
        app: None,
        window: None,
    };
    event_loop
        .run_app(&mut handler)
        .expect("Event loop error");
}
