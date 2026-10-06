//! The game's look, for the launcher: the visor of a suit — plates of dark glass with two corners
//! cut and a hairline round them, a DIN for the lettering, labels small, capital and spaced, one
//! cold accent. It is the HUD's (`crates/app/src/hud.rs`: `style`, `plate`, `emblem`, `key_label`,
//! `section`) and the menu's tabs (`crates/app/src/ui.rs`), copied here because the launcher
//! cannot depend on the game; what the launcher adds of its own is the big button, the cards of
//! the editions and the rows of its lists. Nothing here moves by itself: a window that is not
//! touched is not drawn again.
use egui::{Color32, CornerRadius, FontId, Pos2, Rect, Response, Sense, Shape, Stroke, StrokeKind, pos2, vec2};
use std::sync::Arc;

/// The palette: the HUD's.
pub const TEXT: Color32 = Color32::from_rgb(234, 241, 246);
pub const DIM: Color32 = Color32::from_rgb(146, 162, 176);
pub const ACCENT: Color32 = Color32::from_rgb(108, 216, 242);
pub const GOOD: Color32 = Color32::from_rgb(116, 234, 160);
pub const WARNING: Color32 = Color32::from_rgb(255, 98, 82);
/// What is put away, or waits: an amber that does not shout.
pub const AMBER: Color32 = Color32::from_rgb(232, 178, 96);
/// What is read at length (the news): a little softer than `TEXT`.
pub const BODY: Color32 = Color32::from_rgb(206, 216, 225);
const EDGE: Color32 = Color32::from_rgba_premultiplied(58, 72, 84, 150);
/// A hairline inside a plate.
pub const RULE: Color32 = Color32::from_rgb(40, 52, 62);
/// The top and the bottom of a plate of glass (premultiplied).
const GLASS: (Color32, Color32) = (Color32::from_rgba_premultiplied(16, 22, 29, 238), Color32::from_rgba_premultiplied(6, 9, 12, 246));

/// The type: a DIN from the system if it has one (Bahnschrift, with every Windows since 10), and
/// its plain fixed-width face for what is typed. Without them, egui's own: everything is laid out
/// by what the type measures, so nothing breaks.
fn fonts(ctx: &egui::Context) {
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

/// egui's widgets in the HUD's look: its dark glass, its hairlines, its accent.
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
        v.faint_bg_color = Color32::from_rgba_premultiplied(16, 22, 28, 110);
        v.extreme_bg_color = Color32::from_rgb(5, 8, 11);
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
        for (id, size) in [(egui::TextStyle::Body, 16.0), (egui::TextStyle::Button, 16.0), (egui::TextStyle::Monospace, 13.0), (egui::TextStyle::Heading, 21.0), (egui::TextStyle::Small, 13.0)] {
            if let Some(f) = s.text_styles.get_mut(&id) {
                f.size = size;
            }
        }
        s.spacing.item_spacing = vec2(10.0, 7.0);
        s.spacing.button_padding = vec2(12.0, 6.0);
        s.spacing.interact_size.y = 30.0;
        s.spacing.icon_width = 18.0;
        s.spacing.scroll.bar_width = 7.0;
        s.spacing.scroll.floating = false;
        // (what is written is read, not picked up: what can be copied has its button)
        s.interaction.selectable_labels = false;
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

/// A plate's shape filled from `top` to `bottom`.
fn fill(painter: &egui::Painter, pts: &[Pos2; 6], r: Rect, top: Color32, bottom: Color32) {
    let mut mesh = egui::Mesh::default();
    for p in pts {
        let t = ((p.y - r.top()) / r.height().max(1.0)).clamp(0.0, 1.0);
        mesh.colored_vertex(*p, mix(top, bottom, t));
    }
    for i in 1..5 {
        mesh.add_triangle(0, i, i + 1);
    }
    painter.add(Shape::mesh(mesh));
}

/// A plate of glass under `painter`: its gradient, its hairline, the accent on its cut corners
/// and, with a `mark`, a line of that colour down its left side.
pub fn plate(painter: &egui::Painter, r: Rect, k: f32, mark: Option<Color32>, alpha: f32) {
    let pts = plate_points(r, 9.0 * k);
    fill(painter, &pts, r, fade(GLASS.0, alpha), fade(GLASS.1, alpha));
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

/// The emblem: a world, lit from one side, a small one on its way round it. In a square of side
/// `s` round `c`.
pub fn emblem(painter: &egui::Painter, c: Pos2, s: f32) {
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
    let a = -0.62f32;
    painter.circle_filled(c + vec2(a.cos(), a.sin() * 0.42) * s * 0.48, s * 0.045, ACCENT);
}

/// Letters set apart, as a label is engraved: the text laid out in one line.
pub fn tracked(painter: &egui::Painter, text: &str, size: f32, color: Color32, spacing: f32) -> Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::default();
    job.append(text, 0.0, egui::TextFormat { font_id: FontId::proportional(size), color, extra_letter_spacing: spacing, ..Default::default() });
    painter.layout_job(job)
}

/// A title of a section: its name set apart, a hairline to the edge.
pub fn section(ui: &mut egui::Ui, text: &str) {
    ui.add_space(10.0);
    let g = tracked(ui.painter(), &text.to_uppercase(), 13.0, ACCENT, 2.2);
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), g.size().y + 4.0), Sense::hover());
    let w = g.size().x;
    ui.painter().galley(r.left_top(), g, ACCENT);
    ui.painter().line_segment([pos2(r.left() + w + 12.0, r.center().y), pos2(r.right(), r.center().y)], Stroke::new(1.0, Color32::from_rgb(38, 50, 60)));
    ui.add_space(3.0);
}

