//! UI panels for fractal viewer

mod benchmark_panel;
mod fractal_params;
mod camera_controls;
mod color_settings;
mod control_settings_panel;
mod export_panel;
mod session_panel;

pub use benchmark_panel::BenchmarkPanel;
pub use fractal_params::FractalParamsPanel;
pub use camera_controls::CameraControlsPanel;
pub use color_settings::ColorSettingsPanel;
pub use export_panel::ExportPanel;
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
                // Semi-transparent panel background
                .frame(egui::Frame::window(&ctx.style()).fill(egui::Color32::from_rgba_unmultiplied(30, 30, 30, 220)))
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
                        SessionPanel::show(ui, state);

                        ui.add_space(10.0);
                        changed |= FractalParamsPanel::show(ui, state);

                        ui.add_space(10.0);
                        changed |= ColorSettingsPanel::show(ui, state);

                        ui.add_space(10.0);
                        changed |= Self::show_rendering_settings(ui, state);

                        ui.add_space(10.0);
                        changed |= Self::show_lighting_settings(ui, state);

                        ui.add_space(10.0);
                        changed |= CameraControlsPanel::show(ui, state);

                        ui.add_space(10.0);
                        ExportPanel::show(ui, state);

                        ui.add_space(10.0);
                        BenchmarkPanel::show(ui, state);

                        ui.add_space(10.0);
                        ui.collapsing("Debug", |ui| {
                            if !state.version_info.is_empty() {
                                ui.label(&state.version_info);
                            }
                            ui.checkbox(&mut state.show_debug, "Show debug info");
                            ui.checkbox(&mut state.show_logs, "Show log window");
                            ui.checkbox(&mut state.vsync, "VSync");
                            ui.checkbox(&mut state.auto_rotate, "Auto-rotate");
                            if state.auto_rotate {
                                let rot_range = &state.settings.debug.rotation_speed;
                                ui.add(
                                    rot_range.slider(&mut state.rotation_speed)
                                        .text("Speed"),
                                );
                            }
                            ui.add_space(4.0);
                            control_settings_panel::ControlSettingsPanel::show(ui, state);
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
            let ranges = &state.settings.rendering;
            let config = &mut state.ray_march_config;

            ui.horizontal(|ui| {
                ui.label("Ray Steps:");
                let mut steps = config.max_steps as i32;
                if ui.add(ranges.ray_steps.drag_value(&mut steps)).changed() {
                    config.max_steps = steps as u32;
                    changed = true;
                }
            });

            ui.horizontal(|ui| {
                ui.label("Surface Precision:");
                if ui.add(ranges.epsilon.drag_value(&mut config.epsilon)).changed() {
                    changed = true;
                }
            });

            ui.horizontal(|ui| {
                ui.label("View Distance:");
                if ui.add(ranges.max_distance.drag_value(&mut config.max_distance)).changed() {
                    changed = true;
                }
            });

            ui.horizontal(|ui| {
                ui.label("Normal Precision:");
                if ui.add(ranges.normal_epsilon.drag_value(&mut config.normal_epsilon)).changed() {
                    changed = true;
                }
            });

            ui.horizontal(|ui| {
                ui.label("Anti-Aliasing:");
                egui::ComboBox::from_id_salt("sample_count")
                    .selected_text(format!("{}x", config.sample_count))
                    .show_ui(ui, |ui| {
                        for &count in &ranges.sample_counts {
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

            ui.add_space(6.0);
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("Level of Detail:");
                if ui.checkbox(&mut config.lod_enabled, "").changed() {
                    changed = true;
                }
                if config.lod_enabled {
                    ui.label("Scale:");
                    if ui.add(ranges.lod_scale.drag_value(&mut config.lod_scale)).changed() {
                        changed = true;
                    }
                }
            });
            if config.lod_enabled {
                ui.small("Scales precision with distance. Higher = faster but less detail.");
            }
        });

        changed
    }

    fn show_lighting_settings(ui: &mut egui::Ui, state: &mut UiState) -> bool {
        let mut changed = false;

        egui::CollapsingHeader::new("Lighting").default_open(true).show(ui, |ui| {
            let lighting_ranges = &state.settings.lighting;
            let rendering_ranges = &state.settings.rendering;
            let lighting = &mut state.lighting_config;
            let ray_config = &mut state.ray_march_config;

            // Lighting model selector
            ui.horizontal(|ui| {
                ui.label("Model:");
                egui::ComboBox::from_id_salt("lighting_model")
                    .selected_text(match lighting.lighting_model {
                        0 => "Blinn-Phong",
                        1 => "PBR (GGX)",
                        _ => "Unknown",
                    })
                    .show_ui(ui, |ui| {
                        if ui.selectable_value(&mut lighting.lighting_model, 0, "Blinn-Phong").clicked() {
                            changed = true;
                        }
                        if ui.selectable_value(&mut lighting.lighting_model, 1, "PBR (GGX)").clicked() {
                            changed = true;
                        }
                    });
            });

            // Shared: ambient
            ui.horizontal(|ui| {
                ui.label("Ambient Light:");
                if ui.add(lighting_ranges.ambient.slider(&mut lighting.ambient)).changed() {
                    changed = true;
                }
            });

            if lighting.lighting_model == 0 {
                // Blinn-Phong specific
                ui.horizontal(|ui| {
                    ui.label("Direct Light:");
                    if ui.add(lighting_ranges.diffuse.slider(&mut lighting.diffuse)).changed() {
                        changed = true;
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Reflection:");
                    if ui.add(lighting_ranges.specular.slider(&mut lighting.specular)).changed() {
                        changed = true;
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Gloss:");
                    if ui.add(lighting_ranges.shininess.slider(&mut lighting.shininess)).changed() {
                        changed = true;
                    }
                });
            } else {
                // PBR specific
                ui.horizontal(|ui| {
                    ui.label("Roughness:");
                    if ui.add(lighting_ranges.roughness.slider(&mut lighting.roughness)).changed() {
                        changed = true;
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Metallic:");
                    if ui.add(lighting_ranges.metallic.slider(&mut lighting.metallic)).changed() {
                        changed = true;
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Light Intensity:");
                    if ui.add(lighting_ranges.light_intensity.slider(&mut lighting.light_intensity)).changed() {
                        changed = true;
                    }
                });
            }

            // Shadow sharpness (IQ's k parameter)
            ui.horizontal(|ui| {
                ui.label("Shadow Sharpness:");
                if ui.add(lighting_ranges.shadow_softness.slider(&mut lighting.shadow_softness)).changed() {
                    changed = true;
                }
            });

            // AO controls (moved from Rendering section)
            ui.horizontal(|ui| {
                ui.label("AO Steps:");
                let mut ao = ray_config.ao_steps as i32;
                if ui.add(rendering_ranges.ao_steps.drag_value(&mut ao)).changed() {
                    ray_config.ao_steps = ao as u32;
                    changed = true;
                }
            });

            ui.horizontal(|ui| {
                ui.label("AO Intensity:");
                if ui.add(rendering_ranges.ao_intensity.slider(&mut ray_config.ao_intensity)).changed() {
                    changed = true;
                }
            });

            ui.add_space(4.0);

            // Light direction (editable XYZ)
            ui.horizontal(|ui| {
                ui.label("Light Dir:");
                let dir = &mut lighting.light_dir;
                let mut dir_changed = false;
                dir_changed |= ui.add(egui::DragValue::new(&mut dir[0]).speed(0.01).prefix("x:")).changed();
                dir_changed |= ui.add(egui::DragValue::new(&mut dir[1]).speed(0.01).prefix("y:")).changed();
                dir_changed |= ui.add(egui::DragValue::new(&mut dir[2]).speed(0.01).prefix("z:")).changed();
                if dir_changed {
                    let len = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt();
                    if len > 0.001 {
                        dir[0] /= len;
                        dir[1] /= len;
                        dir[2] /= len;
                    }
                    changed = true;
                }
            });

            if state.light_control_active {
                ui.small("Hold L + drag mouse to change light direction");
            }
        });

        changed
    }
}
