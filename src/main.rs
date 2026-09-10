mod player;
mod playlist;

use std::path::PathBuf;
use std::time::Duration;

use eframe::egui;
use egui::{Color32, Rect, Response, Sense, Stroke, Ui, Vec2};
use player::{Player, VISUALIZER_BARS};
use playlist::Playlist;

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
/// Smallest the playlist section can be squashed to (header + toolbar +
/// a sliver of list).
const MIN_PLAYLIST_HEIGHT: f32 = 110.0;
/// How tall the playlist section starts at when first opened.
const INITIAL_PLAYLIST_HEIGHT: f32 = 300.0;
const CHASSIS_CORNER_RADIUS: u8 = 5;
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
    shuffle_on: bool,
    repeat_on: bool,
    is_playing: bool,
    last_window_size: Option<Vec2>,
    micro_mode: bool,
    playlist: Playlist,
    playlist_open: bool,
    /// Used to detect the open/closed transition, to resize the window
    /// only at that moment rather than fighting the user's manual resize.
    playlist_was_open: bool,
    /// The highlighted row in the playlist list, distinct from the track
    /// that's actually playing (`playlist.current`) — single-click only
    /// selects; double-click plays.
    playlist_selected: Option<usize>,
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

/// Same typeface as the track title/artist marquee (IBM Plex Mono) — more
/// readable than Silkscreen at small sizes, for the KBPS/KHZ/STEREO badges.
fn badge_font(size: f32) -> egui::FontId {
    egui::FontId::new(size, egui::FontFamily::Proportional)
}

/// One LCD status badge (KBPS/KHZ/STEREO). All three share the same pill
/// shape; `lit` toggles between the bright indicator-lamp state and a dim
/// "unlit lamp" state (dark green fill, faint green text) so an off badge
/// still reads as the same component rather than a different one.
fn lcd_badge(ui: &mut Ui, text: &str, width: f32, lit: bool) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 14.0), Sense::hover());
    let (fill, text_color) = if lit {
        (LCD_GREEN, LCD_PANEL_BG)
    } else {
        (
            LCD_GREEN.gamma_multiply(0.15),
            LCD_GREEN.gamma_multiply(0.45),
        )
    };
    ui.painter().rect_filled(rect, 0.0, fill);
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_owned(), badge_font(10.0), text_color);
    let pos = (rect.center() - galley.size() / 2.0 - Vec2::new(0.0, 1.0)).round();
    ui.painter().galley(pos, galley, text_color);
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

#[derive(Clone, Copy)]
enum Direction {
    Prev,
    Next,
}

/// Prev/Next: walks the playlist if one exists, otherwise falls back to
/// restarting the current track (there's nothing to skip to without a
/// queue).
fn play_adjacent_track(
    playlist: &mut Playlist,
    player: &mut Player,
    status: &mut String,
    is_playing: &mut bool,
    direction: Direction,
) {
    let next_path = match direction {
        Direction::Prev => playlist.prev(),
        Direction::Next => playlist.next(),
    };
    match next_path {
        Some(path) => match player.load(path.clone()) {
            Ok(()) => {
                player.play();
                *is_playing = true;
            }
            Err(err) => {
                *status = format!("Failed to load: {err}");
            }
        },
        None => player.seek(Duration::ZERO),
    }
}

/// Queues each dropped path right after the current track, in the order
/// they were dropped, then loads and plays the first one queued — used for
/// files dropped onto the main player window.
fn queue_and_play_first(
    playlist: &mut Playlist,
    player: &mut Player,
    status: &mut String,
    is_playing: &mut bool,
    paths: Vec<PathBuf>,
) {
    let mut first_index = None;
    for path in paths {
        let index = playlist.insert_next(path);
        first_index.get_or_insert(index);
    }
    let Some(index) = first_index else { return };
    if let Some(path) = playlist.set_current(index) {
        match player.load(path) {
            Ok(()) => {
                player.play();
                *is_playing = true;
            }
            Err(err) => {
                *status = format!("Failed to load: {err}");
            }
        }
    }
}

