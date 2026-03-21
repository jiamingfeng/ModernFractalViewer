//! Camera controls panel

use crate::UiState;
use egui::Ui;

pub struct CameraControlsPanel;

impl CameraControlsPanel {
    pub fn show(ui: &mut Ui, state: &mut UiState) -> bool {
        let mut changed = false;

        egui::CollapsingHeader::new("Camera").default_open(true).show(ui, |ui| {
            let ranges = &state.settings.camera;
            let camera = &mut state.camera;

            // FOV slider
            ui.horizontal(|ui| {
                ui.label("FOV:");
                let mut fov_deg = camera.fov.to_degrees();
                if ui.add(ranges.fov.slider(&mut fov_deg).suffix("°")).changed() {
                    camera.fov = fov_deg.to_radians();
                    changed = true;
                }
            });

            // Zoom (derived: zoom = 1/distance, so higher = more magnified)
            ui.horizontal(|ui| {
                ui.label("Zoom:");
                let mut zoom_value = 1.0 / camera.distance;
                if ui.add(ranges.zoom.slider(&mut zoom_value)).changed() {
                    camera.distance = (1.0 / zoom_value)
                        .clamp(ranges.distance_clamp.min, ranges.distance_clamp.max);
                    camera.update_position();
                    changed = true;
                }
            });

            // Position display
            ui.horizontal(|ui| {
                ui.label("Position:");
                ui.label(format!("{:.2}, {:.2}, {:.2}",
                    camera.position.x,
                    camera.position.y,
                    camera.position.z));
            });

            // Control buttons
            ui.horizontal(|ui| {
                if ui.button("Reset Camera").clicked() {
                    camera.reset();
                    changed = true;
                }

                if ui.button("Top View").clicked() {
                    camera.azimuth = 0.0;
                    camera.elevation = std::f32::consts::FRAC_PI_2 - 0.01;
                    camera.update_position();
                    changed = true;
                }

                if ui.button("Front View").clicked() {
                    camera.azimuth = 0.0;
                    camera.elevation = 0.0;
                    camera.update_position();
                    changed = true;
                }
            });

            // Instructions
            ui.add_space(5.0);
            ui.separator();
            ui.small("Controls:");
            ui.small("• Left drag: Orbit camera");
            ui.small("• Right drag: Pan camera");
            ui.small("• Scroll: Zoom in/out");
            ui.small("• Double-click: Reset view");
        });

        changed
    }
}
