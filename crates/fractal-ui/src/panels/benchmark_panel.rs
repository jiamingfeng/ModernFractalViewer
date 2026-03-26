//! In-app benchmark panel with live frame time graph and results table.

use egui::Ui;

use crate::state::UiState;

pub struct BenchmarkPanel;

impl BenchmarkPanel {
    pub fn show(ui: &mut Ui, state: &mut UiState) {
        egui::CollapsingHeader::new("Benchmark")
            .default_open(false)
            .show(ui, |ui| {
                Self::show_inner(ui, state);
            });
    }

    fn show_inner(ui: &mut Ui, state: &mut UiState) {
        if state.benchmark_running {
            // Running state: show progress and stop button
            ui.label(format!("Running: {}", state.benchmark_current_scenario));

            let progress = state.benchmark_progress;
            let bar = egui::ProgressBar::new(progress)
                .text(format!("{:.0}%", progress * 100.0));
            ui.add(bar);

            // Live frame time graph
            if !state.benchmark_frame_times.is_empty() {
                Self::draw_frame_time_graph(ui, &state.benchmark_frame_times);
            }

            if ui.button("Stop Benchmark").clicked() {
                state.benchmark_stop_requested = true;
            }
        } else {
            // Idle state: start button
            if state.export_in_progress {
                ui.label("Cannot benchmark while export is in progress.");
            } else if ui.button("Start Benchmark").clicked() {
                state.pending_benchmark = true;
                state.benchmark_results.clear();
                state.benchmark_frame_times.clear();
            }
        }

        // Results table
        if !state.benchmark_results.is_empty() {
            ui.separator();
            ui.label(format!("{} results", state.benchmark_results.len()));

            egui::ScrollArea::vertical()
                .max_height(300.0)
                .show(ui, |ui| {
                    egui::Grid::new("benchmark_results_grid")
                        .striped(true)
                        .min_col_width(60.0)
                        .show(ui, |ui| {
                            // Header
                            ui.strong("Scenario");
                            ui.strong("Avg (ms)");
                            ui.strong("Min (ms)");
                            ui.strong("Max (ms)");
                            ui.strong("P99 (ms)");
                            ui.strong("FPS");
                            ui.end_row();

                            for r in &state.benchmark_results {
                                ui.label(&r.scenario);
                                ui.label(format!("{:.2}", r.avg_ms));
                                ui.label(format!("{:.2}", r.min_ms));
                                ui.label(format!("{:.2}", r.max_ms));
                                ui.label(format!("{:.2}", r.p99_ms));
                                ui.label(format!("{:.1}", r.avg_fps));
                                ui.end_row();
                            }
                        });
                });

            if ui.button("Clear Results").clicked() {
                state.benchmark_results.clear();
            }
        }
    }

    fn draw_frame_time_graph(ui: &mut Ui, times: &[f64]) {
        let desired_size = egui::vec2(ui.available_width(), 80.0);
        let (rect, _) = ui.allocate_exact_size(desired_size, egui::Sense::hover());

        if rect.width() <= 0.0 || rect.height() <= 0.0 || times.is_empty() {
            return;
        }

        let painter = ui.painter_at(rect);

        // Compute Y range
        let max_ms = times
            .iter()
            .copied()
            .fold(0.0f64, f64::max)
            .max(1.0); // min 1ms for scale

        let n = times.len();
        let x_step = rect.width() / (n.max(1) as f32);

        // Background
        painter.rect_filled(rect, 2.0, egui::Color32::from_gray(20));

        // Reference lines at 16.67ms (60fps) and 33.33ms (30fps) if in range
        for (ms, color) in [
            (16.67, egui::Color32::from_rgba_unmultiplied(100, 100, 100, 80)),
            (33.33, egui::Color32::from_rgba_unmultiplied(100, 50, 50, 80)),
        ] {
            if ms < max_ms {
                let y = rect.bottom() - (ms as f32 / max_ms as f32) * rect.height();
                painter.line_segment(
                    [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
                    egui::Stroke::new(1.0, color),
                );
            }
        }

        // Frame time line
        if n >= 2 {
            let points: Vec<egui::Pos2> = times
                .iter()
                .enumerate()
                .map(|(i, &t)| {
                    let x = rect.left() + i as f32 * x_step;
                    let y = rect.bottom() - (t as f32 / max_ms as f32) * rect.height();
                    egui::pos2(x, y.max(rect.top()))
                })
                .collect();

            for window in points.windows(2) {
                let t = times[(&window[0].x - rect.left()) as usize / x_step.max(0.001) as usize];
                let color = if t < 16.67 {
                    egui::Color32::from_rgb(80, 200, 80) // green: under 60fps budget
                } else if t < 33.33 {
                    egui::Color32::from_rgb(200, 200, 50) // yellow: 30-60fps
                } else {
                    egui::Color32::from_rgb(200, 60, 60) // red: over 30fps budget
                };
                painter.line_segment(
                    [window[0], window[1]],
                    egui::Stroke::new(1.5, color),
                );
            }
        }

        // Label
        if let Some(&last) = times.last() {
            painter.text(
                egui::pos2(rect.left() + 4.0, rect.top() + 2.0),
                egui::Align2::LEFT_TOP,
                format!("{:.2}ms ({:.0} FPS)", last, 1000.0 / last.max(0.001)),
                egui::FontId::proportional(10.0),
                egui::Color32::from_gray(180),
            );
        }
    }
}
