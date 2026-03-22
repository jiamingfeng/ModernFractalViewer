//! Export Mesh UI panel

use crate::UiState;
use fractal_core::mesh;

pub struct ExportPanel;

impl ExportPanel {
    pub fn show(ui: &mut egui::Ui, state: &mut UiState) {
        egui::CollapsingHeader::new("Export Mesh")
            .default_open(false)
            .show(ui, |ui| {
                let ranges = &state.settings.export;

                // Method selector
                ui.horizontal(|ui| {
                    ui.label("Method:");
                    egui::ComboBox::from_id_salt("export_method")
                        .selected_text(state.export_config.method.to_string())
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut state.export_config.method,
                                mesh::MeshMethod::DualContouring,
                                "Dual Contouring",
                            );
                            ui.selectable_value(
                                &mut state.export_config.method,
                                mesh::MeshMethod::MarchingCubes,
                                "Marching Cubes",
                            );
                        });
                });

                ui.add_space(4.0);

                // Resolution selector (presets from settings + custom)
                let presets = &ranges.resolution_presets;
                let is_preset = presets.iter().any(|p| p.value == state.export_config.resolution);
                let current_label: String = if is_preset {
                    presets
                        .iter()
                        .find(|p| p.value == state.export_config.resolution)
                        .map(|p| p.label.clone())
                        .unwrap_or_else(|| "Custom".into())
                } else {
                    format!("Custom ({})", state.export_config.resolution)
                };

                ui.horizontal(|ui| {
                    ui.label("Resolution:");
                    let mut selected = state.export_config.resolution;
                    egui::ComboBox::from_id_salt("export_resolution")
                        .selected_text(&current_label)
                        .show_ui(ui, |ui| {
                            for preset in presets {
                                ui.selectable_value(
                                    &mut selected,
                                    preset.value,
                                    &preset.label,
                                );
                            }
                            // Sentinel 0 = "switch to custom"
                            let custom_val = if is_preset { 0u32 } else { selected };
                            ui.selectable_value(
                                &mut selected,
                                custom_val,
                                "Custom…",
                            );
                        });
                    // Apply: sentinel 0 → keep current value (enters custom mode)
                    if selected == 0 && is_preset {
                        // User clicked "Custom…" — set a non-preset value to enter custom mode
                        state.export_config.resolution = state.export_config.resolution + 1;
                        // Clamp just in case
                        if presets.iter().any(|p| p.value == state.export_config.resolution) {
                            state.export_config.resolution += 1;
                        }
                    } else if selected != 0 {
                        state.export_config.resolution = selected;
                    }
                });

                // Show a custom resolution drag-value when not matching any preset
                if !presets.iter().any(|p| p.value == state.export_config.resolution) {
                    ui.horizontal(|ui| {
                        ui.label("  Value:");
                        let mut res = state.export_config.resolution as i32;
                        if ui
                            .add(ranges.resolution.drag_value(&mut res))
                            .changed()
                        {
                            state.export_config.resolution = (res as u32).clamp(
                                ranges.resolution.min as u32,
                                ranges.resolution.max as u32,
                            );
                        }
                    });
                }

                ui.add_space(4.0);

                // Bounding box (in centimetres)
                ui.horizontal(|ui| {
                    ui.label("Bounds Min (cm):");
                    for (i, label) in ["x:", "y:", "z:"].iter().enumerate() {
                        ui.add(
                            ranges
                                .bounds
                                .drag_value(&mut state.export_config.bounds_min[i])
                                .prefix(*label)
                                .suffix(" cm"),
                        );
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Bounds Max (cm):");
                    for (i, label) in ["x:", "y:", "z:"].iter().enumerate() {
                        ui.add(
                            ranges
                                .bounds
                                .drag_value(&mut state.export_config.bounds_max[i])
                                .prefix(*label)
                                .suffix(" cm"),
                        );
                    }
                });
                if ui.small_button("Auto").clicked() {
                    let (bmin, bmax) =
                        mesh::default_bounds(state.fractal_params.fractal_type);
                    state.export_config.bounds_min = bmin;
                    state.export_config.bounds_max = bmax;
                }

                ui.add_space(4.0);

                // Smooth normals
                ui.checkbox(
                    &mut state.export_config.compute_normals,
                    "Smooth normals",
                );

                ui.add_space(6.0);

                // Export button
                let can_export = !state.export_in_progress;
                ui.add_enabled_ui(can_export, |ui| {
                    if ui.button("Export as glTF (.glb)").clicked() {
                        state.pending_export = true;
                    }
                });

                // Progress bar
                if state.export_in_progress {
                    let progress = state.export_progress.unwrap_or(0.0);
                    ui.add(
                        egui::ProgressBar::new(progress)
                            .text(format!("{:.0}%", progress * 100.0)),
                    );
                }

                // Status message
                if let Some(ref status) = state.export_status {
                    ui.small(status);
                }
            });
    }
}
