//! The HUD: what the game shows over the picture, and where. Whoever has something to show says
//! what it is (a gauge, a tool slot, a card, a notice), never where nor how: every kind of thing
//! has its place, the same in every situation, and the middle of the screen is kept clear for
//! looking.
//!
//! ```text
//!  ┌──────────────────────────────────────────────────────────────┐
//!  │ status              compass · banner                notices  │
//!  │                                                      ·····   │
//!  │                                                              │
//!  │                          +  crosshair                  card  │
//!  │                        prompt                        (what   │
//!  │                                                     is aimed │
//!  │                                                       at)    │
//!  │ gauges             hints                                     │
//!  │ (suit)          tool slots                        readouts   │
//!  └──────────────────────────────────────────────────────────────┘
//! ```
//!
//! One look for all of it — the visor of a suit: plates of dark glass with two corners cut and a
//! hairline round them, lettering in a DIN (the type of instrument panels) with its labels small,
//! capital and spaced, numbers big, one cold accent and the three colours of a caution panel. It
//! is drawn here and nowhere else (`Pen`), scaled to the window, so a new gauge or a new slot
//! looks like the others without anyone drawing it; the windows (menu, catalog, tools) take the
//! same look from `style`.
use egui::{Align2, Color32, CornerRadius, FontId, Pos2, Rect, Shape, Stroke, StrokeKind, Vec2, pos2, vec2};
use std::sync::Arc;

/// How something stands: it colours its bar, its value, its border.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Level {
    #[default]
    Normal,
    Good,
    Caution,
    Warning,
    /// Not in use: dimmed.
    Off,
}

impl Level {
    /// From the levels cards and notes carry (1 caution, 2 warning, 3 off, 4 good).
    pub fn of(n: u8) -> Level {
        match n {
            1 => Level::Caution,
            2 => Level::Warning,
            3 => Level::Off,
            4 => Level::Good,
            _ => Level::Normal,
        }
    }
}

/// A small drawing that says what a thing is without reading it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Icon {
    #[default]
    None,
    /// The jet pack.
    Pack,
    /// A tool that mends and reads (the welder-scanner).
    Welder,
    /// A tool that fires.
    Launcher,
}

/// Something of the suit with a level to watch (gas, air, charge): bottom left.
#[derive(Clone, Debug, Default)]
pub struct Gauge {
    pub label: String,
    /// The key that works it, if any.
    pub key: String,
    /// 0..1.
    pub value: f32,
    /// Its value as read ("71 %").
    pub text: String,
    /// What it is doing ("ENCENDIDA", "APAGADA").
    pub state: String,
    pub level: Level,
    pub icon: Icon,
}

/// A tool of the suit: bottom centre.
#[derive(Clone, Debug, Default)]
pub struct Slot {
    pub key: String,
    pub name: String,
    pub held: bool,
    /// A bar under its name (loading, charge), 0..1, if it has one now.
    pub bar: Option<f32>,
    /// A word about it now ("VISTA", "CARGANDO").
    pub note: String,
    pub icon: Icon,
}

/// What is aimed at: to the right, out of the way of the look.
#[derive(Clone, Debug, Default)]
pub struct Card {
    pub title: String,
    pub value: String,
    pub level: Level,
    pub lines: Vec<String>,
    /// What can be done with it ("Clic: accionar").
    pub hint: String,
}

/// A few words right under the crosshair: what is aimed at, and the key or click that works it.
#[derive(Clone, Debug, Default)]
pub struct Prompt {
    /// The key, as a cap ("E", "Clic"); empty: none.
    pub key: String,
    pub text: String,
    pub level: Level,
}

#[derive(Clone, Debug)]
struct Notice {
    /// What it is about: a new one about the same takes its place.
    about: String,
    text: String,
    level: Level,
    left: f32,
    /// Seconds since it was first said (it comes in over its first moments).
    age: f32,
}

#[derive(Default)]
pub struct Hud {
    pub crosshair: bool,
    pub prompt: Option<Prompt>,
    pub card: Option<Card>,
    pub gauges: Vec<Gauge>,
    pub slots: Vec<Slot>,
    /// Numbers to fly by (name, value): bottom right.
    pub readouts: Vec<(String, String)>,
    /// Lines over the tool slots: how to get out of where you are (a seat, placing something).
    pub hints: Vec<String>,
    /// Lines at the top: a tool that reads the world (the rangefinder).
    pub banner: Vec<String>,
    /// Where one is, in a word or two each (name, value): top left. The world, its pull, the air.
    pub status: Vec<(String, String, Level)>,
    /// What the compass along the top shows (`nav`): the references that mean something where
    /// one is, each as solid as it counts. Empty: no compass.
    pub compass: crate::nav::Compass,
    /// Names over things in the picture (other players): where on the screen (0..1 across and
    /// down), what it says and how solid it is (0..1: it fades with distance).
    pub tags: Vec<([f32; 2], String, f32)>,
    /// The wheel of gestures, while its key is held (`gestures`): low, over the tool slots.
    pub wheel: Option<crate::gestures::WheelView>,
    notices: Vec<Notice>,
}

/// The palette, the HUD's and the windows' (`style`): one look for everything over the picture.
pub const PANEL: Color32 = Color32::from_rgba_premultiplied(7, 10, 14, 226);
pub const EDGE: Color32 = Color32::from_rgba_premultiplied(58, 72, 84, 150);
pub const TEXT: Color32 = Color32::from_rgb(234, 241, 246);
pub const DIM: Color32 = Color32::from_rgb(146, 162, 176);
pub const ACCENT: Color32 = Color32::from_rgb(108, 216, 242);
const GOOD: Color32 = Color32::from_rgb(116, 234, 160);
const CAUTION: Color32 = Color32::from_rgb(255, 192, 72);
const WARNING: Color32 = Color32::from_rgb(255, 98, 82);
/// The top and the bottom of a plate of glass (premultiplied).
const GLASS: (Color32, Color32) = (Color32::from_rgba_premultiplied(16, 22, 29, 238), Color32::from_rgba_premultiplied(6, 9, 12, 246));
/// How much bigger than its design at 1080 lines everything is drawn.
const SCALE: f32 = 1.2;
/// The compass: how many degrees to each side of where one looks its tape shows (`nav`), and
/// from how far over or under one's level a mark says so.
const COMPASS_SPAN: f32 = crate::nav::SPAN;
const COMPASS_RISE: f32 = 4.0;

/// The type: a DIN from the system if it has one (Bahnschrift, with every Windows since 10), and
/// its plain fixed-width face for figures in columns. Without them, egui's own: everything is
/// laid out by what the type measures, so nothing breaks.
pub fn fonts(ctx: &egui::Context) {
    let mut defs = egui::FontDefinitions::default();
    let fonts = std::env::var("WINDIR").map_or_else(|_| "C:/Windows/Fonts".to_string(), |w| format!("{w}/Fonts"));
    let mut put = |name: &str, files: &[&str], family: egui::FontFamily| {
        for f in files {
            if let Ok(bytes) = std::fs::read(format!("{fonts}/{f}")) {
                defs.font_data.insert(name.to_string(), Arc::new(egui::FontData::from_owned(bytes)));
                defs.families.entry(family).or_default().insert(0, name.to_string());
                return;
            }
        }
    };
    put("din", &["bahnschrift.ttf", "segoeui.ttf"], egui::FontFamily::Proportional);
    put("fija", &["consola.ttf"], egui::FontFamily::Monospace);
    ctx.set_fonts(defs);
}

