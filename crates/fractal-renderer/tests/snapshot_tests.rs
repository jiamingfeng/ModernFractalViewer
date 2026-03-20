//! Snapshot / golden-image tests for fractal rendering.
//!
//! These tests render known parameter sets to a small offscreen texture,
//! read back the pixels, and compare against saved reference images.
//!
//! **Requires a GPU** (even headless). Run with:
//!   cargo test -p fractal-renderer --features snapshot-tests
//!
//! To regenerate golden images, set GENERATE_GOLDEN=1:
//!   GENERATE_GOLDEN=1 cargo test -p fractal-renderer --features snapshot-tests

#![cfg(feature = "snapshot-tests")]

use fractal_core::sdf::{ColorConfig, LightingConfig, RayMarchConfig};
use fractal_core::{Camera, FractalParams, FractalType};
use fractal_renderer::{FractalPipeline, ThumbnailCapture};

const WIDTH: u32 = 128;
const HEIGHT: u32 = 128;

/// Set up a headless GPU device + queue.
fn setup_gpu() -> Option<(wgpu::Device, wgpu::Queue)> {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::all(),
        ..Default::default()
    });

    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::LowPower,
        compatible_surface: None,
        force_fallback_adapter: false,
    }))?;

    let (device, queue) = pollster::block_on(adapter.request_device(
        &wgpu::DeviceDescriptor {
            label: Some("Test Device"),
            required_features: wgpu::Features::empty(),
            required_limits: adapter.limits(),
            memory_hints: wgpu::MemoryHints::Performance,
        },
        None,
    ))
    .ok()?;

    Some((device, queue))
}

/// Render a single frame with the given params and return RGBA pixels.
fn render_frame(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    fractal_params: &FractalParams,
    color_config: &ColorConfig,
) -> Vec<u8> {
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let mut pipeline = FractalPipeline::new_headless(device, format);
    let capture = ThumbnailCapture::new(device, format, WIDTH, HEIGHT);

    let camera = Camera::default();
    let ray_march = RayMarchConfig::default();
    let lighting = LightingConfig::default();

    pipeline.uniforms.update_resolution(WIDTH, HEIGHT);
    pipeline
        .uniforms
        .update_camera(&camera, WIDTH as f32 / HEIGHT as f32);
    pipeline.uniforms.update_fractal(fractal_params);
    pipeline.uniforms.update_ray_march(&ray_march);
    pipeline.uniforms.update_lighting(&lighting);
    pipeline.uniforms.update_color(color_config);
    pipeline.update_uniforms(queue);

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("Snapshot Encoder"),
    });
    pipeline.render(&mut encoder, capture.view());
    capture.copy_to_buffer(&mut encoder);
    queue.submit(std::iter::once(encoder.finish()));

    capture.read_pixels(device)
}

/// Compare two pixel buffers with per-channel tolerance.
fn assert_pixels_similar(actual: &[u8], expected: &[u8], tolerance: u8) {
    assert_eq!(
        actual.len(),
        expected.len(),
        "pixel buffer size mismatch: {} vs {}",
        actual.len(),
        expected.len()
    );
    let mut max_diff: u8 = 0;
    let mut diff_count: usize = 0;
    for (i, (a, e)) in actual.iter().zip(expected.iter()).enumerate() {
        let diff = (*a as i16 - *e as i16).unsigned_abs() as u8;
        if diff > tolerance {
            diff_count += 1;
            if diff_count <= 5 {
                eprintln!(
                    "pixel byte {i}: got {a}, expected {e} (diff {diff}, tolerance {tolerance})"
                );
            }
        }
        max_diff = max_diff.max(diff);
    }
    assert!(
        diff_count == 0,
        "{diff_count} pixel bytes exceeded tolerance {tolerance} (max diff: {max_diff})"
    );
}

/// Load or generate a golden image for the given test name.
fn golden_path(name: &str) -> std::path::PathBuf {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .join("tests")
        .join("golden")
        .join(format!("{name}.raw"))
}

fn check_or_generate(name: &str, pixels: &[u8], tolerance: u8) {
    let path = golden_path(name);

    if std::env::var("GENERATE_GOLDEN").is_ok() {
        std::fs::write(&path, pixels).unwrap();
        eprintln!("Generated golden image: {}", path.display());
        return;
    }

    let expected = std::fs::read(&path).unwrap_or_else(|_| {
        panic!(
            "Golden image not found: {}\nRun with GENERATE_GOLDEN=1 to create it.",
            path.display()
        )
    });
    assert_pixels_similar(pixels, &expected, tolerance);
}

macro_rules! snapshot_test {
    ($name:ident, $fractal_type:expr) => {
        #[test]
        fn $name() {
            let Some((device, queue)) = setup_gpu() else {
                eprintln!("No GPU adapter available, skipping snapshot test");
                return;
            };
            let params = FractalParams::for_type($fractal_type);
            let color = ColorConfig::default();
            let pixels = render_frame(&device, &queue, &params, &color);
            check_or_generate(stringify!($name), &pixels, 2);
        }
    };
    ($name:ident, $fractal_type:expr, color_mode = $mode:expr) => {
        #[test]
        fn $name() {
            let Some((device, queue)) = setup_gpu() else {
                eprintln!("No GPU adapter available, skipping snapshot test");
                return;
            };
            let params = FractalParams::for_type($fractal_type);
            let mut color = ColorConfig::default();
            color.color_mode = $mode;
            let pixels = render_frame(&device, &queue, &params, &color);
            check_or_generate(stringify!($name), &pixels, 2);
        }
    };
}

snapshot_test!(test_mandelbulb_default, FractalType::Mandelbulb);
snapshot_test!(test_menger_default, FractalType::Menger);
snapshot_test!(test_julia_default, FractalType::Julia3D);
snapshot_test!(test_mandelbox_default, FractalType::Mandelbox);
snapshot_test!(test_sierpinski_default, FractalType::Sierpinski);
snapshot_test!(test_apollonian_default, FractalType::Apollonian);
snapshot_test!(
    test_color_mode_normal,
    FractalType::Mandelbulb,
    color_mode = 3
);
snapshot_test!(
    test_color_mode_iteration,
    FractalType::Mandelbulb,
    color_mode = 2
);
