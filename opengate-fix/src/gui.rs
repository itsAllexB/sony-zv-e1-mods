//! The window. Empty, it is a drop target with the two options underneath; once
//! clips arrive, the hero shrinks into a header above a list of results.
//!
//! Most of it is painted by hand on top of egui so it reads like a small native
//! utility: one accent colour, Inter for text, Lucide for icons (both embedded).

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Id, Key, LayerId,
    Layout, Margin, Modifiers, Order, Painter, PointerButton, Pos2, Rect, Response, ScrollArea, Sense, Shadow,
    Shape, Stroke, StrokeKind, Ui, UiBuilder, Vec2, ViewportCommand, epaint::text::VariationCoords, pos2,
    text::{Galley, LayoutJob, TextFormat, TextWrapping},
    vec2,
};

use crate::job::{self, OutputMode, Outcome, Settings, Size};

// ---------------------------------------------------------------------------
// Results

#[derive(Clone)]
enum State {
    Waiting,
    Working(f32),
    Fixed { from: Size, to: Size, name: String, path: String, trashed: bool, warning: Option<String> },
    Skipped(Size),
    Failed(String),
}

impl State {
    fn finished(&self) -> bool {
        !matches!(self, State::Waiting | State::Working(_))
    }
}

#[derive(Clone)]
struct Item {
    id: u64,
    name: String,
    state: State,
}

type Items = Arc<Mutex<Vec<Item>>>;

fn set_state(items: &Items, id: u64, state: State) {
    if let Some(item) = items.lock().unwrap().iter_mut().find(|i| i.id == id) {
        item.state = state;
    }
}

fn describe(outcome: Outcome) -> State {
    match outcome {
        Outcome::Fixed { from, to, output, original_trashed, warning } => State::Fixed {
            from,
            to,
            name: output.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
            path: output.display().to_string(),
            trashed: original_trashed,
            warning,
        },
        Outcome::NothingToFix { size } => State::Skipped(size),
    }
}

/// What a file dialog returned.
enum Picked {
    Files(Vec<PathBuf>),
    Folder(PathBuf),
}

// ---------------------------------------------------------------------------
// Look

/// Lucide icons (https://lucide.dev, ISC). Only these code points are kept in the
/// embedded subset; `scripts/subset-fonts.sh` lists the same ones.
mod icon {
    pub const SETTINGS: char = '\u{e154}'; // settings
    pub const BROWSE: char = '\u{e247}'; // folder-open
    pub const ADD: char = '\u{e13d}'; // plus
    pub const SAVE: char = '\u{e332}'; // folder-down
    pub const TRASH: char = '\u{e18e}'; // trash-2
    pub const SAFE: char = '\u{e1ff}'; // shield-check
    pub const FIXED: char = '\u{e226}'; // circle-check
    pub const SKIPPED: char = '\u{e07e}'; // circle-minus
    pub const FAILED: char = '\u{e084}'; // circle-x
    pub const WARNING: char = '\u{e077}'; // circle-alert
    pub const WAITING: char = '\u{e087}'; // clock
    pub const DROP: char = '\u{e455}'; // arrow-down-to-line
    pub const CLEAR: char = '\u{e240}'; // list-x
    pub const FOLDER: char = '\u{e0d7}'; // folder
    pub const DONE: char = '\u{e06c}'; // check
    #[cfg_attr(target_os = "macos", allow(dead_code))]
    pub const MINIMIZE: char = '\u{e11c}'; // minus
    #[cfg_attr(target_os = "macos", allow(dead_code))]
    #[cfg_attr(target_os = "macos", allow(dead_code))]
    #[cfg_attr(target_os = "macos", allow(dead_code))]
    pub const CLOSE: char = '\u{e1b2}'; // x

    #[cfg(test)]
    pub const ALL: [char; 17] = [
        SETTINGS, BROWSE, ADD, SAVE, TRASH, SAFE, FIXED, SKIPPED, FAILED, WARNING, WAITING, DROP, CLEAR,
        FOLDER, DONE, MINIMIZE, CLOSE,
    ];
}

fn icon_family() -> FontFamily {
    FontFamily::Name("icons".into())
}

/// Inter (OFL) for text, Lucide (ISC) for icons; both subset in `assets/fonts`.
fn font_definitions() -> FontDefinitions {
    let mut defs = FontDefinitions::empty();
    defs.font_data
        .insert("inter".into(), Arc::new(FontData::from_static(include_bytes!("../assets/fonts/Inter-subset.ttf"))));
    defs.font_data
        .insert("lucide".into(), Arc::new(FontData::from_static(include_bytes!("../assets/fonts/lucide-subset.ttf"))));
    let text = vec!["inter".to_string(), "lucide".to_string()];
    defs.families.insert(FontFamily::Proportional, text.clone());
    defs.families.insert(FontFamily::Monospace, text);
    defs.families.insert(icon_family(), vec!["lucide".into()]);
    defs
}

/// Every colour the window uses, for one appearance.
#[derive(Clone, Copy)]
struct Pal {
    bg: Color32,
    card: Color32,
    line: Color32,
    well: Color32,
    strong: Color32,
    text: Color32,
    weak: Color32,
    faint: Color32,
    accent: Color32,
    accent_text: Color32,
    tint: Color32,
    tint_hover: Color32,
    track: Color32,
    thumb: Color32,
    toggle_off: Color32,
    /// Unfilled part of a progress ring.
    ring: Color32,
    hover: Color32,
    green: Color32,
    red: Color32,
    amber: Color32,
    shadow: Color32,
}

const fn rgb(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

fn alpha(c: Color32, a: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), (a * 255.0).round() as u8)
}

impl Pal {
    fn of(dark: bool) -> Self {
        if dark {
            let accent = rgb(255, 143, 64);
            Self {
                bg: rgb(28, 28, 30),
                card: rgb(39, 39, 42),
                line: alpha(Color32::WHITE, 0.075),
                well: rgb(42, 42, 45),
                strong: rgb(245, 245, 247),
                text: rgb(214, 214, 219),
                weak: rgb(152, 152, 159),
                faint: rgb(104, 104, 110),
                accent,
                accent_text: rgb(255, 160, 92),
                tint: alpha(accent, 0.15),
                tint_hover: alpha(accent, 0.23),
                track: rgb(29, 29, 31),
                thumb: rgb(64, 64, 69),
                toggle_off: rgb(70, 70, 75),
                ring: alpha(accent, 0.22),
                hover: alpha(Color32::WHITE, 0.055),
                green: rgb(74, 201, 124),
                red: rgb(255, 105, 97),
                amber: rgb(242, 176, 66),
                shadow: alpha(Color32::BLACK, 0.35),
            }
        } else {
            let accent = rgb(234, 108, 26);
            Self {
                bg: rgb(244, 244, 246),
                card: Color32::WHITE,
                line: alpha(Color32::BLACK, 0.07),
                well: rgb(236, 236, 239),
                strong: rgb(28, 28, 30),
                text: rgb(58, 58, 64),
                weak: rgb(128, 128, 136),
                faint: rgb(172, 172, 178),
                accent,
                accent_text: rgb(196, 84, 8),
                tint: alpha(accent, 0.11),
                tint_hover: alpha(accent, 0.18),
                track: rgb(238, 238, 241),
                thumb: Color32::WHITE,
                toggle_off: rgb(216, 216, 221),
                ring: alpha(accent, 0.24),
                hover: alpha(Color32::BLACK, 0.035),
                green: rgb(38, 158, 92),
                red: rgb(222, 56, 56),
                amber: rgb(196, 124, 14),
                shadow: alpha(Color32::BLACK, 0.07),
            }
        }
    }

