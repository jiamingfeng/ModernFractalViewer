//! Core benchmark engine
//!
//! Provides `BenchmarkRunner` — a reusable GPU benchmark harness shared by
//! the headless CLI tool, Criterion benchmarks, and the in-app benchmark panel.

use std::time::Instant;

use fractal_core::benchmark_types::{
    BenchmarkReport, BenchmarkScenario, TimingMethod, compute_stats,
};
use wgpu;

use crate::pipeline::FractalPipeline;

/// Headless GPU context for benchmarking (no window/surface).
pub struct BenchmarkGpu {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub adapter_name: String,
    pub backend: String,
    pub supports_timestamp_query: bool,
}

/// Create a headless GPU context for benchmarking.
///
/// Uses `HighPerformance` power preference (not `LowPower` like snapshot tests)
/// to ensure the discrete GPU is selected on hybrid-GPU laptops.
pub fn setup_headless_gpu() -> Option<BenchmarkGpu> {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::all(),
        ..Default::default()
    });

    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: None,
        force_fallback_adapter: false,
    }))?;

    let info = adapter.get_info();
    let adapter_name = info.name.clone();
    let backend = format!("{:?}", info.backend);
    let supports_timestamp_query = adapter.features().contains(wgpu::Features::TIMESTAMP_QUERY);

    let mut required_features = wgpu::Features::empty();
    if supports_timestamp_query {
        required_features |= wgpu::Features::TIMESTAMP_QUERY;
    }

    let (device, queue) = pollster::block_on(adapter.request_device(
        &wgpu::DeviceDescriptor {
            label: Some("Benchmark Device"),
            required_features,
            required_limits: adapter.limits(),
            memory_hints: wgpu::MemoryHints::Performance,
        },
        None,
    ))
    .ok()?;

    Some(BenchmarkGpu {
        device,
        queue,
        adapter_name,
        backend,
        supports_timestamp_query,
    })
}

/// Lightweight render target — only RENDER_ATTACHMENT, no staging buffer.
struct BenchmarkTarget {
    view: wgpu::TextureView,
}

impl BenchmarkTarget {
    fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Benchmark Target"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        BenchmarkTarget { view }
    }
}

/// Run a single benchmark scenario and return frame times in milliseconds.
///
/// Uses CPU-side timing with `device.poll(Wait)` for GPU synchronization.
pub fn run_scenario_cpu_timing(
    gpu: &BenchmarkGpu,
    scenario: &BenchmarkScenario,
    warmup_frames: u32,
    measure_frames: u32,
) -> Vec<f64> {
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let mut pipeline = FractalPipeline::new_headless(&gpu.device, format);
    let target = BenchmarkTarget::new(&gpu.device, scenario.width, scenario.height);

    let aspect = scenario.width as f32 / scenario.height as f32;

    // Configure uniforms for this scenario
    pipeline.uniforms.update_resolution(scenario.width, scenario.height);
    pipeline.uniforms.update_camera(&scenario.camera, aspect);
    pipeline.uniforms.update_fractal(&scenario.fractal_params);
    pipeline.uniforms.update_ray_march(&scenario.ray_march_config);
    pipeline.uniforms.update_lighting(&scenario.lighting_config);
    pipeline.uniforms.update_color(&scenario.color_config);
    pipeline.update_uniforms(&gpu.queue);

    // Warm-up phase
    for _ in 0..warmup_frames {
        let mut encoder = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Benchmark Warmup"),
            });
        pipeline.render(&mut encoder, &target.view);
        gpu.queue.submit(std::iter::once(encoder.finish()));
        gpu.device.poll(wgpu::Maintain::Wait);
    }

    // Measurement phase
    let mut times_ms = Vec::with_capacity(measure_frames as usize);
    for _ in 0..measure_frames {
        let mut encoder = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Benchmark Measure"),
            });
        pipeline.render(&mut encoder, &target.view);

        let start = Instant::now();
        gpu.queue.submit(std::iter::once(encoder.finish()));
        gpu.device.poll(wgpu::Maintain::Wait);
        let elapsed = start.elapsed();

        times_ms.push(elapsed.as_secs_f64() * 1000.0);
    }

    times_ms
}

