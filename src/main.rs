mod player;

use std::time::Duration;

use eframe::egui;
use egui::{Color32, Rect, Response, Sense, Stroke, Ui, Vec2};
use player::{Player, VISUALIZER_BARS};

// Palette lifted from the "SONIC DECK" Claude Design mockup (Graphite chrome).
const FACE: Color32 = Color32::from_rgb(0x2a, 0x2c, 0x31);
const BEVEL_HI: Color32 = Color32::from_rgb(0x5d, 0x62, 0x6d);
const BEVEL_LO: Color32 = Color32::from_rgb(0x0c, 0x0d, 0x10);
const TITLE_BAR_BG: Color32 = Color32::from_rgb(0x3c, 0x40, 0x49);
const INK: Color32 = Color32::from_rgb(0xcf, 0xd4, 0xdd);
const LABEL_GRAY: Color32 = Color32::from_rgb(0xc9, 0xce, 0xd8);
const DIM_GRAY: Color32 = Color32::from_rgb(0x8d, 0x93, 0x9e);
const INACTIVE: Color32 = Color32::from_rgb(0x7b, 0x80, 0x89);
const CLOSE_RED: Color32 = Color32::from_rgb(0xe0, 0xb3, 0xb3);
const LCD_GREEN: Color32 = Color32::from_rgb(0x4e, 0xe3, 0x9a);
const LCD_PANEL_BG: Color32 = Color32::from_rgb(0x04, 0x07, 0x0a);
const VIS_CANVAS_BG: Color32 = Color32::from_rgb(0x01, 0x04, 0x04);
const TRACK_BG: Color32 = Color32::from_rgb(0x10, 0x12, 0x16);

const LCD_FONT_NAME: &str = "dseg7-classic-bold";
const SILKSCREEN_FONT_NAME: &str = "silkscreen";

const CHASSIS_WIDTH: f32 = 560.0;
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
    remaining_mode: bool,
    viz_bars: [f32; VISUALIZER_BARS],
    eq_on: bool,
    pl_on: bool,
    shuffle_on: bool,
    repeat_on: bool,
    is_playing: bool,
    last_window_size: Option<Vec2>,
    micro_mode: bool,
}

fn format_duration(d: Duration) -> String {
    let total_secs = d.as_secs();
    format!("{:02}:{:02}", total_secs / 60, total_secs % 60)
}

fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        LCD_FONT_NAME.to_owned(),
        std::sync::Arc::new(egui::FontData::from_static(include_bytes!(
            "../assets/fonts/DSEG7Classic-Bold.ttf"
        ))),
    );
    fonts.font_data.insert(
        SILKSCREEN_FONT_NAME.to_owned(),
        std::sync::Arc::new(egui::FontData::from_static(include_bytes!(
            "../assets/fonts/Silkscreen-Regular.ttf"
        ))),
    );
    fonts.font_data.insert(
        "ibm-plex-mono".to_owned(),
        std::sync::Arc::new(egui::FontData::from_static(include_bytes!(
            "../assets/fonts/IBMPlexMono-Regular.ttf"
        ))),
    );

    fonts
        .families
        .entry(egui::FontFamily::Name(LCD_FONT_NAME.into()))
        .or_default()
        .insert(0, LCD_FONT_NAME.to_owned());
    fonts
        .families
        .entry(egui::FontFamily::Name(SILKSCREEN_FONT_NAME.into()))
        .or_default()
        .insert(0, SILKSCREEN_FONT_NAME.to_owned());

    // Make IBM Plex Mono the base UI typeface, matching the mockup.
    fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .insert(0, "ibm-plex-mono".to_owned());
    fonts
        .families
        .entry(egui::FontFamily::Monospace)
        .or_default()
        .insert(0, "ibm-plex-mono".to_owned());

    ctx.set_fonts(fonts);
}

fn lcd_font(size: f32) -> egui::FontId {
    egui::FontId::new(size, egui::FontFamily::Name(LCD_FONT_NAME.into()))
}

fn silkscreen_font(size: f32) -> egui::FontId {
    egui::FontId::new(size, egui::FontFamily::Name(SILKSCREEN_FONT_NAME.into()))
}