    fn visuals(&self, dark: bool) -> egui::Visuals {
        let mut v = if dark { egui::Visuals::dark() } else { egui::Visuals::light() };
        v.panel_fill = self.bg;
        v.window_fill = self.card;
        v.extreme_bg_color = self.card;
        v.faint_bg_color = self.track;
        v.window_stroke = Stroke::new(1.0, self.line);
        v.window_corner_radius = CornerRadius::same(10);
        v.window_shadow = Shadow { offset: [0, 6], blur: 20, spread: 0, color: self.shadow };
        v.popup_shadow = Shadow { offset: [0, 4], blur: 14, spread: 0, color: self.shadow };
        v.selection.bg_fill = self.tint_hover;
        v.selection.stroke = Stroke::new(1.0, self.accent);
        v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, self.text);
        v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, self.line);
        v
    }
}

fn setup_style(ctx: &egui::Context) {
    ctx.set_fonts(font_definitions());
    for (theme, dark) in [(egui::Theme::Light, false), (egui::Theme::Dark, true)] {
        ctx.set_visuals_of(theme, Pal::of(dark).visuals(dark));
    }
    ctx.all_styles_mut(|s| {
        s.spacing.item_spacing = vec2(0.0, 0.0);
        s.spacing.scroll = egui::style::ScrollStyle::floating();
        s.spacing.tooltip_width = 300.0;
        s.spacing.menu_margin = Margin::symmetric(10, 8);
        s.interaction.tooltip_delay = 0.35;
        use egui::TextStyle::*;
        s.text_styles = [
            (Small, FontId::proportional(11.5)),
            (Body, FontId::proportional(13.0)),
            (Button, FontId::proportional(13.0)),
            (Heading, FontId::proportional(20.0)),
            (Monospace, FontId::monospace(12.5)),
        ]
        .into();
    });
}

/// Height of the strip at the top that drags the window. On macOS it matches
/// the band the traffic lights sit in.
#[cfg(target_os = "macos")]
const TOP_BAR: f32 = 34.0;
#[cfg(not(target_os = "macos"))]
const TOP_BAR: f32 = 44.0;
/// Vertical centre of the controls in the top strip (the traffic lights' centre on macOS).
#[cfg(target_os = "macos")]
const BAR_CENTER: f32 = 16.0;
#[cfg(not(target_os = "macos"))]
const BAR_CENTER: f32 = 22.0;
/// Horizontal page margin.
const SIDE: f32 = 20.0;
/// Widest the centred column gets on a wide window.
const COLUMN: f32 = 420.0;
const ROW_H: f32 = 54.0;

/// Lays out one run of Inter. `max_width` truncates with an ellipsis; `wrap` wraps instead.
fn text_job(text: &str, size: f32, weight: f32, color: Color32, max_width: f32, wrap: bool) -> LayoutJob {
    let format = TextFormat {
        font_id: FontId::proportional(size),
        color,
        // Inter's optical size follows the point size, like browsers do with `font-optical-sizing: auto`.
        coords: VariationCoords::new([(b"wght", weight), (b"opsz", size.clamp(14.0, 32.0))]),
        line_height: wrap.then_some((size * 1.42).round()),
        ..Default::default()
    };
    let mut job = LayoutJob::single_section(text.to_owned(), format);
    job.wrap = if wrap { TextWrapping::wrap_at_width(max_width) } else { TextWrapping::truncate_at_width(max_width) };
    job
}

fn text(ui: &Ui, s: &str, size: f32, weight: f32, color: Color32, max_width: f32) -> Arc<Galley> {
    ui.painter().layout_job(text_job(s, size, weight, color, max_width, false))
}

fn para(ui: &Ui, s: &str, size: f32, color: Color32, max_width: f32) -> Arc<Galley> {
    ui.painter().layout_job(text_job(s, size, 400.0, color, max_width, true))
}

fn paint_icon(p: &Painter, center: Pos2, glyph: char, size: f32, color: Color32) {
    p.text(center, Align2::CENTER_CENTER, glyph, FontId::new(size, icon_family()), color);
}

/// Round-capped segment, to match Lucide's strokes.
fn seg(p: &Painter, a: Pos2, b: Pos2, width: f32, color: Color32) {
    p.line_segment([a, b], Stroke::new(width, color));
    p.circle_filled(a, width / 2.0, color);
    p.circle_filled(b, width / 2.0, color);
}

/// The app's mark, drawn in Lucide's 24-unit grid: a 3:2 viewfinder whose frame
/// opens vertically (the taller open gate picture).
fn paint_mark(p: &Painter, center: Pos2, size: f32, width: f32, color: Color32) {
    let u = size / 24.0;
    let at = |x: f32, y: f32| pos2(center.x + (x - 12.0) * u, center.y + (y - 12.0) * u);
    let (x0, x1, y0, y1, arm) = (1.5, 22.5, 5.0, 19.0, 4.5);
    for (x, y, dx, dy) in [(x0, y0, 1.0, 1.0), (x1, y0, -1.0, 1.0), (x0, y1, 1.0, -1.0), (x1, y1, -1.0, -1.0)] {
        seg(p, at(x, y + dy * arm), at(x, y), width, color);
        seg(p, at(x, y), at(x + dx * arm, y), width, color);
    }
    seg(p, at(12.0, 8.0), at(12.0, 16.0), width, color);
    for (tip, back) in [(8.0, 10.4), (16.0, 13.6)] {
        seg(p, at(9.6, back), at(12.0, tip), width, color);
        seg(p, at(14.4, back), at(12.0, tip), width, color);
    }
}

/// An arc from 12 o'clock, clockwise, `turns` long, starting `start` turns in.
fn paint_arc(p: &Painter, center: Pos2, radius: f32, start: f32, turns: f32, width: f32, color: Color32) {
    let steps = (turns * 48.0).ceil().max(2.0) as usize;
    let points: Vec<Pos2> = (0..=steps)
        .map(|i| {
            let a = (start + turns * i as f32 / steps as f32 - 0.25) * std::f32::consts::TAU;
            center + vec2(a.cos(), a.sin()) * radius
        })
        .collect();
    p.circle_filled(points[0], width / 2.0, color);
    p.circle_filled(*points.last().unwrap(), width / 2.0, color);
    p.add(Shape::line(points, Stroke::new(width, color)));
}

/// Outline of a rounded rectangle, for the dashed drop target.
fn rounded_outline(rect: Rect, r: f32) -> Vec<Pos2> {
    let mut pts = Vec::new();
    let corners = [
        (pos2(rect.right() - r, rect.top() + r), -0.25),
        (pos2(rect.right() - r, rect.bottom() - r), 0.0),
        (pos2(rect.left() + r, rect.bottom() - r), 0.25),
        (pos2(rect.left() + r, rect.top() + r), 0.5),
    ];
    for (c, start) in corners {
        for i in 0..=8 {
            let a = (start + 0.25 * i as f32 / 8.0) * std::f32::consts::TAU;
            pts.push(c + vec2(a.cos(), a.sin()) * r);
        }
    }
    pts.push(pts[0]);
    pts
}

