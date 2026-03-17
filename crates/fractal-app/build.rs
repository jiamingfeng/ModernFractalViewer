//! Build script for fractal-app
//!
//! On Windows, embeds the application icon into the executable.

fn main() {
    // Embed icon into Windows executable
    #[cfg(target_os = "windows")]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        if let Err(e) = res.compile() {
            eprintln!("Warning: Failed to embed Windows icon: {}", e);
        }
    }
}