/// The windows (menu, catalog, tools) in the HUD's look: its dark glass, its hairlines, its accent.
pub fn style(ctx: &egui::Context) {
    fonts(ctx);
    ctx.all_styles_mut(|s| {
        let v = &mut s.visuals;
        *v = egui::Visuals::dark();
        v.window_fill = Color32::from_rgba_premultiplied(9, 13, 18, 246);
        v.panel_fill = v.window_fill;
        v.window_stroke = Stroke::new(1.0, Color32::from_rgb(54, 68, 80));
        v.window_corner_radius = CornerRadius::same(3);
        v.menu_corner_radius = CornerRadius::same(3);
        v.window_shadow = egui::Shadow { offset: [0, 10], blur: 32, spread: 0, color: Color32::from_black_alpha(150) };
        v.popup_shadow = egui::Shadow { offset: [0, 6], blur: 18, spread: 0, color: Color32::from_black_alpha(130) };
        v.override_text_color = Some(Color32::from_rgb(212, 222, 230));
        v.selection.bg_fill = Color32::from_rgb(30, 116, 142);
        v.selection.stroke = Stroke::new(1.0, TEXT);
        v.hyperlink_color = ACCENT;
        v.striped = true;
        v.faint_bg_color = Color32::from_rgba_premultiplied(16, 22, 28, 110);
        v.extreme_bg_color = Color32::from_rgb(5, 8, 11);
        v.slider_trailing_fill = true;
        v.handle_shape = egui::style::HandleShape::Rect { aspect_ratio: 0.42 };
        let w = &mut v.widgets;
        let r = CornerRadius::same(2);
        w.noninteractive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(36, 46, 56));
        w.noninteractive.fg_stroke = Stroke::new(1.0, Color32::from_rgb(196, 208, 218));
        w.noninteractive.corner_radius = r;
        w.inactive.weak_bg_fill = Color32::from_rgb(20, 27, 35);
        w.inactive.bg_fill = Color32::from_rgb(28, 38, 48);
        w.inactive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(52, 66, 78));
        w.inactive.fg_stroke = Stroke::new(1.0, Color32::from_rgb(206, 218, 228));
        w.inactive.corner_radius = r;
        w.hovered.weak_bg_fill = Color32::from_rgb(26, 44, 56);
        w.hovered.bg_fill = Color32::from_rgb(34, 60, 74);
        w.hovered.bg_stroke = Stroke::new(1.0, Color32::from_rgb(78, 160, 184));
        w.hovered.fg_stroke = Stroke::new(1.5, TEXT);
        w.hovered.corner_radius = r;
        w.active.weak_bg_fill = Color32::from_rgb(30, 96, 118);
        w.active.bg_fill = Color32::from_rgb(36, 118, 144);
        w.active.bg_stroke = Stroke::new(1.0, ACCENT);
        w.active.fg_stroke = Stroke::new(1.5, Color32::WHITE);
        w.active.corner_radius = r;
        w.open = w.hovered;
        for (id, size) in [(egui::TextStyle::Body, 16.0), (egui::TextStyle::Button, 16.0), (egui::TextStyle::Monospace, 14.0), (egui::TextStyle::Heading, 21.0), (egui::TextStyle::Small, 13.0)] {
            if let Some(f) = s.text_styles.get_mut(&id) {
                f.size = size;
            }
        }
        s.spacing.item_spacing = vec2(10.0, 7.0);
        s.spacing.button_padding = vec2(10.0, 4.0);
        s.spacing.window_margin = egui::Margin::same(14);
        s.spacing.slider_width = 180.0;
        s.spacing.slider_rail_height = 5.0;
        s.spacing.scroll.bar_width = 7.0;
        s.spacing.scroll.floating = false;
    });
}

/// The six corners of a plate: a rectangle with its top-left and bottom-right corners cut by `c`.
fn plate_points(r: Rect, c: f32) -> [Pos2; 6] {
    let c = c.min(r.height() * 0.45).min(r.width() * 0.45);
    [pos2(r.left() + c, r.top()), r.right_top(), pos2(r.right(), r.bottom() - c), pos2(r.right() - c, r.bottom()), r.left_bottom(), pos2(r.left(), r.top() + c)]
}

fn fade(c: Color32, a: f32) -> Color32 {
    c.gamma_multiply(a.clamp(0.0, 1.0))
}

fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * t).round() as u8;
    Color32::from_rgba_premultiplied(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()), l(a.a(), b.a()))
}

/// A plate of glass under `painter`: its gradient, its hairline, the accent on its cut corners
/// and, with a `mark`, a line of that colour down its left side.
pub fn plate(painter: &egui::Painter, r: Rect, k: f32, mark: Option<Color32>, alpha: f32) {
    let pts = plate_points(r, 9.0 * k);
    let mut mesh = egui::Mesh::default();
    for p in pts {
        let t = ((p.y - r.top()) / r.height().max(1.0)).clamp(0.0, 1.0);
        mesh.colored_vertex(p, fade(mix(GLASS.0, GLASS.1, t), alpha));
    }
    for i in 1..5 {
        mesh.add_triangle(0, i, i + 1);
    }
    painter.add(Shape::mesh(mesh));
    painter.add(Shape::closed_line(pts.to_vec(), Stroke::new(1.0, fade(EDGE, alpha))));
    // a light along the top, as glass has
    painter.line_segment([pts[0] + vec2(1.0, 1.0), pts[1] + vec2(-1.0, 1.0)], Stroke::new(1.0, fade(Color32::from_white_alpha(16), alpha)));
    let corner = fade(mark.unwrap_or(Color32::from_rgb(92, 150, 172)), alpha);
    painter.line_segment([pts[5], pts[0]], Stroke::new(1.6 * k, corner));
    painter.line_segment([pts[2], pts[3]], Stroke::new(1.6 * k, corner));
    if let Some(m) = mark {
        let (a, b) = (pts[5] + vec2(1.2 * k, 2.0 * k), pts[4] + vec2(1.2 * k, -1.0));
        painter.line_segment([a, b], Stroke::new(6.0 * k, fade(m, alpha * 0.16)));
        painter.line_segment([a, b], Stroke::new(2.4 * k, fade(m, alpha)));
    }
}

/// The frame of a tool window (the catalog, the inspector, the editor, the figures): the same
/// dark glass as everything else, a hairline round it.
pub fn frame() -> egui::Frame {
    egui::Frame::new().fill(PANEL).stroke(Stroke::new(1.0, EDGE)).corner_radius(CornerRadius::same(2)).inner_margin(egui::Margin::symmetric(16, 12)).shadow(egui::Shadow { offset: [0, 12], blur: 36, spread: 0, color: Color32::from_black_alpha(140) })
}

/// What dresses a window that has been shown at `r`: the accent down its left side, a tick at
/// each corner — the plates' marks, on a window.
pub fn dress(ctx: &egui::Context, r: Rect) {
    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("dress")));
    let (a, b) = (r.left_top() + vec2(1.0, 10.0), r.left_bottom() + vec2(1.0, -10.0));
    painter.line_segment([a, b], Stroke::new(6.0, fade(ACCENT, 0.14)));
    painter.line_segment([a, b], Stroke::new(2.0, ACCENT));
    let tick = Stroke::new(1.4, Color32::from_rgb(92, 150, 172));
    for (c, dx, dy) in [(r.right_top(), -1.0, 1.0), (r.right_bottom(), -1.0, -1.0)] {
        painter.line_segment([c + vec2(dx * 14.0, 0.0), c], tick);
        painter.line_segment([c, c + vec2(0.0, dy * 14.0)], tick);
    }
}

/// The emblem: a world, lit from one side, a small one going round it. In a square of side `s`
/// round `c`.
pub fn emblem(painter: &egui::Painter, c: Pos2, s: f32, t: f32) {
    let r = s * 0.3;
    painter.circle_stroke(c, s * 0.48, Stroke::new(1.0, fade(ACCENT, 0.35)));
    painter.circle_filled(c, r, Color32::from_rgb(34, 44, 54));
    // its lit limb: a crescent, as arcs of two circles
    let mut pts = Vec::with_capacity(34);
    for i in 0..=16 {
        let a = -std::f32::consts::FRAC_PI_2 + std::f32::consts::PI * i as f32 / 16.0;
        pts.push(c + vec2(-a.cos() * r, a.sin() * r));
    }
    for i in (0..=16).rev() {
        let a = -std::f32::consts::FRAC_PI_2 + std::f32::consts::PI * i as f32 / 16.0;
        pts.push(c + vec2(-a.cos() * r * 0.45, a.sin() * r));
    }
    painter.add(Shape::convex_polygon(pts[..17].iter().copied().chain(std::iter::once(c)).collect(), Color32::from_rgb(196, 208, 216), Stroke::NONE));
    painter.add(Shape::convex_polygon(pts[17..].iter().copied().chain(std::iter::once(c)).collect(), Color32::from_rgb(34, 44, 54), Stroke::NONE));
    painter.circle_stroke(c, r, Stroke::new(1.0, Color32::from_rgb(120, 138, 150)));
    let a = t * 0.35;
    painter.circle_filled(c + vec2(a.cos(), a.sin() * 0.42) * s * 0.48, s * 0.045, ACCENT);
}