/// Draws a beveled rectangle: raised (button/chassis) or sunken (recessed
/// panel/track), matching the mockup's inset/outset box-shadow chrome.
fn bevel_rect(ui: &Ui, rect: Rect, fill: Color32, raised: bool) {
    let painter = ui.painter();
    painter.rect_filled(rect, 0.0, fill);
    let (top_left, bottom_right) = if raised {
        (BEVEL_HI, BEVEL_LO)
    } else {
        (BEVEL_LO, BEVEL_HI)
    };
    painter.line_segment(
        [rect.left_top(), rect.right_top()],
        Stroke::new(1.0, top_left),
    );
    painter.line_segment(
        [rect.left_top(), rect.left_bottom()],
        Stroke::new(1.0, top_left),
    );
    painter.line_segment(
        [rect.right_top(), rect.right_bottom()],
        Stroke::new(1.0, bottom_right),
    );
    painter.line_segment(
        [rect.left_bottom(), rect.right_bottom()],
        Stroke::new(1.0, bottom_right),
    );
}

/// A horizontal track-and-handle slider drawn in the chassis chrome style.
/// Returns the response (drag it) plus the value implied by the current
/// pointer position while being dragged.
fn bevel_slider(ui: &mut Ui, size: Vec2, value: f32, fill: Color32) -> (Response, Option<f32>) {
    let (rect, response) = ui.allocate_exact_size(size, Sense::click_and_drag());
    bevel_rect(ui, rect, TRACK_BG, false);

    let value = value.clamp(0.0, 1.0);
    let filled_w = rect.width() * value;
    if filled_w > 0.0 {
        let filled_rect = Rect::from_min_size(rect.left_top(), Vec2::new(filled_w, rect.height()));
        ui.painter().rect_filled(filled_rect, 0.0, fill);
    }

    let handle_size = Vec2::new(12.0, rect.height() + 10.0);
    let handle_center = egui::pos2(rect.left() + filled_w, rect.center().y);
    let handle_rect = Rect::from_center_size(handle_center, handle_size);
    bevel_rect(ui, handle_rect, FACE, true);

    let drag_value = response.dragged().then(|| {
        response
            .interact_pointer_pos()
            .map(|pos| ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0))
    });

    (response, drag_value.flatten())
}

/// Draws the seekbar: same chrome as `bevel_slider` but with a dashed fill
/// and a wider handle, matching the mockup.
fn seek_bar(ui: &mut Ui, size: Vec2, value: f32) -> (Response, Option<f32>) {
    let (rect, response) = ui.allocate_exact_size(size, Sense::drag());
    bevel_rect(ui, rect, TRACK_BG, false);

    let value = value.clamp(0.0, 1.0);
    let filled_w = rect.width() * value;
    let painter = ui.painter();
    let mut x = rect.left();
    while x < rect.left() + filled_w {
        let dash = Rect::from_min_size(
            egui::pos2(x, rect.top()),
            Vec2::new(2.0_f32.min(rect.left() + filled_w - x), rect.height()),
        );
        painter.rect_filled(dash, 0.0, LCD_GREEN.gamma_multiply(0.55));
        x += 4.0;
    }

    let handle_size = Vec2::new(20.0, rect.height() + 6.0);
    let handle_center = egui::pos2(rect.left() + filled_w, rect.center().y);
    let handle_rect = Rect::from_center_size(handle_center, handle_size);
    bevel_rect(ui, handle_rect, FACE, true);

    let drag_value = response.dragged().then(|| {
        response
            .interact_pointer_pos()
            .map(|pos| ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0))
    });

    (response, drag_value.flatten())
}

#[derive(Clone, Copy)]
enum Icon {
    Eject,
    Prev,
    Play,
    Pause,
    Stop,
    Next,
}

const DEFAULT_ICON_BUTTON_SIZE: Vec2 = Vec2::new(36.0, 28.0);

/// A square toolbar button in the chassis chrome, drawing its own vector
/// icon (no font glyph dependency).
fn icon_button(ui: &mut Ui, icon: Icon) -> Response {
    icon_button_sized(ui, icon, DEFAULT_ICON_BUTTON_SIZE)
}