/// A round "well" holding the mark: the hero, and the small one in the list header.
fn paint_well(p: &Painter, pal: &Pal, center: Pos2, diameter: f32, hover: f32) {
    let r = diameter / 2.0;
    let rect = Rect::from_center_size(center, Vec2::splat(diameter));
    let lift = (diameter / 14.0).min(8.0);
    p.add(
        Shadow { offset: [0, lift as i8], blur: (lift * 2.6) as u8, spread: 0, color: pal.shadow }
            .as_shape(rect, CornerRadius::same(r.min(255.0) as u8)),
    );
    p.circle_filled(center, r, pal.well);
    let ring = pal.line.lerp_to_gamma(alpha(pal.accent, 0.55), hover);
    p.circle_stroke(center, r - 0.5, Stroke::new(1.0, ring));
    paint_mark(p, center, diameter * 0.5, (diameter * 0.027).max(1.6), pal.accent);
}

/// Accent-tinted button with an icon and a label.
fn pill_size(ui: &Ui, label: &str) -> Vec2 {
    vec2(14.0 + 16.0 + 7.0 + text(ui, label, 13.5, 590.0, Color32::WHITE, f32::INFINITY).size().x + 17.0, 34.0)
}

fn pill(ui: &mut Ui, pal: &Pal, rect: Rect, id: Id, glyph: char, label: &str) -> Response {
    let resp = ui.interact(rect, id, Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
    let fill = if resp.is_pointer_button_down_on() {
        pal.tint_hover.gamma_multiply(1.3)
    } else if resp.hovered() {
        pal.tint_hover
    } else {
        pal.tint
    };
    let p = ui.painter();
    // Same corner radius as the segmented control and the folder field.
    p.rect_filled(rect, CornerRadius::same(9), fill);
    let g = text(ui, label, 13.5, 590.0, pal.accent_text, f32::INFINITY);
    // Content centred, nudged left by a hair so the icon side does not look heavier.
    let x = rect.center().x - (16.0 + 7.0 + g.size().x) / 2.0 - 1.0;
    paint_icon(p, pos2(x + 8.0, rect.center().y), glyph, 16.0, pal.accent_text);
    p.galley(pos2(x + 16.0 + 7.0, rect.center().y - g.size().y / 2.0), g, pal.accent_text);
    resp
}

/// Square icon button that only shows a background on hover.
fn icon_button(ui: &mut Ui, pal: &Pal, rect: Rect, id: Id, glyph: char, tip: &str) -> Response {
    let resp = ui.interact(rect, id, Sense::click());
    let p = ui.painter();
    if resp.hovered() {
        p.rect_filled(rect, CornerRadius::same(8), pal.hover);
    }
    let color = if resp.hovered() { pal.strong } else { pal.weak };
    paint_icon(p, rect.center(), glyph, 17.0, color);
    if tip.is_empty() { resp } else { resp.on_hover_text(tip) }
}

/// Text button without a background until hovered (the list's "Clear").
fn ghost_button(ui: &mut Ui, pal: &Pal, right: Pos2, id: Id, glyph: char, label: &str, enabled: bool) -> Response {
    let g = text(ui, label, 13.0, 540.0, pal.weak, f32::INFINITY);
    let rect = Rect::from_min_max(pos2(right.x - g.size().x - 16.0 - 6.0 - 20.0, right.y - 15.0), pos2(right.x, right.y + 15.0));
    let resp = ui.interact(rect, id, if enabled { Sense::click() } else { Sense::hover() });
    let color = if !enabled {
        pal.faint
    } else if resp.hovered() {
        pal.strong
    } else {
        pal.weak
    };
    let p = ui.painter();
    if enabled && resp.hovered() {
        p.rect_filled(rect, CornerRadius::same(8), pal.hover);
    }
    paint_icon(p, pos2(rect.left() + 10.0 + 8.0, rect.center().y), glyph, 16.0, color);
    p.galley(pos2(rect.left() + 10.0 + 16.0 + 6.0, rect.center().y - g.size().y / 2.0), g, color);
    resp
}

fn paint_toggle(p: &Painter, pal: &Pal, rect: Rect, t: f32) {
    let track = pal.toggle_off.lerp_to_gamma(pal.accent, t);
    p.rect_filled(rect, CornerRadius::same((rect.height() / 2.0) as u8), track);
    let r = rect.height() / 2.0 - 2.5;
    let x = egui::lerp((rect.left() + r + 2.5)..=(rect.right() - r - 2.5), t);
    let c = pos2(x, rect.center().y);
    p.circle_filled(c + vec2(0.0, 1.0), r + 0.5, alpha(Color32::BLACK, 0.12));
    p.circle_filled(c, r, Color32::WHITE);
}

/// A card: white (or raised grey in dark mode) rounded panel with a hairline.
fn card<R>(ui: &mut Ui, pal: &Pal, margin: Margin, add: impl FnOnce(&mut Ui) -> R) -> R {
    egui::Frame::new()
        .fill(pal.card)
        .stroke(Stroke::new(1.0, pal.line))
        .corner_radius(CornerRadius::same(14))
        .shadow(Shadow { offset: [0, 1], blur: 4, spread: 0, color: pal.shadow })
        .inner_margin(margin)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

/// Icon, title and description of one option; returns the row's rect.
fn option_row(ui: &mut Ui, pal: &Pal, glyph: char, title: &str, desc: &str, right: f32) -> Rect {
    let w = ui.available_width();
    let text_x = 34.0;
    let text_w = (w - text_x - if right > 0.0 { right + 14.0 } else { 0.0 }).max(60.0);
    let t = text(ui, title, 14.0, 570.0, pal.strong, text_w);
    let d = para(ui, desc, 12.5, pal.weak, text_w);
    let block = t.size().y + 1.0 + d.size().y;
    let (rect, _) = ui.allocate_exact_size(vec2(w, block.max(34.0)), Sense::hover());
    let top = rect.center().y - block / 2.0;
    let p = ui.painter();
    paint_icon(p, pos2(rect.left() + 11.0, rect.center().y), glyph, 20.0, pal.accent);
    let th = t.size().y;
    p.galley(pos2(rect.left() + text_x, top), t, pal.strong);
    p.galley(pos2(rect.left() + text_x, top + th + 1.0), d, pal.weak);
    rect
}

/// Height the "One folder" row adds to its card: a gap plus the 34pt row.
const FOLDER_ROW: f32 = 44.0;

/// Segmented control; returns the index the user clicked, if any.
fn segmented(ui: &mut Ui, pal: &Pal, id: Id, labels: &[&str], selected: usize) -> Option<usize> {
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(w, 32.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(9), pal.track);
    let seg_w = (rect.width() - 4.0) / labels.len() as f32;
    let at = ui.ctx().animate_value_with_time(id, selected as f32, 0.16);
    let thumb = Rect::from_min_size(pos2(rect.left() + 2.0 + at * seg_w, rect.top() + 2.0), vec2(seg_w, 28.0));
    p.add(Shadow { offset: [0, 1], blur: 3, spread: 0, color: alpha(Color32::BLACK, 0.10) }.as_shape(thumb, 7));
    p.rect_filled(thumb, CornerRadius::same(7), pal.thumb);
    let mut clicked = None;
    for (i, label) in labels.iter().enumerate() {
        let r = Rect::from_min_size(pos2(rect.left() + 2.0 + i as f32 * seg_w, rect.top() + 2.0), vec2(seg_w, 28.0));
        let resp = ui.interact(r, id.with(i), Sense::click());
        let on = i == selected;
        let color = if on { pal.strong } else if resp.hovered() { pal.text } else { pal.weak };
        let g = text(ui, label, 12.5, if on { 600.0 } else { 500.0 }, color, seg_w - 8.0);
        ui.painter().galley(r.center() - g.size() / 2.0, g, color);
        if resp.clicked() && !on {
            clicked = Some(i);
        }
    }
    clicked
}

/// Shortens a path from the front so the folder name stays visible.
fn elide_front(ui: &Ui, s: &str, size: f32, max_width: f32) -> String {
    let fits = |t: &str| text(ui, t, size, 450.0, Color32::WHITE, f32::INFINITY).size().x <= max_width;
    if fits(s) {
        return s.to_owned();
    }
    let chars: Vec<char> = s.chars().collect();
    let (mut lo, mut hi) = (0, chars.len());
    while lo < hi {
        let mid = (lo + hi) / 2;
        let candidate: String = std::iter::once('…').chain(chars[mid..].iter().copied()).collect();
        if fits(&candidate) { hi = mid } else { lo = mid + 1 }
    }
    // Cut at a folder boundary when there is one: "…/Open Gate/Fixed" rather than "…en Gate/Fixed".
    let start = chars[lo..].iter().position(|&c| c == '/' || c == '\\').map_or(lo, |i| lo + i);
    std::iter::once('…').chain(chars[start..].iter().copied()).collect()
}

// ---------------------------------------------------------------------------
// App

pub struct App {
    settings: Settings,
    items: Items,
    jobs: Sender<(u64, PathBuf, Settings)>,
    next_id: u64,
    show_settings: bool,
    picked_tx: Sender<Picked>,
    picked_rx: Receiver<Picked>,
    dialog_open: Arc<AtomicBool>,
    /// Clip being worked on in the previous frame, to scroll the list along.
    active: Option<u64>,
    /// Height of the empty page's content last frame, to centre it.
    empty_height: f32,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, initial: Vec<PathBuf>) -> Self {
        setup_style(&cc.egui_ctx);
        let items: Items = Arc::default();
        let (jobs, rx) = mpsc::channel::<(u64, PathBuf, Settings)>();
        let ctx = cc.egui_ctx.clone();
        let shared = items.clone();
        std::thread::spawn(move || {
            for (id, path, settings) in rx {
                set_state(&shared, id, State::Working(0.0));
                ctx.request_repaint();
                let last = std::cell::Cell::new(Instant::now());
                let result = job::process(&path, &settings, &|p| {
                    if last.get().elapsed() > Duration::from_millis(40) {
                        last.set(Instant::now());
                        set_state(&shared, id, State::Working(p));
                        ctx.request_repaint();
                    }
                });
                set_state(&shared, id, result.map(describe).unwrap_or_else(State::Failed));
                ctx.request_repaint();
            }
        });
        let (picked_tx, picked_rx) = mpsc::channel();
        let mut app = Self {
            settings: Settings::load(),
            items,
            jobs,
            next_id: 0,
            show_settings: false,
            picked_tx,
            picked_rx,
            dialog_open: Arc::default(),
            active: None,
            empty_height: 0.0,
        };
        app.add(initial);
        app
    }

    fn add(&mut self, paths: Vec<PathBuf>) {
        for path in job::collect(&paths) {
            let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            let id = self.next_id;
            self.next_id += 1;
            self.items.lock().unwrap().push(Item { id, name, state: State::Waiting });
            let _ = self.jobs.send((id, path, self.settings.clone()));
        }
    }

    /// Opens a file or folder dialog without blocking the window. The dialog must
    /// not run inside the window's own event handling (that aborts on macOS), so
    /// it runs from a helper thread and the answer comes back through a channel.
    fn open_dialog(&self, ctx: &egui::Context, folder: bool) {
        if self.dialog_open.swap(true, Ordering::SeqCst) {
            return;
        }
        let (tx, open, ctx) = (self.picked_tx.clone(), self.dialog_open.clone(), ctx.clone());
        std::thread::spawn(move || {
            let picked = if folder {
                pollster::block_on(rfd::AsyncFileDialog::new().pick_folder())
                    .map(|d| Picked::Folder(d.path().to_path_buf()))
            } else {
                let dialog = rfd::AsyncFileDialog::new().add_filter("Video", &["mp4", "MP4", "mov", "MOV"]);
                pollster::block_on(dialog.pick_files())
                    .map(|files| Picked::Files(files.iter().map(|f| f.path().to_path_buf()).collect()))
            };
            if let Some(picked) = picked {
                let _ = tx.send(picked);
            }
            open.store(false, Ordering::SeqCst);
            ctx.request_repaint();
        });
    }

    // --- top strip -----------------------------------------------------------

    fn title_bar(&mut self, ui: &mut Ui, pal: &Pal, bar: Rect, has_items: bool) {
        // Registered first so the buttons drawn on top of it win the clicks.
        let drag = ui.interact(bar, Id::unique("title_bar"), Sense::click_and_drag());
        if drag.drag_started_by(PointerButton::Primary) {
            ui.ctx().send_viewport_cmd(ViewportCommand::StartDrag);
        }

        let cy = bar.top() + BAR_CENTER;
        #[cfg(target_os = "macos")]
        let right = bar.right() - 10.0;
        #[cfg(not(target_os = "macos"))]
        let right = window_controls(ui, pal, bar) - 6.0;

        if has_items && !self.show_settings {
            let rect = Rect::from_center_size(pos2(right - 14.0, cy), Vec2::splat(28.0));
            if icon_button(ui, pal, rect, Id::unique("settings"), icon::SETTINGS, "Settings").clicked() {
                self.show_settings = true;
            }
        }
    }

    // --- empty state ---------------------------------------------------------

    fn empty_page(&mut self, ui: &mut Ui, pal: &Pal) {
        let avail = ui.available_height();
        // Centre the page as if the folder row were hidden, so choosing "One folder"
        // adds the row below without shifting everything else. Only when the
        // window is too short for that does the content move up to stay in view.
        let folder_row = if self.settings.output == OutputMode::Custom { FOLDER_ROW } else { 0.0 };
        let centred = (avail - (self.empty_height - folder_row)) * 0.42;
        let pad = centred.min(avail - self.empty_height).max(4.0);
        let mut measured = 0.0;
        ScrollArea::vertical().id_salt("empty").auto_shrink([false, false]).show(ui, |ui| {
            let full_w = ui.available_width();
            let col_w = (full_w - 2.0 * SIDE).min(COLUMN);
            let left = ui.max_rect().left() + (full_w - col_w) / 2.0;
            let top = ui.cursor().top();
            let column = Rect::from_min_size(pos2(left, top + pad), vec2(col_w, f32::INFINITY));
            let inner = ui.scope_builder(UiBuilder::new().max_rect(column).layout(Layout::top_down(Align::Min)), |ui| {
                let start = ui.cursor().top();
                self.hero(ui, pal, col_w);
                ui.add_space(26.0);
                self.options(ui, pal);
                ui.add_space(16.0);
                self.about_line(ui, pal, col_w);
                ui.add_space(18.0);
                ui.cursor().top() - start
            });
            // `scope_builder` already advanced the scroll area's cursor past the column.
            measured = inner.inner;
        });
        if (measured - self.empty_height).abs() > 0.5 {
            self.empty_height = measured;
            ui.ctx().request_repaint();
        }
    }

    fn hero(&mut self, ui: &mut Ui, pal: &Pal, w: f32) {
        let d = 118.0;
        let (strip, _) = ui.allocate_exact_size(vec2(w, d + 12.0), Sense::hover());
        let center = pos2(strip.center().x, strip.top() + d / 2.0 + 4.0);
        let well = Rect::from_center_size(center, Vec2::splat(d));
        let resp = ui.interact(well, Id::unique("hero"), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
        let hover = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.15);
        paint_well(ui.painter(), pal, center, d, hover);
        if resp.clicked() {
            self.open_dialog(ui.ctx(), false);
        }

        ui.add_space(18.0);
        let title = text(ui, "Open Gate Fix", 26.0, 680.0, pal.strong, w);
        let (r, _) = ui.allocate_exact_size(vec2(w, title.size().y), Sense::hover());
        ui.painter().galley(pos2(r.center().x - title.size().x / 2.0, r.top()), title, pal.strong);
        ui.add_space(5.0);
        let sub = text(ui, "Drop open gate clips or folders here", 14.0, 420.0, pal.weak, w);
        let (r, _) = ui.allocate_exact_size(vec2(w, sub.size().y), Sense::hover());
        ui.painter().galley(pos2(r.center().x - sub.size().x / 2.0, r.top()), sub, pal.weak);

        ui.add_space(18.0);
        let label = "Or choose clips";
        let size = pill_size(ui, label);
        let (r, _) = ui.allocate_exact_size(vec2(w, size.y), Sense::hover());
        let rect = Rect::from_center_size(r.center(), size);
        if pill(ui, pal, rect, Id::unique("browse"), icon::BROWSE, label).clicked() {
            self.open_dialog(ui.ctx(), false);
        }
    }

    fn about_line(&self, ui: &mut Ui, pal: &Pal, w: f32) {
        let lines = ["Only the size label changes. Nothing is re-encoded.", "The camera's gyro and lens data is kept."];
        for (i, line) in lines.into_iter().enumerate() {
            let g = text(ui, line, 12.0, 450.0, pal.faint, w - 24.0);
            let (r, _) = ui.allocate_exact_size(vec2(w, g.size().y.max(16.0)), Sense::hover());
            // The shield sits before the first line only; both lines are centred as text.
            let x = r.center().x - g.size().x / 2.0;
            if i == 0 {
                paint_icon(ui.painter(), pos2(x - 13.0, r.center().y), icon::SAFE, 14.0, pal.faint);
            }
            ui.painter().galley(pos2(x, r.center().y - g.size().y / 2.0), g, pal.faint);
            ui.add_space(2.0);
        }
    }

    // --- options -------------------------------------------------------------

    fn options(&mut self, ui: &mut Ui, pal: &Pal) {
        let before = (self.settings.output, self.settings.custom_dir.clone(), self.settings.trash_originals);

        card(ui, pal, Margin::symmetric(16, 14), |ui| {
            let desc = match self.settings.output {
                OutputMode::Alongside => format!("Beside the original, as A4182{}.MP4", job::SUFFIX),
                OutputMode::Subfolder => format!("In a “{}” folder beside each clip", job::SUBFOLDER),
                OutputMode::Custom => "All in one folder you choose".to_string(),
            };
            option_row(ui, pal, icon::SAVE, "Save fixed copies", &desc, 0.0);
            ui.add_space(12.0);
            let modes = [OutputMode::Alongside, OutputMode::Subfolder, OutputMode::Custom];
            let selected = modes.iter().position(|m| *m == self.settings.output).unwrap_or(0);
            let labels = ["Same folder", "Subfolder", "One folder"];
            if let Some(i) = segmented(ui, pal, Id::unique("output_mode"), &labels, selected) {
                self.settings.output = modes[i];
            }
            if self.settings.output == OutputMode::Custom {
                ui.add_space(FOLDER_ROW - 34.0);
                self.folder_row(ui, pal);
            }
        });

        ui.add_space(10.0);
        card(ui, pal, Margin::symmetric(16, 14), |ui| {
            let row = option_row(ui, pal, icon::TRASH, "Move originals to the trash", "After a verified fix. Off by default.", 44.0);
            let switch = Rect::from_min_size(pos2(row.right() - 44.0, row.center().y - 13.0), vec2(44.0, 26.0));
            let resp = ui.interact(row, Id::unique("trash_toggle"), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
            if resp.clicked() {
                self.settings.trash_originals = !self.settings.trash_originals;
            }
            let t = ui.ctx().animate_bool_with_time(resp.id, self.settings.trash_originals, 0.14);
            paint_toggle(ui.painter(), pal, switch, t);

        });

        let after = (self.settings.output, self.settings.custom_dir.clone(), self.settings.trash_originals);
        if before != after {
            self.settings.save();
        }
    }

    /// The chosen folder (or a prompt) and a button to change it.
    fn folder_row(&mut self, ui: &mut Ui, pal: &Pal) {
        let w = ui.available_width();
        let (rect, _) = ui.allocate_exact_size(vec2(w, 34.0), Sense::hover());
        let label = if self.settings.custom_dir.is_some() { "Change" } else { "Choose" };
        let size = pill_size(ui, label);
        let button = Rect::from_min_size(pos2(rect.right() - size.x + 4.0, rect.center().y - 15.0), vec2(size.x - 4.0, 30.0));
        if pill(ui, pal, button, Id::unique("choose_folder"), icon::BROWSE, label).clicked() {
            self.open_dialog(ui.ctx(), true);
        }
        let well = Rect::from_min_max(rect.min, pos2(button.left() - 8.0, rect.max.y));
        ui.painter().rect_filled(well, CornerRadius::same(9), pal.track);
        let max = well.width() - 12.0 - 16.0 - 8.0 - 10.0;
        let (shown, color) = match &self.settings.custom_dir {
            Some(d) => (elide_front(ui, &d.display().to_string(), 12.5, max), pal.text),
            None => ("No folder chosen yet".to_string(), pal.amber),
        };
        paint_icon(ui.painter(), pos2(well.left() + 12.0 + 8.0, well.center().y), icon::FOLDER, 15.0, pal.weak);
        let g = text(ui, &shown, 12.5, 450.0, color, max);
        ui.painter().galley(pos2(well.left() + 12.0 + 16.0 + 8.0, well.center().y - g.size().y / 2.0), g, color);
        if let Some(d) = &self.settings.custom_dir {
            ui.interact(well, Id::unique("folder_path"), Sense::hover()).on_hover_text(d.display().to_string());
        }
    }

    fn settings_page(&mut self, ui: &mut Ui, pal: &Pal) {
        ScrollArea::vertical().id_salt("settings").auto_shrink([false, false]).show(ui, |ui| {
            let full_w = ui.available_width();
            let col_w = (full_w - 2.0 * SIDE).min(COLUMN);
            let left = ui.max_rect().left() + (full_w - col_w) / 2.0;
            let column = Rect::from_min_size(pos2(left, ui.cursor().top() + 4.0), vec2(col_w, f32::INFINITY));
            ui.scope_builder(UiBuilder::new().max_rect(column).layout(Layout::top_down(Align::Min)), |ui| {
                let title = text(ui, "Settings", 22.0, 680.0, pal.strong, col_w);
                let (r, _) = ui.allocate_exact_size(vec2(col_w, title.size().y.max(32.0)), Sense::hover());
                ui.painter().galley(pos2(r.left() + 2.0, r.center().y - title.size().y / 2.0), title, pal.strong);
                let size = pill_size(ui, "Done");
                let done = Rect::from_min_size(pos2(r.right() - size.x + 8.0, r.center().y - 15.0), vec2(size.x - 8.0, 30.0));
                if pill(ui, pal, done, Id::unique("done"), icon::DONE, "Done").clicked() {
                    self.show_settings = false;
                }
                ui.add_space(16.0);
                self.options(ui, pal);
                ui.add_space(22.0);
                // Aligned with the text inside the cards above.
                let about = para(ui, crate::ABOUT_NOTE, 12.0, pal.weak, col_w - 66.0);
                let (r, _) = ui.allocate_exact_size(vec2(col_w, about.size().y), Sense::hover());
                paint_icon(ui.painter(), pos2(r.left() + 27.0, r.top() + 8.5), icon::SAFE, 14.0, pal.weak);
                ui.painter().galley(pos2(r.left() + 50.0, r.top()), about, pal.weak);
                ui.add_space(20.0);
            });
        });
    }

    // --- results -------------------------------------------------------------

    fn results_page(&mut self, ui: &mut Ui, pal: &Pal, items: &[Item]) {
        let full = ui.max_rect();
        let w = full.width() - 2.0 * SIDE;
        let left = full.left() + SIDE;

        // Header: the hero, shrunk to a row.
        let header = Rect::from_min_size(pos2(left, full.top()), vec2(w, 52.0));
        let well_c = pos2(header.left() + 24.0, header.center().y);
        let well = Rect::from_center_size(well_c, Vec2::splat(48.0));
        let resp = ui.interact(well, Id::unique("small_hero"), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
        let hover = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.15);
        paint_well(ui.painter(), pal, well_c, 48.0, hover);
        if resp.clicked() {
            self.open_dialog(ui.ctx(), false);
        }
        let label = "Add clips";
        let size = pill_size(ui, label);
        let button = Rect::from_min_size(pos2(header.right() - size.x, header.center().y - size.y / 2.0), size);
        if pill(ui, pal, button, Id::unique("add"), icon::ADD, label).clicked() {
            self.open_dialog(ui.ctx(), false);
        }
        let text_x = well.right() + 14.0;
        let text_w = button.left() - 12.0 - text_x;
        let t = text(ui, "Open Gate Fix", 17.0, 660.0, pal.strong, text_w);
        let s = text(ui, "Drop more clips", 12.5, 420.0, pal.weak, text_w);
        let top = header.center().y - (t.size().y + s.size().y) / 2.0;
        let th = t.size().y;
        ui.painter().galley(pos2(text_x, top), t, pal.strong);
        ui.painter().galley(pos2(text_x, top + th), s, pal.weak);

        // Footer: summary and Clear.
        let footer = Rect::from_min_max(pos2(left, full.bottom() - 50.0), pos2(left + w, full.bottom()));
        self.footer(ui, pal, footer, items);

        // The list.
        let list = Rect::from_min_max(pos2(left, header.bottom() + 14.0), pos2(left + w, footer.top()));
        let p = ui.painter();
        p.add(Shadow { offset: [0, 1], blur: 4, spread: 0, color: pal.shadow }.as_shape(list, 14));
        p.rect_filled(list, CornerRadius::same(14), pal.card);
        p.rect_stroke(list, CornerRadius::same(14), Stroke::new(1.0, pal.line), StrokeKind::Inside);

        let active = items.iter().find(|i| matches!(i.state, State::Working(_))).map(|i| i.id);
        let follow = if active.is_some() && active != self.active { active } else { None };
        self.active = active;

        let inner = list.shrink2(vec2(1.0, 6.0));
        let mut list_ui = ui.new_child(UiBuilder::new().max_rect(inner).layout(Layout::top_down(Align::Min)));
        list_ui.set_clip_rect(list.shrink(1.0));
        ScrollArea::vertical().id_salt("results").auto_shrink([false, false]).show_viewport(&mut list_ui, |ui, viewport| {
            ui.set_height(ROW_H * items.len() as f32);
            let origin = ui.max_rect().top();
            let first = ((viewport.min.y / ROW_H).floor().max(0.0) as usize).min(items.len());
            let last = (((viewport.max.y / ROW_H).ceil() as usize) + 1).min(items.len());
            let x = ui.max_rect().x_range();
            let row_rect = |i: usize| Rect::from_x_y_ranges(x, origin + i as f32 * ROW_H..=origin + (i + 1) as f32 * ROW_H);
            for i in first..last {
                row(ui, pal, row_rect(i), &items[i], i + 1 == items.len());
            }
            if let Some(id) = follow {
                if let Some(i) = items.iter().position(|it| it.id == id) {
                    ui.scroll_to_rect(row_rect(i), None);
                }
            }
        });
    }

    fn footer(&mut self, ui: &mut Ui, pal: &Pal, rect: Rect, items: &[Item]) {
        let count = |f: fn(&State) -> bool| items.iter().filter(|i| f(&i.state)).count();
        let fixed = count(|s| matches!(s, State::Fixed { .. }));
        let skipped = count(|s| matches!(s, State::Skipped(_)));
        let failed = count(|s| matches!(s, State::Failed(_)));
        let done = fixed + skipped + failed;
        let busy = items.len() - done;

        let cy = rect.center().y + 2.0;
        let clear = ghost_button(ui, pal, pos2(rect.right() + 6.0, cy), Id::unique("clear"), icon::CLEAR, "Clear", done > 0);
        if clear.clicked() {
            self.items.lock().unwrap().retain(|i| !i.state.finished());
        }
        let clear = if busy > 0 && done > 0 { clear.on_hover_text("Removes finished clips from the list") } else { clear };

        // "2 fixed · 1 skipped · 1 failed", counts in their state's colour.
        let mut parts: Vec<(String, Color32)> = Vec::new();
        if busy > 0 {
            parts.push((format!("{done} of {} done", items.len()), pal.text));
        } else {
            if fixed > 0 || (skipped == 0 && failed == 0) {
                parts.push((format!("{fixed} fixed"), pal.green));
            }
            if skipped > 0 {
                parts.push((format!("{skipped} skipped"), pal.weak));
            }
            if failed > 0 {
                parts.push((format!("{failed} failed"), pal.red));
            }
        }
        let mut job = LayoutJob::default();
        for (i, (s, color)) in parts.iter().enumerate() {
            if i > 0 {
                job.append("  ·  ", 0.0, text_job("", 13.0, 450.0, pal.faint, 0.0, false).sections[0].format.clone());
            }
            let mut f = text_job("", 13.0, 560.0, *color, 0.0, false).sections[0].format.clone();
            f.color = *color;
            job.append(s, 0.0, f);
        }
        job.wrap = TextWrapping::truncate_at_width(clear.rect.left() - rect.left() - 12.0);
        let g = ui.painter().layout_job(job);
        let mut x = rect.left() + 4.0;
        if busy > 0 {
            let t = ui.input(|i| i.time) as f32;
            ui.painter().circle_stroke(pos2(x + 7.0, cy), 6.0, Stroke::new(1.8, pal.ring));
            paint_arc(ui.painter(), pos2(x + 7.0, cy), 6.0, t * 1.2, 0.3, 1.8, pal.accent);
            x += 22.0;
            ui.ctx().request_repaint_after(Duration::from_millis(16));
        }
        ui.painter().galley(pos2(x, cy - g.size().y / 2.0), g, pal.text);
    }

    // --- frame -----------------------------------------------------------------

    fn handle_input(&mut self, ui: &mut Ui) {
        let dropped: Vec<PathBuf> =
            ui.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).collect());
        if !dropped.is_empty() {
            self.show_settings = false;
            self.add(dropped);
        }
        while let Ok(picked) = self.picked_rx.try_recv() {
            match picked {
                Picked::Files(files) => {
                    self.show_settings = false;
                    self.add(files);
                }
                Picked::Folder(dir) => {
                    self.settings.custom_dir = Some(dir);
                    self.settings.output = OutputMode::Custom;
                    self.settings.save();
                }
            }
        }
        if ui.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::O)) {
            self.open_dialog(ui.ctx(), false);
        }
        if ui.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::Comma)) {
            self.show_settings = !self.show_settings;
        }
        if self.show_settings && ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
            self.show_settings = false;
        }
    }
}

