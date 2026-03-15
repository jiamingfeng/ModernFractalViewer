//! UI panels for fractal viewer

mod fractal_params;
mod camera_controls;
mod color_settings;

pub use fractal_params::FractalParamsPanel;
pub use camera_controls::CameraControlsPanel;
pub use color_settings::ColorSettingsPanel;

use crate::UiState;
use egui::Context;

/// Main panel that combines all sub-panels
pub struct FractalPanel;

impl FractalPanel {
    /// Render the main control panel
    pub fn show(ctx: &Context, state: &mut UiState) -> bool {
        let mut changed = false;
        
        egui::SidePanel::left("fractal_panel")
            .default_width(280.0)
            .resizable(true)
            .show(ctx, |ui| {
                ui.heading("🔷 Fractal Viewer");
                ui.separator();
                
                egui::ScrollArea::vertical().show(ui, |ui| {
                    // Fractal type and parameters
                    changed |= FractalParamsPanel::show(ui, state);
                    
                    ui.add_space(10.0);
                    
                    // Rendering settings
                    changed |= Self::show_rendering_settings(ui, state);
                    
                    ui.add_space(10.0);
                    
                    // Color settings
                    changed |= ColorSettingsPanel::show(ui, state);
                    
                    ui.add_space(10.0);
                    
                    // Camera controls
                    changed |= CameraControlsPanel::show(ui, state);
                    
                    ui.add_space(10.0);
                    
                    // Debug options
                    ui.collapsing("Debug", |ui| {
                        ui.checkbox(&mut state.show_debug, "Show debug info");
                        ui.checkbox(&mut state.auto_rotate, "Auto-rotate");
                        if state.auto_rotate {
                            ui.add(egui::Slider::new(&mut state.rotation_speed, 0.1..=2.0)
                                .text("Speed"));
                        }
                    });
                });
            });
        
        changed
    }

    fn show_rendering_settings(ui: &mut egui::Ui, state: &mut UiState) -> bool {
        let mut changed = false;
        
        ui.collapsing("Rendering", |ui| {
            let config = &mut state.ray_march_config;
            
            ui.horizontal(|ui| {
                ui.label("Max Steps:");
                let mut steps = config.max_steps as i32;
                if ui.add(egui::DragValue::new(&mut steps).range(16..=512)).changed() {
                    config.max_steps = steps as u32;
                    changed = true;
                }
            });
            
            ui.horizontal(|ui| {
                ui.label("Epsilon:");
                if ui.add(egui::DragValue::new(&mut config.epsilon)
                    .speed(0.0001)
                    .range(0.00001..=0.01)
                    .fixed_decimals(5))
                    .changed() {
                    changed = true;
                }
            });
            
            ui.horizontal(|ui| {
                ui.label("Max Distance:");
                if ui.add(egui::DragValue::new(&mut config.max_distance)
                    .range(10.0..=1000.0))
                    .changed() {
                    changed = true;
                }
            });
            
            ui.horizontal(|ui| {
                ui.label("AO Steps:");
                let mut ao = config.ao_steps as i32;
                if ui.add(egui::DragValue::new(&mut ao).range(0..=16)).changed() {
                    config.ao_steps = ao as u32;
                    changed = true;
                }
            });
            
            ui.horizontal(|ui| {
                ui.label("AO Intensity:");
                if ui.add(egui::Slider::new(&mut config.ao_intensity, 0.0..=1.0))
                    .changed() {
                    changed = true;
                }
            });
        });
        
        changed
    }
}
