//! Data-driven UI control ranges.
//!
//! All slider/drag value min/max/speed/decimals are defined here and can be
//! loaded from a TOML config file at runtime.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Primitive range types
// ---------------------------------------------------------------------------

/// Range descriptor for a floating-point slider or drag value.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FloatRange {
    pub min: f32,
    pub max: f32,
    /// Drag speed (for `DragValue` controls). `None` → use `Slider` instead.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speed: Option<f32>,
    /// Fixed decimal places for display. `None` → auto.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decimals: Option<usize>,
    /// Whether to use logarithmic scale.
    #[serde(default)]
    pub logarithmic: bool,
}

impl Default for FloatRange {
    fn default() -> Self {
        Self {
            min: 0.0,
            max: 1.0,
            speed: None,
            decimals: None,
            logarithmic: false,
        }
    }
}

impl FloatRange {
    /// Construct a range with just min/max.
    pub const fn new(min: f32, max: f32) -> Self {
        Self { min, max, speed: None, decimals: None, logarithmic: false }
    }

    /// Create an egui `Slider` configured with these ranges.
    pub fn slider<'a>(&self, value: &'a mut f32) -> egui::Slider<'a> {
        let mut s = egui::Slider::new(value, self.min..=self.max);
        if self.logarithmic {
            s = s.logarithmic(true);
        }
        if let Some(d) = self.decimals {
            s = s.fixed_decimals(d);
        }
        s
    }

    /// Create an egui `DragValue` configured with these ranges.
    pub fn drag_value<'a>(&self, value: &'a mut f32) -> egui::DragValue<'a> {
        let mut d = egui::DragValue::new(value).range(self.min..=self.max);
        if let Some(speed) = self.speed {
            d = d.speed(speed);
        }
        if let Some(dec) = self.decimals {
            d = d.fixed_decimals(dec);
        }
        d
    }
}

/// Range descriptor for an integer slider or drag value.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct IntRange {
    pub min: i32,
    pub max: i32,
}

impl Default for IntRange {
    fn default() -> Self {
        Self { min: 0, max: 100 }
    }
}

impl IntRange {
    pub const fn new(min: i32, max: i32) -> Self {
        Self { min, max }
    }

    pub fn slider<'a>(&self, value: &'a mut i32) -> egui::Slider<'a> {
        egui::Slider::new(value, self.min..=self.max)
    }

    pub fn drag_value<'a>(&self, value: &'a mut i32) -> egui::DragValue<'a> {
        egui::DragValue::new(value).range(self.min..=self.max)
    }
}

// ---------------------------------------------------------------------------
// Per-fractal ranges
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MandelbulbRanges {
    pub power: FloatRange,
    pub iterations: IntRange,
    pub bailout: FloatRange,
}