/// One clip in the list: status, name, detail, and a short state on the right.
fn row(ui: &mut Ui, pal: &Pal, rect: Rect, item: &Item, last: bool) {
    let resp = ui.interact(rect, Id::unique(("row", item.id)), Sense::hover());
    let p = ui.painter();
    if resp.hovered() {
        p.rect_filled(rect.shrink2(vec2(6.0, 2.0)), CornerRadius::same(9), pal.hover);
    }
    let icon_c = pos2(rect.left() + 16.0 + 11.0, rect.center().y);
    let text_x = rect.left() + 16.0 + 22.0 + 12.0;
    let right = rect.right() - 16.0;
    if !last {
        p.hline(text_x..=right, rect.bottom() - 0.5, Stroke::new(1.0, pal.line));
    }

    let (right_label, right_color, detail, detail_color, hover): (String, Color32, String, Color32, Option<String>) =
        match &item.state {
            State::Waiting => {
                paint_icon(p, icon_c, icon::WAITING, 19.0, pal.faint);
                (String::new(), pal.weak, "Waiting".into(), pal.faint, None)
            }
            State::Working(f) => {
                if *f <= 0.0 {
                    let t = ui.input(|i| i.time) as f32;
                    p.circle_stroke(icon_c, 8.5, Stroke::new(2.0, pal.ring));
                    paint_arc(p, icon_c, 8.5, t * 1.2, 0.28, 2.0, pal.accent);
                    ui.ctx().request_repaint_after(Duration::from_millis(16));
                    (String::new(), pal.accent_text, "Reading…".into(), pal.weak, None)
                } else {
                    p.circle_stroke(icon_c, 8.5, Stroke::new(2.0, pal.ring));
                    paint_arc(p, icon_c, 8.5, 0.0, f.clamp(0.0, 1.0), 2.0, pal.accent);
                    (format!("{}%", (f * 100.0).floor() as u32), pal.accent_text, "Writing the fixed copy…".into(), pal.weak, None)
                }
            }
            State::Fixed { from, to, name, path, trashed, warning } => {
                let (glyph, color) = if warning.is_some() { (icon::WARNING, pal.amber) } else { (icon::FIXED, pal.green) };
                paint_icon(p, icon_c, glyph, 20.0, color);
                let mut detail = format!("{}×{} · saved as {name}", to.0, to.1);
                let mut full = format!("Was labelled {}×{}, now {}×{}\nSaved as {path}", from.0, from.1, to.0, to.1);
                if *trashed {
                    detail.push_str(" · original in the trash");
                    full.push_str("\nThe original was moved to the trash");
                }
                match warning {
                    Some(w) => ("Fixed".into(), pal.green, w.clone(), pal.amber, Some(format!("{w}\n\n{full}"))),
                    None => ("Fixed".into(), pal.green, detail, pal.weak, Some(full)),
                }
            }
            State::Skipped(size) => {
                paint_icon(p, icon_c, icon::SKIPPED, 20.0, pal.faint);
                ("Skipped".into(), pal.weak, format!("Already {}×{}, nothing to fix", size.0, size.1), pal.weak, None)
            }
            State::Failed(why) => {
                paint_icon(p, icon_c, icon::FAILED, 20.0, pal.red);
                ("Failed".into(), pal.red, why.clone(), pal.weak, Some(why.clone()))
            }
        };

    let label = text(ui, &right_label, 12.5, 560.0, right_color, 90.0);
    let label_w = label.size().x;
    if !right_label.is_empty() {
        ui.painter().galley(pos2(right - label_w, rect.center().y - label.size().y / 2.0 - 8.0), label, right_color);
    }
    let name_w = (right - text_x - if label_w > 0.0 { label_w + 12.0 } else { 0.0 }).max(40.0);
    let name = text(ui, &item.name, 13.5, 560.0, pal.strong, name_w);
    let detail_g = text(ui, &detail, 12.0, 420.0, detail_color, right - text_x);
    let elided = name.elided || detail_g.elided;
    let top = rect.center().y - (name.size().y + 1.0 + detail_g.size().y) / 2.0;
    let nh = name.size().y;
    ui.painter().galley(pos2(text_x, top), name, pal.strong);
    ui.painter().galley(pos2(text_x, top + nh + 1.0), detail_g, detail_color);

    if let Some(h) = hover {
        resp.on_hover_text(h);
    } else if elided {
        resp.on_hover_text(format!("{}\n{detail}", item.name));
    }
}