/// The lettering of a plate's tabs, and the room between them, as they are (`roomy`) or come
/// closer for a narrow plate: the size, the spacing of the letters and the gap.
pub fn tab_lettering(roomy: bool) -> (f32, f32, f32) {
    if roomy { (15.0, 2.3, 26.0) } else { (13.5, 1.5, 17.0) }
}

/// A tab of a plate: a word, with the accent under the one that is open.
pub fn tab(ui: &mut egui::Ui, name: &str, on: bool, roomy: bool) -> Response {
    let (size, spacing, _) = tab_lettering(roomy);
    let g = tracked(ui.painter(), name, size, Color32::PLACEHOLDER, spacing);
    let (r, resp) = ui.allocate_exact_size(g.size() + vec2(0.0, 14.0), Sense::click());
    let ink = if on {
        TEXT
    } else if resp.hovered() {
        Color32::from_rgb(200, 226, 236)
    } else {
        DIM
    };
    ui.painter().galley(r.left_top() + vec2(0.0, 2.0), g, ink);
    if on {
        let line = [r.left_bottom() + vec2(0.0, -1.0), r.right_bottom() + vec2(0.0, -1.0)];
        ui.painter().line_segment(line, Stroke::new(5.0, ACCENT.gamma_multiply(0.18)));
        ui.painter().line_segment(line, Stroke::new(2.0, ACCENT));
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// The fill, the edge and the ink of a thing to choose: dull if it cannot be chosen, lit if it is
/// the one chosen, a little lit under the pointer.
fn choice_colours(enabled: bool, on: bool, hovered: bool) -> (Color32, Color32, Color32) {
    if !enabled {
        (Color32::from_rgb(13, 18, 24), Color32::from_rgb(36, 46, 56), DIM.gamma_multiply(0.55))
    } else if on {
        (Color32::from_rgb(24, 82, 102), ACCENT, Color32::WHITE)
    } else if hovered {
        (Color32::from_rgb(26, 44, 56), Color32::from_rgb(78, 160, 184), TEXT)
    } else {
        (Color32::from_rgb(16, 22, 29), Color32::from_rgb(52, 66, 78), Color32::from_rgb(190, 204, 216))
    }
}

/// One of a few things to choose from, `width` wide at least: a word in a box, lit when it is the
/// one chosen.
pub fn choice(ui: &mut egui::Ui, text: &str, on: bool, width: f32) -> Response {
    let g = tracked(ui.painter(), text, 13.5, Color32::PLACEHOLDER, 1.6);
    let size = vec2(width.max(g.size().x + 24.0), 30.0);
    let (r, resp) = ui.allocate_exact_size(size, Sense::click());
    let (bg, edge, ink) = choice_colours(ui.is_enabled(), on, resp.hovered());
    ui.painter().rect(r, CornerRadius::same(2), bg, Stroke::new(1.0, edge), StrokeKind::Inside);
    ui.painter().galley(r.center() - g.size() * 0.5, g, ink);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A few things to choose one from, side by side in one box exactly `width` wide, each as wide as
/// its word asks: the one chosen (`on`) is lit. Their words are lettered as a `choice`'s if the
/// width lets them, and closer and smaller, step by step, until they fit. The one that was
/// pressed, if any.
pub fn segments(ui: &mut egui::Ui, names: &[&str], on: Option<usize>, width: f32) -> Option<usize> {
    /// The least room at each side of a word.
    const MARGIN: f32 = 11.0;
    /// The letterings to try, widest first: size and spacing.
    const LETTERINGS: [(f32, f32); 6] = [(13.5, 1.6), (13.0, 1.4), (12.5, 1.3), (12.5, 1.0), (12.0, 0.7), (11.5, 0.3)];
    let count = names.len().max(1) as f32;
    let mut spacing = 0.0;
    let mut words = Vec::new();
    // (the spacing after a word's last letter is not part of it)
    let taken = |words: &[Arc<egui::Galley>], spacing: f32| words.iter().map(|g| g.size().x - spacing).sum::<f32>();
    for (size, apart) in LETTERINGS {
        spacing = apart;
        words = names.iter().map(|name| tracked(ui.painter(), name, size, Color32::PLACEHOLDER, spacing)).collect();
        if taken(&words, spacing) + 2.0 * MARGIN * count <= width {
            break;
        }
    }
    let room = ((width - taken(&words, spacing)) / (2.0 * count)).max(1.0);
    let mut pressed = None;
    let enabled = ui.is_enabled();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        let last = words.len().saturating_sub(1);
        let mut left = width;
        let mut cells = Vec::with_capacity(words.len());
        for (n, g) in words.into_iter().enumerate() {
            // (the last one takes what is left: the box comes out exactly as wide as asked)
            let wide = if n == last { left } else { (g.size().x - spacing + 2.0 * room).min(left) };
            left -= wide;
            let (r, resp) = ui.allocate_exact_size(vec2(wide, 30.0), Sense::click());
            if resp.clicked() {
                pressed = Some(n);
            }
            let (chosen, hovered) = (on == Some(n), resp.hovered());
            resp.on_hover_cursor(egui::CursorIcon::PointingHand);
            cells.push((n, r, g, chosen, hovered));
        }
        // neighbours share an edge; the lit ones are painted last, their edge over the dull ones'
        cells.sort_by_key(|(_, _, _, chosen, hovered)| (*chosen, *hovered));
        for (n, r, g, chosen, hovered) in cells {
            let (bg, edge, ink) = choice_colours(enabled, chosen, hovered);
            let (first, end) = (if n == 0 { 2 } else { 0 }, if n == last { 2 } else { 0 });
            let boxed = Rect::from_min_max(pos2(r.left() - if n == 0 { 0.0 } else { 1.0 }, r.top()), r.max);
            ui.painter().rect(boxed, CornerRadius { nw: first, sw: first, ne: end, se: end }, bg, Stroke::new(1.0, edge), StrokeKind::Inside);
            ui.painter().galley(pos2(r.center().x - (g.size().x - spacing) * 0.5, r.center().y - g.size().y * 0.5), g, ink);
        }
    });
    pressed
}

/// A line of text to type in, `width` wide and as tall as a choice: dark glass in a hairline, the
/// accent round it while it has the keyboard. It takes `most` characters at most; while it is
/// empty it shows `hint`, dim.
pub fn field(ui: &mut egui::Ui, text: &mut String, width: f32, most: usize, hint: &str) -> Response {
    ui.scope(|ui| {
        ui.visuals_mut().selection.stroke = Stroke::new(1.0, ACCENT);
        let hint = egui::RichText::new(hint).color(DIM.gamma_multiply(0.7));
        ui.add(egui::TextEdit::singleline(text).desired_width(width).min_size(vec2(width, 30.0)).margin(egui::Margin { left: 10, right: 10, top: 6, bottom: 3 }).vertical_align(egui::Align::Center).char_limit(most).hint_text(hint).text_color(TEXT))
    })
    .inner
}

/// A plain button of the launcher, `width` wide: a small plate with its word set apart.
pub fn button(ui: &mut egui::Ui, text: &str, width: f32) -> Response {
    let g = tracked(ui.painter(), text, 13.5, Color32::PLACEHOLDER, 1.8);
    let size = vec2(width.max(g.size().x + 26.0), 34.0);
    let (r, resp) = ui.allocate_exact_size(size, Sense::click());
    let lit = ui.is_enabled() && resp.hovered();
    let pts = plate_points(r, 7.0);
    let (top, bottom) = if resp.is_pointer_button_down_on() {
        (Color32::from_rgb(30, 96, 118), Color32::from_rgb(24, 78, 98))
    } else if lit {
        (Color32::from_rgb(30, 50, 62), Color32::from_rgb(18, 32, 42))
    } else {
        (Color32::from_rgb(22, 30, 38), Color32::from_rgb(12, 17, 23))
    };
    fill(ui.painter(), &pts, r, top, bottom);
    ui.painter().add(Shape::closed_line(pts.to_vec(), Stroke::new(1.0, if lit { Color32::from_rgb(78, 160, 184) } else { Color32::from_rgb(58, 74, 88) })));
    let ink = if !ui.is_enabled() {
        DIM.gamma_multiply(0.6)
    } else if lit {
        TEXT
    } else {
        Color32::from_rgb(206, 218, 228)
    };
    ui.painter().galley(r.center() - g.size() * 0.5, g, ink);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// The big button, the one thing to press: a plate of the accent with a glow round it, its word
/// large and, under it, what it starts ("V34 · DEMO"). Without `ready` it is dull and says why.
pub fn big_button(ui: &mut egui::Ui, size: egui::Vec2, word: &str, under: &str, ready: bool) -> Response {
    let (r, resp) = ui.allocate_exact_size(size, if ready { Sense::click() } else { Sense::hover() });
    let painter = ui.painter();
    let pts = plate_points(r, 14.0);
    let hot = ready && resp.hovered();
    let down = ready && resp.is_pointer_button_down_on();
    if ready {
        // a glow, stronger under the pointer
        let glow = if hot { 0.34 } else { 0.2 };
        for (grow, a) in [(9.0, 0.10), (5.5, 0.22), (2.5, 0.45)] {
            painter.add(Shape::closed_line(plate_points(r.expand(grow * 0.5), 14.0 + grow * 0.3).to_vec(), Stroke::new(grow, ACCENT.gamma_multiply(glow * a))));
        }
    }
    let (top, bottom) = match (ready, down, hot) {
        (false, _, _) => (Color32::from_rgb(24, 31, 39), Color32::from_rgb(14, 19, 25)),
        (_, true, _) => (Color32::from_rgb(22, 104, 130), Color32::from_rgb(16, 78, 100)),
        (_, _, true) => (Color32::from_rgb(52, 168, 200), Color32::from_rgb(22, 104, 132)),
        _ => (Color32::from_rgb(40, 146, 176), Color32::from_rgb(18, 88, 112)),
    };
    fill(painter, &pts, r, top, bottom);
    painter.add(Shape::closed_line(pts.to_vec(), Stroke::new(1.2, if ready { Color32::from_rgb(150, 232, 250) } else { Color32::from_rgb(52, 66, 78) })));
    painter.line_segment([pts[0] + vec2(1.0, 1.5), pts[1] + vec2(-1.5, 1.5)], Stroke::new(1.0, Color32::from_white_alpha(if ready { 70 } else { 14 })));
    let (ink, soft) = if ready { (Color32::WHITE, Color32::from_rgb(214, 244, 252)) } else { (DIM, DIM.gamma_multiply(0.7)) };
    let big = tracked(painter, word, 32.0, ink, 10.0);
    // (what it starts is told whole: its letters come closer if the button is narrow for them)
    let mut small = tracked(painter, under, 14.0, soft, 2.6);
    if small.size().x > r.width() - 28.0 {
        small = tracked(painter, under, 13.0, soft, 1.2);
    }
    if small.size().x > r.width() - 20.0 {
        small = line(painter, under, 13.0, soft, r.width() - 24.0);
    }
    // (the spacing after the last letter is not part of the word: it is set back by half of it)
    let c = r.center() + vec2(0.0, if down { 1.0 } else { 0.0 });
    let gap = 6.0;
    let top_y = c.y - (big.size().y + gap + small.size().y) * 0.5;
    painter.galley(pos2(c.x - (big.size().x - 10.0) * 0.5, top_y), big.clone(), ink);
    painter.galley(pos2(c.x - (small.size().x - 2.6) * 0.5, top_y + big.size().y + gap), small, soft);
    if ready { resp.on_hover_cursor(egui::CursorIcon::PointingHand) } else { resp }
}

/// A key, as a cap, its right end at `right`. How wide it came out.
pub fn key_cap(painter: &egui::Painter, right: Pos2, key: &str) -> f32 {
    let g = painter.layout_no_wrap(key.to_string(), FontId::proportional(12.0), ACCENT);
    let size = vec2((g.size().x + 14.0).max(26.0), g.size().y + 6.0);
    let r = Rect::from_min_size(right - vec2(size.x, 0.0), size);
    painter.rect(r, CornerRadius::same(2), Color32::from_rgb(18, 26, 34), Stroke::new(1.0, Color32::from_rgb(66, 86, 100)), StrokeKind::Inside);
    painter.line_segment([r.left_bottom() + vec2(2.0, -1.0), r.right_bottom() + vec2(-2.0, -1.0)], Stroke::new(1.0, Color32::from_rgb(40, 110, 132)));
    painter.galley(r.center() - g.size() * 0.5, g, ACCENT);
    size.x
}

/// How an edition's card stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CardState {
    /// It is the one that is started.
    Chosen,
    /// It can be chosen.
    Free,
    /// It cannot: what it says is why.
    Missing,
}

/// An edition's card, `width` wide: its key, its name, and under it a line or two of what it is
/// (or of why it cannot be chosen). The one chosen is lit; one that cannot be is dull.
pub fn card(ui: &mut egui::Ui, width: f32, key: &str, name: &str, says: &str, state: CardState) -> Response {
    const LEFT: f32 = 48.0;
    let mut job = egui::text::LayoutJob::single_section(says.to_string(), egui::TextFormat { font_id: FontId::proportional(13.0), line_height: Some(16.5), color: Color32::PLACEHOLDER, ..Default::default() });
    job.wrap = egui::text::TextWrapping { max_width: width - LEFT - 14.0, max_rows: 2, ..Default::default() };
    let words = ui.painter().layout_job(job);
    let size = vec2(width, 76.0);
    let (r, resp) = ui.allocate_exact_size(size, if state == CardState::Free { Sense::click() } else { Sense::hover() });
    let painter = ui.painter();
    let hot = state == CardState::Free && resp.hovered();
    let pts = plate_points(r, 9.0);
    let (top, bottom, edge, ink, soft) = match state {
        CardState::Chosen => (Color32::from_rgba_premultiplied(20, 70, 88, 244), Color32::from_rgba_premultiplied(10, 38, 50, 248), ACCENT, Color32::WHITE, Color32::from_rgb(206, 232, 240)),
        CardState::Free if hot => (Color32::from_rgba_premultiplied(24, 38, 48, 242), Color32::from_rgba_premultiplied(12, 20, 27, 246), Color32::from_rgb(78, 160, 184), TEXT, BODY),
        CardState::Free => (GLASS.0, GLASS.1, Color32::from_rgb(58, 74, 88), Color32::from_rgb(214, 224, 232), DIM),
        CardState::Missing => (Color32::from_rgba_premultiplied(10, 14, 18, 214), Color32::from_rgba_premultiplied(5, 7, 10, 222), Color32::from_rgb(34, 42, 50), DIM.gamma_multiply(0.62), DIM.gamma_multiply(0.72)),
    };
    fill(painter, &pts, r, top, bottom);
    painter.add(Shape::closed_line(pts.to_vec(), Stroke::new(1.0, edge)));
    if state == CardState::Chosen {
        let (a, b) = (pts[5] + vec2(1.4, 2.0), pts[4] + vec2(1.4, -1.0));
        painter.line_segment([a, b], Stroke::new(7.0, ACCENT.gamma_multiply(0.16)));
        painter.line_segment([a, b], Stroke::new(2.6, ACCENT));
        painter.line_segment([pts[2], pts[3]], Stroke::new(1.8, ACCENT));
    }
    // its key, as a cap
    let cap = Rect::from_center_size(pos2(r.left() + 25.0, r.top() + 23.0), vec2(22.0, 22.0));
    let (cap_edge, cap_ink) = if state == CardState::Missing { (Color32::from_rgb(40, 50, 60), DIM.gamma_multiply(0.5)) } else { (Color32::from_rgb(66, 86, 100), ACCENT) };
    painter.rect(cap, CornerRadius::same(2), Color32::from_rgb(14, 20, 27), Stroke::new(1.0, cap_edge), StrokeKind::Inside);
    painter.text(cap.center(), egui::Align2::CENTER_CENTER, key, FontId::proportional(13.0), cap_ink);
    let title = tracked(painter, name, 18.0, ink, 3.2);
    painter.galley(pos2(r.left() + LEFT, r.top() + 11.0), title, ink);
    painter.galley(pos2(r.left() + LEFT, r.top() + 36.0), words, soft);
    // at its right, how it stands: a lamp lit on the one chosen, a word on the one that is not there
    match state {
        CardState::Chosen => lamp(painter, pos2(r.right() - 20.0, r.top() + 23.0), Some(ACCENT)),
        CardState::Free => {
            painter.circle_stroke(pos2(r.right() - 20.0, r.top() + 23.0), 3.6, Stroke::new(1.0, if hot { ACCENT } else { Color32::from_rgb(70, 88, 102) }));
        }
        CardState::Missing => {
            let ink = DIM.gamma_multiply(0.7);
            let word = tracked(painter, "NO ESTÁ", 10.5, ink, 1.4);
            painter.galley(pos2(r.right() - 14.0 - (word.size().x - 1.4), r.top() + 16.0), word, ink);
        }
    }
    if state == CardState::Free { resp.on_hover_cursor(egui::CursorIcon::PointingHand) } else { resp }
}

/// A small word in a hairline box, its left end at `at`: what a row of a list is besides its
/// name ("ARCHIVADA"). It is only drawn if it is no wider than `room`. How wide it came out (0:
/// it did not fit).
pub fn tag(painter: &egui::Painter, at: Pos2, text: &str, color: Color32, room: f32) -> f32 {
    let g = tracked(painter, text, 10.5, color, 1.3);
    let size = vec2(g.size().x - 1.3 + 12.0, g.size().y + 4.0);
    if size.x > room {
        return 0.0;
    }
    let r = Rect::from_min_size(at, size);
    painter.rect(r, CornerRadius::same(2), color.gamma_multiply(0.1), Stroke::new(1.0, color.gamma_multiply(0.55)), StrokeKind::Inside);
    painter.galley(r.min + vec2(6.0, 2.0), g, color);
    size.x
}

/// A lamp: lit (in its colour, with a glow) or out (a ring).
pub fn lamp(painter: &egui::Painter, c: Pos2, lit: Option<Color32>) {
    match lit {
        Some(color) => {
            painter.circle_filled(c, 8.0, color.gamma_multiply(0.2));
            painter.circle_filled(c, 4.0, color);
        }
        None => {
            painter.circle_stroke(c, 4.0, Stroke::new(1.2, Color32::from_rgb(86, 104, 118)));
        }
    }
}

/// The ground of a row of a list: lit, with the accent down its left side, if it is the one
/// chosen; a little lit under the pointer; a hairline under it otherwise.
pub fn row(painter: &egui::Painter, r: Rect, chosen: bool, hovered: bool) {
    if chosen {
        painter.rect_filled(r, CornerRadius::same(2), Color32::from_rgba_premultiplied(18, 62, 78, 236));
        painter.rect_stroke(r, CornerRadius::same(2), Stroke::new(1.0, ACCENT.gamma_multiply(0.75)), StrokeKind::Inside);
        painter.line_segment([r.left_top() + vec2(1.5, 2.0), r.left_bottom() + vec2(1.5, -2.0)], Stroke::new(3.0, ACCENT));
    } else {
        if hovered {
            painter.rect_filled(r, CornerRadius::same(2), Color32::from_rgba_premultiplied(22, 34, 44, 200));
        }
        painter.line_segment([r.left_bottom() + vec2(6.0, 0.0), r.right_bottom() + vec2(-6.0, 0.0)], Stroke::new(1.0, RULE));
    }
}

/// One line of text, cut with "…" if it is wider than `width`.
pub fn line(painter: &egui::Painter, text: &str, size: f32, color: Color32, width: f32) -> Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::single_section(text.to_string(), egui::TextFormat { font_id: FontId::proportional(size), color, ..Default::default() });
    job.wrap = egui::text::TextWrapping { max_width: width, max_rows: 1, break_anywhere: true, ..Default::default() };
    painter.layout_job(job)
}

/// A pair of arrow keys (up and down), as one cap, its right end at `right`: the arrows are drawn
/// (the lettering has no such letters). How wide it came out.
pub fn arrows_cap(painter: &egui::Painter, right: Pos2) -> f32 {
    let size = vec2(34.0, 20.0);
    let r = Rect::from_min_size(right - vec2(size.x, 0.0), size);
    painter.rect(r, CornerRadius::same(2), Color32::from_rgb(18, 26, 34), Stroke::new(1.0, Color32::from_rgb(66, 86, 100)), StrokeKind::Inside);
    painter.line_segment([r.left_bottom() + vec2(2.0, -1.0), r.right_bottom() + vec2(-2.0, -1.0)], Stroke::new(1.0, Color32::from_rgb(40, 110, 132)));
    for (x, up) in [(r.center().x - 7.0, true), (r.center().x + 7.0, false)] {
        let (tip, base) = if up { (r.center().y - 4.0, r.center().y + 3.0) } else { (r.center().y + 4.0, r.center().y - 3.0) };
        painter.add(Shape::convex_polygon(vec![pos2(x, tip), pos2(x + 4.5, base), pos2(x - 4.5, base)], ACCENT, Stroke::NONE));
    }
    size.x
}
