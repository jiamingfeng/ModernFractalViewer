//! Headless GPU rendering benchmark for ModernFractalViewer.
//!
//! Run with: `cargo run -p fractal-app --bin fractal-bench --features benchmark --release`

use clap::Parser;
use fractal_core::benchmark_types::{default_scenarios, filter_scenarios};
use fractal_renderer::benchmark::{
    format_csv, format_text, run_benchmark, setup_headless_gpu,
};

#[derive(Parser)]
#[command(name = "fractal-bench", about = "ModernFractalViewer GPU rendering benchmark")]
struct Args {
    /// Filter to one fractal type (e.g., mandelbulb, menger, julia3d)
    #[arg(long)]
    fractal: Option<String>,

    /// Filter to one resolution (e.g., 512x512, 1920x1080)
    #[arg(long)]
    resolution: Option<String>,

    /// Number of frames to render per scenario
    #[arg(long, default_value = "50")]
    frames: u32,

    /// Number of warm-up frames per scenario
    #[arg(long, default_value = "5")]
    warmup: u32,

    /// Output format: text, json, csv
    #[arg(long, default_value = "text")]
    output: String,

    /// Quick mode: 1 resolution (512x512), 10 frames
    #[arg(long)]
    quick: bool,

    /// Filter to one color mode (0=solid, 1=orbit-trap, 2=iteration, 3=normal, 4=combined)
    #[arg(long, name = "color-mode")]
    color_mode: Option<u32>,

    /// Filter to one lighting model (0=phong, 1=pbr)
    #[arg(long)]
    lighting: Option<u32>,
}

fn main() {
    let args = Args::parse();

    let gpu = match setup_headless_gpu() {
        Some(gpu) => gpu,
        None => {
            eprintln!("Error: No GPU adapter found. Cannot run benchmarks.");
            eprintln!("Ensure a GPU with Vulkan, DirectX 12, or Metal support is available.");
            std::process::exit(1);
        }
    };

    eprintln!(
        "GPU: {} ({})",
        gpu.adapter_name, gpu.backend
    );
    if gpu.supports_timestamp_query {
        eprintln!("GPU timestamp queries: supported");
    } else {
        eprintln!("GPU timestamp queries: not supported (using CPU timing)");
    }

    // Build and filter scenarios
    let (frames, warmup) = if args.quick {
        eprintln!("Quick mode: 512x512, 10 frames");
        (10, 3)
    } else {
        (args.frames, args.warmup)
    };

    let resolution = if args.quick {
        Some((512u32, 512u32))
    } else {
        args.resolution.as_ref().and_then(|r| {
            let parts: Vec<&str> = r.split('x').collect();
            if parts.len() == 2 {
                let w = parts[0].parse().ok()?;
                let h = parts[1].parse().ok()?;
                Some((w, h))
            } else {
                None
            }
        })
    };

    let scenarios = default_scenarios();
    let scenarios = filter_scenarios(
        scenarios,
        args.fractal.as_deref(),
        resolution,
        args.color_mode,
        args.lighting,
    );

    if scenarios.is_empty() {
        eprintln!("No scenarios match the given filters.");
        std::process::exit(1);
    }

    eprintln!(
        "Running {} scenarios ({} frames each, {} warmup)...\n",
        scenarios.len(),
        frames,
        warmup
    );

    let report = run_benchmark(&gpu, &scenarios, warmup, frames, |i, total, name| {
        if i < total {
            eprintln!("[{}/{}] {}", i + 1, total, name);
        }
    });

    // Output
    match args.output.as_str() {
        "json" => {
            let json = serde_json::to_string_pretty(&report).expect("Failed to serialize JSON");
            println!("{json}");
        }
        "csv" => {
            print!("{}", format_csv(&report));
        }
        _ => {
            print!("{}", format_text(&report));
        }
    }
}
