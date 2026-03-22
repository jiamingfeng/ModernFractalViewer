//! Export Mesh UI panel

use crate::UiState;
use fractal_core::mesh;

/// Resolution presets for mesh export.
const RESOLUTION_PRESETS: &[(u32, &str)] = &[
    (64, "Low (64)"),
    (128, "Medium (128)"),
    (256, "High (256)"),
    (512, "Very High (512)"),
];

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

                // Resolution selector
                ui.horizontal(|ui| {
                    ui.label("Resolution:");
                    let current_label = RESOLUTION_PRESETS
                        .iter()
                        .find(|(v, _)| *v == state.export_config.resolution)
                        .map(|(_, label)| *label)
                        .unwrap_or("Custom");
                    egui::ComboBox::from_id_salt("export_resolution")
                        .selected_text(current_label)
                        .show_ui(ui, |ui| {
                            for &(value, label) in RESOLUTION_PRESETS {
                                ui.selectable_value(
                                    &mut state.export_config.resolution,
                                    value,
                                    label,
                                );
                            }
                        });
                });

                ui.add_space(4.0);

                // Bounding box
                ui.horizontal(|ui| {
                    ui.label("Bounds Min:");
                    for (i, label) in ["x:", "y:", "z:"].iter().enumerate() {
                        ui.add(
                            ranges
                                .bounds
                                .drag_value(&mut state.export_config.bounds_min[i])
                                .prefix(*label),
                        );
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Bounds Max:");
                    for (i, label) in ["x:", "y:", "z:"].iter().enumerate() {
                        ui.add(
                            ranges
                                .bounds
                                .drag_value(&mut state.export_config.bounds_max[i])
                                .prefix(*label),
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