/// Same as [`icon_button`] but at a caller-chosen size (e.g. the compact
/// mini-player's smaller transport buttons).
fn icon_button_sized(ui: &mut Ui, icon: Icon, size: Vec2) -> Response {
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());

    if ui.is_rect_visible(rect) {
        let raised = !(ui.is_enabled() && response.is_pointer_button_down_on());
        bevel_rect(ui, rect, FACE, raised);

        let icon_color = if ui.is_enabled() { INK } else { INACTIVE };
        let play_color = if ui.is_enabled() { LCD_GREEN } else { INACTIVE };
        let painter = ui.painter();
        let center = rect.center();
        let s = size.y.min(size.x) * 0.4;

        match icon {
            Icon::Play => {
                let points = vec![
                    egui::pos2(center.x - s * 0.4, center.y - s * 0.55),
                    egui::pos2(center.x - s * 0.4, center.y + s * 0.55),
                    egui::pos2(center.x + s * 0.65, center.y),
                ];
                painter.add(egui::Shape::convex_polygon(points, play_color, Stroke::NONE));
            }
            Icon::Pause => {
                let bar_size = Vec2::new(s * 0.28, s * 1.1);
                for dx in [-(s * 0.24), s * 0.24] {
                    let bar =
                        Rect::from_center_size(egui::pos2(center.x + dx, center.y), bar_size);
                    painter.rect_filled(bar, 1.0, icon_color);
                }
            }
            Icon::Stop => {
                let square = Rect::from_center_size(center, Vec2::splat(s * 0.85));
                painter.rect_filled(square, 1.0, icon_color);
            }
            Icon::Prev => {
                let bar = Rect::from_center_size(
                    egui::pos2(center.x - s * 0.45, center.y),
                    Vec2::new(s * 0.22, s * 1.05),
                );
                painter.rect_filled(bar, 0.0, icon_color);
                let points = vec![
                    egui::pos2(center.x + s * 0.5, center.y - s * 0.55),
                    egui::pos2(center.x + s * 0.5, center.y + s * 0.55),
                    egui::pos2(center.x - s * 0.2, center.y),
                ];
                painter.add(egui::Shape::convex_polygon(points, icon_color, Stroke::NONE));
            }
            Icon::Next => {
                let points = vec![
                    egui::pos2(center.x - s * 0.5, center.y - s * 0.55),
                    egui::pos2(center.x - s * 0.5, center.y + s * 0.55),
                    egui::pos2(center.x + s * 0.2, center.y),
                ];
                painter.add(egui::Shape::convex_polygon(points, icon_color, Stroke::NONE));
                let bar = Rect::from_center_size(
                    egui::pos2(center.x + s * 0.45, center.y),
                    Vec2::new(s * 0.22, s * 1.05),
                );
                painter.rect_filled(bar, 0.0, icon_color);
            }
            Icon::Eject => {
                let points = vec![
                    egui::pos2(center.x, center.y - s * 0.55),
                    egui::pos2(center.x + s * 0.55, center.y + s * 0.15),
                    egui::pos2(center.x - s * 0.55, center.y + s * 0.15),
                ];
                painter.add(egui::Shape::convex_polygon(points, icon_color, Stroke::NONE));
                let bar = Rect::from_min_size(
                    egui::pos2(center.x - s * 0.55, center.y + s * 0.4),
                    Vec2::new(s * 1.1, s * 0.22),
                );
                painter.rect_filled(bar, 0.0, icon_color);
            }
        }
    }

    response
}