/// Minimise / maximise / close for the platforms where the window has no native
/// title bar. Returns the left edge of the group.
#[cfg(not(target_os = "macos"))]
fn window_controls(ui: &mut Ui, pal: &Pal, bar: Rect) -> f32 {
    let (w, h) = (40.0, 30.0);
    let mut x = bar.right() - 8.0;
    let cy = bar.top() + BAR_CENTER;
    let buttons = [
        (icon::CLOSE, ViewportCommand::Close, true),
        (icon::MINIMIZE, ViewportCommand::Minimized(true), false),
    ];
    for (i, (glyph, command, danger)) in buttons.into_iter().enumerate() {
        let rect = Rect::from_min_max(pos2(x - w, cy - h / 2.0), pos2(x, cy + h / 2.0));
        let resp = ui.interact(rect, Id::unique(("window_button", i)), Sense::click());
        let p = ui.painter();
        let mut color = pal.weak;
        if resp.hovered() {
            let fill = if danger { rgb(226, 44, 44) } else { pal.hover };
            p.rect_filled(rect, CornerRadius::same(8), fill);
            color = if danger { Color32::WHITE } else { pal.strong };
        }
        let size = if glyph == icon::CLOSE { 17.0 } else { 15.0 };
        paint_icon(p, rect.center(), glyph, size, color);
        if resp.clicked() {
            ui.ctx().send_viewport_cmd(command);
        }
        x -= w + 2.0;
    }
    x
}

