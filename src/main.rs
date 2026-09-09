mod player;

use std::time::Duration;

use eframe::egui;
use egui::{Color32, RichText};
use player::Player;

const LCD_GREEN: Color32 = Color32::from_rgb(57, 255, 20);
const LCD_BG: Color32 = Color32::from_rgb(10, 20, 10);

struct WhenAmpApp {
    player: Option<Player>,
    init_error: Option<String>,
    status: String,
    seek_drag_secs: Option<f32>,
}

fn format_duration(d: Duration) -> String {
    let total_secs = d.as_secs();
    format!("{:02}:{:02}", total_secs / 60, total_secs % 60)
}

impl WhenAmpApp {
    fn new() -> Self {
        match Player::new() {
            Ok(player) => Self {
                player: Some(player),
                init_error: None,
                status: "No song loaded".to_string(),
                seek_drag_secs: None,
            },
            Err(err) => Self {
                player: None,
                init_error: Some(format!("Audio init failed: {err}")),
                status: String::new(),
                seek_drag_secs: None,
            },
        }
    }
}

impl eframe::App for WhenAmpApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let window_title = self
            .player
            .as_ref()
            .and_then(|player| player.display_name())
            .unwrap_or_else(|| "WhenAmp".to_string());
        ui.ctx()
            .send_viewport_cmd(egui::ViewportCommand::Title(window_title));

        if let Some(err) = &self.init_error {
            ui.colored_label(egui::Color32::RED, err);
            return;
        }

        let Some(player) = self.player.as_mut() else {
            return;
        };

        let has_song = player.loaded_path().is_some();
        let duration = player.duration().unwrap_or_default();
        let duration_secs = duration.as_secs_f32().max(0.001);
        let mut pos_secs = self
            .seek_drag_secs
            .unwrap_or_else(|| player.position().as_secs_f32().min(duration_secs));

        let lcd_text = if has_song {
            format_duration(Duration::from_secs_f32(pos_secs))
        } else {
            "LOAD".to_string()
        };

        ui.horizontal(|ui| {
            egui::Frame::new()
                .fill(LCD_BG)
                .corner_radius(6.0)
                .inner_margin(egui::Margin::symmetric(16, 10))
                .show(ui, |ui| {
                    ui.label(
                        RichText::new(lcd_text)
                            .monospace()
                            .size(48.0)
                            .color(LCD_GREEN),
                    );
                });

            egui::Frame::new()
                .fill(egui::Color32::from_gray(24))
                .corner_radius(6.0)
                .inner_margin(egui::Margin::symmetric(10, 10))
                .show(ui, |ui| {
                    ui.set_min_size(egui::vec2(160.0, 68.0));
                    ui.vertical(|ui| {
                        if has_song {
                            let name = player.display_name().unwrap_or_default();
                            ui.label(RichText::new(name).strong());
                            ui.label(format!("({})", format_duration(duration)));
                        } else {
                            ui.label(RichText::new("No song loaded").weak());
                        }
                    });
                });
        });

        ui.add_space(8.0);

        if ui.button("Load Song").clicked() {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("Audio", &["mp3", "wav", "flac", "ogg"])
                .pick_file()
            {
                match player.load(path.clone()) {
                    Ok(()) => {
                        self.status = format!(
                            "Loaded: {}",
                            path.file_name()
                                .map(|n| n.to_string_lossy().to_string())
                                .unwrap_or_else(|| path.display().to_string())
                        );
                    }
                    Err(err) => {
                        self.status = format!("Failed to load: {err}");
                    }
                }
            }
        }

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui
                .add_enabled(has_song, egui::Button::new("Play"))
                .clicked()
            {
                player.play();
            }
            if ui
                .add_enabled(has_song, egui::Button::new("Pause"))
                .clicked()
            {
                player.pause();
            }
            if ui
                .add_enabled(has_song, egui::Button::new("Stop"))
                .clicked()
            {
                player.stop();
            }
        });

        ui.add_space(8.0);
        let slider = egui::Slider::new(&mut pos_secs, 0.0..=duration_secs).show_value(false);
        let response = ui.add_enabled(has_song, slider);
        if response.dragged() {
            self.seek_drag_secs = Some(pos_secs);
        }
        if response.drag_stopped() {
            player.seek(Duration::from_secs_f32(pos_secs));
            self.seek_drag_secs = None;
        }

        if has_song {
            ui.ctx().request_repaint_after(Duration::from_millis(200));
        }

        ui.add_space(8.0);
        ui.label(&self.status);
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([420.0, 300.0])
            .with_resizable(false),
        ..Default::default()
    };

    eframe::run_native(
        "WhenAmp",
        options,
        Box::new(|_cc| Ok(Box::new(WhenAmpApp::new()))),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_seconds_as_mmss() {
        assert_eq!(format_duration(Duration::from_secs(0)), "00:00");
        assert_eq!(format_duration(Duration::from_secs(5)), "00:05");
        assert_eq!(format_duration(Duration::from_secs(65)), "01:05");
        assert_eq!(format_duration(Duration::from_secs(3661)), "61:01");
    }
}
