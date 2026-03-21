//! Color settings panel

use crate::UiState;
use egui::Ui;
use fractal_core::sdf::PALETTE_PRESETS;

pub struct ColorSettingsPanel;

impl ColorSettingsPanel {
    pub fn show(ui: &mut Ui, state: &mut UiState) -> bool {
        let mut changed = false;

        egui::CollapsingHeader::new("Colors").default_open(true).show(ui, |ui| {
            let color_ranges = &state.settings.color;
            let lighting_ranges = &state.settings.lighting;
            let color_config = &mut state.color_config;

            // Color mode selector
            ui.horizontal(|ui| {
                ui.label("Color Mode:");
                egui::ComboBox::from_id_salt("color_mode")
                    .selected_text(match color_config.color_mode {
                        0 => "Solid",
                        1 => "Orbit Trap",
                        2 => "Iteration",
                        3 => "Normal",
                        4 => "Combined",
                        _ => "Unknown",
                    })
                    .show_ui(ui, |ui| {
                        for (val, label) in [
                            (0, "Solid"),
                            (1, "Orbit Trap"),
                            (2, "Iteration"),
                            (3, "Normal"),
                            (4, "Combined"),
                        ] {
                            if ui.selectable_value(&mut color_config.color_mode, val, label).clicked() {
                                changed = true;
                            }
                        }
                    });
            });

            // Palette controls — hidden in Normal mode (mode 3) which uses surface normals directly
            if color_config.color_mode != 3 {
                ui.add_space(4.0);

                // Palette preset selector
                ui.horizontal(|ui| {
                    ui.label("Palette:");
                    let preset_name = PALETTE_PRESETS
                        .get(color_config.palette_preset)
                        .map_or("Custom", |p| p.name);
                    egui::ComboBox::from_id_salt("palette_preset")
                        .selected_text(preset_name)
                        .show_ui(ui, |ui| {
                            for (i, preset) in PALETTE_PRESETS.iter().enumerate() {
                                if ui.selectable_value(&mut color_config.palette_preset, i, preset.name).clicked() {
                                    // Apply preset
                                    color_config.palette_count = preset.colors.len() as u32;
                                    color_config.palette_colors = [[0.0; 3]; 8];
                                    for (j, c) in preset.colors.iter().enumerate() {
                                        color_config.palette_colors[j] = *c;
                                    }
                                    color_config.base_color = preset.colors[0];
                                    color_config.secondary_color = preset.colors[preset.colors.len() - 1];
                                    changed = true;
                                }
                            }
                        });
                });

                // Palette color stops
                let max_colors = color_ranges.max_palette_colors as usize;
                let count = color_config.palette_count as usize;
                let mut remove_index: Option<usize> = None;

                for i in 0..count {
                    ui.horizontal(|ui| {
                        ui.label(format!("  {}:", i + 1));
                        let mut color = color_config.palette_colors[i];
                        if ui.color_edit_button_rgb(&mut color).changed() {
                            color_config.palette_colors[i] = color;
                            color_config.palette_preset = PALETTE_PRESETS.len(); // mark custom
                            changed = true;
                        }
                        if count > 2 && ui.small_button("X").clicked() {
                            remove_index = Some(i);
                        }
                    });
                }

                if let Some(idx) = remove_index {
                    let count = color_config.palette_count as usize;
                    for j in idx..count - 1 {
                        color_config.palette_colors[j] = color_config.palette_colors[j + 1];
                    }
                    color_config.palette_colors[count - 1] = [0.0; 3];
                    color_config.palette_count -= 1;
                    color_config.palette_preset = PALETTE_PRESETS.len();
                    changed = true;
                }

                if count < max_colors {
                    ui.horizontal(|ui| {
                        ui.add_space(20.0);
                        if ui.small_button("+ Add Color").clicked() {
                            // Duplicate last color as starting point
                            let last = color_config.palette_colors[count.saturating_sub(1)];
                            color_config.palette_colors[count] = last;
                            color_config.palette_count += 1;
                            color_config.palette_preset = PALETTE_PRESETS.len();
                            changed = true;
                        }
                    });
                }

                ui.add_space(4.0);

                // Palette scale and offset
                ui.horizontal(|ui| {
                    ui.label("Color Spread:");
                    if ui.add(color_ranges.palette_scale.slider(&mut color_config.palette_scale)).changed() {
                        changed = true;
                    }
                });

                ui.horizontal(|ui| {
                    ui.label("Color Shift:");
                    if ui.add(color_ranges.palette_offset.slider(&mut color_config.palette_offset)).changed() {
                        changed = true;
                    }
                });
            }

            ui.add_space(4.0);

            // Background color picker
            ui.horizontal(|ui| {
                ui.label("Background:");
                let mut color = color_config.background_color;
                if ui.color_edit_button_rgb(&mut color).changed() {
                    color_config.background_color = color;
                    changed = true;
                }
            });

            ui.add_space(5.0);

            // Lighting settings
            ui.label("Lighting:");

            let lighting = &mut state.lighting_config;

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

            // Shared: shadow softness
            ui.horizontal(|ui| {
                ui.label("Shadow Softness:");
                if ui.add(lighting_ranges.shadow_softness.slider(&mut lighting.shadow_softness)).changed() {
                    changed = true;
                }
            });

            // Light direction (editable XYZ)
            ui.horizontal(|ui| {
                ui.label("Light Dir:");
                let dir = &mut lighting.light_dir;
                let mut dir_changed = false;
                dir_changed |= ui.add(egui::DragValue::new(&mut dir[0]).speed(0.01).prefix("x:")).changed();
                dir_changed |= ui.add(egui::DragValue::new(&mut dir[1]).speed(0.01).prefix("y:")).changed();
                dir_changed |= ui.add(egui::DragValue::new(&mut dir[2]).speed(0.01).prefix("z:")).changed();
                if dir_changed {
                    // Normalize
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

            ui.add_space(4.0);

            // Dither strength
            ui.horizontal(|ui| {
                ui.label("Noise Smoothing:");
                if ui.add(color_ranges.dither_strength.slider(&mut color_config.dither_strength)).changed() {
                    changed = true;
                }
            });
        });

        changed
    }
}