/// The game's name with its emblem, as a whole screen carries it (the start menu, the start-up
/// screen): its left top at `at`, everything `k` times its size at 900 lines, `alpha` of it
/// there. Gives back where the name is (what goes under it is set from that).
pub fn wordmark(painter: &egui::Painter, at: Pos2, k: f32, t: f32, alpha: f32) -> Rect {
    let name = tracked(painter, crate::GAME, 66.0 * k, fade(TEXT, alpha), 19.0 * k);
    let r = Rect::from_min_size(at + vec2(96.0 * k, 0.0), name.size());
    if alpha > 0.02 {
        emblem(painter, pos2(at.x + 38.0 * k, r.center().y + 2.0 * k), 76.0 * k, t);
    }
    painter.galley(r.min, name, fade(TEXT, alpha));
    r
}

/// Capital letters set apart, as a label is engraved.
fn tracked(painter: &egui::Painter, text: &str, size: f32, color: Color32, spacing: f32) -> Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::default();
    job.append(text, 0.0, egui::TextFormat { font_id: FontId::proportional(size), color, extra_letter_spacing: spacing, ..Default::default() });
    painter.layout_job(job)
}

/// A key in a window, as a cap (the HUD draws its own the same way).
pub fn key_label(ui: &mut egui::Ui, key: &str) {
    let g = ui.painter().layout_no_wrap(key.to_string(), FontId::proportional(14.5), ACCENT);
    let size = vec2(g.size().x + 16.0, g.size().y + 6.0).max(vec2(30.0, 0.0));
    let (r, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    ui.painter().rect(r, CornerRadius::same(2), Color32::from_rgb(18, 26, 34), Stroke::new(1.0, Color32::from_rgb(66, 86, 100)), StrokeKind::Inside);
    ui.painter().line_segment([r.left_bottom() + vec2(2.0, -1.0), r.right_bottom() + vec2(-2.0, -1.0)], Stroke::new(1.0, Color32::from_rgb(40, 110, 132)));
    ui.painter().galley(r.center() - g.size() * 0.5, g, ACCENT);
}

/// A title of a section of a window: its name set apart, a hairline to the edge.
pub fn section(ui: &mut egui::Ui, text: &str) {
    ui.add_space(14.0);
    let g = tracked(ui.painter(), &text.to_uppercase(), 13.0, ACCENT, 2.2);
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), g.size().y + 4.0), egui::Sense::hover());
    let w = g.size().x;
    ui.painter().galley(r.left_top(), g, ACCENT);
    ui.painter().line_segment([pos2(r.left() + w + 12.0, r.center().y), pos2(r.right(), r.center().y)], Stroke::new(1.0, Color32::from_rgb(38, 50, 60)));
    ui.add_space(3.0);
}

/// Seconds a notice fades out over, and comes in over.
const FADE: f32 = 0.6;
const ARRIVE: f32 = 0.22;
/// Notices at once at most (the oldest go).
const NOTICES: usize = 5;

impl Hud {
    /// A new frame: what was said last frame is forgotten (notices stay their time).
    pub fn begin(&mut self, dt: f32) {
        self.crosshair = false;
        self.prompt = None;
        self.card = None;
        self.gauges.clear();
        self.slots.clear();
        self.readouts.clear();
        self.hints.clear();
        self.banner.clear();
        self.status.clear();
        self.tags.clear();
        self.wheel = None;
        self.compass.clear();
        for n in &mut self.notices {
            n.left -= dt;
            n.age += dt;
        }
        self.notices.retain(|n| n.left > 0.0);
    }

    /// Something to tell once, for `secs` seconds. It is `about` something (the jet pack, the
    /// hands): what is said next about the same takes its place instead of piling up.
    pub fn notice(&mut self, about: &str, text: &str, level: Level, secs: f32) {
        if let Some(n) = self.notices.iter_mut().find(|n| n.about == about) {
            if n.text != text {
                n.text = text.to_string();
                n.age = 0.0;
            }
            n.left = secs;
            n.level = level;
            return;
        }
        if self.notices.len() >= NOTICES {
            self.notices.remove(0);
        }
        self.notices.push(Notice { about: about.to_string(), text: text.to_string(), level, left: secs, age: 0.0 });
    }

    /// Whether there is anything to draw.
    pub fn is_empty(&self) -> bool {
        !self.crosshair
            && self.prompt.is_none()
            && self.card.is_none()
            && self.gauges.is_empty()
            && self.slots.is_empty()
            && self.readouts.is_empty()
            && self.hints.is_empty()
            && self.banner.is_empty()
            && self.status.is_empty()
            && self.tags.is_empty()
            && self.wheel.is_none()
            && self.compass.is_empty()
            && self.notices.is_empty()
    }

    pub fn draw(&self, ctx: &egui::Context) {
        let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Background, egui::Id::new("hud")));
        let screen = ctx.content_rect();
        let t = Theme::new(screen);
        let p = Pen { painter: &painter, t: &t };
        let c = screen.center();
        p.visor(screen);
        // ---- over what they name: the tags ----
        for (at, text, alpha) in &self.tags {
            let g = tracked(&painter, text, 12.5 * t.k, fade(TEXT, *alpha), 0.8 * t.k);
            let r = Rect::from_center_size(pos2(screen.left() + screen.width() * at[0], screen.top() + screen.height() * at[1]), g.size() + vec2(14.0, 6.0) * t.k);
            painter.rect_filled(r, 2.0, fade(PANEL, *alpha * 0.8));
            painter.line_segment([r.left_bottom(), r.right_bottom()], Stroke::new(1.0, fade(ACCENT, *alpha * 0.8)));
            painter.galley(r.center() - g.size() * 0.5, g, fade(TEXT, *alpha));
        }
        if self.crosshair {
            p.crosshair(c, self.prompt.as_ref().map(|pr| pr.level));
        }
        if let Some(pr) = &self.prompt {
            p.prompt(pos2(c.x, c.y + 26.0 * t.k), pr);
        }
        // ---- bottom left: the suit's gauges, one over another ----
        let mut y = screen.bottom() - t.margin;
        for g in self.gauges.iter().rev() {
            y = p.gauge(pos2(screen.left() + t.margin, y), g) - t.gap;
        }
        // ---- bottom centre: tool slots in a row, hints over them ----
        let mut top = screen.bottom() - t.margin;
        if !self.slots.is_empty() {
            let (w, h) = (t.slot.x, t.slot.y);
            let total = self.slots.len() as f32 * w + (self.slots.len() as f32 - 1.0) * t.gap;
            let mut x = c.x - total * 0.5;
            for s in &self.slots {
                p.slot(Rect::from_min_size(pos2(x, top - h), vec2(w, h)), s);
                x += w + t.gap;
            }
            top -= h + t.gap;
        }
        for h in self.hints.iter().rev() {
            top = p.hint(pos2(c.x, top), h).top() - t.gap * 0.6;
        }
        // ---- bottom right: readouts ----
        if !self.readouts.is_empty() {
            p.readouts(pos2(screen.right() - t.margin, screen.bottom() - t.margin), &self.readouts);
        }
        // ---- right: the card of what is aimed at ----
        if let Some(card) = &self.card {
            p.card(pos2(screen.right() - t.margin, c.y), card);
        }
        // ---- top left: where one is ----
        let mut x = screen.left() + t.margin;
        for (name, value, level) in &self.status {
            x = p.chip(pos2(x, screen.top() + t.margin), name, value, *level).right() + t.gap;
        }
        // ---- top centre: the compass, the banner under it ----
        let mut y = screen.top() + t.margin * 0.7;
        if !self.compass.is_empty() {
            y = p.compass(pos2(c.x, y), &self.compass) + t.gap;
        }
        for (i, line) in self.banner.iter().enumerate() {
            y = p.banner(pos2(c.x, y), line, i == 0).bottom() + t.gap * 0.5;
        }
        // ---- low in the middle, over the slots: the wheel of gestures while its key is held ----
        if let Some(w) = &self.wheel {
            crate::gestures::draw_wheel(&painter, screen, t.k, w);
        }
        // ---- top right: notices, the newest lowest ----
        let mut y = screen.top() + t.margin;
        for n in &self.notices {
            let arrive = (n.age / ARRIVE).clamp(0.0, 1.0);
            let ease = 1.0 - (1.0 - arrive).powi(3);
            let alpha = (n.left / FADE).clamp(0.0, 1.0) * ease;
            y = p.notice(pos2(screen.right() - t.margin + (1.0 - ease) * 46.0 * t.k, y), n, alpha) + t.gap;
        }
    }
}

