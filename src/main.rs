mod player;

use std::time::Duration;

use eframe::egui;
use egui::{Color32, RichText};
use player::Player;

const LCD_GREEN: Color32 = Color32::from_rgb(57, 255, 20);
const LCD_BG: Color32 = Color32::from_rgb(10, 20, 10);
const LCD_FONT_NAME: &str = "dseg7-classic-bold";
const TITLE_MARQUEE_WIDTH: f32 = 150.0;
const TITLE_MARQUEE_DELAY_SECS: f64 = 2.0;
const TITLE_MARQUEE_SPEED_PPS: f32 = 40.0;
const TITLE_MARQUEE_GAP: &str = "          "; // 10 chars

struct WhenAmpApp {
    player: Option<Player>,
    init_error: Option<String>,
    status: String,
    seek_drag_secs: Option<f32>,
    title_marquee_text: String,
    title_marquee_started_at: f64,
}

fn format_duration(d: Duration) -> String {
    let total_secs = d.as_secs();
    format!("{:02}:{:02}", total_secs / 60, total_secs % 60)
}

fn install_lcd_font(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        LCD_FONT_NAME.to_owned(),
        std::sync::Arc::new(egui::FontData::from_static(include_bytes!(
            "../assets/fonts/DSEG7Classic-Bold.ttf"
        ))),
    );
    fonts
        .families
        .entry(egui::FontFamily::Name(LCD_FONT_NAME.into()))
        .or_default()
        .insert(0, LCD_FONT_NAME.to_owned());
    ctx.set_fonts(fonts);
}

fn lcd_font(size: f32) -> egui::FontId {
    egui::FontId::new(size, egui::FontFamily::Name(LCD_FONT_NAME.into()))
}

#[derive(Clone, Copy)]
enum Icon {
    Load,
    Play,
    Pause,
    Stop,
}

/// A square toolbar button that draws its own vector icon (no font glyph
/// dependency, so it renders identically regardless of installed fonts).
fn icon_button(ui: &mut egui::Ui, icon: Icon) -> egui::Response {
    let size = egui::vec2(36.0, 32.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());

    if ui.is_rect_visible(rect) {
        let enabled = ui.is_enabled();
        let visuals = if enabled {
            ui.style().interact(&response)
        } else {
            &ui.style().visuals.widgets.inactive
        };

        let painter = ui.painter();
        painter.rect(
            rect,
            4.0,
            visuals.bg_fill,
            visuals.bg_stroke,
            egui::StrokeKind::Inside,
        );

        let icon_color = visuals.fg_stroke.color;
        let center = rect.center();
        let s = 12.0_f32;

        match icon {
            Icon::Play => {
                let points = vec![
                    egui::pos2(center.x - s * 0.4, center.y - s * 0.5),
                    egui::pos2(center.x - s * 0.4, center.y + s * 0.5),
                    egui::pos2(center.x + s * 0.6, center.y),
                ];
                painter.add(egui::Shape::convex_polygon(
                    points,
                    icon_color,
                    egui::Stroke::NONE,
                ));
            }
            Icon::Pause => {
                let bar_size = egui::vec2(s * 0.28, s);
                for dx in [-(s * 0.24), s * 0.24] {
                    let bar = egui::Rect::from_center_size(
                        egui::pos2(center.x + dx, center.y),
                        bar_size,
                    );
                    painter.rect_filled(bar, 1.0, icon_color);
                }
            }
            Icon::Stop => {
                let square = egui::Rect::from_center_size(center, egui::vec2(s * 0.85, s * 0.85));
                painter.rect_filled(square, 1.0, icon_color);
            }
            Icon::Load => {
                let body = egui::Rect::from_center_size(
                    egui::pos2(center.x, center.y + s * 0.08),
                    egui::vec2(s * 1.15, s * 0.8),
                );
                painter.rect_filled(body, 2.0, icon_color);
                let tab = egui::Rect::from_min_size(
                    egui::pos2(body.left() + s * 0.1, body.top() - s * 0.2),
                    egui::vec2(s * 0.45, s * 0.26),
                );
                painter.rect_filled(tab, 1.0, icon_color);
            }
        }
    }

    response
}

