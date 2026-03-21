//! Fractal parameters panel

use crate::UiState;
use crate::app_settings::IntRange;
use egui::Ui;
use fractal_core::FractalType;

pub struct FractalParamsPanel;

impl FractalParamsPanel {
    pub fn show(ui: &mut Ui, state: &mut UiState) -> bool {
        let mut changed = false;

        egui::CollapsingHeader::new("Fractal Type").default_open(true).show(ui, |ui| {
            let current_type = state.fractal_params.fractal_type;

            egui::ComboBox::from_label("")
                .selected_text(current_type.name())
                .show_ui(ui, |ui| {
                    for fractal_type in FractalType::all() {
                        if ui
                            .selectable_value(
                                &mut state.fractal_params.fractal_type,
                                *fractal_type,
                                fractal_type.name(),
                            )
                            .clicked()
                        {
                            // Load defaults for new fractal type
                            state.set_fractal_type(*fractal_type);
                            changed = true;
                        }
                    }
                });

            ui.add_space(5.0);

            egui::CollapsingHeader::new("Parameters").default_open(true).show(ui, |ui| {
                // Split borrow: &mut fractal_params + &settings.fractal
                let ranges = &state.settings.fractal;
                let params = &mut state.fractal_params;

                match params.fractal_type {
                    FractalType::Mandelbulb => {
                        changed |= Self::show_mandelbulb_params(ui, params, &ranges.mandelbulb);
                    }
                    FractalType::Menger => {
                        changed |= show_iterations(ui, params, &ranges.menger.iterations);
                    }
                    FractalType::Julia3D => {
                        changed |= Self::show_julia_params(ui, params, &ranges.julia3d);
                    }
                    FractalType::Mandelbox => {
                        changed |= Self::show_mandelbox_params(ui, params, &ranges.mandelbox);
                    }
                    FractalType::Sierpinski => {
                        changed |= Self::show_sierpinski_params(ui, params, &ranges.sierpinski);
                    }
                    FractalType::Apollonian => {
                        changed |= show_iterations(ui, params, &ranges.apollonian.iterations);
                    }
                }
            });
        });
        changed
    }

    fn show_mandelbulb_params(
        ui: &mut Ui,
        params: &mut fractal_core::FractalParams,
        ranges: &crate::app_settings::MandelbulbRanges,
    ) -> bool {
        let mut changed = false;

        ui.horizontal(|ui| {
            ui.label("Power:");
            if ui.add(ranges.power.slider(&mut params.power)).changed() {
                changed = true;
            }
        });

        changed |= show_iterations(ui, params, &ranges.iterations);

        ui.horizontal(|ui| {
            ui.label("Escape Radius:");
            if ui.add(ranges.bailout.slider(&mut params.bailout)).changed() {
                changed = true;
            }
        });

        changed
    }

    fn show_julia_params(
        ui: &mut Ui,
        params: &mut fractal_core::FractalParams,
        ranges: &crate::app_settings::Julia3DRanges,
    ) -> bool {
        let mut changed = false;

        changed |= show_iterations(ui, params, &ranges.iterations);

        ui.label("Julia C:");
        for (i, axis) in ["x", "y", "z"].iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label(format!("{axis}:"));
                if ui.add(ranges.julia_c.drag_value(&mut params.julia_c[i])).changed() {
                    changed = true;
                }
            });
        }

        changed
    }

    fn show_mandelbox_params(
        ui: &mut Ui,
        params: &mut fractal_core::FractalParams,
        ranges: &crate::app_settings::MandelboxRanges,
    ) -> bool {
        let mut changed = false;

        ui.horizontal(|ui| {
            ui.label("Box Scale:");
            if ui.add(ranges.box_scale.slider(&mut params.scale)).changed() {
                changed = true;
            }
        });

        changed |= show_iterations(ui, params, &ranges.iterations);

        ui.horizontal(|ui| {
            ui.label("Fold Range:");
            if ui.add(ranges.fold_limit.slider(&mut params.fold_limit)).changed() {
                changed = true;
            }
        });

        ui.horizontal(|ui| {
            ui.label("Inner Radius:");
            if ui.add(ranges.min_radius_sq.slider(&mut params.min_radius_sq)).changed() {
                changed = true;
            }
        });

        changed
    }

    fn show_sierpinski_params(
        ui: &mut Ui,
        params: &mut fractal_core::FractalParams,
        ranges: &crate::app_settings::SierpinskiRanges,
    ) -> bool {
        let mut changed = false;

        changed |= show_iterations(ui, params, &ranges.iterations);

        ui.horizontal(|ui| {
            ui.label("Size Ratio:");
            if ui.add(ranges.size_ratio.slider(&mut params.scale)).changed() {
                changed = true;
            }
        });

        changed
    }
}

/// Shared iterations slider used by multiple fractal types.
fn show_iterations(
    ui: &mut Ui,
    params: &mut fractal_core::FractalParams,
    range: &IntRange,
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label("Iterations:");
        let mut iter = params.iterations as i32;
        if ui.add(range.slider(&mut iter)).changed() {
            params.iterations = iter as u32;
            changed = true;
        }
    });
    changed
}