/// Invisible strips along the window's edges that start an OS resize, since the
/// window has no native frame here.
#[cfg(not(target_os = "macos"))]
fn resize_handles(ui: &mut Ui) {
    use egui::{CursorIcon as C, ResizeDirection as D};
    if ui.input(|i| i.viewport().maximized.unwrap_or(false)) {
        return;
    }
    let r = ui.max_rect();
    let (e, c) = (5.0, 14.0);
    let zones = [
        (Rect::from_min_max(pos2(r.left() + c, r.top()), pos2(r.right() - c, r.top() + e)), D::North, C::ResizeNorth),
        (Rect::from_min_max(pos2(r.left() + c, r.bottom() - e), pos2(r.right() - c, r.bottom())), D::South, C::ResizeSouth),
        (Rect::from_min_max(pos2(r.left(), r.top() + c), pos2(r.left() + e, r.bottom() - c)), D::West, C::ResizeWest),
        (Rect::from_min_max(pos2(r.right() - e, r.top() + c), pos2(r.right(), r.bottom() - c)), D::East, C::ResizeEast),
        (Rect::from_min_size(r.left_top(), Vec2::splat(c)), D::NorthWest, C::ResizeNorthWest),
        (Rect::from_min_size(pos2(r.right() - c, r.top()), Vec2::splat(c)), D::NorthEast, C::ResizeNorthEast),
        (Rect::from_min_size(pos2(r.left(), r.bottom() - c), Vec2::splat(c)), D::SouthWest, C::ResizeSouthWest),
        (Rect::from_min_size(r.right_bottom() - Vec2::splat(c), Vec2::splat(c)), D::SouthEast, C::ResizeSouthEast),
    ];
    for (i, (rect, dir, cursor)) in zones.into_iter().enumerate() {
        let resp = ui.interact(rect, Id::unique(("resize", i)), Sense::drag()).on_hover_cursor(cursor);
        if resp.drag_started_by(PointerButton::Primary) {
            ui.ctx().send_viewport_cmd(ViewportCommand::BeginResize(dir));
        }
    }
}