/// The menu's ground: the picture dimmed, darker toward the edges, so that the window over it is
/// read and the world is still there.
pub fn backdrop(ctx: &egui::Context, alpha: f32) {
    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Background, egui::Id::new("hud")));
    let r = ctx.content_rect();
    let (mid, rim) = (Color32::from_black_alpha((120.0 * alpha) as u8), Color32::from_black_alpha((215.0 * alpha) as u8));
    let mut mesh = egui::Mesh::default();
    let c = r.center();
    mesh.colored_vertex(c, mid);
    let ring = [r.left_top(), pos2(c.x, r.top()), r.right_top(), pos2(r.right(), c.y), r.right_bottom(), pos2(c.x, r.bottom()), r.left_bottom(), pos2(r.left(), c.y)];
    for p in ring {
        mesh.colored_vertex(p, rim);
    }
    for i in 0..8u32 {
        mesh.add_triangle(0, 1 + i, 1 + (i + 1) % 8);
    }
    painter.add(Shape::mesh(mesh));
}

/// Sizes and colours, scaled to the window (as designed at 1080 lines).
struct Theme {
    k: f32,
    margin: f32,
    gap: f32,
    pad: f32,
    slot: Vec2,
    gauge_w: f32,
    card_w: f32,
    text: Color32,
    dim: Color32,
    accent: Color32,
}

impl Theme {
    fn new(screen: Rect) -> Theme {
        let k = (screen.height() / 1080.0).clamp(0.75, 1.8) * SCALE;
        Theme { k, margin: 26.0 * k, gap: 8.0 * k, pad: 11.0 * k, slot: vec2(206.0, 52.0) * k, gauge_w: 300.0 * k, card_w: 380.0 * k, text: TEXT, dim: DIM, accent: ACCENT }
    }

    fn color(&self, l: Level) -> Color32 {
        match l {
            Level::Normal => self.text,
            Level::Good => GOOD,
            Level::Caution => CAUTION,
            Level::Warning => WARNING,
            Level::Off => self.dim,
        }
    }

    /// The colour of a bar or a mark at a level (the accent when all is well).
    fn bar(&self, l: Level) -> Color32 {
        match l {
            Level::Normal | Level::Good => self.accent,
            Level::Off => Color32::from_rgb(86, 96, 108),
            l => self.color(l),
        }
    }

    fn title(&self) -> FontId {
        FontId::proportional(21.0 * self.k)
    }
    fn body(&self) -> FontId {
        FontId::proportional(17.0 * self.k)
    }
    fn small(&self) -> FontId {
        FontId::proportional(15.0 * self.k)
    }
}

struct Pen<'a> {
    painter: &'a egui::Painter,
    t: &'a Theme,
}