/// Queues each dropped path right after the current track, without
/// changing playback — used for files dropped onto the playlist window.
fn queue_tracks(playlist: &mut Playlist, paths: Vec<PathBuf>) {
    for path in paths {
        playlist.insert_next(path);
    }
}

/// Absolute local file paths from whatever was dropped onto this viewport
/// this frame.
fn dropped_file_paths(ui: &Ui) -> Vec<PathBuf> {
    ui.ctx().input(|i| {
        i.raw
            .dropped_files
            .iter()
            .map(|f| f.path().to_path_buf())
            .collect()
    })
}

/// The playlist section, fused directly into the main player window
/// (rendered right below the chassis, same OS window — toggled by the PL
/// button, not a separate window). Its track list fills whatever height is
/// left in the window, so dragging the window's bottom edge taller shows
/// more of it.
fn playlist_section(
    ui: &mut Ui,
    playlist: &mut Playlist,
    player: &mut Player,
    status: &mut String,
    is_playing: &mut bool,
    selected: &mut Option<usize>,
) {
    egui::Frame::new()
        .fill(FACE)
        .corner_radius(egui::CornerRadius {
            nw: 0,
            ne: 0,
            sw: CHASSIS_CORNER_RADIUS,
            se: CHASSIS_CORNER_RADIUS,
        })
        .inner_margin(egui::Margin {
            left: 3,
            right: 3,
            top: 0,
            bottom: 3,
        })
        .show(ui, |ui| {
            ui.set_width(CHASSIS_WIDTH);

            let title_rect =
                ui.allocate_exact_size(Vec2::new(CHASSIS_WIDTH, 20.0), Sense::hover()).0;
            ui.painter().rect_filled(title_rect, 0.0, TITLE_BAR_BG);
            ui.scope_builder(egui::UiBuilder::new().max_rect(title_rect), |ui| {
                ui.horizontal(|ui| {
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new("PLAYLIST")
                            .font(silkscreen_font(9.0))
                            .color(LABEL_GRAY),
                    );
                });
            });

            ui.add_space(8.0);
            playlist_body(ui, playlist, player, status, is_playing, selected);
        });
}

