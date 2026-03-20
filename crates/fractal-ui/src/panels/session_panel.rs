//! Session save/load panel

use crate::UiState;
use egui::Ui;

pub struct SessionPanel;

impl SessionPanel {
    pub fn show(ui: &mut Ui, state: &mut UiState) {
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
                    let mut overwrite_id: Option<String> = None;
                    let mut delete_id: Option<String> = None;

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
                                                    overwrite_id = Some(slot.id.clone());
                                                }
                                                if ui.small_button("Delete").clicked() {
                                                    delete_id = Some(slot.id.clone());
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
                    if let Some(id) = overwrite_id {
                        state.pending_overwrite = Some(id);
                    }
                    if let Some(id) = delete_id {
                        state.pending_delete = Some(id);
                    }
                }
            });
    }
}
