mod player;

use player::Player;

struct WhenAmpApp {
    player: Option<Player>,
    init_error: Option<String>,
    status: String,
}

impl WhenAmpApp {
    fn new() -> Self {
        match Player::new() {
            Ok(player) => Self {
                player: Some(player),
                init_error: None,
                status: "No song loaded".to_string(),
            },
            Err(err) => Self {
                player: None,
                init_error: Some(format!("Audio init failed: {err}")),
                status: String::new(),
            },
        }
    }
}

impl eframe::App for WhenAmpApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.heading("WhenAmp");
        ui.add_space(8.0);

        if let Some(err) = &self.init_error {
            ui.colored_label(egui::Color32::RED, err);
            return;
        }

        let Some(player) = self.player.as_mut() else {
            return;
        };

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
            let has_song = player.loaded_path().is_some();
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
        ui.label(&self.status);
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([360.0, 220.0]),
        ..Default::default()
    };

    eframe::run_native(
        "WhenAmp",
        options,
        Box::new(|_cc| Ok(Box::new(WhenAmpApp::new()))),
    )
}