/// Draws `text` clipped to a single row of `width` points. If the text is too
/// wide to fit, it waits `TITLE_MARQUEE_DELAY_SECS` (measured from
/// `started_at`) and then scrolls continuously, looping with a gap between
/// repeats.
fn marquee_label(ui: &mut egui::Ui, text: &str, width: f32, started_at: f64) {
    let font_id = egui::TextStyle::Body.resolve(ui.style());
    let color = ui.visuals().text_color();
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_owned(), font_id.clone(), color);
    let text_width = galley.size().x;
    let row_height = galley.size().y;

    let (rect, _response) =
        ui.allocate_exact_size(egui::vec2(width, row_height), egui::Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }

    let painter = ui.painter_at(rect);

    if text_width <= width {
        painter.galley(rect.left_top(), galley, color);
        return;
    }

    let gap_galley = painter.layout_no_wrap(TITLE_MARQUEE_GAP.to_owned(), font_id, color);
    let cycle_width = text_width + gap_galley.size().x;

    let now = ui.ctx().input(|i| i.time);
    let elapsed = (now - started_at).max(0.0);
    let offset = if elapsed <= TITLE_MARQUEE_DELAY_SECS {
        0.0
    } else {
        (((elapsed - TITLE_MARQUEE_DELAY_SECS) as f32) * TITLE_MARQUEE_SPEED_PPS) % cycle_width
    };

    let start_x = rect.left() - offset;
    painter.galley(egui::pos2(start_x, rect.top()), galley.clone(), color);
    painter.galley(egui::pos2(start_x + cycle_width, rect.top()), galley, color);

    ui.ctx().request_repaint();
}

impl WhenAmpApp {
    fn new() -> Self {
        match Player::new() {
            Ok(player) => Self {
                player: Some(player),
                init_error: None,
                status: "No song loaded".to_string(),
                seek_drag_secs: None,
                title_marquee_text: String::new(),
                title_marquee_started_at: 0.0,
            },
            Err(err) => Self {
                player: None,
                init_error: Some(format!("Audio init failed: {err}")),
                status: String::new(),
                seek_drag_secs: None,
                title_marquee_text: String::new(),
                title_marquee_started_at: 0.0,
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

        let title_text = if has_song {
            player.display_name().unwrap_or_default()
        } else {
            "No song loaded".to_string()
        };
        if title_text != self.title_marquee_text {
            self.title_marquee_text = title_text.clone();
            self.title_marquee_started_at = ui.ctx().input(|i| i.time);
        }

        ui.horizontal(|ui| {
            egui::Frame::new()
                .fill(LCD_BG)
                .corner_radius(6.0)
                .inner_margin(egui::Margin::symmetric(16, 10))
                .show(ui, |ui| {
                    ui.label(
                        RichText::new(lcd_text)
                            .font(lcd_font(48.0))
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
                        marquee_label(
                            ui,
                            &title_text,
                            TITLE_MARQUEE_WIDTH,
                            self.title_marquee_started_at,
                        );
                        if has_song {
                            ui.label(format!("({})", format_duration(duration)));
                        }
                    });
                });
        });

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if icon_button(ui, Icon::Load)
                .on_hover_text("Load Song")
                .clicked()
            {
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

            ui.add_space(4.0);

            ui.add_enabled_ui(has_song, |ui| {
                if icon_button(ui, Icon::Play).on_hover_text("Play").clicked() {
                    player.play();
                }
                if icon_button(ui, Icon::Pause)
                    .on_hover_text("Pause")
                    .clicked()
                {
                    player.pause();
                }
                if icon_button(ui, Icon::Stop).on_hover_text("Stop").clicked() {
                    player.stop();
                }
            });
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
        Box::new(|cc| {
            install_lcd_font(&cc.egui_ctx);
            Ok(Box::new(WhenAmpApp::new()))
        }),
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