impl Default for MandelbulbRanges {
    fn default() -> Self {
        Self {
            power: FloatRange::new(1.0, 16.0),
            iterations: IntRange::new(1, 32),
            bailout: FloatRange::new(1.0, 8.0),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MengerRanges {
    pub iterations: IntRange,
}

impl Default for MengerRanges {
    fn default() -> Self {
        Self {
            iterations: IntRange::new(1, 8),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Julia3DRanges {
    pub iterations: IntRange,
    pub julia_c: FloatRange,
}

impl Default for Julia3DRanges {
    fn default() -> Self {
        Self {
            iterations: IntRange::new(1, 32),
            julia_c: FloatRange {
                min: -2.0,
                max: 2.0,
                speed: Some(0.01),
                decimals: None,
                logarithmic: false,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MandelboxRanges {
    pub box_scale: FloatRange,
    pub iterations: IntRange,
    pub fold_limit: FloatRange,
    pub min_radius_sq: FloatRange,
}

impl Default for MandelboxRanges {
    fn default() -> Self {
        Self {
            box_scale: FloatRange::new(-3.0, 3.0),
            iterations: IntRange::new(1, 32),
            fold_limit: FloatRange::new(0.5, 2.0),
            min_radius_sq: FloatRange::new(0.01, 1.0),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SierpinskiRanges {
    pub iterations: IntRange,
    pub size_ratio: FloatRange,
}

impl Default for SierpinskiRanges {
    fn default() -> Self {
        Self {
            iterations: IntRange::new(1, 20),
            size_ratio: FloatRange::new(1.5, 3.0),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ApollonianRanges {
    pub iterations: IntRange,
}

impl Default for ApollonianRanges {
    fn default() -> Self {
        Self {
            iterations: IntRange::new(1, 12),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FractalRanges {
    pub mandelbulb: MandelbulbRanges,
    pub menger: MengerRanges,
    pub julia3d: Julia3DRanges,
    pub mandelbox: MandelboxRanges,
    pub sierpinski: SierpinskiRanges,
    pub apollonian: ApollonianRanges,
}

impl Default for FractalRanges {
    fn default() -> Self {
        Self {
            mandelbulb: MandelbulbRanges::default(),
            menger: MengerRanges::default(),
            julia3d: Julia3DRanges::default(),
            mandelbox: MandelboxRanges::default(),
            sierpinski: SierpinskiRanges::default(),
            apollonian: ApollonianRanges::default(),
        }
    }
}

// ---------------------------------------------------------------------------
// Camera, rendering, lighting, color, debug
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CameraRanges {
    pub fov: FloatRange,
    pub zoom: FloatRange,
    pub distance_clamp: FloatRange,
}

impl Default for CameraRanges {
    fn default() -> Self {
        Self {
            fov: FloatRange::new(30.0, 120.0),
            zoom: FloatRange {
                min: 0.05,
                max: 1000.0,
                logarithmic: true,
                ..Default::default()
            },
            distance_clamp: FloatRange::new(0.001, 20.0),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RenderingRanges {
    pub ray_steps: IntRange,
    pub epsilon: FloatRange,
    pub max_distance: FloatRange,
    pub ao_steps: IntRange,
    pub ao_intensity: FloatRange,
    pub normal_epsilon: FloatRange,
    pub sample_counts: Vec<u32>,
}

impl Default for RenderingRanges {
    fn default() -> Self {
        Self {
            ray_steps: IntRange::new(16, 512),
            epsilon: FloatRange {
                min: 0.00001,
                max: 0.01,
                speed: Some(0.0001),
                decimals: Some(5),
                logarithmic: false,
            },
            max_distance: FloatRange::new(10.0, 1000.0),
            ao_steps: IntRange::new(0, 16),
            ao_intensity: FloatRange::new(0.0, 1.0),
            normal_epsilon: FloatRange {
                min: 0.000001,
                max: 0.01,
                speed: Some(0.00001),
                decimals: Some(6),
                logarithmic: false,
            },
            sample_counts: vec![1, 2, 4],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LightingRanges {
    pub ambient: FloatRange,
    pub diffuse: FloatRange,
    pub specular: FloatRange,
    pub shininess: FloatRange,
}

impl Default for LightingRanges {
    fn default() -> Self {
        Self {
            ambient: FloatRange::new(0.0, 1.0),
            diffuse: FloatRange::new(0.0, 1.0),
            specular: FloatRange::new(0.0, 1.0),
            shininess: FloatRange::new(1.0, 128.0),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ColorRanges {
    pub palette_scale: FloatRange,
    pub palette_offset: FloatRange,
    pub dither_strength: FloatRange,
    pub max_palette_colors: u32,
}

impl Default for ColorRanges {
    fn default() -> Self {
        Self {
            palette_scale: FloatRange {
                min: 0.1,
                max: 10.0,
                logarithmic: true,
                ..Default::default()
            },
            palette_offset: FloatRange::new(0.0, 1.0),
            dither_strength: FloatRange::new(0.0, 2.0),
            max_palette_colors: 8,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DebugRanges {
    pub rotation_speed: FloatRange,
}

impl Default for DebugRanges {
    fn default() -> Self {
        Self {
            rotation_speed: FloatRange::new(0.1, 2.0),
        }
    }
}

// ---------------------------------------------------------------------------
// Top-level config
// ---------------------------------------------------------------------------

/// All UI control ranges, organized by panel category.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UiControlRanges {
    pub fractal: FractalRanges,
    pub camera: CameraRanges,
    pub rendering: RenderingRanges,
    pub lighting: LightingRanges,
    pub color: ColorRanges,
    pub debug: DebugRanges,
}

impl Default for UiControlRanges {
    fn default() -> Self {
        Self {
            fractal: FractalRanges::default(),
            camera: CameraRanges::default(),
            rendering: RenderingRanges::default(),
            lighting: LightingRanges::default(),
            color: ColorRanges::default(),
            debug: DebugRanges::default(),
        }
    }
}

impl UiControlRanges {
    /// The default config as a TOML string, embedded at compile time.
    pub const DEFAULT_TOML: &'static str = include_str!("default_control_ranges.toml");
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_roundtrips_through_json() {
        let original = UiControlRanges::default();
        let json = serde_json::to_string(&original).unwrap();
        let restored: UiControlRanges = serde_json::from_str(&json).unwrap();
        assert_eq!(original, restored);
    }

    #[test]
    fn empty_json_deserializes_to_defaults() {
        let restored: UiControlRanges = serde_json::from_str("{}").unwrap();
        assert_eq!(restored, UiControlRanges::default());
    }

    #[test]
    fn partial_json_fills_missing_with_defaults() {
        let json = r#"{"camera": {"fov": {"min": 10.0, "max": 180.0}}}"#;
        let restored: UiControlRanges = serde_json::from_str(json).unwrap();
        assert_eq!(restored.camera.fov.min, 10.0);
        assert_eq!(restored.camera.fov.max, 180.0);
        // Everything else should be default
        assert_eq!(restored.fractal, FractalRanges::default());
        assert_eq!(restored.rendering, RenderingRanges::default());
    }

    #[test]
    fn float_range_slider_does_not_panic() {
        let range = FloatRange::new(0.0, 10.0);
        let mut val = 5.0_f32;
        let _ = range.slider(&mut val);
    }

    #[test]
    fn float_range_drag_value_does_not_panic() {
        let range = FloatRange {
            min: 0.0,
            max: 1.0,
            speed: Some(0.01),
            decimals: Some(3),
            logarithmic: false,
        };
        let mut val = 0.5_f32;
        let _ = range.drag_value(&mut val);
    }

    #[test]
    fn int_range_slider_does_not_panic() {
        let range = IntRange::new(1, 32);
        let mut val = 16_i32;
        let _ = range.slider(&mut val);
    }
}