/// Run the full benchmark suite and return a report.
pub fn run_benchmark(
    gpu: &BenchmarkGpu,
    scenarios: &[BenchmarkScenario],
    warmup_frames: u32,
    measure_frames: u32,
    mut on_progress: impl FnMut(usize, usize, &str),
) -> BenchmarkReport {
    let timing_method = TimingMethod::CpuPollWait;
    let total = scenarios.len();
    let mut results = Vec::with_capacity(total);

    for (i, scenario) in scenarios.iter().enumerate() {
        on_progress(i, total, &scenario.name);

        let mut times_ms =
            run_scenario_cpu_timing(gpu, scenario, warmup_frames, measure_frames);
        let result = compute_stats(&scenario.name, timing_method, &mut times_ms);
        results.push(result);
    }

    on_progress(total, total, "Done");

    BenchmarkReport {
        timestamp: now_iso8601(),
        gpu_adapter_name: gpu.adapter_name.clone(),
        gpu_backend: gpu.backend.clone(),
        timing_method,
        results,
    }
}

/// Render a single frame for a given scenario (used by Criterion benchmarks).
pub fn render_one_frame(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    pipeline: &mut FractalPipeline,
    target_view: &wgpu::TextureView,
) {
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("Bench Frame"),
    });
    pipeline.render(&mut encoder, target_view);
    queue.submit(std::iter::once(encoder.finish()));
    device.poll(wgpu::Maintain::Wait);
}

/// Configure a pipeline for a given scenario (used by Criterion benchmarks).
pub fn configure_pipeline(
    pipeline: &mut FractalPipeline,
    queue: &wgpu::Queue,
    scenario: &BenchmarkScenario,
) {
    let aspect = scenario.width as f32 / scenario.height as f32;
    pipeline.uniforms.update_resolution(scenario.width, scenario.height);
    pipeline.uniforms.update_camera(&scenario.camera, aspect);
    pipeline.uniforms.update_fractal(&scenario.fractal_params);
    pipeline.uniforms.update_ray_march(&scenario.ray_march_config);
    pipeline.uniforms.update_lighting(&scenario.lighting_config);
    pipeline.uniforms.update_color(&scenario.color_config);
    pipeline.update_uniforms(queue);
}

/// Create a benchmark render target texture view.
pub fn create_benchmark_target(device: &wgpu::Device, width: u32, height: u32) -> wgpu::TextureView {
    BenchmarkTarget::new(device, width, height).view
}

/// Format report as text.
pub fn format_text(report: &BenchmarkReport) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "=== ModernFractalViewer Rendering Benchmark ===\n\
         GPU: {} ({})\n\
         Timing: {}\n\
         Date: {}\n\n",
        report.gpu_adapter_name, report.gpu_backend, report.timing_method, report.timestamp
    ));

    for r in &report.results {
        out.push_str(&format!(
            "{}\n  Frames: {} | Min: {:.2}ms | Avg: {:.2}ms | Max: {:.2}ms | \
             Median: {:.2}ms | P95: {:.2}ms | P99: {:.2}ms | FPS: {:.1}\n\n",
            r.scenario,
            r.frame_count,
            r.min_ms,
            r.avg_ms,
            r.max_ms,
            r.median_ms,
            r.p95_ms,
            r.p99_ms,
            r.avg_fps,
        ));
    }
    out
}

/// Format report as CSV.
pub fn format_csv(report: &BenchmarkReport) -> String {
    let mut out = String::new();
    out.push_str("scenario,frame_count,min_ms,avg_ms,max_ms,median_ms,p95_ms,p99_ms,avg_fps\n");
    for r in &report.results {
        out.push_str(&format!(
            "\"{}\",{},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.1}\n",
            r.scenario,
            r.frame_count,
            r.min_ms,
            r.avg_ms,
            r.max_ms,
            r.median_ms,
            r.p95_ms,
            r.p99_ms,
            r.avg_fps,
        ));
    }
    out
}

fn now_iso8601() -> String {
    // Simple ISO 8601 timestamp using std::time
    let now = std::time::SystemTime::now();
    let secs = now
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // Approximate — good enough for a report timestamp
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    // Approximate year/month/day from Unix epoch
    let (year, month, day) = days_to_ymd(days);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year, month, day, hours, minutes, seconds
    )
}

fn days_to_ymd(total_days: u64) -> (u64, u64, u64) {
    // Compute year/month/day from days since Unix epoch (1970-01-01)
    let mut days = total_days;
    let mut year = 1970u64;
    loop {
        let days_in_year = if is_leap(year) { 366 } else { 365 };
        if days < days_in_year {
            break;
        }
        days -= days_in_year;
        year += 1;
    }
    let month_days = if is_leap(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut month = 1u64;
    for &md in &month_days {
        if days < md {
            break;
        }
        days -= md;
        month += 1;
    }
    (year, month, days + 1)
}

fn is_leap(year: u64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}