/// While files are dragged over the window: a calm full-window target.
fn drop_overlay(ui: &Ui, pal: &Pal, hovering: bool) {
    let t = ui.ctx().animate_bool_with_time(Id::unique("drop_overlay"), hovering, 0.14);
    if t <= 0.0 {
        return;
    }
    let p = ui.ctx().layer_painter(LayerId::new(Order::Foreground, Id::unique("drop_layer")));
    let full = ui.max_rect();
    p.rect_filled(full, 0.0, pal.bg.gamma_multiply(t));
    let target = Rect::from_min_max(pos2(full.left() + 14.0, full.top() + TOP_BAR - 4.0), full.max - vec2(14.0, 14.0));
    p.rect_filled(target, CornerRadius::same(20), pal.tint.gamma_multiply(0.6 * t));
    p.add(Shape::dashed_line(&rounded_outline(target, 20.0), Stroke::new(1.75, pal.accent.gamma_multiply(t)), 8.0, 6.0));

    let grow = 0.94 + 0.06 * t;
    let c = target.center() - vec2(0.0, 34.0);
    let d = 112.0 * grow;
    p.circle_filled(c, d / 2.0, pal.card.gamma_multiply(t));
    p.circle_stroke(c, d / 2.0, Stroke::new(1.0, alpha(pal.accent, 0.4 * t)));
    paint_icon(&p, c, icon::DROP, 46.0 * grow, pal.accent.gamma_multiply(t));
    let title = p.layout_job(text_job("Drop to fix", 22.0, 660.0, pal.strong.gamma_multiply(t), full.width(), false));
    let ty = c.y + d / 2.0 + 20.0;
    let th = title.size().y;
    p.galley(pos2(c.x - title.size().x / 2.0, ty), title, pal.strong);
    let sub = p.layout_job(text_job("Clips or whole folders", 13.5, 420.0, pal.weak.gamma_multiply(t), full.width(), false));
    p.galley(pos2(c.x - sub.size().x / 2.0, ty + th + 4.0), sub, pal.weak);
}

