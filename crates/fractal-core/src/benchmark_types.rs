//! Benchmark data types
//!
//! These types are shared by the CLI benchmark, Criterion benchmarks, and the
//! in-app benchmark panel. They live in `fractal-core` (not `fractal-renderer`)
//! so that `fractal-ui` can import them without depending on the renderer.

use serde::{Deserialize, Serialize};

use crate::camera::Camera;
use crate::fractals::{FractalParams, FractalType};
use crate::sdf::{ColorConfig, LightingConfig, RayMarchConfig};

/// A single benchmark scenario to run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkScenario {
    pub name: String,
    pub fractal_params: FractalParams,
    pub camera: Camera,
    pub width: u32,
    pub height: u32,
    pub ray_march_config: RayMarchConfig,
    pub color_config: ColorConfig,
    pub lighting_config: LightingConfig,
}

/// Which timing method was used for measurement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TimingMethod {
    /// GPU-side timestamp queries (most accurate).
    GpuTimestamp,
    /// CPU-side Instant timing with `device.poll(Wait)` synchronization.
    CpuPollWait,
}

impl std::fmt::Display for TimingMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TimingMethod::GpuTimestamp => write!(f, "GPU Timestamp"),
            TimingMethod::CpuPollWait => write!(f, "CPU Poll+Wait"),
        }
    }
}

/// Timing results for a single scenario.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkResult {
    pub scenario: String,
    pub timing_method: TimingMethod,
    pub frame_count: u32,
    pub total_time_ms: f64,
    pub min_ms: f64,
    pub max_ms: f64,
    pub avg_ms: f64,
    pub median_ms: f64,
    pub p95_ms: f64,
    pub p99_ms: f64,
    pub avg_fps: f64,
}

/// Full benchmark report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkReport {
    pub timestamp: String,
    pub gpu_adapter_name: String,
    pub gpu_backend: String,
    pub timing_method: TimingMethod,
    pub results: Vec<BenchmarkResult>,
}

/// Compute statistics from a sorted slice of frame times (in milliseconds).
pub fn compute_stats(scenario: &str, timing_method: TimingMethod, times_ms: &mut Vec<f64>) -> BenchmarkResult {
    times_ms.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = times_ms.len();
    let total: f64 = times_ms.iter().sum();
    let avg = total / n as f64;
    let median = if n % 2 == 0 {
        (times_ms[n / 2 - 1] + times_ms[n / 2]) / 2.0
    } else {
        times_ms[n / 2]
    };
    let p95_idx = ((n as f64) * 0.95).ceil() as usize - 1;
    let p99_idx = ((n as f64) * 0.99).ceil() as usize - 1;

    BenchmarkResult {
        scenario: scenario.to_string(),
        timing_method,
        frame_count: n as u32,
        total_time_ms: total,
        min_ms: times_ms[0],
        max_ms: times_ms[n - 1],
        avg_ms: avg,
        median_ms: median,
        p95_ms: times_ms[p95_idx.min(n - 1)],
        p99_ms: times_ms[p99_idx.min(n - 1)],
        avg_fps: 1000.0 / avg,
    }
}

/// Build the default benchmark matrix.
pub fn default_scenarios() -> Vec<BenchmarkScenario> {
    let resolutions: &[(u32, u32)] = &[(256, 256), (512, 512), (1024, 1024), (1920, 1080)];
    // Color modes: 1=orbit trap, 2=iteration, 3=normal, 4=combined
    let color_modes: &[u32] = &[1, 2, 3, 4];
    // Lighting models: 0=Blinn-Phong, 1=PBR
    let lighting_models: &[u32] = &[0, 1];
    let camera = Camera::default();

    let mut scenarios = Vec::new();
    for &ft in FractalType::all() {
        let params = FractalParams::for_type(ft);
        for &(w, h) in resolutions {
            for &cm in color_modes {
                for &lm in lighting_models {
                    let mut color_config = ColorConfig::default();
                    color_config.color_mode = cm;
                    let mut lighting_config = LightingConfig::default();
                    lighting_config.lighting_model = lm;

                    let cm_name = match cm {
                        1 => "orbit-trap",
                        2 => "iteration",
                        3 => "normal",
                        4 => "combined",
                        _ => "solid",
                    };
                    let lm_name = if lm == 0 { "Blinn-Phong" } else { "PBR" };

                    scenarios.push(BenchmarkScenario {
                        name: format!("{} @ {}x{} / {} / {}", ft.name(), w, h, cm_name, lm_name),
                        fractal_params: params,
                        camera: camera.clone(),
                        width: w,
                        height: h,
                        ray_march_config: RayMarchConfig::default(),
                        color_config,
                        lighting_config,
                    });
                }
            }
        }
    }
    scenarios
}

