//! Control ranges editor panel (advanced / debug feature).

use crate::UiState;
use crate::control_ranges::{FloatRange, IntRange, UiControlRanges};
use egui::Ui;
use fractal_core::FractalType;

pub struct ControlRangesPanel;

impl ControlRangesPanel {
    pub fn show(ui: &mut Ui, state: &mut UiState) {
        egui::CollapsingHeader::new("Control Ranges").default_open(false).show(ui, |ui| {
            ui.small("Customize slider min/max ranges.");
            ui.add_space(4.0);

            // Show ranges for current fractal type
            let fractal_type = state.fractal_params.fractal_type;
            ui.label(format!("{} ranges:", fractal_type.name()));

            match fractal_type {
                FractalType::Mandelbulb => {
                    let r = &mut state.control_ranges.fractal.mandelbulb;
                    show_float_range(ui, "Power", &mut r.power, &mut state.control_ranges_dirty);
                    show_int_range(ui, "Iterations", &mut r.iterations, &mut state.control_ranges_dirty);
                    show_float_range(ui, "Bailout", &mut r.bailout, &mut state.control_ranges_dirty);
                }
                FractalType::Menger => {
                    show_int_range(ui, "Iterations", &mut state.control_ranges.fractal.menger.iterations, &mut state.control_ranges_dirty);
                }
                FractalType::Julia3D => {
                    let r = &mut state.control_ranges.fractal.julia3d;
                    show_int_range(ui, "Iterations", &mut r.iterations, &mut state.control_ranges_dirty);
                    show_float_range(ui, "Julia C", &mut r.julia_c, &mut state.control_ranges_dirty);
                }
                FractalType::Mandelbox => {
                    let r = &mut state.control_ranges.fractal.mandelbox;
                    show_float_range(ui, "Box Scale", &mut r.box_scale, &mut state.control_ranges_dirty);
                    show_int_range(ui, "Iterations", &mut r.iterations, &mut state.control_ranges_dirty);
                    show_float_range(ui, "Fold Limit", &mut r.fold_limit, &mut state.control_ranges_dirty);
                    show_float_range(ui, "Min Radius", &mut r.min_radius_sq, &mut state.control_ranges_dirty);
                }
                FractalType::Sierpinski => {
                    let r = &mut state.control_ranges.fractal.sierpinski;
                    show_int_range(ui, "Iterations", &mut r.iterations, &mut state.control_ranges_dirty);
                    show_float_range(ui, "Size Ratio", &mut r.size_ratio, &mut state.control_ranges_dirty);
                }
                FractalType::Apollonian => {
                    show_int_range(ui, "Iterations", &mut state.control_ranges.fractal.apollonian.iterations, &mut state.control_ranges_dirty);
                }
            }

            ui.separator();
            ui.label("Rendering:");
            {
                let r = &mut state.control_ranges.rendering;
                show_int_range(ui, "Ray Steps", &mut r.ray_steps, &mut state.control_ranges_dirty);
                show_float_range(ui, "Epsilon", &mut r.epsilon, &mut state.control_ranges_dirty);
                show_float_range(ui, "Max Dist", &mut r.max_distance, &mut state.control_ranges_dirty);
                show_int_range(ui, "AO Steps", &mut r.ao_steps, &mut state.control_ranges_dirty);
                show_float_range(ui, "AO Intensity", &mut r.ao_intensity, &mut state.control_ranges_dirty);
            }

            ui.separator();
            ui.label("Camera:");
            {
                let r = &mut state.control_ranges.camera;
                show_float_range(ui, "FOV", &mut r.fov, &mut state.control_ranges_dirty);
                show_float_range(ui, "Zoom", &mut r.zoom, &mut state.control_ranges_dirty);
            }

            ui.separator();
            ui.label("Lighting:");
            {
                let r = &mut state.control_ranges.lighting;
                show_float_range(ui, "Ambient", &mut r.ambient, &mut state.control_ranges_dirty);
                show_float_range(ui, "Diffuse", &mut r.diffuse, &mut state.control_ranges_dirty);
                show_float_range(ui, "Specular", &mut r.specular, &mut state.control_ranges_dirty);
                show_float_range(ui, "Shininess", &mut r.shininess, &mut state.control_ranges_dirty);
            }

            ui.separator();
            ui.label("Color:");
            {
                let r = &mut state.control_ranges.color;
                show_float_range(ui, "Palette Scale", &mut r.palette_scale, &mut state.control_ranges_dirty);
                show_float_range(ui, "Palette Offset", &mut r.palette_offset, &mut state.control_ranges_dirty);
                show_float_range(ui, "Dither", &mut r.dither_strength, &mut state.control_ranges_dirty);
            }

            ui.add_space(8.0);
            if ui.button("Reset All to Defaults").clicked() {
                state.control_ranges = UiControlRanges::default();
                state.control_ranges_dirty = true;
            }
        });
    }
}

fn show_float_range(ui: &mut Ui, label: &str, range: &mut FloatRange, dirty: &mut bool) {
    ui.horizontal(|ui| {
        ui.label(format!("  {label}:"));
        ui.label("min");
        if ui.add(egui::DragValue::new(&mut range.min).speed(0.01)).changed() {
            *dirty = true;
        }
        ui.label("max");
        if ui.add(egui::DragValue::new(&mut range.max).speed(0.01)).changed() {
            *dirty = true;
        }
    });
}

fn show_int_range(ui: &mut Ui, label: &str, range: &mut IntRange, dirty: &mut bool) {
    ui.horizontal(|ui| {
        ui.label(format!("  {label}:"));
        ui.label("min");
        if ui.add(egui::DragValue::new(&mut range.min)).changed() {
            *dirty = true;
        }
        ui.label("max");
        if ui.add(egui::DragValue::new(&mut range.max)).changed() {
            *dirty = true;
        }
    });
}
