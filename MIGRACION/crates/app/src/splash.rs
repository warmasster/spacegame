//! What is shown while the game starts up, in the look of the game's own menus: the name and the
//! emblem where the start menu has them and, where that menu's entries will be, what the loader
//! is at — the stage under way, a hairline that fills, the stages done and in how long. Behind
//! it, space and the Moon's limb, on which day breaks as the start goes on: when everything is
//! in, the whole limb is lit and the game comes up with its menu in the same place
//! (`Start::after_splash`). There is nothing in it to click. All of it is drawn (no pictures)
//! from `boot::Snapshot`, at any size.
use crate::{
    boot::{STAGES, Snapshot, Stage},
    hud,
};
use egui::{Color32, FontId, Painter, Pos2, Rect, Shape, Stroke, pos2, vec2};

/// The end: how long everything stays done before the game comes up (s).
const OVER: f32 = 0.55;
/// How fast what is shown catches up with what is done (1/s).
const EASE: f32 = 5.0;
/// The column's width, as the start menu's entries (px at 900 lines).
const WIDE: f32 = 430.0;
/// The Moon's limb: its radius and where its top is (parts of the screen's height and width),
/// and how many pieces its arc is drawn in.
const LIMB: (f32, f32, f32) = (1.25, 0.86, 0.62);
const ARC: usize = 96;
/// Stars, the same every time.
const STARS: u32 = 170;
const WAITING: Color32 = Color32::from_rgb(86, 98, 110);
const LIT: Color32 = Color32::from_rgb(206, 216, 224);

#[derive(Default)]
pub struct Splash {
    /// How much of the whole is shown done (0..1): what is done, eased.
    shown: f32,
    /// Seconds since everything was done (0 until then), and since it came up.
    ending: f32,
    t: f32,
}

/// A number with a comma for its decimals, as it is written here.
fn secs(t: f32) -> String {
    format!("{t:.1}").replace('.', ",")
}

/// A value in 0..1 from an integer (the stars' places: the same every time).
fn hash(n: u32) -> f32 {
    let mut x = n.wrapping_mul(0x9e37_79b9) ^ 0x85eb_ca6b;
    x ^= x >> 15;
    x = x.wrapping_mul(0x2c1b_3c6d);
    x ^= x >> 12;
    (x & 0xffff) as f32 / 65535.0
}

/// Capital letters set apart at `at` (its left top, or its right top with `right`): where it is.
fn text(p: &Painter, at: Pos2, right: bool, s: &str, size: f32, color: Color32, spacing: f32) -> Rect {
    let mut job = egui::text::LayoutJob::default();
    job.append(s, 0.0, egui::TextFormat { font_id: FontId::proportional(size), color, extra_letter_spacing: spacing, ..Default::default() });
    let g = p.layout_job(job);
    let at = if right { at - vec2(g.size().x - spacing, 0.0) } else { at };
    let r = Rect::from_min_size(at, g.size());
    p.galley(at, g, color);
    r
}

impl Splash {
    /// Whether it has shown its end: the game can come up.
    pub fn over(&self) -> bool {
        self.ending >= OVER
    }

    /// A frame of it over the whole of `ctx`'s screen.
    pub fn frame(&mut self, ctx: &egui::Context, snap: &Snapshot, dt: f32, build: &str) {
        self.t += dt;
        if snap.finished {
            self.ending += dt;
        }
        let goal = if snap.finished { 1.0 } else { snap.done.clamp(0.0, 1.0) };
        self.shown += (goal - self.shown) * (dt * EASE).min(1.0);
        let screen = ctx.content_rect();
        // (the start menu's own scale: what is common to both is in the same place)
        let k = (screen.height() / 900.0).clamp(0.72, 1.8);
        let p = ctx.layer_painter(egui::LayerId::new(egui::Order::Background, egui::Id::new("arranque")));
        self.sky(&p, screen, k);
        self.column(&p, screen, k, snap, build);
    }