impl eframe::App for App {
    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        Pal::of(visuals.dark_mode).bg.to_normalized_gamma_f32()
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        self.handle_input(ui);
        let hovering = ui.input(|i| !i.raw.hovered_files.is_empty());
        let pal = Pal::of(ui.visuals().dark_mode);
        let full = ui.max_rect();
        ui.painter().rect_filled(full, 0.0, pal.bg);

        let items = self.items.lock().unwrap().clone();
        let bar = Rect::from_min_size(full.min, vec2(full.width(), TOP_BAR));
        self.title_bar(ui, &pal, bar, !items.is_empty());

        let body = Rect::from_min_max(pos2(full.left(), bar.bottom()), full.max);
        let mut body_ui = ui.new_child(UiBuilder::new().max_rect(body).layout(Layout::top_down(Align::Min)));
        if self.show_settings {
            self.settings_page(&mut body_ui, &pal);
        } else if items.is_empty() {
            self.empty_page(&mut body_ui, &pal);
        } else {
            self.results_page(&mut body_ui, &pal, &items);
        }

        #[cfg(not(target_os = "macos"))]
        {
            #[cfg(not(target_os = "windows"))]
            ui.painter().rect_stroke(full, 0.0, Stroke::new(1.0, pal.line), StrokeKind::Inside);
            resize_handles(ui);
        }
        drop_overlay(ui, &pal, hovering);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// egui shows an empty box for a glyph the embedded fonts lack, so check them all.
    #[test]
    fn every_glyph_we_draw_exists() {
        let mut fonts = egui::epaint::text::Fonts::new(Default::default(), font_definitions());
        let mut view = fonts.with_pixels_per_point(2.0);
        let icons = FontId::new(16.0, icon_family());
        for c in icon::ALL {
            assert!(view.has_glyph(&icons, c), "missing icon U+{:04X}", c as u32);
        }
        let strings = [
            "Open Gate Fix",
            "Drop open gate clips or folders here",
            "Or choose clips",
            "Add clips",
            "Drop more clips",
            "Only the size label changes. Nothing is re-encoded.",
            "The camera's gyro and lens data is kept.",
            "Save fixed copies",
            "Beside the original, as A4182_remuxed.MP4",
            "In a “remuxed” folder beside each clip",
            "All in one folder you choose",
            "Same folder Subfolder One folder Change Choose No folder chosen yet",
            "Move originals to the trash",
            "After a verified fix. Off by default.",
            "Settings Done Clear Removes finished clips from the list",
            "Drop to fix Clips or whole folders",
            "Waiting Reading… Writing the fixed copy… 0123456789%",
            "3840×2560 · saved as · original in the trash Was labelled , now Saved as The original was moved to the trash",
            "Fixed Skipped Failed Already , nothing to fix  of  done",
            "…",
            crate::ABOUT_NOTE,
        ];
        let font = FontId::proportional(13.0);
        assert!(!view.has_glyph(&font, '✔'), "the check itself must catch a missing glyph");
        for s in strings {
            for c in s.chars().filter(|c| !c.is_whitespace()) {
                assert!(view.has_glyph(&font, c), "missing {c:?} (U+{:04X}) in {s:?}", c as u32);
            }
        }
    }
}