/// A small pixel-label toggle button (EQ / PL / SHUFFLE / REPEAT), styled
/// like the mockup's chrome tabs.
fn toggle_label_button(ui: &mut Ui, label: &str, active: bool, width: f32) -> Response {
    let size = Vec2::new(width, 22.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    if ui.is_rect_visible(rect) {
        bevel_rect(ui, rect, FACE, true);
        let color = if active { LCD_GREEN } else { INACTIVE };
        let galley = ui
            .painter()
            .layout_no_wrap(label.to_owned(), silkscreen_font(9.0), color);
        let pos = (rect.center() - galley.size() / 2.0).round();
        ui.painter().galley(pos, galley, color);
    }
    response
}

/// Draws `text` clipped to a single row of `width` points. If the text is too
/// wide to fit, it waits `TITLE_MARQUEE_DELAY_SECS` (measured from
/// `started_at`) and then scrolls continuously, looping with a gap between
/// repeats.
fn marquee_label(ui: &mut Ui, text: &str, width: f32, color: Color32, started_at: f64) {
    let font_id = egui::FontId::new(13.0, egui::FontFamily::Proportional);
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_owned(), font_id.clone(), color);
    let text_width = galley.size().x;
    let row_height = galley.size().y;

    let (rect, _response) = ui.allocate_exact_size(Vec2::new(width, row_height), Sense::hover());
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

fn visualizer(ui: &mut Ui, size: Vec2, bars: &[f32; VISUALIZER_BARS]) {
    let (rect, _response) = ui.allocate_exact_size(size, Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter();
    painter.rect_filled(rect, 0.0, VIS_CANVAS_BG);
    painter.rect_stroke(
        rect,
        0.0,
        Stroke::new(1.0, LCD_GREEN.gamma_multiply(0.14)),
        egui::StrokeKind::Inside,
    );

    let n = bars.len() as f32;
    let bar_w = rect.width() / n;
    for (i, level) in bars.iter().enumerate() {
        let h = (level.clamp(0.0, 1.0) * (rect.height() - 6.0)).max(1.0);
        let bar_rect = Rect::from_min_size(
            egui::pos2(rect.left() + i as f32 * bar_w + 1.0, rect.bottom() - h),
            Vec2::new((bar_w - 2.0).max(1.0), h),
        );
        painter.rect_filled(bar_rect, 0.0, LCD_GREEN.gamma_multiply(0.9));
    }
}

impl WhenAmpApp {
    fn new() -> Self {
        let base = Self {
            player: None,
            init_error: None,
            status: "No song loaded".to_string(),
            seek_drag_secs: None,
            title_marquee_text: String::new(),
            title_marquee_started_at: 0.0,
            remaining_mode: false,
            viz_bars: [0.0; VISUALIZER_BARS],
            eq_on: true,
            pl_on: false,
            shuffle_on: false,
            repeat_on: false,
            is_playing: false,
            last_window_size: None,
            micro_mode: false,
        };
        match Player::new() {
            Ok(player) => Self {
                player: Some(player),
                ..base
            },
            Err(err) => Self {
                init_error: Some(format!("Audio init failed: {err}")),
                status: String::new(),
                ..base
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

        {

            let has_song = player.loaded_path().is_some();
            let duration = player.duration().unwrap_or_default();
            let duration_secs = duration.as_secs_f32().max(0.001);
            let mut pos_secs = self
                .seek_drag_secs
                .unwrap_or_else(|| player.position().as_secs_f32().min(duration_secs));

            if self.repeat_on && has_song && pos_secs >= duration_secs - 0.05 {
                player.seek(Duration::ZERO);
                player.play();
                self.is_playing = true;
                pos_secs = 0.0;
            }

            if has_song && ui.ctx().input(|i| i.key_pressed(egui::Key::Space)) {
                if self.is_playing {
                    player.pause();
                    self.is_playing = false;
                } else {
                    player.play();
                    self.is_playing = true;
                }
            }

            let title_text = if has_song {
                player.display_name().unwrap_or_default()
            } else {
                "No song loaded".to_string()
            };
            if title_text != self.title_marquee_text {
                self.title_marquee_text = title_text.clone();
                self.title_marquee_started_at = ui.ctx().input(|i| i.time);
            }

            let info = player.track_info();
            let elapsed_secs = Duration::from_secs_f32(pos_secs);
            let shown = if self.remaining_mode {
                Duration::from_secs_f32((duration_secs - pos_secs).max(0.0))
            } else {
                elapsed_secs
            };
            let lcd_text = if has_song {
                format_duration(shown)
            } else {
                "LOAD".to_string()
            };

            // Outer chassis, sized to exactly fill the (undecorated) window.
            let chassis_frame = egui::Frame::new()
                .fill(FACE)
                .inner_margin(egui::Margin::same(3));
            let chassis_response = chassis_frame.show(ui, |ui| {
                    ui.set_width(CHASSIS_WIDTH);

                    if self.micro_mode {
                        // Same height as the full-mode title strip.
                        let row_size = Vec2::new(CHASSIS_WIDTH, 24.0);
                        let (row_rect, row_drag) =
                            ui.allocate_exact_size(row_size, Sense::click_and_drag());
                        if row_drag.drag_started() {
                            ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
                        }
                        ui.painter().rect_filled(row_rect, 0.0, TITLE_BAR_BG);

                        ui.scope_builder(
                            egui::UiBuilder::new()
                                .max_rect(row_rect.shrink2(Vec2::new(6.0, 1.0))),
                            |ui| {
                                ui.with_layout(
                                    egui::Layout::left_to_right(egui::Align::Center),
                                    |ui| {
                                        let (dash_rect, dash_resp) = ui.allocate_exact_size(
                                            Vec2::new(16.0, 14.0),
                                            Sense::click(),
                                        );
                                        bevel_rect(ui, dash_rect, FACE, true);
                                        let dash = Rect::from_center_size(
                                            egui::pos2(
                                                dash_rect.center().x,
                                                dash_rect.bottom() - 4.0,
                                            ),
                                            Vec2::new(8.0, 2.0),
                                        );
                                        ui.painter().rect_filled(dash, 0.0, INK);
                                        if dash_resp.on_hover_text("Minimize").clicked() {
                                            ui.ctx().send_viewport_cmd(
                                                egui::ViewportCommand::Minimized(true),
                                            );
                                        }

                                        ui.add_space(2.0);
                                        let (restore_rect, restore_resp) = ui.allocate_exact_size(
                                            Vec2::new(16.0, 14.0),
                                            Sense::click(),
                                        );
                                        bevel_rect(ui, restore_rect, FACE, true);
                                        let inner = restore_rect.shrink(4.0);
                                        ui.painter().rect_stroke(
                                            inner,
                                            0.0,
                                            Stroke::new(1.0, INK),
                                            egui::StrokeKind::Inside,
                                        );
                                        if restore_resp.on_hover_text("Restore").clicked() {
                                            self.micro_mode = false;
                                        }

                                        ui.add_space(2.0);
                                        let (close_rect, close_resp) = ui.allocate_exact_size(
                                            Vec2::new(16.0, 14.0),
                                            Sense::click(),
                                        );
                                        bevel_rect(
                                            ui,
                                            close_rect,
                                            FACE,
                                            !close_resp.is_pointer_button_down_on(),
                                        );
                                        let c = close_rect.center();
                                        ui.painter().line_segment(
                                            [c + Vec2::new(-3.0, -3.0), c + Vec2::new(3.0, 3.0)],
                                            Stroke::new(1.0, CLOSE_RED),
                                        );
                                        ui.painter().line_segment(
                                            [c + Vec2::new(-3.0, 3.0), c + Vec2::new(3.0, -3.0)],
                                            Stroke::new(1.0, CLOSE_RED),
                                        );
                                        if close_resp.clicked() {
                                            ui.ctx()
                                                .send_viewport_cmd(egui::ViewportCommand::Close);
                                        }

                                        ui.add_space(8.0);
                                        ui.label(
                                            egui::RichText::new(lcd_text.clone())
                                                .font(lcd_font(14.0))
                                                .color(LCD_GREEN),
                                        );

                                        ui.add_space(8.0);
                                        let btn_size = Vec2::new(18.0, 16.0);
                                        ui.add_enabled_ui(has_song, |ui| {
                                            if icon_button_sized(ui, Icon::Play, btn_size)
                                                .on_hover_text("Play")
                                                .clicked()
                                            {
                                                player.play();
                                                self.is_playing = true;
                                            }
                                            if icon_button_sized(ui, Icon::Pause, btn_size)
                                                .on_hover_text("Pause")
                                                .clicked()
                                            {
                                                player.pause();
                                                self.is_playing = false;
                                            }
                                            if icon_button_sized(ui, Icon::Stop, btn_size)
                                                .on_hover_text("Stop")
                                                .clicked()
                                            {
                                                player.stop();
                                                self.is_playing = false;
                                            }
                                        });

                                        ui.add_space(8.0);
                                        let spectrum_width = 90.0;
                                        let ticker_width =
                                            (ui.available_width() - spectrum_width - 8.0).max(40.0);
                                        marquee_label(
                                            ui,
                                            &title_text,
                                            ticker_width,
                                            LCD_GREEN,
                                            self.title_marquee_started_at,
                                        );

                                        ui.add_space(8.0);
                                        visualizer(
                                            ui,
                                            Vec2::new(spectrum_width, 16.0),
                                            &self.viz_bars,
                                        );
                                    },
                                );
                            },
                        );
                        return;
                    }

                    // Title strip. Doubles as the window's drag handle, since
                    // decorations are off and this is the only chrome we have.
                    let (title_rect, title_drag) = ui.allocate_exact_size(
                        Vec2::new(CHASSIS_WIDTH, 24.0),
                        Sense::click_and_drag(),
                    );
                    if title_drag.drag_started() {
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
                    }
                    ui.painter().rect_filled(title_rect, 0.0, TITLE_BAR_BG);
                    ui.scope_builder(
                        egui::UiBuilder::new().max_rect(title_rect),
                        |ui| {
                            ui.horizontal(|ui| {
                                ui.add_space(8.0);
                                let (led_rect, _) =
                                    ui.allocate_exact_size(Vec2::splat(8.0), Sense::hover());
                                ui.painter().circle_filled(
                                    led_rect.center(),
                                    5.0,
                                    LCD_GREEN.gamma_multiply(0.35),
                                );
                                ui.painter().circle_filled(led_rect.center(), 3.0, LCD_GREEN);

                                ui.add_space(4.0);
                                let label_galley = ui.painter().layout_no_wrap(
                                    "W H E N A M P   ·   1 . 0".to_string(),
                                    silkscreen_font(10.0),
                                    LABEL_GRAY,
                                );
                                let remaining =
                                    title_rect.width() - 8.0 - 8.0 - 4.0 - 3.0 * 20.0 - 16.0;
                                let text_x = title_rect.left()
                                    + 8.0
                                    + 8.0
                                    + 4.0
                                    + (remaining - label_galley.size().x).max(0.0) / 2.0;
                                ui.painter().galley(
                                    egui::pos2(
                                        text_x,
                                        title_rect.center().y - label_galley.size().y / 2.0,
                                    ),
                                    label_galley,
                                    LABEL_GRAY,
                                );

                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    ui.add_space(4.0);
                                    let (close_rect, close_resp) = ui
                                        .allocate_exact_size(Vec2::new(16.0, 14.0), Sense::click());
                                    bevel_rect(ui, close_rect, FACE, !close_resp.is_pointer_button_down_on());
                                    let c = close_rect.center();
                                    ui.painter().line_segment(
                                        [c + Vec2::new(-3.0, -3.0), c + Vec2::new(3.0, 3.0)],
                                        Stroke::new(1.0, CLOSE_RED),
                                    );
                                    ui.painter().line_segment(
                                        [c + Vec2::new(-3.0, 3.0), c + Vec2::new(3.0, -3.0)],
                                        Stroke::new(1.0, CLOSE_RED),
                                    );
                                    if close_resp.clicked() {
                                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                                    }

                                    ui.add_space(3.0);
                                    let (sq_rect, sq_resp) = ui
                                        .allocate_exact_size(Vec2::new(16.0, 14.0), Sense::click());
                                    bevel_rect(ui, sq_rect, FACE, true);
                                    let inner = sq_rect.shrink(4.0);
                                    ui.painter().rect_stroke(
                                        inner,
                                        0.0,
                                        Stroke::new(1.0, INK),
                                        egui::StrokeKind::Inside,
                                    );
                                    if sq_resp.on_hover_text("Mini player").clicked() {
                                        self.micro_mode = !self.micro_mode;
                                    }

                                    ui.add_space(3.0);
                                    let (dash_rect, dash_resp) = ui
                                        .allocate_exact_size(Vec2::new(16.0, 14.0), Sense::click());
                                    bevel_rect(ui, dash_rect, FACE, true);
                                    let dash = Rect::from_center_size(
                                        egui::pos2(dash_rect.center().x, dash_rect.bottom() - 4.0),
                                        Vec2::new(8.0, 2.0),
                                    );
                                    ui.painter().rect_filled(dash, 0.0, INK);
                                    if dash_resp.on_hover_text("Minimize").clicked() {
                                        ui.ctx()
                                            .send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                                    }
                                });
                            });
                        },
                    );

                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.add_space(8.0);
                        ui.vertical(|ui| {
                            ui.set_width(CHASSIS_WIDTH - 16.0);

                            // LCD row: time+badges on the left, visualizer+marquee on the right.
                            let lcd_size = Vec2::new(CHASSIS_WIDTH - 16.0, 104.0);
                            let (lcd_rect, _) = ui.allocate_exact_size(lcd_size, Sense::hover());
                            bevel_rect(ui, lcd_rect, LCD_PANEL_BG, false);

                            ui.scope_builder(
                                egui::UiBuilder::new().max_rect(lcd_rect.shrink(8.0)),
                                |ui| {
                                    ui.horizontal(|ui| {
                                        ui.vertical(|ui| {
                                            ui.set_width(178.0);
                                            let time_resp = ui.horizontal(|ui| {
                                                let galley = ui.painter().layout_no_wrap(
                                                    lcd_text.clone(),
                                                    lcd_font(40.0),
                                                    LCD_GREEN,
                                                );
                                                let (rect, resp) = ui.allocate_exact_size(
                                                    galley.size(),
                                                    Sense::click(),
                                                );
                                                ui.painter().galley(
                                                    rect.left_top(),
                                                    galley,
                                                    LCD_GREEN,
                                                );
                                                ui.add_space(4.0);
                                                ui.label(
                                                    egui::RichText::new(if self.remaining_mode {
                                                        "REM"
                                                    } else {
                                                        "ELAPSED"
                                                    })
                                                    .font(silkscreen_font(9.0))
                                                    .color(LCD_GREEN.gamma_multiply(0.6)),
                                                );
                                                resp
                                            });
                                            if time_resp.inner.clicked() {
                                                self.remaining_mode = !self.remaining_mode;
                                            }

                                            ui.add_space(6.0);
                                            ui.horizontal(|ui| {
                                                let kbps_text = info
                                                    .kbps
                                                    .map(|k| format!("{k} KBPS"))
                                                    .unwrap_or_else(|| "-- KBPS".to_string());
                                                let khz_text = info
                                                    .sample_rate_hz
                                                    .map(|hz| format!("{} KHZ", hz / 1000))
                                                    .unwrap_or_else(|| "-- KHZ".to_string());

                                                let (kbps_rect, _) = ui.allocate_exact_size(
                                                    Vec2::new(66.0, 14.0),
                                                    Sense::hover(),
                                                );
                                                ui.painter().rect_filled(kbps_rect, 0.0, LCD_GREEN);
                                                let g = ui.painter().layout_no_wrap(
                                                    kbps_text,
                                                    silkscreen_font(9.0),
                                                    LCD_PANEL_BG,
                                                );
                                                let pos = (kbps_rect.center()
                                                    - g.size() / 2.0
                                                    - Vec2::new(0.0, 4.0))
                                                .round();
                                                ui.painter().galley(pos, g, LCD_PANEL_BG);

                                                ui.add_space(4.0);
                                                let (khz_rect, _) = ui.allocate_exact_size(
                                                    Vec2::new(48.0, 14.0),
                                                    Sense::hover(),
                                                );
                                                ui.painter().rect_stroke(
                                                    khz_rect,
                                                    0.0,
                                                    Stroke::new(1.0, LCD_GREEN.gamma_multiply(0.4)),
                                                    egui::StrokeKind::Inside,
                                                );
                                                let g = ui.painter().layout_no_wrap(
                                                    khz_text,
                                                    silkscreen_font(9.0),
                                                    LCD_GREEN,
                                                );
                                                let pos = (khz_rect.center()
                                                    - g.size() / 2.0
                                                    - Vec2::new(0.0, 4.0))
                                                .round();
                                                ui.painter().galley(pos, g, LCD_GREEN);

                                                ui.add_space(4.0);
                                                let stereo_on = info.channels.unwrap_or(1) >= 2;
                                                let (st_rect, _) = ui.allocate_exact_size(
                                                    Vec2::new(52.0, 14.0),
                                                    Sense::hover(),
                                                );
                                                let alpha = if has_song && stereo_on {
                                                    1.0
                                                } else {
                                                    0.25
                                                };
                                                ui.painter().rect_filled(
                                                    st_rect,
                                                    0.0,
                                                    LCD_GREEN.gamma_multiply(alpha),
                                                );
                                                let g = ui.painter().layout_no_wrap(
                                                    "STEREO".to_string(),
                                                    silkscreen_font(9.0),
                                                    LCD_PANEL_BG,
                                                );
                                                let pos = (st_rect.center()
                                                    - g.size() / 2.0
                                                    - Vec2::new(0.0, 4.0))
                                                .round();
                                                ui.painter().galley(pos, g, LCD_PANEL_BG);
                                            });
                                        });

                                        ui.add_space(8.0);
                                        ui.vertical(|ui| {
                                            let remaining_w = ui.available_width();
                                            visualizer(
                                                ui,
                                                Vec2::new(remaining_w, 60.0),
                                                &self.viz_bars,
                                            );
                                            ui.add_space(8.0);
                                            marquee_label(
                                                ui,
                                                &title_text,
                                                remaining_w,
                                                LCD_GREEN,
                                                self.title_marquee_started_at,
                                            );
                                        });
                                    });
                                },
                            );

                            ui.add_space(8.0);

                            // Volume / balance / EQ / PL row.
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new("VOL")
                                        .font(silkscreen_font(8.0))
                                        .color(DIM_GRAY),
                                );
                                let vol = player.volume();
                                let (_resp, drag) = bevel_slider(
                                    ui,
                                    Vec2::new(150.0, 6.0),
                                    vol,
                                    LCD_GREEN,
                                );
                                if let Some(v) = drag {
                                    player.set_volume(v);
                                }

                                ui.add_space(6.0);
                                ui.label(
                                    egui::RichText::new("BAL")
                                        .font(silkscreen_font(8.0))
                                        .color(DIM_GRAY),
                                );
                                let bal = player.balance();
                                let (bal_resp, drag) = bevel_slider(
                                    ui,
                                    Vec2::new(90.0, 6.0),
                                    bal,
                                    LCD_GREEN,
                                );
                                if let Some(v) = drag {
                                    player.set_balance(v);
                                }
                                if bal_resp.double_clicked() {
                                    player.set_balance(0.5);
                                }

                                ui.add_space(6.0);
                                if toggle_label_button(ui, "EQ", self.eq_on, 34.0).clicked() {
                                    self.eq_on = !self.eq_on;
                                }
                                ui.add_space(3.0);
                                if toggle_label_button(ui, "PL", self.pl_on, 34.0).clicked() {
                                    self.pl_on = !self.pl_on;
                                }
                            });

                            ui.add_space(8.0);

                            // Seek bar, inset 5px further on each side than the
                            // content column so it doesn't crowd the chassis edge.
                            let seek_value = pos_secs / duration_secs;
                            let (seek_resp, seek_drag) = ui
                                .horizontal(|ui| {
                                    ui.add_space(5.0);
                                    seek_bar(ui, Vec2::new(CHASSIS_WIDTH - 26.0, 10.0), seek_value)
                                })
                                .inner;
                            if has_song {
                                if let Some(v) = seek_drag {
                                    self.seek_drag_secs = Some(v * duration_secs);
                                }
                                if seek_resp.drag_stopped() {
                                    if let Some(v) = self.seek_drag_secs {
                                        player.seek(Duration::from_secs_f32(v));
                                    }
                                    self.seek_drag_secs = None;
                                }
                            }

                            ui.add_space(8.0);

                            // Transport row.
                            ui.horizontal(|ui| {
                                if icon_button(ui, Icon::Eject)
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
                                                self.is_playing = false;
                                            }
                                            Err(err) => {
                                                self.status = format!("Failed to load: {err}");
                                            }
                                        }
                                    }
                                }
                                ui.add_space(4.0);

                                ui.add_enabled_ui(has_song, |ui| {
                                    if icon_button(ui, Icon::Prev)
                                        .on_hover_text("Restart")
                                        .clicked()
                                    {
                                        player.seek(Duration::ZERO);
                                    }
                                    if icon_button(ui, Icon::Play).on_hover_text("Play").clicked()
                                    {
                                        player.play();
                                        self.is_playing = true;
                                    }
                                    if icon_button(ui, Icon::Pause)
                                        .on_hover_text("Pause")
                                        .clicked()
                                    {
                                        player.pause();
                                        self.is_playing = false;
                                    }
                                    if icon_button(ui, Icon::Stop).on_hover_text("Stop").clicked()
                                    {
                                        player.stop();
                                        self.is_playing = false;
                                    }
                                    if icon_button(ui, Icon::Next)
                                        .on_hover_text("Restart")
                                        .clicked()
                                    {
                                        player.seek(Duration::ZERO);
                                    }
                                });

                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if toggle_label_button(
                                            ui,
                                            "REPEAT",
                                            self.repeat_on,
                                            60.0,
                                        )
                                        .clicked()
                                        {
                                            self.repeat_on = !self.repeat_on;
                                        }
                                        ui.add_space(3.0);
                                        if toggle_label_button(
                                            ui,
                                            "SHUFFLE",
                                            self.shuffle_on,
                                            60.0,
                                        )
                                        .clicked()
                                        {
                                            self.shuffle_on = !self.shuffle_on;
                                        }
                                    },
                                );
                            });
                        });
                    });

                    ui.add_space(8.0);
                });

            // Snap the (undecorated, non-resizable) window to exactly fit the
            // chassis, so there's no leftover background around it.
            let desired_size = chassis_response.response.rect.size();
            if self
                .last_window_size
                .is_none_or(|last| (last - desired_size).length() > 0.5)
            {
                self.last_window_size = Some(desired_size);
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::InnerSize(desired_size));
            }

            self.viz_bars = player.sample_visualizer();

            if has_song {
                ui.ctx().request_repaint_after(Duration::from_millis(33));
            }
        }
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        FACE.to_normalized_gamma_f32()
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([566.0, 260.0])
            .with_resizable(false)
            .with_decorations(false)
            .with_transparent(false),
        ..Default::default()
    };

    eframe::run_native(
        "WhenAmp",
        options,
        Box::new(|cc| {
            install_fonts(&cc.egui_ctx);
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