    /// Space, and the Moon's limb with day breaking along it as far as the start has gone.
    fn sky(&self, p: &Painter, screen: Rect, k: f32) {
        let (w, h) = (screen.width(), screen.height());
        let mut ground = egui::Mesh::default();
        for (at, c) in [(screen.left_top(), 13u8), (screen.right_top(), 13), (screen.right_bottom(), 5), (screen.left_bottom(), 5)] {
            ground.colored_vertex(at, Color32::from_rgb(c / 2, c * 2 / 3, c));
        }
        ground.add_triangle(0, 1, 2);
        ground.add_triangle(0, 2, 3);
        p.add(Shape::mesh(ground));
        let r = h * LIMB.0;
        let c = pos2(screen.left() + w * LIMB.1, screen.top() + h * LIMB.2 + r);
        for i in 0..STARS {
            let at = pos2(screen.left() + hash(i * 3 + 1) * w, screen.top() + hash(i * 3 + 2) * h);
            if at.distance(c) < r + 6.0 * k {
                continue;
            }
            let big = hash(i * 3 + 3);
            let glint = 0.82 + 0.18 * (self.t * (0.5 + big) + i as f32).sin();
            p.circle_filled(at, (0.45 + big * big * 1.1) * k, Color32::from_white_alpha(((40.0 + big * 150.0) * glint) as u8));
        }
        // ---- the Moon: its night side, and the part of its limb that is on the screen, from
        // where it comes up out of the foot of the screen to the right edge
        p.circle_filled(c, r, Color32::from_rgb(9, 11, 14));
        // (angles from the disc's middle, y down: its top is at -π/2)
        let from = -std::f32::consts::PI - ((h * (1.0 - LIMB.2) - r) / r).clamp(-1.0, 1.0).asin();
        let to = -((w * (1.0 - LIMB.1)) / r).clamp(-1.0, 1.0).acos();
        // (a little past both ends, so that nothing of it ends on the screen)
        let (first, last) = (-(ARC as i32) / 8, ARC as i32 + ARC as i32 / 8);
        let point = |u: f32, deep: f32| {
            let a = from + (to - from) * u;
            c + vec2(a.cos(), a.sin()) * (r - deep)
        };
        // (how far day has come at `u` along the limb — as far as what is done —, over so wide
        // an edge)
        let day = |u: f32, edge: f32| {
            let d = ((self.shown * (1.0 + edge) - u.clamp(0.0, 1.0)) / edge).clamp(0.0, 1.0);
            d * d * (3.0 - 2.0 * d)
        };
        let dawn = (self.ending / OVER).clamp(0.0, 1.0);
        // the day side: the ground a shade lighter, to the terminator; and the light on its rim
        let (mut ground, mut rim) = (egui::Mesh::default(), egui::Mesh::default());
        for i in first..=last {
            let u = i as f32 / ARC as f32;
            let lit = Color32::from_rgb(23, 26, 30).gamma_multiply(day(u, 0.22));
            ground.colored_vertex(point(u, 0.0), lit);
            ground.colored_vertex(point(u, h * 0.8), lit);
            rim.colored_vertex(point(u, 0.0), Color32::from_rgb(150, 160, 170).gamma_multiply(day(u, 0.06) * (0.5 + 0.3 * dawn)));
            rim.colored_vertex(point(u, h * 0.045), Color32::TRANSPARENT);
        }
        for mesh in [&mut ground, &mut rim] {
            for i in 0..(last - first) as u32 {
                mesh.add_triangle(2 * i, 2 * i + 1, 2 * i + 2);
                mesh.add_triangle(2 * i + 1, 2 * i + 3, 2 * i + 2);
            }
        }
        p.add(Shape::mesh(ground));
        p.add(Shape::mesh(rim));
        for i in first..last {
            let (u0, u1) = (i as f32 / ARC as f32, (i + 1) as f32 / ARC as f32);
            let d = day((u0 + u1) * 0.5, 0.06);
            let (color, width) = if d > 0.0 { (LIT.gamma_multiply(0.25 + 0.75 * d), (1.0 + 0.8 * d) * k) } else { (hud::EDGE, 1.0) };
            p.line_segment([point(u0, 0.0), point(u1, 0.0)], Stroke::new(width, color));
        }
        // the Sun about to come over the limb, where day has got to
        let head = point((self.shown * 1.06 - 0.03).clamp(0.0, 1.0), 0.0);
        let glow = self.shown.min(1.0).sqrt() * (1.0 + 0.6 * dawn);
        for (size, alpha) in [(46.0, 7.0), (26.0, 14.0), (13.0, 34.0), (5.5, 110.0), (2.2, 235.0)] {
            p.circle_filled(head, size * k * glow, Color32::from_white_alpha((alpha * glow.min(1.0)) as u8));
        }
    }

