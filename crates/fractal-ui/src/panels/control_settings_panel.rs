//! Control settings editor panel (advanced / debug feature).

use crate::UiState;
use crate::app_settings::{AppSettings, FloatRange, IntRange};
use egui::Ui;
use fractal_core::FractalType;

pub struct ControlSettingsPanel;

impl ControlSettingsPanel {
    pub fn show(ui: &mut Ui, state: &mut UiState) {
        egui::CollapsingHeader::new("Control Settings").default_open(false).show(ui, |ui| {
            ui.small("Customize slider min/max ranges.");

            ui.horizontal(|ui| {
                #[cfg(not(target_arch = "wasm32"))]
                if ui.small_button("Open Config File").clicked() {
                    state.open_config_requested = true;
                }
                if ui.small_button("Reset All to Defaults").clicked() {
                    state.settings = AppSettings::default();
                    state.settings_dirty = true;
                }
            });

            ui.add_space(4.0);
            ui.separator();
            ui.add_space(4.0);

            // Show ranges for current fractal type
            let fractal_type = state.fractal_params.fractal_type;
            ui.label(format!("{} settings:", fractal_type.name()));

            match fractal_type {
                FractalType::Mandelbulb => {
                    let r = &mut state.settings.fractal.mandelbulb;
                    show_float_range(ui, "Power", &mut r.power, &mut state.settings_dirty);
                    show_int_range(ui, "Iterations", &mut r.iterations, &mut state.settings_dirty);
                    show_float_range(ui, "Bailout", &mut r.bailout, &mut state.settings_dirty);
                }
                FractalType::Menger => {
                    show_int_range(ui, "Iterations", &mut state.settings.fractal.menger.iterations, &mut state.settings_dirty);
                }
                FractalType::Julia3D => {
                    let r = &mut state.settings.fractal.julia3d;
                    show_int_range(ui, "Iterations", &mut r.iterations, &mut state.settings_dirty);
                    show_float_range(ui, "Julia C", &mut r.julia_c, &mut state.settings_dirty);
                }
                FractalType::Mandelbox => {
                    let r = &mut state.settings.fractal.mandelbox;
                    show_float_range(ui, "Box Scale", &mut r.box_scale, &mut state.settings_dirty);
                    show_int_range(ui, "Iterations", &mut r.iterations, &mut state.settings_dirty);
                    show_float_range(ui, "Fold Limit", &mut r.fold_limit, &mut state.settings_dirty);
                    show_float_range(ui, "Min Radius", &mut r.min_radius_sq, &mut state.settings_dirty);
                }
                FractalType::Sierpinski => {
                    let r = &mut state.settings.fractal.sierpinski;
                    show_int_range(ui, "Iterations", &mut r.iterations, &mut state.settings_dirty);
                    show_float_range(ui, "Size Ratio", &mut r.size_ratio, &mut state.settings_dirty);
                }
                FractalType::Apollonian => {
                    show_int_range(ui, "Iterations", &mut state.settings.fractal.apollonian.iterations, &mut state.settings_dirty);
                }
            }

            ui.separator();
            ui.label("Rendering:");
            {
                let r = &mut state.settings.rendering;
                show_int_range(ui, "Ray Steps", &mut r.ray_steps, &mut state.settings_dirty);
                show_float_range(ui, "Epsilon", &mut r.epsilon, &mut state.settings_dirty);
                show_float_range(ui, "Max Dist", &mut r.max_distance, &mut state.settings_dirty);
                show_int_range(ui, "AO Steps", &mut r.ao_steps, &mut state.settings_dirty);
                show_float_range(ui, "AO Intensity", &mut r.ao_intensity, &mut state.settings_dirty);
            }

            ui.separator();
            ui.label("Camera:");
            {
                let r = &mut state.settings.camera;
                show_float_range(ui, "FOV", &mut r.fov, &mut state.settings_dirty);
                show_float_range(ui, "Zoom", &mut r.zoom, &mut state.settings_dirty);
            }

            ui.separator();
            ui.label("Lighting:");
            {
                let r = &mut state.settings.lighting;
                show_float_range(ui, "Ambient", &mut r.ambient, &mut state.settings_dirty);
                show_float_range(ui, "Diffuse", &mut r.diffuse, &mut state.settings_dirty);
                show_float_range(ui, "Specular", &mut r.specular, &mut state.settings_dirty);
                show_float_range(ui, "Shininess", &mut r.shininess, &mut state.settings_dirty);
            }

            ui.separator();
            ui.label("Color:");
            {
                let r = &mut state.settings.color;
                show_float_range(ui, "Palette Scale", &mut r.palette_scale, &mut state.settings_dirty);
                show_float_range(ui, "Palette Offset", &mut r.palette_offset, &mut state.settings_dirty);
                show_float_range(ui, "Dither", &mut r.dither_strength, &mut state.settings_dirty);
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
