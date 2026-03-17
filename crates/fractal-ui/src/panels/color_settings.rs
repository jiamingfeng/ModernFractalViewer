//! Color settings panel

use crate::UiState;
use egui::Ui;

pub struct ColorSettingsPanel;

impl ColorSettingsPanel {
    pub fn show(ui: &mut Ui, state: &mut UiState) -> bool {
        let mut changed = false;
        
        egui::CollapsingHeader::new("Colors").default_open(true).show(ui, |ui| {
            let color_config = &mut state.color_config;
            
            // Color mode selector
            ui.horizontal(|ui| {
                ui.label("Mode:");
                egui::ComboBox::from_id_salt("color_mode")
                    .selected_text(match color_config.color_mode {
                        0 => "Solid",
                        1 => "Orbit Trap",
                        2 => "Iteration",
                        3 => "Normal",
                        _ => "Unknown",
                    })
                    .show_ui(ui, |ui| {
                        if ui.selectable_value(&mut color_config.color_mode, 0, "Solid").clicked() {
                            changed = true;
                        }
                        if ui.selectable_value(&mut color_config.color_mode, 1, "Orbit Trap").clicked() {
                            changed = true;
                        }
                        if ui.selectable_value(&mut color_config.color_mode, 2, "Iteration").clicked() {
                            changed = true;
                        }
                        if ui.selectable_value(&mut color_config.color_mode, 3, "Normal").clicked() {
                            changed = true;
                        }
                    });
            });
            
            // Base color picker
            ui.horizontal(|ui| {
                ui.label("Base Color:");
                let mut color = color_config.base_color;
                if ui.color_edit_button_rgb(&mut color).changed() {
                    color_config.base_color = color;
                    changed = true;
                }
            });
            
            // Secondary color picker
            ui.horizontal(|ui| {
                ui.label("Secondary:");
                let mut color = color_config.secondary_color;
                if ui.color_edit_button_rgb(&mut color).changed() {
                    color_config.secondary_color = color;
                    changed = true;
                }
            });
            
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
            
            ui.horizontal(|ui| {
                ui.label("Ambient:");
                if ui.add(egui::Slider::new(&mut lighting.ambient, 0.0..=1.0)).changed() {
                    changed = true;
                }
            });
            
            ui.horizontal(|ui| {
                ui.label("Diffuse:");
                if ui.add(egui::Slider::new(&mut lighting.diffuse, 0.0..=1.0)).changed() {
                    changed = true;
                }
            });
            
            ui.horizontal(|ui| {
                ui.label("Specular:");
                if ui.add(egui::Slider::new(&mut lighting.specular, 0.0..=1.0)).changed() {
                    changed = true;
                }
            });
            
            ui.horizontal(|ui| {
                ui.label("Shininess:");
                if ui.add(egui::Slider::new(&mut lighting.shininess, 1.0..=128.0)).changed() {
                    changed = true;
                }
            });
        });
        
        changed
    }
}