/// The ADD/SAVE/LOAD/CLEAR toolbar plus the scrollable track list. The list
/// fills whatever vertical space remains in the window.
fn playlist_body(
    ui: &mut Ui,
    playlist: &mut Playlist,
    player: &mut Player,
    status: &mut String,
    is_playing: &mut bool,
    selected: &mut Option<usize>,
) {
    {
            ui.horizontal(|ui| {
                ui.add_space(8.0);
                if toggle_label_button(ui, "ADD", false, 42.0)
                    .on_hover_text("Add files")
                    .clicked()
                {
                    if let Some(paths) = rfd::FileDialog::new()
                        .add_filter("Audio", &["mp3", "wav", "flac", "ogg"])
                        .pick_files()
                    {
                        queue_tracks(playlist, paths);
                    }
                }
                ui.add_space(3.0);
                if toggle_label_button(ui, "SAVE", false, 42.0)
                    .on_hover_text("Save playlist (.m3u8)")
                    .clicked()
                {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Playlist", &["m3u", "m3u8"])
                        .set_file_name("playlist.m3u8")
                        .save_file()
                    {
                        if let Err(err) = playlist.save_m3u(&path) {
                            *status = format!("Failed to save playlist: {err}");
                        }
                    }
                }
                ui.add_space(3.0);
                if toggle_label_button(ui, "LOAD", false, 42.0)
                    .on_hover_text("Import playlist")
                    .clicked()
                {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Playlist", &["m3u", "m3u8"])
                        .pick_file()
                    {
                        match Playlist::load_m3u(&path) {
                            Ok(paths) => {
                                for p in paths {
                                    playlist.push(p);
                                }
                            }
                            Err(err) => {
                                *status = format!("Failed to import playlist: {err}");
                            }
                        }
                    }
                }
                ui.add_space(3.0);
                if toggle_label_button(ui, "CLEAR", false, 42.0)
                    .on_hover_text("Clear playlist")
                    .clicked()
                {
                    playlist.clear();
                }
            });

            ui.add_space(8.0);

            let mut to_play = None;
            let mut to_remove = None;

            // Recessed panel behind the track list, matching the main LCD
            // display's chrome — same 8px margins and inner padding.
            let panel_rect = ui
                .horizontal(|ui| {
                    ui.add_space(8.0);
                    let size = Vec2::new(
                        CHASSIS_WIDTH - 16.0,
                        (ui.available_height() - 8.0).max(0.0),
                    );
                    ui.allocate_exact_size(size, Sense::hover()).0
                })
                .inner;
            bevel_rect(ui, panel_rect, LCD_PANEL_BG, false);
            ui.scope_builder(
                egui::UiBuilder::new().max_rect(panel_rect.shrink(8.0)),
                |ui| {
                    // Restore normal spacing for the list rows; the chassis
                    // zeroes item_spacing for its fixed chrome.
                    ui.spacing_mut().item_spacing = Vec2::new(6.0, 4.0);
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            if playlist.entries.is_empty() {
                                ui.label(
                                    egui::RichText::new("Drop audio files here, or use ADD.")
                                        .font(silkscreen_font(9.0))
                                        .color(DIM_GRAY),
                                );
                            }
                            for (index, entry) in playlist.entries.iter().enumerate() {
                                let is_current = playlist.current == Some(index);
                                let is_selected = *selected == Some(index);
                                ui.horizontal(|ui| {
                                    ui.label(
                                        egui::RichText::new(format!("{}.", index + 1))
                                            .font(badge_font(11.0))
                                            .color(DIM_GRAY),
                                    );
                                    let color = if is_current { LCD_GREEN } else { INK };
                                    let label =
                                        egui::RichText::new(&entry.display_name).color(color);
                                    let response = ui.selectable_label(is_selected, label);
                                    if response.double_clicked() {
                                        to_play = Some(index);
                                        *selected = Some(index);
                                    } else if response.clicked() {
                                        *selected = Some(index);
                                    }
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            if ui
                                                .small_button("×")
                                                .on_hover_text("Remove")
                                                .clicked()
                                            {
                                                to_remove = Some(index);
                                            }
                                        },
                                    );
                                });
                            }
                        });
                },
            );

            if let Some(index) = to_play {
                if let Some(path) = playlist.set_current(index) {
                    match player.load(path) {
                        Ok(()) => {
                            player.play();
                            *is_playing = true;
                        }
                        Err(err) => {
                            *status = format!("Failed to load: {err}");
                        }
                    }
                }
            }
            if let Some(index) = to_remove {
                playlist.remove(index);
                *selected = match *selected {
                    Some(s) if s == index => None,
                    Some(s) if s > index => Some(s - 1),
                    other => other,
                };
            }
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
            shuffle_on: false,
            repeat_on: false,
            is_playing: false,
            last_window_size: None,
            micro_mode: false,
            playlist: Playlist::new(),
            playlist_open: false,
            playlist_was_open: false,
            playlist_selected: None,
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
            let dropped = dropped_file_paths(ui);
            if !dropped.is_empty() {
                queue_and_play_first(
                    &mut self.playlist,
                    player,
                    &mut self.status,
                    &mut self.is_playing,
                    dropped,
                );
            }

            let has_song = player.loaded_path().is_some();
            let duration = player.duration().unwrap_or_default();
            let duration_secs = duration.as_secs_f32().max(0.001);
            let mut pos_secs = self
                .seek_drag_secs
                .unwrap_or_else(|| player.position().as_secs_f32().min(duration_secs));

            // Track finished: repeat it, advance the queue, or stop.
            if self.is_playing && has_song && pos_secs >= duration_secs - 0.05 {
                if self.repeat_on {
                    player.seek(Duration::ZERO);
                    player.play();
                } else if let Some(next_path) = self.playlist.next() {
                    match player.load(next_path) {
                        Ok(()) => player.play(),
                        Err(err) => self.status = format!("Failed to load: {err}"),
                    }
                } else {
                    player.pause();
                    self.is_playing = false;
                }
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
                String::new()
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
                "--:--".to_string()
            };

            // The playlist, when open, is fused into this same window
            // (rendered right below the chassis) — so the chassis's own
            // bottom corners stay flat where they meet it.
            let show_playlist = self.playlist_open;

            let chassis_frame = egui::Frame::new()
                .fill(FACE)
                .corner_radius(if show_playlist {
                    egui::CornerRadius {
                        nw: CHASSIS_CORNER_RADIUS,
                        ne: CHASSIS_CORNER_RADIUS,
                        sw: 0,
                        se: 0,
                    }
                } else {
                    egui::CornerRadius::same(CHASSIS_CORNER_RADIUS)
                })
                .inner_margin(egui::Margin::same(3));

            let combined_response = ui.vertical(|ui| {
                // All gaps in the chassis are explicit add_space calls;
                // egui's default item_spacing would otherwise silently add
                // ~8px between siblings on top of them, unevenly.
                ui.spacing_mut().item_spacing = Vec2::ZERO;

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
                        ui.painter().rect_filled(
                            row_rect,
                            egui::CornerRadius::same(CHASSIS_CORNER_RADIUS),
                            TITLE_BAR_BG,
                        );

                        ui.scope_builder(
                            egui::UiBuilder::new()
                                .max_rect(row_rect.shrink2(Vec2::new(8.0, 1.0))),
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

                                        ui.add_space(3.0);
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

                                        ui.add_space(3.0);
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
                                            ui.add_space(3.0);
                                            if icon_button_sized(ui, Icon::Pause, btn_size)
                                                .on_hover_text("Pause")
                                                .clicked()
                                            {
                                                player.pause();
                                                self.is_playing = false;
                                            }
                                            ui.add_space(3.0);
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
                                        let spectrum_right_padding = 8.0;
                                        let ticker_width = (ui.available_width()
                                            - spectrum_width
                                            - spectrum_right_padding
                                            - 8.0)
                                            .max(40.0);
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
                                        ui.add_space(spectrum_right_padding);
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
                    ui.painter().rect_filled(
                        title_rect,
                        egui::CornerRadius {
                            nw: CHASSIS_CORNER_RADIUS,
                            ne: CHASSIS_CORNER_RADIUS,
                            sw: 0,
                            se: 0,
                        },
                        TITLE_BAR_BG,
                    );
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
                                // Center the title on the full bar; the LED
                                // and window buttons are visually light
                                // enough that true centering reads best.
                                let pos = (title_rect.center()
                                    - label_galley.size() / 2.0)
                                    .round();
                                ui.painter().galley(pos, label_galley, LABEL_GRAY);

                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    ui.add_space(8.0);
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
                                            // Bottom-align so the mode label
                                            // sits on the digits' baseline.
                                            let time_resp = ui.with_layout(
                                                egui::Layout::left_to_right(egui::Align::Max),
                                                |ui| {
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
                                                    ui.add_space(6.0);
                                                    ui.label(
                                                        egui::RichText::new(
                                                            if self.remaining_mode {
                                                                "REM"
                                                            } else {
                                                                "ELAPSED"
                                                            },
                                                        )
                                                        .font(silkscreen_font(9.0))
                                                        .color(LCD_GREEN.gamma_multiply(0.6)),
                                                    );
                                                    resp
                                                },
                                            );
                                            if time_resp.inner.clicked() {
                                                self.remaining_mode = !self.remaining_mode;
                                            }

                                            ui.add_space(6.0);
                                            // Badge row spans exactly the time
                                            // column width: 66 + 5 + 51 + 5 + 51.
                                            ui.horizontal(|ui| {
                                                let kbps_text = info
                                                    .kbps
                                                    .map(|k| format!("{k} KBPS"))
                                                    .unwrap_or_else(|| "-- KBPS".to_string());
                                                let khz_text = info
                                                    .sample_rate_hz
                                                    .map(|hz| format!("{} KHZ", hz / 1000))
                                                    .unwrap_or_else(|| "-- KHZ".to_string());
                                                let stereo_on = info.channels.unwrap_or(1) >= 2;

                                                lcd_badge(ui, &kbps_text, 66.0, has_song);
                                                ui.add_space(5.0);
                                                lcd_badge(ui, &khz_text, 51.0, has_song);
                                                ui.add_space(5.0);
                                                lcd_badge(
                                                    ui,
                                                    "STEREO",
                                                    51.0,
                                                    has_song && stereo_on,
                                                );
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

                            // Volume / balance / EQ / PL row. The volume
                            // slider flexes to fill; EQ/PL dock flush to the
                            // right gutter, mirroring SHUFFLE/REPEAT below.
                            ui.horizontal(|ui| {
                                let label_font = silkscreen_font(8.0);
                                let vol_label_w = ui
                                    .painter()
                                    .layout_no_wrap("VOL".into(), label_font.clone(), DIM_GRAY)
                                    .size()
                                    .x;
                                let bal_label_w = ui
                                    .painter()
                                    .layout_no_wrap("BAL".into(), label_font.clone(), DIM_GRAY)
                                    .size()
                                    .x;
                                // labels + BAL slider (120) + EQ/PL (34+3+34)
                                // + four 8px gaps.
                                let reserved = vol_label_w + bal_label_w + 120.0 + 71.0 + 32.0;
                                let vol_w = (ui.available_width() - reserved).max(100.0);

                                ui.label(
                                    egui::RichText::new("VOL")
                                        .font(label_font.clone())
                                        .color(DIM_GRAY),
                                );
                                ui.add_space(8.0);
                                let vol = player.volume();
                                let (_resp, drag) =
                                    bevel_slider(ui, Vec2::new(vol_w, 6.0), vol, LCD_GREEN);
                                if let Some(v) = drag {
                                    player.set_volume(v);
                                }

                                ui.add_space(8.0);
                                ui.label(
                                    egui::RichText::new("BAL").font(label_font).color(DIM_GRAY),
                                );
                                ui.add_space(8.0);
                                let bal = player.balance();
                                let (bal_resp, drag) =
                                    bevel_slider(ui, Vec2::new(120.0, 6.0), bal, LCD_GREEN);
                                if let Some(v) = drag {
                                    player.set_balance(v);
                                }
                                if bal_resp.double_clicked() {
                                    player.set_balance(0.5);
                                }

                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if toggle_label_button(ui, "PL", self.playlist_open, 34.0)
                                            .on_hover_text("Playlist")
                                            .clicked()
                                        {
                                            self.playlist_open = !self.playlist_open;
                                        }
                                        ui.add_space(3.0);
                                        if toggle_label_button(ui, "EQ", self.eq_on, 34.0)
                                            .clicked()
                                        {
                                            self.eq_on = !self.eq_on;
                                        }
                                    },
                                );
                            });

                            ui.add_space(8.0);

                            // Seek bar: the 20px handle is centered on the
                            // playhead, so at the extremes it overhangs the
                            // track by 10px per side. Inset the track by that
                            // much so the handle sits flush with the 8px
                            // gutter at 0% and 100%.
                            let seek_value = pos_secs / duration_secs;
                            let (seek_resp, seek_drag) = ui
                                .horizontal(|ui| {
                                    ui.add_space(10.0);
                                    seek_bar(ui, Vec2::new(CHASSIS_WIDTH - 36.0, 10.0), seek_value)
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
                                ui.add_space(8.0);

                                ui.add_enabled_ui(has_song, |ui| {
                                    if icon_button(ui, Icon::Prev)
                                        .on_hover_text("Previous")
                                        .clicked()
                                    {
                                        play_adjacent_track(
                                            &mut self.playlist,
                                            player,
                                            &mut self.status,
                                            &mut self.is_playing,
                                            Direction::Prev,
                                        );
                                    }
                                    ui.add_space(3.0);
                                    if icon_button(ui, Icon::Play).on_hover_text("Play").clicked()
                                    {
                                        player.play();
                                        self.is_playing = true;
                                    }
                                    ui.add_space(3.0);
                                    if icon_button(ui, Icon::Pause)
                                        .on_hover_text("Pause")
                                        .clicked()
                                    {
                                        player.pause();
                                        self.is_playing = false;
                                    }
                                    ui.add_space(3.0);
                                    if icon_button(ui, Icon::Stop).on_hover_text("Stop").clicked()
                                    {
                                        player.stop();
                                        self.is_playing = false;
                                    }
                                    ui.add_space(3.0);
                                    if icon_button(ui, Icon::Next)
                                        .on_hover_text("Next")
                                        .clicked()
                                    {
                                        play_adjacent_track(
                                            &mut self.playlist,
                                            player,
                                            &mut self.status,
                                            &mut self.is_playing,
                                            Direction::Next,
                                        );
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

                if show_playlist {
                    playlist_section(
                        ui,
                        &mut self.playlist,
                        player,
                        &mut self.status,
                        &mut self.is_playing,
                        &mut self.playlist_selected,
                    );
                }

                chassis_response.response.rect.height()
            });

            let just_opened = self.playlist_open && !self.playlist_was_open;
            let just_closed = !self.playlist_open && self.playlist_was_open;
            self.playlist_was_open = self.playlist_open;

            let window_width = CHASSIS_WIDTH + 6.0;
            if just_opened {
                // Let the window resize vertically (dragging the bottom edge
                // or a bottom corner), but keep the width locked to the
                // chassis: min/max width are the same value.
                let chassis_height = combined_response.inner;
                let min_height = chassis_height + MIN_PLAYLIST_HEIGHT;
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::Resizable(true));
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::MinInnerSize(
                    Vec2::new(window_width, min_height),
                ));
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::MaxInnerSize(
                    Vec2::new(window_width, 4000.0),
                ));
                let initial_height = chassis_height + INITIAL_PLAYLIST_HEIGHT;
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::InnerSize(
                    Vec2::new(window_width, initial_height),
                ));
                self.last_window_size = None;
            } else if just_closed {
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::Resizable(false));
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::MinInnerSize(Vec2::ZERO));
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::MaxInnerSize(
                    Vec2::new(100_000.0, 100_000.0),
                ));
                self.last_window_size = None;
            }

            if self.playlist_open {
                // Playlist showing: the window is user-resizable vertically,
                // so only correct width drift (e.g. from dragging a bottom
                // corner), preserving whatever height the user has set.
                if let Some(inner) = ui.ctx().input(|i| i.viewport().inner_rect) {
                    if (inner.width() - window_width).abs() > 0.5 {
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::InnerSize(
                            Vec2::new(window_width, inner.height()),
                        ));
                    }
                }
            } else {
                // No playlist: snap the (non-resizable) window to exactly
                // fit the chassis. Use the chassis-only height captured
                // above rather than the drawn rect's size — on the very
                // frame the playlist closes, that rect still reflects the
                // stale (tall) layout from before the toggle, which would
                // otherwise send the wrong size for one frame and then
                // correct it the next, causing a visible flicker.
                let desired_size = Vec2::new(window_width, combined_response.inner);
                if self
                    .last_window_size
                    .is_none_or(|last| (last - desired_size).length() > 0.5)
                {
                    self.last_window_size = Some(desired_size);
                    ui.ctx()
                        .send_viewport_cmd(egui::ViewportCommand::InnerSize(desired_size));
                }
            }

            self.viz_bars = player.sample_visualizer();

            if has_song {
                ui.ctx().request_repaint_after(Duration::from_millis(33));
            }
        }
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        // Fully transparent: the chassis frame paints its own rounded FACE
        // background, and letting the area outside that rounded shape stay
        // transparent (rather than opaque FACE) is what makes the rounded
        // corners actually visible against the desktop.
        egui::Color32::TRANSPARENT.to_normalized_gamma_f32()
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([566.0, 260.0])
            .with_resizable(false)
            .with_decorations(false)
            .with_transparent(true),
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