    /// The name, as the start menu has it, and under it what the loader is at.
    fn column(&self, p: &Painter, screen: Rect, k: f32, snap: &Snapshot, build: &str) {
        let x = screen.left() + 84.0 * k;
        let y = screen.top() + screen.height() * 0.13;
        let name = hud::wordmark(p, pos2(x, y), k, self.t, 1.0);
        text(p, pos2(name.left() + 4.0 * k, name.bottom() - 2.0 * k), false, &format!("{}  ·  {build}", crate::EDITION), 14.0 * k, hud::DIM, 4.0 * k);
        let mut y = name.bottom() + 60.0 * k;
        // ---- the stage under way, where the menu's first entry will be
        let now = snap.stages.iter().position(|s| matches!(s, Stage::Running(_)));
        let (label, what) = match now {
            Some(n) => ("INICIANDO", STAGES.get(n).map_or("", |s| s.name)),
            None if snap.finished => ("LISTO", "TODO EN MARCHA"),
            None => ("INICIANDO", ""),
        };
        text(p, pos2(x, y), false, label, 12.5 * k, hud::ACCENT, 3.2 * k);
        text(p, pos2(x + WIDE * k, y), true, &format!("{:.0} %", self.shown.min(1.0) * 100.0), 12.5 * k, hud::DIM, 2.0 * k);
        y += 24.0 * k;
        text(p, pos2(x, y), false, what, 26.0 * k, hud::TEXT, 4.4 * k);
        y += 46.0 * k;
        // ---- how much of the whole is done: a hairline that fills
        let (a, b) = (pos2(x, y), pos2(x + WIDE * k, y));
        p.line_segment([a, b], Stroke::new(1.0, hud::EDGE));
        let head = a + (b - a) * self.shown.clamp(0.0, 1.0);
        p.line_segment([a, head], Stroke::new(5.0 * k, hud::ACCENT.gamma_multiply(0.16)));
        p.line_segment([a, head], Stroke::new(2.0 * k, hud::ACCENT));
        p.line_segment([head - vec2(0.0, 4.5 * k), head + vec2(0.0, 4.5 * k)], Stroke::new(1.6 * k, hud::TEXT));
        y += 24.0 * k;
        // ---- every stage: done and in how long, under way, to come
        for (st, def) in snap.stages.iter().zip(STAGES) {
            let mark = pos2(x + 5.0 * k, y + 8.0 * k);
            let (color, time) = match st {
                Stage::Done(t) => {
                    let tick = [mark + vec2(-4.0, 0.0) * k, mark + vec2(-1.0, 3.2) * k, mark + vec2(4.6, -3.6) * k];
                    p.add(Shape::line(tick.to_vec(), Stroke::new(1.5 * k, hud::ACCENT)));
                    (hud::DIM, Some((secs(*t), hud::DIM)))
                }
                Stage::Running(t) => {
                    let beat = 0.6 + 0.4 * (self.t * 4.0).sin();
                    p.rect_filled(Rect::from_center_size(mark, vec2(6.0, 6.0) * k), 0.0, hud::ACCENT.gamma_multiply(beat));
                    (hud::TEXT, Some((secs(*t), hud::ACCENT)))
                }
                Stage::Waiting => {
                    p.rect_stroke(Rect::from_center_size(mark, vec2(5.0, 5.0) * k), 0.0, Stroke::new(1.0, WAITING), egui::StrokeKind::Inside);
                    (WAITING, None)
                }
            };
            text(p, pos2(x + 22.0 * k, y), false, def.name, 12.5 * k, color, 2.4 * k);
            if let Some((t, c)) = time {
                text(p, pos2(x + WIDE * k, y), true, &format!("{t} s"), 12.5 * k, c, 1.0 * k);
            }
            y += 22.0 * k;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boot::Boot;
    use egui::vec2;

    /// A frame of it on a screen of `size`: how many shapes it drew.
    fn shapes(s: &mut Splash, snap: &Snapshot, size: (f32, f32), dt: f32) -> usize {
        let ctx = egui::Context::default();
        let input = egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(size.0, size.1))), ..Default::default() };
        let mut out = ctx.run_ui(input, |ui| {
            let ctx = ui.ctx().clone();
            s.frame(&ctx, snap, dt, "V0");
        });
        out.textures_delta.clear();
        out.shapes.len()
    }

    #[test]
    fn it_is_drawn_at_every_stage_and_any_size() {
        let boot = Boot::new(None);
        let mut s = Splash::default();
        for size in [(640.0, 360.0), (1600.0, 900.0), (3440.0, 1440.0), (900.0, 1200.0)] {
            assert!(shapes(&mut s, &boot.snapshot(), size, 0.016) > 150);
        }
        for st in STAGES {
            boot.done(st.id);
            assert!(shapes(&mut s, &boot.snapshot(), (1600.0, 900.0), 0.016) > 150);
        }
    }

    #[test]
    fn what_is_shown_follows_what_is_done_and_it_is_over_a_moment_after_everything_is() {
        let boot = Boot::new(None);
        let mut s = Splash::default();
        for _ in 0..30 {
            shapes(&mut s, &boot.snapshot(), (1600.0, 900.0), 0.016);
        }
        assert!(!s.over() && s.shown < 0.5);
        for st in STAGES {
            boot.done(st.id);
        }
        let snap = boot.snapshot();
        assert!(snap.finished);
        let mut frames = 0;
        while !s.over() {
            shapes(&mut s, &snap, (1600.0, 900.0), 0.016);
            frames += 1;
            assert!(frames < 200, "it never ends");
        }
        // (a moment with everything done, not at once; by then the hairline is full)
        assert!(frames > 25 && (s.shown - 1.0).abs() < 0.08, "{frames} frames, shown {}", s.shown);
    }
}
