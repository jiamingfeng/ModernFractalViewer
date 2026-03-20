//! Fractal parameters panel

use crate::UiState;
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
                let params = &mut state.fractal_params;

                match params.fractal_type {
                    FractalType::Mandelbulb => {
                        changed |= Self::show_mandelbulb_params(ui, params);
                    }
                    FractalType::Menger => {
                        changed |= Self::show_menger_params(ui, params);
                    }
                    FractalType::Julia3D => {
                        changed |= Self::show_julia_params(ui, params);
                    }
                    FractalType::Mandelbox => {
                        changed |= Self::show_mandelbox_params(ui, params);
                    }
                    FractalType::Sierpinski => {
                        changed |= Self::show_sierpinski_params(ui, params);
                    }
                    FractalType::Apollonian => {
                        changed |= Self::show_apollonian_params(ui, params);
                    }
                }
            });
        });
        changed
    }

    fn show_mandelbulb_params(ui: &mut Ui, params: &mut fractal_core::FractalParams) -> bool {
        let mut changed = false;

        ui.horizontal(|ui| {
            ui.label("Power:");
            if ui
                .add(egui::Slider::new(&mut params.power, 1.0..=16.0))
                .changed()
            {
                changed = true;
            }
        });

        ui.horizontal(|ui| {
            ui.label("Iterations:");
            let mut iter = params.iterations as i32;
            if ui.add(egui::Slider::new(&mut iter, 1..=32)).changed() {
                params.iterations = iter as u32;
                changed = true;
            }
        });

        ui.horizontal(|ui| {
            ui.label("Escape Radius:");
            if ui
                .add(egui::Slider::new(&mut params.bailout, 1.0..=8.0))
                .changed()
            {
                changed = true;
            }
        });

        changed
    }

    fn show_menger_params(ui: &mut Ui, params: &mut fractal_core::FractalParams) -> bool {
        let mut changed = false;

        ui.horizontal(|ui| {
            ui.label("Iterations:");
            let mut iter = params.iterations as i32;
            if ui.add(egui::Slider::new(&mut iter, 1..=8)).changed() {
                params.iterations = iter as u32;
                changed = true;
            }
        });

        changed
    }

    fn show_julia_params(ui: &mut Ui, params: &mut fractal_core::FractalParams) -> bool {
        let mut changed = false;

        ui.horizontal(|ui| {
            ui.label("Iterations:");
            let mut iter = params.iterations as i32;
            if ui.add(egui::Slider::new(&mut iter, 1..=32)).changed() {
                params.iterations = iter as u32;
                changed = true;
            }
        });

        ui.label("Julia C:");
        ui.horizontal(|ui| {
            ui.label("x:");
            if ui
                .add(
                    egui::DragValue::new(&mut params.julia_c[0])
                        .speed(0.01)
                        .range(-2.0..=2.0),
                )
                .changed()
            {
                changed = true;
            }
        });
        ui.horizontal(|ui| {
            ui.label("y:");
            if ui
                .add(
                    egui::DragValue::new(&mut params.julia_c[1])
                        .speed(0.01)
                        .range(-2.0..=2.0),
                )
                .changed()
            {
                changed = true;
            }
        });
        ui.horizontal(|ui| {
            ui.label("z:");
            if ui
                .add(
                    egui::DragValue::new(&mut params.julia_c[2])
                        .speed(0.01)
                        .range(-2.0..=2.0),
                )
                .changed()
            {
                changed = true;
            }
        });

        changed
    }

    fn show_mandelbox_params(ui: &mut Ui, params: &mut fractal_core::FractalParams) -> bool {
        let mut changed = false;

        ui.horizontal(|ui| {
            ui.label("Box Scale:");
            if ui
                .add(egui::Slider::new(&mut params.scale, -3.0..=3.0))
                .changed()
            {
                changed = true;
            }
        });

        ui.horizontal(|ui| {
            ui.label("Iterations:");
            let mut iter = params.iterations as i32;
            if ui.add(egui::Slider::new(&mut iter, 1..=32)).changed() {
                params.iterations = iter as u32;
                changed = true;
            }
        });

        ui.horizontal(|ui| {
            ui.label("Fold Range:");
            if ui
                .add(egui::Slider::new(&mut params.fold_limit, 0.5..=2.0))
                .changed()
            {
                changed = true;
            }
        });

        ui.horizontal(|ui| {
            ui.label("Inner Radius:");
            if ui
                .add(egui::Slider::new(&mut params.min_radius_sq, 0.01..=1.0))
                .changed()
            {
                changed = true;
            }
        });

        changed
    }

    fn show_sierpinski_params(ui: &mut Ui, params: &mut fractal_core::FractalParams) -> bool {
        let mut changed = false;

        ui.horizontal(|ui| {
            ui.label("Iterations:");
            let mut iter = params.iterations as i32;
            if ui.add(egui::Slider::new(&mut iter, 1..=20)).changed() {
                params.iterations = iter as u32;
                changed = true;
            }
        });

        ui.horizontal(|ui| {
            ui.label("Size Ratio:");
            if ui
                .add(egui::Slider::new(&mut params.scale, 1.5..=3.0))
                .changed()
            {
                changed = true;
            }
        });

        changed
    }

    fn show_apollonian_params(ui: &mut Ui, params: &mut fractal_core::FractalParams) -> bool {
        let mut changed = false;

        ui.horizontal(|ui| {
            ui.label("Iterations:");
            let mut iter = params.iterations as i32;
            if ui.add(egui::Slider::new(&mut iter, 1..=12)).changed() {
                params.iterations = iter as u32;
                changed = true;
            }
        });

        changed
    }
}
