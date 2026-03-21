//! UI panels for fractal viewer

mod fractal_params;
mod camera_controls;
mod color_settings;
mod session_panel;

pub use fractal_params::FractalParamsPanel;
pub use camera_controls::CameraControlsPanel;
pub use color_settings::ColorSettingsPanel;
pub use session_panel::SessionPanel;

use crate::UiState;
use egui::Context;

/// Main panel that combines all sub-panels
pub struct FractalPanel;

impl FractalPanel {
    /// Render the main control panel.
    ///
    /// When expanded, shows a floating, draggable, resizable panel anchored to the
    /// left edge by default. The toggle button lives inside the panel header, to the
    /// left of the title. When collapsed, a small `☰` button floats at the top-left
    /// of the screen so the panel can be re-opened on any platform.
    pub fn show(ctx: &Context, state: &mut UiState) -> bool {
        let mut changed = false;
        let toggle_btn_size = egui::vec2(24.0, 24.0);

        if state.show_panel {
            let screen_height = ctx.screen_rect().height();

            egui::Window::new("fractal_panel")
                // Custom header — standard title bar is disabled so we can embed
                // the toggle button directly to the left of the heading text.
                .title_bar(false)
                // Allow the window to be dragged by non-interactive areas (e.g. the
                // heading text row) and resized from its edges/corners.
                .movable(true)
                .resizable(true)
                // Start flush to the left edge, spanning the full window height.
                .default_pos([0.0, 0.0])
                .default_width(280.0)
                .default_height(screen_height)
                .min_height(180.0)
                .show(ctx, |ui| {
                    // ── Header row ────────────────────────────────────────────
                    // The heading fills the left side and acts as the drag
                    // handle; the close button is anchored to the right.
                    ui.horizontal(|ui| {
                        ui.heading("🔷 Modern Fractal Viewer");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.add(egui::Button::new("X").min_size(toggle_btn_size)).clicked() {
                                state.show_panel = false;
                            }
                        });
                    });
                    ui.separator();

                    // ── Scrollable content ────────────────────────────────────
                    let available_height = ui.available_height();
                    egui::ScrollArea::vertical()
                        .max_height(available_height)
                        .show(ui, |ui| {
                        changed |= FractalParamsPanel::show(ui, state);

                        ui.add_space(10.0);
                        changed |= Self::show_rendering_settings(ui, state);

                        ui.add_space(10.0);
                        changed |= ColorSettingsPanel::show(ui, state);

                        ui.add_space(10.0);
                        changed |= CameraControlsPanel::show(ui, state);

                        ui.add_space(10.0);
                        SessionPanel::show(ui, state);

                        ui.add_space(10.0);
                        ui.collapsing("Debug", |ui| {
                            if !state.version_info.is_empty() {
                                ui.label(&state.version_info);
                            }
                            ui.checkbox(&mut state.show_debug, "Show debug info");
                            ui.checkbox(&mut state.vsync, "VSync");
                            ui.checkbox(&mut state.auto_rotate, "Auto-rotate");
                            if state.auto_rotate {
                                ui.add(
                                    egui::Slider::new(&mut state.rotation_speed, 0.1..=2.0)
                                        .text("Speed"),
                                );
                            }
                        });
                    });
                });
        } else {
            // Collapsed state: a small floating button so the panel can be
            // re-opened on all platforms (touch, mouse, keyboard).
            egui::Area::new(egui::Id::new("panel_open_btn"))
                .anchor(egui::Align2::LEFT_TOP, [4.0, 4.0])
                .order(egui::Order::Foreground)
                .default_size(toggle_btn_size)
                .show(ctx, |ui| {
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    if ui.add(egui::Button::new("☰").min_size(toggle_btn_size)).clicked() {
                        state.show_panel = true;
                    }
                    });
                });
        }

        changed
    }

    fn show_rendering_settings(ui: &mut egui::Ui, state: &mut UiState) -> bool {
        let mut changed = false;

        egui::CollapsingHeader::new("Rendering").default_open(true).show(ui, |ui| {
            let config = &mut state.ray_march_config;

            ui.horizontal(|ui| {
                ui.label("Ray Steps:");
                let mut steps = config.max_steps as i32;
                if ui.add(egui::DragValue::new(&mut steps).range(16..=512)).changed() {
                    config.max_steps = steps as u32;
                    changed = true;
                }
            });

            ui.horizontal(|ui| {
                ui.label("Surface Precision:");
                if ui
                    .add(
                        egui::DragValue::new(&mut config.epsilon)
                            .speed(0.0001)
                            .range(0.00001..=0.01)
                            .fixed_decimals(5),
                    )
                    .changed()
                {
                    changed = true;
                }
            });

            ui.horizontal(|ui| {
                ui.label("View Distance:");
                if ui
                    .add(egui::DragValue::new(&mut config.max_distance).range(10.0..=1000.0))
                    .changed()
                {
                    changed = true;
                }
            });

            ui.horizontal(|ui| {
                ui.label("Shadow Detail:");
                let mut ao = config.ao_steps as i32;
                if ui.add(egui::DragValue::new(&mut ao).range(0..=16)).changed() {
                    config.ao_steps = ao as u32;
                    changed = true;
                }
            });

            ui.horizontal(|ui| {
                ui.label("Shadow Depth:");
                if ui
                    .add(egui::Slider::new(&mut config.ao_intensity, 0.0..=1.0))
                    .changed()
                {
                    changed = true;
                }
            });

            ui.horizontal(|ui| {
                ui.label("Normal Precision:");
                if ui
                    .add(
                        egui::DragValue::new(&mut config.normal_epsilon)
                            .speed(0.00001)
                            .range(0.000001..=0.01)
                            .fixed_decimals(6),
                    )
                    .changed()
                {
                    changed = true;
                }
            });

            ui.horizontal(|ui| {
                ui.label("Anti-Aliasing:");
                egui::ComboBox::from_id_salt("sample_count")
                    .selected_text(format!("{}x", config.sample_count))
                    .show_ui(ui, |ui| {
                        for &count in &[1u32, 2, 4] {
                            if ui
                                .selectable_value(
                                    &mut config.sample_count,
                                    count,
                                    format!("{}x", count),
                                )
                                .clicked()
                            {
                                changed = true;
                            }
                        }
                    });
            });
            if config.sample_count > 1 {
                ui.small("Higher values improve quality but reduce FPS.");
            }
        });

        changed
    }
}
