//! Session save/load panel

use crate::UiState;
use egui::Ui;

pub struct SessionPanel;

impl SessionPanel {
    pub fn show(ui: &mut Ui, state: &mut UiState) {
        // Render confirmation dialogs as top-level windows (outside collapsing header)
        Self::show_confirmation_dialogs(ui, state);

        egui::CollapsingHeader::new("Sessions")
            .default_open(false)
            .show(ui, |ui| {
                // -- Save New button --
                if ui.button("Save New Session").clicked() {
                    state.pending_save = true;
                }

                ui.add_space(6.0);
                ui.separator();
                ui.add_space(4.0);

                // -- Saved sessions list --
                if state.session_slots.is_empty() {
                    ui.weak("No saved sessions.");
                } else {
                    let mut load_id: Option<String> = None;
                    let mut overwrite_info: Option<(String, String)> = None;
                    let mut delete_info: Option<(String, String)> = None;

                    egui::ScrollArea::vertical()
                        .max_height(300.0)
                        .show(ui, |ui| {
                            for slot in &state.session_slots {
                                ui.group(|ui| {
                                    ui.horizontal(|ui| {
                                        // Thumbnail
                                        if let Some(ref tex) = slot.thumbnail {
                                            let size = egui::vec2(80.0, 45.0);
                                            ui.image(egui::load::SizedTexture::new(
                                                tex.id(),
                                                size,
                                            ));
                                        } else {
                                            // Placeholder
                                            let (rect, _) = ui.allocate_exact_size(
                                                egui::vec2(80.0, 45.0),
                                                egui::Sense::hover(),
                                            );
                                            ui.painter().rect_filled(
                                                rect,
                                                2.0,
                                                egui::Color32::from_gray(40),
                                            );
                                        }

                                        ui.vertical(|ui| {
                                            ui.strong(&slot.name);
                                            ui.weak(format!(
                                                "{} · {}",
                                                &slot.timestamp, &slot.fractal_type_name
                                            ));
                                            ui.horizontal(|ui| {
                                                if ui.small_button("Load").clicked() {
                                                    load_id = Some(slot.id.clone());
                                                }
                                                if ui.small_button("Save").clicked() {
                                                    overwrite_info = Some((
                                                        slot.id.clone(),
                                                        slot.name.clone(),
                                                    ));
                                                }
                                                if ui.small_button("Delete").clicked() {
                                                    delete_info = Some((
                                                        slot.id.clone(),
                                                        slot.name.clone(),
                                                    ));
                                                }
                                            });
                                        });
                                    });
                                });
                                ui.add_space(2.0);
                            }
                        });

                    if let Some(id) = load_id {
                        state.pending_load = Some(id);
                    }
                    if let Some(info) = overwrite_info {
                        state.confirming_delete = None;
                        state.confirming_overwrite = Some(info);
                    }
                    if let Some(info) = delete_info {
                        state.confirming_overwrite = None;
                        state.confirming_delete = Some(info);
                    }
                }
            });
    }

    fn show_confirmation_dialogs(ui: &mut Ui, state: &mut UiState) {
        // Overwrite confirmation
        if let Some((ref id, ref name)) = state.confirming_overwrite.clone() {
            let mut dismiss = false;
            egui::Window::new("Confirm Overwrite")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ui.ctx(), |ui| {
                    ui.label(format!("Overwrite session \"{}\"?", name));
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("Overwrite").clicked() {
                            state.pending_overwrite = Some(id.clone());
                            dismiss = true;
                        }
                        if ui.button("Cancel").clicked() {
                            dismiss = true;
                        }
                    });
                });
            if dismiss {
                state.confirming_overwrite = None;
            }
        }

        // Delete confirmation
        if let Some((ref id, ref name)) = state.confirming_delete.clone() {
            let mut dismiss = false;
            egui::Window::new("Confirm Delete")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ui.ctx(), |ui| {
                    ui.label(format!(
                        "Delete session \"{}\"? This cannot be undone.",
                        name
                    ));
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("Delete").clicked() {
                            state.pending_delete = Some(id.clone());
                            dismiss = true;
                        }
                        if ui.button("Cancel").clicked() {
                            dismiss = true;
                        }
                    });
                });
            if dismiss {
                state.confirming_delete = None;
            }
        }
    }
}
