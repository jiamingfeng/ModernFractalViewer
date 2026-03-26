//! Criterion benchmarks for the fractal rendering pipeline.
//!
//! Run: `cargo bench -p fractal-renderer`

use criterion::{criterion_group, criterion_main, Criterion, BenchmarkId};
use std::time::Duration;

use fractal_core::benchmark_types::BenchmarkScenario;
use fractal_core::camera::Camera;
use fractal_core::fractals::{FractalParams, FractalType};
use fractal_core::sdf::{ColorConfig, LightingConfig, RayMarchConfig};
use fractal_renderer::benchmark::{
    configure_pipeline, create_benchmark_target, render_one_frame, setup_headless_gpu,
    BenchmarkGpu,
};
use fractal_renderer::FractalPipeline;

fn make_scenario(ft: FractalType, width: u32, height: u32) -> BenchmarkScenario {
    BenchmarkScenario {
        name: format!("{} @ {}x{}", ft.name(), width, height),
        fractal_params: FractalParams::for_type(ft),
        camera: Camera::default(),
        width,
        height,
        ray_march_config: RayMarchConfig::default(),
        color_config: ColorConfig::default(),
        lighting_config: LightingConfig::default(),
    }
}

fn setup_gpu_or_skip() -> Option<BenchmarkGpu> {
    setup_headless_gpu()
}

fn per_fractal_type(c: &mut Criterion) {
    let gpu = match setup_gpu_or_skip() {
        Some(g) => g,
        None => {
            eprintln!("No GPU found — skipping per_fractal_type benchmarks");
            return;
        }
    };
    let format = wgpu::TextureFormat::Rgba8Unorm;

    let mut group = c.benchmark_group("per_fractal_type");
    group.sample_size(20);
    group.warm_up_time(Duration::from_secs(2));

    for &ft in FractalType::all() {
        let scenario = make_scenario(ft, 512, 512);
        let target_view = create_benchmark_target(&gpu.device, 512, 512);
        let mut pipeline = FractalPipeline::new_headless(&gpu.device, format);
        configure_pipeline(&mut pipeline, &gpu.queue, &scenario);

        group.bench_with_input(BenchmarkId::from_parameter(ft.name()), &(), |b, _| {
            b.iter_custom(|iters| {
                let start = std::time::Instant::now();
                for _ in 0..iters {
                    render_one_frame(&gpu.device, &gpu.queue, &mut pipeline, &target_view);
                }
                start.elapsed()
            });
        });
    }
    group.finish();
}

fn per_resolution(c: &mut Criterion) {
    let gpu = match setup_gpu_or_skip() {
        Some(g) => g,
        None => {
            eprintln!("No GPU found — skipping per_resolution benchmarks");
            return;
        }
    };
    let format = wgpu::TextureFormat::Rgba8Unorm;

    let mut group = c.benchmark_group("per_resolution");
    group.sample_size(10);
    group.warm_up_time(Duration::from_secs(2));

    let resolutions = [(256, 256), (512, 512), (1024, 1024), (1920, 1080)];

    for (w, h) in resolutions {
        let scenario = make_scenario(FractalType::Mandelbulb, w, h);
        let target_view = create_benchmark_target(&gpu.device, w, h);
        let mut pipeline = FractalPipeline::new_headless(&gpu.device, format);
        configure_pipeline(&mut pipeline, &gpu.queue, &scenario);

        let label = format!("{}x{}", w, h);
        group.bench_with_input(BenchmarkId::from_parameter(&label), &(), |b, _| {
            b.iter_custom(|iters| {
                let start = std::time::Instant::now();
                for _ in 0..iters {
                    render_one_frame(&gpu.device, &gpu.queue, &mut pipeline, &target_view);
                }
                start.elapsed()
            });
        });
    }
    group.finish();
}

fn pipeline_creation(c: &mut Criterion) {
    let gpu = match setup_gpu_or_skip() {
        Some(g) => g,
        None => {
            eprintln!("No GPU found — skipping pipeline_creation benchmark");
            return;
        }
    };
    let format = wgpu::TextureFormat::Rgba8Unorm;

    c.bench_function("pipeline_creation", |b| {
        b.iter(|| {
            let _pipeline = FractalPipeline::new_headless(&gpu.device, format);
        });
    });
}

fn color_mode_comparison(c: &mut Criterion) {
    let gpu = match setup_gpu_or_skip() {
        Some(g) => g,
        None => {
            eprintln!("No GPU found — skipping color_mode_comparison benchmarks");
            return;
        }
    };
    let format = wgpu::TextureFormat::Rgba8Unorm;

    let mut group = c.benchmark_group("color_mode_comparison");
    group.sample_size(20);
    group.warm_up_time(Duration::from_secs(2));

    let modes = [
        (1, "orbit-trap"),
        (2, "iteration"),
        (3, "normal"),
        (4, "combined"),
    ];

    for (mode, name) in modes {
        let mut scenario = make_scenario(FractalType::Mandelbulb, 512, 512);
        scenario.color_config.color_mode = mode;
        let target_view = create_benchmark_target(&gpu.device, 512, 512);
        let mut pipeline = FractalPipeline::new_headless(&gpu.device, format);
        configure_pipeline(&mut pipeline, &gpu.queue, &scenario);

        group.bench_with_input(BenchmarkId::from_parameter(name), &(), |b, _| {
            b.iter_custom(|iters| {
                let start = std::time::Instant::now();
                for _ in 0..iters {
                    render_one_frame(&gpu.device, &gpu.queue, &mut pipeline, &target_view);
                }
                start.elapsed()
            });
        });
    }
    group.finish();
}

fn lighting_model_comparison(c: &mut Criterion) {
    let gpu = match setup_gpu_or_skip() {
        Some(g) => g,
        None => {
            eprintln!("No GPU found — skipping lighting_model_comparison benchmarks");
            return;
        }
    };
    let format = wgpu::TextureFormat::Rgba8Unorm;

    let mut group = c.benchmark_group("lighting_model_comparison");
    group.sample_size(20);
    group.warm_up_time(Duration::from_secs(2));

    let models = [(0, "Blinn-Phong"), (1, "PBR")];

    for (model, name) in models {
        let mut scenario = make_scenario(FractalType::Mandelbulb, 512, 512);
        scenario.lighting_config.lighting_model = model;
        let target_view = create_benchmark_target(&gpu.device, 512, 512);
        let mut pipeline = FractalPipeline::new_headless(&gpu.device, format);
        configure_pipeline(&mut pipeline, &gpu.queue, &scenario);

        group.bench_with_input(BenchmarkId::from_parameter(name), &(), |b, _| {
            b.iter_custom(|iters| {
                let start = std::time::Instant::now();
                for _ in 0..iters {
                    render_one_frame(&gpu.device, &gpu.queue, &mut pipeline, &target_view);
                }
                start.elapsed()
            });
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    per_fractal_type,
    per_resolution,
    pipeline_creation,
    color_mode_comparison,
    lighting_model_comparison,
);
criterion_main!(benches);
