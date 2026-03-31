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
                            ui.selectable_value(
                                &mut state.export_config.method,
                                mesh::MeshMethod::SurfaceNets,
                                "Surface Nets",
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

                // Boundary extension
                ui.checkbox(
                    &mut state.export_config.boundary_extension,
                    "Extend boundaries",
                ).on_hover_text(
                    "Expand sampling volume by one voxel + iso-level on each side \
                     to capture surface features at the boundary."
                );

                ui.add_space(4.0);

                // Iso-level controls
                ui.checkbox(
                    &mut state.export_config.adaptive_iso,
                    "Adaptive iso-level",
                ).on_hover_text(
                    "Automatically compute iso-level from voxel size. \
                     Helps preserve thin features at coarser resolutions."
                );
                if state.export_config.adaptive_iso {
                    ui.horizontal(|ui| {
                        ui.label("  Factor:");
                        ui.add(
                            egui::DragValue::new(&mut state.export_config.adaptive_iso_factor)
                                .range(0.01..=0.5)
                                .speed(0.005)
                                .fixed_decimals(2),
                        );
                    });
                } else {
                    ui.horizontal(|ui| {
                        ui.label("  Iso level:");
                        ui.add(
                            egui::DragValue::new(&mut state.export_config.iso_level)
                                .range(-1.0..=1.0)
                                .speed(0.001)
                                .fixed_decimals(3),
                        );
                    });
                }

                ui.add_space(4.0);

                // Smoothing controls
                ui.horizontal(|ui| {
                    ui.label("Smoothing:");
                    egui::ComboBox::from_id_salt("export_smooth_method")
                        .selected_text(state.export_config.smooth_method.to_string())
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut state.export_config.smooth_method,
                                mesh::SmoothMethod::None,
                                "None",
                            );
                            ui.selectable_value(
                                &mut state.export_config.smooth_method,
                                mesh::SmoothMethod::Laplacian,
                                "Laplacian",
                            );
                            ui.selectable_value(
                                &mut state.export_config.smooth_method,
                                mesh::SmoothMethod::Taubin,
                                "Taubin (volume-preserving)",
                            );
                        });
                });
                if state.export_config.smooth_method != mesh::SmoothMethod::None {
                    ui.horizontal(|ui| {
                        ui.label("  Iterations:");
                        ui.add(
                            egui::DragValue::new(&mut state.export_config.smooth_iterations)
                                .range(1..=10)
                                .speed(0.1),
                        );
                    }).response.on_hover_text(
                        "Higher values produce smoother meshes but increase export time. \
                         1\u{2013}3 is fast, 5+ is noticeably slower on large meshes."
                    );
                    ui.horizontal(|ui| {
                        ui.label("  Lambda:");
                        ui.add(
                            egui::DragValue::new(&mut state.export_config.smooth_lambda)
                                .range(0.01..=1.0)
                                .speed(0.01)
                                .fixed_decimals(2),
                        );
                    }).response.on_hover_text(
                        "Controls smoothing strength per iteration. \
                         Lower values (0.1\u{2013}0.3) give subtle smoothing; \
                         higher values (0.5+) smooth more aggressively. \
                         Does not affect export time."
                    );
                }

                ui.add_space(4.0);

                // Decimation controls
                ui.checkbox(
                    &mut state.export_config.decimate,
                    "Simplify mesh",
                ).on_hover_text(
                    "Reduce triangle count using Quadric Error Metrics. \
                     Preserves shape while producing smaller files."
                );
                if state.export_config.decimate {
                    ui.horizontal(|ui| {
                        ui.label("  Target ratio:");
                        ui.add(
                            egui::DragValue::new(&mut state.export_config.decimate_target_ratio)
                                .range(0.01..=1.0)
                                .speed(0.01)
                                .fixed_decimals(0)
                                .custom_formatter(|v, _| format!("{:.0}%", v * 100.0))
                                .custom_parser(|s| {
                                    s.trim_end_matches('%')
                                        .trim()
                                        .parse::<f64>()
                                        .ok()
                                        .map(|v| v / 100.0)
                                }),
                        );
                    });
                }

                ui.add_space(4.0);

                // Format selector
                ui.horizontal(|ui| {
                    ui.label("Format:");
                    egui::ComboBox::from_id_salt("export_format")
                        .selected_text(state.export_config.export_format.to_string())
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut state.export_config.export_format,
                                mesh::ExportFormat::Glb,
                                "glTF Binary (.glb)",
                            );
                            ui.selectable_value(
                                &mut state.export_config.export_format,
                                mesh::ExportFormat::Obj,
                                "Wavefront OBJ (.obj)",
                            );
                            ui.selectable_value(
                                &mut state.export_config.export_format,
                                mesh::ExportFormat::Ply,
                                "Stanford PLY (.ply)",
                            );
                        });
                });

                // Android-only: custom filename input
                if cfg!(target_os = "android") {
                    // Lazy-init: if empty, populate with auto-generated name
                    if state.export_filename.is_empty() {
                        let fractal_name = state.fractal_params.fractal_type.name();
                        state.export_filename =
                            state.export_config.export_format.default_filename(fractal_name);
                    }

                    // Keep extension in sync when format changes
                    let new_ext = state.export_config.export_format.extension();
                    let wrong_ext = !state.export_filename
                        .rsplit('.')
                        .next()
                        .map(|e| e.eq_ignore_ascii_case(new_ext))
                        .unwrap_or(false);
                    if wrong_ext {
                        let stem = state.export_filename
                            .rsplit_once('.')
                            .map(|(s, _)| s)
                            .unwrap_or(&state.export_filename)
                            .to_string();
                        state.export_filename = format!("{stem}.{new_ext}");
                    }

                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label("Filename:");
                        let resp = ui.text_edit_singleline(&mut state.export_filename);
                        if resp.lost_focus() {
                            state.export_filename = sanitise_export_filename(
                                &state.export_filename,
                                state.export_config.export_format.extension(),
                            );
                            if state.export_filename.is_empty() {
                                let fractal_name = state.fractal_params.fractal_type.name();
                                state.export_filename =
                                    state.export_config.export_format.default_filename(fractal_name);
                            }
                        }
                    });
                }

                ui.add_space(6.0);

                // Export button
                let can_export = !state.export_in_progress;
                let button_label = format!("Export as {}", state.export_config.export_format);
                ui.add_enabled_ui(can_export, |ui| {
                    if ui.button(&button_label).clicked() {
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

/// Strip path separators from a candidate filename and enforce the required extension.
fn sanitise_export_filename(raw: &str, ext: &str) -> String {
    let clean: String = raw.chars().filter(|&c| c != '/' && c != '\\').collect();
    let clean = clean.trim().to_string();
    if clean.is_empty() {
        return String::new();
    }
    match clean.rsplit_once('.') {
        Some((_stem, e)) if e.eq_ignore_ascii_case(ext) => clean,
        Some((stem, _)) => format!("{stem}.{ext}"),
        None => format!("{clean}.{ext}"),
    }
}