impl Pen<'_> {
    fn plate(&self, r: Rect, mark: Option<Color32>, alpha: f32) {
        plate(self.painter, r, self.t.k, mark, alpha);
    }

    /// The helmet's glass round the picture: its corners darker, a bracket in each, a tick at
    /// the middle of each side — nothing in the way of what is looked at.
    fn visor(&self, screen: Rect) {
        let k = self.t.k;
        // (darker toward the corners only: the middle of every side stays clear)
        let shade = Color32::from_black_alpha(96);
        let d = screen.height() * 0.3;
        for (corner, sx, sy) in [(screen.left_top(), 1.0, 1.0), (screen.right_top(), -1.0, 1.0), (screen.left_bottom(), 1.0, -1.0), (screen.right_bottom(), -1.0, -1.0)] {
            let mut mesh = egui::Mesh::default();
            mesh.colored_vertex(corner, shade);
            mesh.colored_vertex(corner + vec2(sx * d, 0.0), Color32::TRANSPARENT);
            mesh.colored_vertex(corner + vec2(0.0, sy * d), Color32::TRANSPARENT);
            mesh.add_triangle(0, 1, 2);
            self.painter.add(Shape::mesh(mesh));
            let (o, l) = (10.0 * k, 22.0 * k);
            let at = corner + vec2(sx * o, sy * o);
            let ink = Stroke::new(1.2 * k, Color32::from_rgba_premultiplied(74, 104, 120, 150));
            self.painter.line_segment([at, at + vec2(sx * l, 0.0)], ink);
            self.painter.line_segment([at, at + vec2(0.0, sy * l)], ink);
        }
        let ink = Stroke::new(1.0 * k, Color32::from_rgba_premultiplied(58, 80, 94, 120));
        for (m, dir) in [(pos2(screen.left() + 10.0 * k, screen.center().y), vec2(1.0, 0.0)), (pos2(screen.right() - 10.0 * k, screen.center().y), vec2(-1.0, 0.0))] {
            self.painter.line_segment([m, m + dir * 9.0 * k], ink);
            for dy in [-14.0, 14.0] {
                self.painter.line_segment([m + vec2(0.0, dy * k), m + vec2(0.0, dy * k) + dir * 4.0 * k], ink);
            }
        }
    }

    /// A label: small capitals set apart.
    fn label(&self, text: &str, color: Color32) -> Arc<egui::Galley> {
        tracked(self.painter, &text.to_uppercase(), 11.5 * self.t.k, color, 1.7 * self.t.k)
    }

    /// The mark at the middle of the view: a point in a broken ring, which opens and takes the
    /// colour of what can be worked when something is aimed at.
    fn crosshair(&self, c: Pos2, aimed: Option<Level>) {
        let k = self.t.k;
        let (r, color) = match aimed {
            Some(Level::Off) => (8.0 * k, fade(self.t.dim, 0.9)),
            Some(l) => (9.5 * k, self.t.bar(l)),
            None => (7.0 * k, Color32::from_white_alpha(150)),
        };
        self.painter.circle_filled(c, 2.6 * k, Color32::from_black_alpha(110));
        self.painter.circle_filled(c, 1.7 * k, Color32::from_white_alpha(240));
        // four arcs with gaps at the cardinal points
        for q in 0..4 {
            let a0 = std::f32::consts::FRAC_PI_2 * q as f32 + 0.38;
            let pts: Vec<Pos2> = (0..=6).map(|i| a0 + (std::f32::consts::FRAC_PI_2 - 0.76) * i as f32 / 6.0).map(|a| c + vec2(a.cos(), a.sin()) * r).collect();
            self.painter.add(Shape::line(pts.clone(), Stroke::new(2.6 * k, Color32::from_black_alpha(70))));
            self.painter.add(Shape::line(pts, Stroke::new(1.3 * k, color)));
        }
    }

    /// A line of help over the slots, its bottom middle at `at`. Its rectangle.
    fn hint(&self, at: Pos2, text: &str) -> Rect {
        let t = self.t;
        let g = self.painter.layout_no_wrap(text.to_string(), t.small(), t.text);
        let size = g.size() + vec2(t.pad * 2.2, t.pad * 0.8);
        let r = Align2::CENTER_BOTTOM.anchor_size(at, size);
        self.plate(r, None, 0.92);
        self.painter.galley(r.center() - g.size() * 0.5, g, t.text);
        r
    }

    /// A line of a tool that reads the world, its top middle at `at`: the first one large, with
    /// a bracket either side.
    fn banner(&self, at: Pos2, text: &str, first: bool) -> Rect {
        let t = self.t;
        let g = self.painter.layout_no_wrap(text.to_string(), if first { t.body() } else { t.small() }, if first { t.text } else { t.dim });
        let size = g.size() + vec2(t.pad * 2.6, t.pad * 0.8);
        let r = Align2::CENTER_TOP.anchor_size(at, size);
        self.plate(r, first.then_some(t.accent), 0.94);
        self.painter.galley(r.center() - g.size() * 0.5, g, t.text);
        r
    }

    /// A prompt with the middle of its top at `at`: the key as a cap, then the words.
    fn prompt(&self, at: Pos2, pr: &Prompt) {
        let t = self.t;
        let color = t.color(pr.level);
        let words = self.painter.layout_no_wrap(pr.text.clone(), t.small(), color);
        let cap = (!pr.key.is_empty()).then(|| self.keycap_size(&pr.key));
        let cap_w = cap.map_or(0.0, |c| c.x + 9.0 * t.k);
        let size = vec2(cap_w + words.size().x + t.pad * 2.0, words.size().y.max(cap.map_or(0.0, |c| c.y)) + t.pad * 0.9);
        let r = Rect::from_min_size(pos2(at.x - size.x * 0.5, at.y), size);
        self.plate(r, None, 0.94);
        let mut x = r.left() + t.pad;
        if let Some(c) = cap {
            self.keycap(pos2(x, r.center().y - c.y * 0.5), &pr.key, false);
            x += c.x + 9.0 * t.k;
        }
        self.painter.galley(pos2(x, r.center().y - words.size().y * 0.5), words, color);
    }

    fn keycap_size(&self, key: &str) -> Vec2 {
        let t = self.t;
        let g = self.painter.layout_no_wrap(key.to_string(), t.small(), t.text);
        vec2((g.size().y + 5.0 * t.k).max(g.size().x + 12.0 * t.k), g.size().y + 5.0 * t.k)
    }

    /// A key as a cap, its top-left corner at `at`: a hairline box with a line under it; lit, it
    /// is filled.
    fn keycap(&self, at: Pos2, key: &str, lit: bool) -> Rect {
        let t = self.t;
        let r = Rect::from_min_size(at, self.keycap_size(key));
        let ink = if lit { Color32::from_rgb(6, 16, 20) } else { t.accent };
        let g = self.painter.layout_no_wrap(key.to_string(), t.small(), ink);
        if lit {
            self.painter.rect_filled(r, CornerRadius::same(2), t.accent);
        } else {
            self.painter.rect(r, CornerRadius::same(2), Color32::from_rgba_unmultiplied(255, 255, 255, 14), Stroke::new(1.0, Color32::from_rgb(70, 92, 106)), StrokeKind::Inside);
            self.painter.line_segment([r.left_bottom() + vec2(2.0, -1.0), r.right_bottom() + vec2(-2.0, -1.0)], Stroke::new(1.0, fade(t.accent, 0.55)));
        }
        self.painter.galley(r.center() - g.size() * 0.5, g, ink);
        r
    }

    /// A bar of leaning segments filled to `value`, with a scale of ticks under it.
    fn bar(&self, r: Rect, value: f32, color: Color32, segments: usize, ticks: bool) {
        let k = self.t.k;
        let (gap, lean) = (2.0 * k, r.height() * 0.45);
        let w = (r.width() - lean - gap * (segments as f32 - 1.0)) / segments as f32;
        let lit = value.clamp(0.0, 1.0) * segments as f32;
        for i in 0..segments {
            let x = r.left() + (w + gap) * i as f32;
            let quad = |x0: f32, x1: f32| vec![pos2(x0 + lean, r.top()), pos2(x1 + lean, r.top()), pos2(x1, r.bottom()), pos2(x0, r.bottom())];
            self.painter.add(Shape::convex_polygon(quad(x, x + w), Color32::from_white_alpha(20), Stroke::NONE));
            let part = (lit - i as f32).clamp(0.0, 1.0);
            if part > 0.0 {
                self.painter.add(Shape::convex_polygon(quad(x, x + w * part), color, Stroke::NONE));
            }
        }
        if ticks {
            for i in 0..=4 {
                let x = r.left() + (r.width() - lean) * i as f32 / 4.0;
                self.painter.line_segment([pos2(x, r.bottom() + 2.5 * k), pos2(x, r.bottom() + if i % 2 == 0 { 6.5 } else { 4.5 } * k)], Stroke::new(1.0, Color32::from_white_alpha(70)));
            }
        }
    }

    /// A small drawing in a square of side `s` round `c`.
    fn icon(&self, icon: Icon, c: Pos2, s: f32, color: Color32) {
        let at = |x: f32, y: f32| c + vec2(x, y) * s;
        let stroke = Stroke::new((1.4 * self.t.k).max(1.0), color);
        let line = |pts: &[(f32, f32)], closed: bool| {
            let pts: Vec<Pos2> = pts.iter().map(|&(x, y)| at(x, y)).collect();
            self.painter.add(if closed { Shape::closed_line(pts, stroke) } else { Shape::line(pts, stroke) });
        };
        match icon {
            Icon::None => {}
            Icon::Pack => {
                // two bottles on a frame, a nozzle under each, what leaves them
                for side in [-1.0f32, 1.0] {
                    let x = |v: f32| side * v;
                    line(&[(x(0.1), -0.34), (x(0.1), 0.16), (x(0.2), 0.28), (x(0.34), 0.28), (x(0.44), 0.16), (x(0.44), -0.34), (x(0.36), -0.46), (x(0.18), -0.46)], true);
                    line(&[(x(0.22), 0.28), (x(0.17), 0.4), (x(0.37), 0.4), (x(0.32), 0.28)], false);
                    line(&[(x(0.27), 0.46), (x(0.27), 0.5)], false);
                }
                line(&[(-0.1, -0.2), (0.1, -0.2)], false);
                line(&[(-0.1, 0.02), (0.1, 0.02)], false);
            }
            Icon::Welder => {
                // a body with a grip, a nozzle and its spark
                line(&[(-0.48, -0.2), (0.12, -0.2), (0.3, -0.1), (0.3, 0.04), (0.02, 0.06), (-0.06, 0.46), (-0.3, 0.46), (-0.22, 0.06), (-0.48, 0.06)], true);
                line(&[(0.3, -0.03), (0.4, -0.03)], false);
                for (dx, dy) in [(0.1, 0.0), (0.07, -0.09), (0.07, 0.09)] {
                    line(&[(0.44, -0.03), (0.44 + dx, -0.03 + dy)], false);
                }
                line(&[(-0.38, -0.2), (-0.38, -0.32), (-0.12, -0.32), (-0.12, -0.2)], false);
            }
            Icon::Launcher => {
                // a tube open at both ends, a grip, a sight
                line(&[(-0.5, -0.2), (-0.4, -0.14), (0.34, -0.14), (0.48, -0.22), (0.48, 0.14), (0.34, 0.06), (-0.4, 0.06), (-0.5, 0.12)], true);
                line(&[(-0.12, 0.06), (-0.18, 0.44), (0.0, 0.44), (0.02, 0.06)], false);
                line(&[(0.1, -0.14), (0.1, -0.3), (0.2, -0.3)], false);
                line(&[(0.14, 0.06), (0.16, 0.2), (0.3, 0.2)], false);
            }
        }
    }

    /// A gauge with its bottom-left corner at `at`. Its top.
    fn gauge(&self, at: Pos2, g: &Gauge) -> f32 {
        let t = self.t;
        let h = 84.0 * t.k;
        let r = Rect::from_min_size(pos2(at.x, at.y - h), vec2(t.gauge_w, h));
        let color = t.bar(g.level);
        self.plate(r, Some(color), 1.0);
        let inner = r.shrink2(vec2(t.pad + 5.0 * t.k, t.pad));
        // its drawing, its name over what it is doing; its value big to the right; its key by
        // its name; the bar under all
        let mut x = inner.left();
        if g.icon != Icon::None {
            let s = 32.0 * t.k;
            self.icon(g.icon, pos2(x + s * 0.5, inner.top() + s * 0.5), s, if g.level == Level::Off { t.dim } else { color });
            x += s + 10.0 * t.k;
        }
        let name = self.label(&g.label, t.text);
        let name_w = name.size().x;
        self.painter.galley(pos2(x, inner.top()), name, t.text);
        if !g.key.is_empty() {
            self.keycap(pos2(x + name_w + 9.0 * t.k, inner.top() - 4.0 * t.k), &g.key, false);
        }
        let state = self.label(&g.state, t.color(g.level));
        self.painter.galley(pos2(x, inner.top() + 19.0 * t.k), state, t.color(g.level));
        // (its figure and its unit: the unit small, on the figure's line)
        let ink = if g.level == Level::Off { t.dim } else { t.color(g.level) };
        let (figure, unit) = match g.text.split_once(' ') {
            Some((f, u)) => (f, u),
            None => (g.text.as_str(), ""),
        };
        let unit = self.painter.layout_no_wrap(unit.to_string(), t.small(), fade(ink, 0.75));
        let figure = self.painter.layout_no_wrap(figure.to_string(), FontId::proportional(34.0 * t.k), ink);
        let fx = inner.right() - unit.size().x - 4.0 * t.k - figure.size().x;
        let base = inner.top() - 6.0 * t.k + figure.size().y;
        self.painter.galley(pos2(inner.right() - unit.size().x, base - unit.size().y - 3.0 * t.k), unit, ink);
        self.painter.galley(pos2(fx, inner.top() - 6.0 * t.k), figure, ink);
        self.bar(Rect::from_min_max(pos2(inner.left(), inner.bottom() - 15.0 * t.k), pos2(inner.right(), inner.bottom() - 7.0 * t.k)), g.value, color, 30, true);
        r.top()
    }

    fn slot(&self, r: Rect, s: &Slot) {
        let t = self.t;
        self.plate(r, s.held.then_some(t.accent), if s.held { 1.0 } else { 0.78 });
        if s.held {
            // lit from below
            let y = r.bottom() - 1.0;
            self.painter.line_segment([pos2(r.left() + 8.0 * t.k, y), pos2(r.right() - 14.0 * t.k, y)], Stroke::new(5.0 * t.k, fade(t.accent, 0.14)));
            self.painter.line_segment([pos2(r.left() + 8.0 * t.k, y), pos2(r.right() - 14.0 * t.k, y)], Stroke::new(1.6 * t.k, t.accent));
        }
        let inner = r.shrink2(vec2(t.pad, t.pad * 0.7));
        let ink = if s.held { t.text } else { t.dim };
        let cap = self.keycap(pos2(inner.left(), inner.center().y - self.keycap_size(&s.key).y * 0.5), &s.key, s.held);
        let mut x = cap.right() + 9.0 * t.k;
        if s.icon != Icon::None {
            let side = 26.0 * t.k;
            self.icon(s.icon, pos2(x + side * 0.5, inner.center().y), side, if s.held { t.accent } else { fade(t.dim, 0.8) });
            x += side + 8.0 * t.k;
        }
        let busy = !s.note.is_empty() || s.bar.is_some();
        let name = self.painter.layout_no_wrap(s.name.clone(), t.small(), ink);
        let name_y = if busy { inner.top() - 2.0 * t.k } else { inner.center().y - name.size().y * 0.5 };
        self.painter.galley(pos2(x, name_y), name, ink);
        if let Some(b) = s.bar {
            self.bar(Rect::from_min_max(pos2(x, inner.bottom() - 6.0 * t.k), pos2(inner.right(), inner.bottom() - 1.0 * t.k)), b, t.color(Level::Caution), 14, false);
        } else if !s.note.is_empty() {
            let note = self.label(&s.note, t.accent);
            self.painter.galley(pos2(x, inner.bottom() - note.size().y + 2.0 * t.k), note, t.accent);
        }
    }

    /// Readouts with their bottom-right corner at `at`: each a name and its figure on a line, a
    /// hairline between one and the next.
    fn readouts(&self, at: Pos2, rows: &[(String, String)]) {
        let t = self.t;
        let row = 36.0 * t.k;
        let names: Vec<_> = rows.iter().map(|(name, _)| self.label(name, t.dim)).collect();
        let values: Vec<_> = rows.iter().map(|(_, value)| self.painter.layout_no_wrap(value.clone(), t.title(), t.text)).collect();
        let widest = names.iter().zip(&values).map(|(n, v)| n.size().x + v.size().x).fold(0.0, f32::max);
        let (w, h) = ((widest + t.pad * 2.0 + 30.0 * t.k).max(236.0 * t.k), rows.len() as f32 * row + t.pad * 0.6);
        let r = Rect::from_min_size(pos2(at.x - w, at.y - h), vec2(w, h));
        self.plate(r, None, 1.0);
        for (i, (name, value)) in names.into_iter().zip(values).enumerate() {
            let y = r.top() + t.pad * 0.3 + row * i as f32;
            if i > 0 {
                self.painter.line_segment([pos2(r.left() + t.pad, y), pos2(r.right() - t.pad, y)], Stroke::new(1.0, Color32::from_white_alpha(18)));
            }
            self.painter.galley(pos2(r.left() + t.pad + 2.0 * t.k, y + (row - name.size().y) * 0.5 + 1.0 * t.k), name, t.dim);
            self.painter.galley(pos2(r.right() - t.pad - value.size().x, y + (row - value.size().y) * 0.5), value, t.text);
        }
    }

    /// A word or two about where one is, its top-left corner at `at`. Its rectangle.
    fn chip(&self, at: Pos2, name: &str, value: &str, level: Level) -> Rect {
        let t = self.t;
        let name = self.label(name, t.dim);
        let value = self.painter.layout_no_wrap(value.to_string(), t.small(), t.color(level));
        let size = vec2(name.size().x.max(value.size().x) + t.pad * 2.0 + 4.0 * t.k, name.size().y + value.size().y + t.pad * 1.1);
        let r = Rect::from_min_size(at, size);
        self.plate(r, (level != Level::Normal).then(|| t.bar(level)), 0.9);
        let x = r.left() + t.pad + 3.0 * t.k;
        let h = name.size().y;
        self.painter.galley(pos2(x, r.top() + t.pad * 0.5), name, t.dim);
        self.painter.galley(pos2(x, r.top() + t.pad * 0.5 + h + 1.0 * t.k), value, t.color(level));
        r
    }

    /// The compass, the middle of its top at `at`: a tape round one's own way up that slides
    /// under a mark, with every reference that means something now marked on it (`nav`) and the
    /// figures of the scale that counts most under it. Each thing is drawn as solid as it
    /// counts, so nothing on it ever jumps. Its bottom.
    fn compass(&self, at: Pos2, compass: &crate::nav::Compass) -> f32 {
        let t = self.t;
        let k = t.k;
        let (half, h) = (250.0 * k, 30.0 * k);
        let per_degree = half / COMPASS_SPAN;
        let r = Rect::from_min_size(pos2(at.x - half, at.y), vec2(half * 2.0, h));
        // glass that thins out to the sides
        let mut mesh = egui::Mesh::default();
        for (x, a) in [(r.left(), 0.0), (r.left() + half * 0.45, 0.72), (r.right() - half * 0.45, 0.72), (r.right(), 0.0)] {
            mesh.colored_vertex(pos2(x, r.top()), fade(GLASS.0, a));
            mesh.colored_vertex(pos2(x, r.bottom()), fade(GLASS.1, a));
        }
        for i in 0..3u32 {
            mesh.add_triangle(i * 2, i * 2 + 1, i * 2 + 2);
            mesh.add_triangle(i * 2 + 1, i * 2 + 3, i * 2 + 2);
        }
        self.painter.add(Shape::mesh(mesh));
        // the scales of degrees: ticks every five, figures every thirty
        for scale in compass.scales() {
            let first = ((scale.heading - COMPASS_SPAN) / 5.0).ceil() as i32;
            for step in first..=first + 25 {
                let deg = step * 5;
                let off = deg as f32 - scale.heading;
                if off.abs() > COMPASS_SPAN {
                    continue;
                }
                let x = at.x + off * per_degree;
                let near = (1.0 - (off.abs() / COMPASS_SPAN).powi(2)) * scale.alpha;
                let d = deg.rem_euclid(360);
                let (len, ink) = if d % 45 == 0 { (10.0, 235) } else if d % 15 == 0 { (7.0, 150) } else { (4.0, 90) };
                self.painter.line_segment([pos2(x, r.bottom() - len * k), pos2(x, r.bottom())], Stroke::new(1.0, fade(Color32::from_white_alpha(ink), near)));
                if d % 30 == 0 && d % 45 != 0 {
                    self.painter.text(pos2(x, r.top() + 3.0 * k), Align2::CENTER_TOP, format!("{d:03}"), FontId::proportional(11.0 * k), fade(t.dim, near * 0.85));
                }
            }
        }
        // the marks: each where it is; one that waits at the edge, there, on the side to turn to
        for m in compass.marks() {
            let (off, near, out) = m.shown();
            if near <= 0.0 {
                continue;
            }
            let x = at.x + off * per_degree;
            let color = if m.major { t.text } else { t.dim };
            let size = if m.major { 15.0 } else { 12.0 } * k;
            let align = if !out { Align2::CENTER_TOP } else if off < 0.0 { Align2::LEFT_TOP } else { Align2::RIGHT_TOP };
            let text = self.painter.text(pos2(x, r.top() + 1.0 * k), align, &m.text, FontId::proportional(size), fade(if out { t.accent } else { color }, near));
            if out {
                // a small arrow outside it: turn this way
                let (tip, way) = if off < 0.0 { (text.left() - 4.0 * k, -1.0) } else { (text.right() + 4.0 * k, 1.0) };
                let c = pos2(tip, text.center().y);
                self.painter.add(Shape::line(vec![c + vec2(-3.0 * way, -4.0) * k, c + vec2(2.0 * way, 0.0) * k, c + vec2(-3.0 * way, 4.0) * k], Stroke::new(1.5 * k, fade(t.accent, near))));
            } else {
                self.painter.line_segment([pos2(x, r.bottom() - 10.0 * k), pos2(x, r.bottom())], Stroke::new(1.0, fade(Color32::from_white_alpha(235), near)));
            }
            // over or under one's level: which, and by how much
            if m.rise.abs() > COMPASS_RISE {
                let c = pos2(text.center().x, r.bottom() + 6.0 * k);
                let s = if m.rise > 0.0 { -1.0 } else { 1.0 };
                self.painter.add(Shape::convex_polygon(vec![c + vec2(0.0, 3.5 * s) * k, c + vec2(-3.5, -2.5 * s) * k, c + vec2(3.5, -2.5 * s) * k], fade(color, near), Stroke::NONE));
                self.painter.text(c + vec2(6.0 * k, 0.0), Align2::LEFT_CENTER, format!("{:.0}°", m.rise.abs()), FontId::proportional(10.0 * k), fade(t.dim, near));
            }
        }
        self.painter.line_segment([pos2(r.left() + half * 0.2, r.bottom()), pos2(r.right() - half * 0.2, r.bottom())], Stroke::new(1.0, Color32::from_white_alpha(46)));
        // the mark, and under it the figures of the scale that counts most: gone while two count
        // alike (one giving way to another), so they never change at a stroke
        let tip = pos2(at.x, r.bottom() + 1.0);
        self.painter.add(Shape::convex_polygon(vec![tip, tip + vec2(-5.0 * k, 7.0 * k), tip + vec2(5.0 * k, 7.0 * k)], t.accent, Stroke::NONE));
        let Some(main) = compass.main().filter(|s| s.alpha > 0.5) else { return tip.y + 8.0 * k };
        let alpha = (main.alpha * 2.0 - 1.0).clamp(0.0, 1.0);
        let name = self.label(&main.name, fade(t.dim, alpha));
        let figures = self.painter.layout_no_wrap(format!("{:03}°", main.heading.rem_euclid(360.0).round() as i32 % 360), t.small(), fade(t.text, alpha));
        let size = vec2(name.size().x + figures.size().x + t.pad * 2.2, figures.size().y.max(name.size().y) + t.pad * 0.45);
        let plate = Rect::from_min_size(pos2(at.x - size.x * 0.5, tip.y + 8.0 * k), size);
        self.plate(plate, None, 0.9 * alpha);
        self.painter.galley(pos2(plate.left() + t.pad * 0.8, plate.center().y - name.size().y * 0.5), name, fade(t.dim, alpha));
        self.painter.galley(pos2(plate.right() - t.pad * 0.8 - figures.size().x, plate.center().y - figures.size().y * 0.5), figures, fade(t.text, alpha));
        plate.bottom()
    }

    /// The card with the middle of its right side at `at`.
    fn card(&self, at: Pos2, c: &Card) {
        let t = self.t;
        let w = t.card_w - t.pad * 2.0 - 6.0 * t.k;
        let title = self.painter.layout(c.title.clone(), t.title(), t.text, w);
        let value = (!c.value.is_empty()).then(|| self.painter.layout(c.value.clone(), t.body(), t.color(c.level), w));
        let lines: Vec<_> = c.lines.iter().map(|l| self.painter.layout(l.clone(), t.small(), Color32::from_rgb(190, 201, 210), w)).collect();
        let hint = (!c.hint.is_empty()).then(|| self.painter.layout(c.hint.clone(), t.small(), t.accent, w - 14.0 * t.k));
        let space = 6.0 * t.k;
        let rule = if lines.is_empty() && hint.is_none() { 0.0 } else { space * 2.4 };
        let mut h = t.pad * 2.0 + title.size().y;
        if let Some(v) = &value {
            h += space * 0.5 + v.size().y;
        }
        h += rule + lines.iter().map(|l| space + l.size().y).sum::<f32>();
        if let Some(x) = &hint {
            h += space * 1.6 + x.size().y;
        }
        let r = Rect::from_min_size(pos2(at.x - t.card_w, at.y - h * 0.5), vec2(t.card_w, h));
        self.plate(r, Some(t.bar(c.level)), 1.0);
        let mut y = r.top() + t.pad;
        let x = r.left() + t.pad + 6.0 * t.k;
        let put = |y: &mut f32, g: Arc<egui::Galley>, gap: f32, dx: f32, color: Color32| {
            *y += gap;
            let height = g.size().y;
            self.painter.galley(pos2(x + dx, *y), g, color);
            *y += height;
        };
        put(&mut y, title, 0.0, 0.0, t.text);
        if let Some(v) = value {
            put(&mut y, v, space * 0.5, 0.0, t.color(c.level));
        }
        if rule > 0.0 {
            // a hairline under its head, its first stretch in its colour
            let ry = y + rule * 0.5;
            self.painter.line_segment([pos2(x, ry), pos2(r.right() - t.pad, ry)], Stroke::new(1.0, Color32::from_white_alpha(24)));
            self.painter.line_segment([pos2(x, ry), pos2(x + 28.0 * t.k, ry)], Stroke::new(1.6 * t.k, t.bar(c.level)));
            y += rule - space;
        }
        for l in lines {
            put(&mut y, l, space, 0.0, t.dim);
        }
        if let Some(g) = hint {
            put(&mut y, g, space * 1.6, 14.0 * t.k, t.accent);
            // (a small arrow before what can be done)
            let c = pos2(x + 4.0 * t.k, y - 9.0 * t.k);
            self.painter.add(Shape::line(vec![c + vec2(-3.0, -4.0) * t.k, c + vec2(2.0, 0.0) * t.k, c + vec2(-3.0, 4.0) * t.k], Stroke::new(1.5 * t.k, t.accent)));
        }
    }

    /// A notice with its top-right corner at `at`. Its bottom.
    fn notice(&self, at: Pos2, n: &Notice, alpha: f32) -> f32 {
        let t = self.t;
        let color = t.color(n.level);
        let mark = if n.level == Level::Normal { t.accent } else { color };
        let g = self.painter.layout(n.text.clone(), t.body(), fade(color, alpha), 430.0 * t.k);
        let sign = 22.0 * t.k;
        let size = vec2(g.size().x + sign + t.pad * 2.6 + 4.0 * t.k, g.size().y.max(sign) + t.pad * 1.3);
        let r = Rect::from_min_size(pos2(at.x - size.x, at.y), size);
        self.plate(r, Some(mark), alpha);
        // its sign: a point in a ring (told), a tick (done), a mark in a diamond or a triangle
        let c = pos2(r.left() + t.pad + 6.0 * t.k + sign * 0.5, r.top() + t.pad * 0.65 + g.size().y.min(sign * 1.2) * 0.5);
        let ink = Stroke::new(1.5 * t.k, fade(mark, alpha));
        let s = sign * 0.5;
        match n.level {
            Level::Good => {
                self.painter.circle_stroke(c, s * 0.9, ink);
                self.painter.add(Shape::line(vec![c + vec2(-0.42, 0.02) * s, c + vec2(-0.1, 0.36) * s, c + vec2(0.46, -0.32) * s], ink));
            }
            Level::Caution => {
                self.painter.add(Shape::closed_line(vec![c + vec2(0.0, -s), c + vec2(s, 0.0), c + vec2(0.0, s), c + vec2(-s, 0.0)], ink));
                self.painter.line_segment([c + vec2(0.0, -0.42) * s, c + vec2(0.0, 0.12) * s], ink);
                self.painter.circle_filled(c + vec2(0.0, 0.42) * s, 1.2 * t.k, ink.color);
            }
            Level::Warning => {
                self.painter.add(Shape::closed_line(vec![c + vec2(0.0, -0.95) * s, c + vec2(1.0, 0.8) * s, c + vec2(-1.0, 0.8) * s], ink));
                self.painter.line_segment([c + vec2(0.0, -0.36) * s, c + vec2(0.0, 0.24) * s], ink);
                self.painter.circle_filled(c + vec2(0.0, 0.52) * s, 1.2 * t.k, ink.color);
            }
            _ => {
                self.painter.circle_stroke(c, s * 0.9, ink);
                self.painter.circle_filled(c, 2.0 * t.k, ink.color);
            }
        }
        self.painter.galley(pos2(c.x + sign * 0.5 + t.pad * 0.8, r.top() + t.pad * 0.65), g, fade(color, alpha));
        r.bottom()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notices_stay_their_time_and_what_is_said_each_frame_does_not() {
        let mut h = Hud::default();
        assert!(h.is_empty());
        h.gauges.push(Gauge { label: "MOCHILA".into(), value: 0.5, ..Gauge::default() });
        h.notice("manos", "Soldador en la mano", Level::Normal, 3.0);
        h.notice("manos", "Manos libres", Level::Normal, 3.0);
        assert_eq!(h.notices.len(), 1, "what is said about the same thing takes the place of what was");
        h.begin(1.0);
        assert!(h.gauges.is_empty() && !h.is_empty(), "gauges are said again each frame; the notice stays");
        h.begin(2.5);
        assert!(h.is_empty());
        for k in 0..9 {
            h.notice(&format!("cosa {k}"), &format!("aviso {k}"), Level::Caution, 5.0);
        }
        assert_eq!(h.notices.len(), NOTICES);
        assert_eq!(Level::of(2), Level::Warning);
    }

    #[test]
    fn it_draws_whatever_it_is_given() {
        // every kind of thing at once, in a small window and a big one: nothing panics, and
        // something is drawn
        for size in [vec2(960.0, 540.0), vec2(2560.0, 1440.0)] {
            let ctx = egui::Context::default();
            let mut h = Hud { crosshair: true, prompt: Some(Prompt { key: "Clic".into(), text: "Rampa · ABIERTA".into(), level: Level::Normal }), ..Hud::default() };
            h.card = Some(Card { title: "Rampa".into(), value: "ABIERTA".into(), level: Level::Caution, lines: vec!["Abre o cierra la rampa de la bodega con sus dos cabrestantes eléctricos.".into()], hint: "Clic: accionar".into() });
            h.gauges.push(Gauge { label: "MOCHILA".into(), key: "J".into(), value: 0.71, text: "71 %".into(), state: "ENCENDIDA".into(), level: Level::Normal, icon: Icon::Pack });
            h.slots.push(Slot { key: "1".into(), name: "Soldador-escáner".into(), held: true, bar: None, note: "VISTA".into(), icon: Icon::Welder });
            h.slots.push(Slot { key: "2".into(), name: "Lanzacohetes".into(), held: false, bar: Some(0.4), note: String::new(), icon: Icon::Launcher });
            h.readouts.push(("ALTURA".into(), "12,4 m".into()));
            h.hints.push("Espacio: levantarse".into());
            h.banner.push("TELÉMETRO 12,3 m".into());
            h.status.push(("Entorno".into(), "VACÍO".into(), Level::Caution));
            let moon: lunar_core::body::BodyDef = lunar_core::defs::parse("luna", include_str!("../../../assets/defs/bodies/luna.jsonc")).unwrap();
            let bodies = lunar_core::body::BodyRegistry::new(vec![lunar_core::body::Body::from_def("luna", &moon).unwrap()]);
            let nav = crate::nav::Nav::load(&crate::root().join("assets/defs/navegacion.jsonc")).unwrap();
            let at = bodies.get(0).above_ground(glam::DVec3::Y, 2.0);
            let who = crate::nav::Who { at, vel: glam::DVec3::ZERO, up: glam::DVec3::Y, ahead: glam::DVec3::new(0.3, 0.0, -0.95).normalize(), ship: None, sun: glam::DVec3::X };
            nav.compass(&bodies, &who, &mut h.compass);
            assert!(h.compass.main().is_some_and(|s| s.name == "RUMBO") && h.compass.marks().len() == 8);
            for (k, level) in [Level::Normal, Level::Good, Level::Caution, Level::Warning].into_iter().enumerate() {
                h.notice(&format!("aviso {k}"), "Mochila encendida", level, 3.0);
            }
            let input = egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)), ..Default::default() };
            let mut out = ctx.run_ui(input, |ui| {
                h.draw(ui.ctx());
                backdrop(ui.ctx(), 1.0);
            });
            assert!(out.shapes.len() > 120, "{} shapes", out.shapes.len());
            // (nobody draws this frame: its textures are not uploaded anywhere)
            out.textures_delta.clear();
        }
    }

    #[test]
    fn a_plate_is_cut_at_two_corners_and_never_more_than_it_has() {
        let r = Rect::from_min_size(pos2(10.0, 20.0), vec2(200.0, 60.0));
        let p = plate_points(r, 9.0);
        assert_eq!(p[0], pos2(19.0, 20.0));
        assert_eq!(p[3], pos2(201.0, 80.0));
        // a plate smaller than its cut is still a shape
        let tiny = plate_points(Rect::from_min_size(Pos2::ZERO, vec2(8.0, 6.0)), 9.0);
        assert!(tiny[0].x < 8.0 && tiny[5].y < 6.0);
    }
}