/// Filter scenarios by optional criteria.
///
/// When `resolution` is provided, scenarios are deduplicated to one per
/// fractal/color/lighting combination and overridden to the requested size.
/// Any `WxH` value is accepted.
pub fn filter_scenarios(
    scenarios: Vec<BenchmarkScenario>,
    fractal: Option<&str>,
    resolution: Option<(u32, u32)>,
    color_mode: Option<u32>,
    lighting: Option<u32>,
) -> Vec<BenchmarkScenario> {
    // When a resolution is specified, keep only the first default resolution
    // per combination so we can override it below.
    let dedup_res = resolution.is_some();
    let first_res = scenarios.first().map(|s| (s.width, s.height));

    let mut result: Vec<BenchmarkScenario> = scenarios
        .into_iter()
        .filter(|s| {
            if let Some(f) = fractal {
                let f_lower = f.to_lowercase();
                if !s.fractal_params.fractal_type.name().to_lowercase().contains(&f_lower) {
                    return false;
                }
            }
            if let Some(cm) = color_mode {
                if s.color_config.color_mode != cm {
                    return false;
                }
            }
            if let Some(lm) = lighting {
                if s.lighting_config.lighting_model != lm {
                    return false;
                }
            }
            if dedup_res {
                if let Some((fw, fh)) = first_res {
                    if s.width != fw || s.height != fh {
                        return false;
                    }
                }
            }
            true
        })
        .collect();

    if let Some((w, h)) = resolution {
        for s in &mut result {
            let old_res = format!("{}x{}", s.width, s.height);
            let new_res = format!("{}x{}", w, h);
            s.name = s.name.replace(&old_res, &new_res);
            s.width = w;
            s.height = h;
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_scenarios_count() {
        let scenarios = default_scenarios();
        // 6 fractals × 4 resolutions × 4 color modes × 2 lighting = 192
        assert_eq!(scenarios.len(), 192);
    }

    #[test]
    fn test_filter_by_fractal() {
        let scenarios = default_scenarios();
        let filtered = filter_scenarios(scenarios, Some("mandelbulb"), None, None, None);
        // 1 fractal × 4 res × 4 color × 2 lighting = 32
        assert_eq!(filtered.len(), 32);
    }

    #[test]
    fn test_filter_by_default_resolution() {
        let scenarios = default_scenarios();
        let filtered = filter_scenarios(scenarios, None, Some((512, 512)), None, None);
        // 6 fractals × 1 res × 4 color × 2 lighting = 48
        assert_eq!(filtered.len(), 48);
        for s in &filtered {
            assert_eq!(s.width, 512);
            assert_eq!(s.height, 512);
        }
    }

    #[test]
    fn test_filter_by_custom_resolution() {
        let scenarios = default_scenarios();
        let filtered = filter_scenarios(scenarios, None, Some((3840, 2160)), None, None);
        // 6 fractals × 1 res × 4 color × 2 lighting = 48
        assert_eq!(filtered.len(), 48);
        for s in &filtered {
            assert_eq!(s.width, 3840);
            assert_eq!(s.height, 2160);
            assert!(s.name.contains("3840x2160"));
        }
    }

    #[test]
    fn test_compute_stats_basic() {
        let mut times = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0];
        let result = compute_stats("test", TimingMethod::CpuPollWait, &mut times);
        assert_eq!(result.frame_count, 10);
        assert!((result.avg_ms - 5.5).abs() < 0.001);
        assert!((result.min_ms - 1.0).abs() < 0.001);
        assert!((result.max_ms - 10.0).abs() < 0.001);
        assert!((result.median_ms - 5.5).abs() < 0.001);
    }

    #[test]
    fn test_timing_method_display() {
        assert_eq!(format!("{}", TimingMethod::GpuTimestamp), "GPU Timestamp");
        assert_eq!(format!("{}", TimingMethod::CpuPollWait), "CPU Poll+Wait");
    }

    #[test]
    fn test_serde_roundtrip_result() {
        let mut times = vec![1.0, 2.0, 3.0];
        let result = compute_stats("test", TimingMethod::GpuTimestamp, &mut times);
        let json = serde_json::to_string(&result).unwrap();
        let result2: BenchmarkResult = serde_json::from_str(&json).unwrap();
        assert_eq!(result.scenario, result2.scenario);
        assert_eq!(result.timing_method, result2.timing_method);
        assert_eq!(result.frame_count, result2.frame_count);
    }
}
