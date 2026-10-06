//! The start menu: what the game opens with. The world is already there behind it — the camera
//! goes slowly round where one is to start — and over it the name of the game and what to do:
//! play, where to start, options and controls (the menu's own tabs), leave. Where to start is
//! read off what is there (the scenario's start and every ship standing: beside it, or sat at
//! its controls): nothing is listed by hand.
use crate::{hud, ships::Ships};
use egui::{Color32, Pos2, Rect, Sense, Stroke, pos2, vec2};
use glam::DVec3;
use lunar_core::{body::BodyRegistry, structure::set::Structures};
use lunar_render::View;

/// What was chosen in it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Play,
    Options,
    Quit,
    /// Join the server written in the menu (`Start::server`, `Start::name`), or leave it.
    Connect,
    Disconnect,
}

/// Where a place to start is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Where {
    /// Where the player already is (the game was going).
    Here,
    /// The scenario's start.
    Spawn,
    /// On the ground by a ship (its structure), looking at it.
    Beside(u64),
    /// Sat in a seat of a ship.
    Seat(u64, usize),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Place {
    pub name: String,
    pub what: String,
    pub at: Where,
}

/// How fast the camera goes round (rad/s), how fast it goes from a place to the next chosen
/// (1/s), and how long the menu takes to come up (s).
const TURN: f64 = 0.045;
const MOVE: f64 = 2.2;
const FADE: f32 = 0.7;
/// How long the world takes to come up out of the dark after the start-up screen (s).
const VEIL: f32 = 0.9;
/// How far up the camera is tried when something hides what it goes round, and how fast it goes
/// there (1/s).
const LIFTS: [f64; 4] = [0.0, 0.5, 1.0, 1.7];
const LIFT: f64 = 1.5;

pub struct Start {
    pub places: Vec<Place>,
    pub chosen: usize,
    /// Seconds it has been up.
    t: f64,
    /// What is left of the dark it came up out of (1..0): after the start-up screen, which has
    /// the name in the same place, the world comes up behind it.
    veil: f32,
    /// What the camera goes round now: its middle and its size (m), eased toward the place chosen.
    subject: Option<(DVec3, f64)>,
    /// How far over its usual height the camera is (0..): what stands between it and what it
    /// goes round lifts it over it.
    lift: f64,
    /// The game was going when it came up (the first thing to do is go on).
    resumed: bool,
    /// With others (the editions that can): the server to join and the name to go by, as
    /// written in the menu.
    pub server: String,
    pub name: String,
}

/// The server the menu offers until another is written, and the longest a name is.
pub const SERVER: &str = "127.0.0.1:47600";
const NAME: usize = 24;

impl Start {
    /// The places to start at: where one is or the scenario's start, then every ship standing
    /// free (not carried by another): beside it, and sat in its first seat.
    pub fn new(ships: &Ships, set: &Structures, resumed: bool) -> Start {
        let mut places = Vec::new();
        if resumed {
            places.push(Place { name: "Donde estabas".into(), what: "seguir la partida".into(), at: Where::Here });
        }
        places.push(Place { name: "En la base".into(), what: "a pie, en el punto de inicio".into(), at: Where::Spawn });
        for sh in &ships.list {
            let Some(s) = set.get(sh.structure) else { continue };
            if s.held.is_some() {
                continue;
            }
            let (name, class) = (&sh.kind.def.nombre, sh.kind.def.clase.clone().unwrap_or_default());
            places.push(Place { name: format!("Junto al {name}"), what: if class.is_empty() { "a pie".into() } else { format!("a pie · {}", class.to_lowercase()) }, at: Where::Beside(sh.structure) });
            if let Some(seat) = sh.kind.seats.first() {
                places.push(Place { name: format!("A los mandos del {name}"), what: seat.def.nombre.to_lowercase(), at: Where::Seat(sh.structure, 0) });
            }
        }
        let name = std::env::var("USERNAME").ok().filter(|n| !n.trim().is_empty()).unwrap_or_else(|| "Jugador".into());
        Start { places, chosen: 0, t: 0.0, veil: 0.0, subject: None, lift: 0.0, resumed, server: SERVER.into(), name: name.chars().take(NAME).collect() }
    }

    /// It comes up after the start-up screen (`splash`): what both have in common is there at
    /// once, and the world comes up out of the dark behind it.
    pub fn after_splash(&mut self) {
        self.t = f64::from(FADE);
        self.veil = 1.0;
    }

    /// The place chosen: the one before or after it (the arrow keys).
    pub fn step(&mut self, by: i32) {
        let n = self.places.len() as i32;
        if n > 0 {
            self.chosen = (self.chosen as i32 + by).rem_euclid(n) as usize;
        }
    }

    pub fn place(&self) -> Option<Where> {
        self.places.get(self.chosen).map(|p| p.at)
    }

    /// The picture behind it: a camera going round the place chosen (a ship, or the player).
    /// `own` is the player's own view.
    pub fn view(&mut self, dt: f64, own: &View, set: &Structures, bodies: &BodyRegistry) -> View {
        self.t += dt;
        let ship = match self.place() {
            Some(Where::Beside(id) | Where::Seat(id, _)) => set.get(id),
            _ => None,
        };
        let want = ship.map_or((own.eye, 2.2), |s| (s.to_world(s.center), f64::from(s.radius)));
        let k = 1.0 - (-dt * MOVE).exp();
        let (centre, size) = match self.subject {
            Some((c, r)) => (c + (want.0 - c) * k, r + (want.1 - r) * k),
            None => want,
        };
        self.subject = Some((centre, size));
        let body = bodies.get(bodies.dominant(centre));
        let up = body.up(centre);
        let a = 0.7 + self.t * TURN;
        let north = up.any_orthonormal_vector();
        let east = up.cross(north);
        let ring = centre + (north * a.cos() + east * a.sin()) * (size * 1.9 + 4.5);
        let at = |lift: f64| ring + up * (size * 0.42 + 0.9 + lift * (size * 0.9 + 3.0));
        // what stands between the camera and what it goes round (a building, another ship)
        // lifts it over it, with time
        let hidden = |eye: DVec3| {
            let to = centre - eye;
            let d = to.length();
            set.raycast_solid(eye, to / d, (d - size * 1.15).max(0.0), 0.5).is_some()
        };
        let want = LIFTS.into_iter().find(|k| !hidden(at(*k))).unwrap_or(LIFTS[LIFTS.len() - 1]);
        self.lift += (want - self.lift) * (1.0 - (-dt * LIFT).exp());
        let mut eye = at(self.lift);
        let low = 0.9 - body.altitude(eye);
        if low > 0.0 {
            eye += body.up(eye) * low;
        }
        // (the place a little to the right of the picture: what is written is on its left)
        let to = centre + up * (size * 0.12) - eye;
        let side = to.cross(up).normalize_or(east);
        View { eye, forward: (to - side * to.length() * 0.22).normalize_or(own.forward), up, ..*own }
    }

    /// Drawn over the picture. What was chosen in it this frame.
    /// `net`: how the connection to the other players is, if there is one to speak of.
    pub fn draw(&mut self, ctx: &egui::Context, build: &str, adapter: &str, net: Option<&str>) -> Option<Action> {
        let screen = ctx.content_rect();
        let k = (screen.height() / 900.0).clamp(0.72, 1.8);
        let fade = (self.t as f32 / FADE).clamp(0.0, 1.0);
        let fade = fade * fade * (3.0 - 2.0 * fade);
        // ---- the left of the picture darkened, so that what is written on it reads
        let bg = ctx.layer_painter(egui::LayerId::new(egui::Order::Background, egui::Id::new("inicio")));
        let dark = |a: f32| Color32::from_black_alpha((a * fade * 255.0) as u8);
        if self.veil > 0.0 {
            let v = self.veil * self.veil * (3.0 - 2.0 * self.veil);
            bg.rect_filled(screen, 0.0, Color32::from_black_alpha((v * 255.0) as u8));
            self.veil = (self.veil - ctx.input(|i| i.stable_dt).min(0.05) / VEIL).max(0.0);
        }
        let mut mesh = egui::Mesh::default();
        let cols = [(0.0, 0.86), (0.3, 0.7), (0.62, 0.0)];
        for (x, a) in cols {
            mesh.colored_vertex(pos2(screen.left() + screen.width() * x, screen.top()), dark(a));
            mesh.colored_vertex(pos2(screen.left() + screen.width() * x, screen.bottom()), dark(a));
        }
        for i in 0..cols.len() as u32 - 1 {
            mesh.add_triangle(2 * i, 2 * i + 1, 2 * i + 2);
            mesh.add_triangle(2 * i + 1, 2 * i + 3, 2 * i + 2);
        }
        bg.add(egui::Shape::mesh(mesh));
        let mut action = None;
        egui::Area::new(egui::Id::new("inicio")).order(egui::Order::Middle).fixed_pos(screen.min).show(ctx, |ui| {
            ui.set_min_size(screen.size());
            let p = ui.painter().clone();
            let tint = |c: Color32| c.gamma_multiply(fade);
            let text = |at: Pos2, s: &str, size: f32, color: Color32, spacing: f32| -> Rect {
                let mut job = egui::text::LayoutJob::default();
                job.append(s, 0.0, egui::TextFormat { font_id: egui::FontId::proportional(size), color: tint(color), extra_letter_spacing: spacing, ..Default::default() });
                let g = p.layout_job(job);
                let r = Rect::from_min_size(at, g.size());
                p.galley(at, g, tint(color));
                r
            };
            let x = screen.left() + 84.0 * k;
            let mut y = screen.top() + screen.height() * 0.13;
            // ---- the name
            let r = hud::wordmark(&p, pos2(x, y), k, ui.input(|i| i.time) as f32, fade);
            let sub = text(pos2(r.left() + 4.0 * k, r.bottom() - 2.0 * k), &format!("{}  ·  {build}", crate::EDITION), 14.0 * k, hud::DIM, 4.0 * k);
            if let Some(net) = net {
                text(pos2(r.left() + 4.0 * k, sub.bottom() + 8.0 * k), &format!("RED  ·  {}", net.to_uppercase()), 12.0 * k, hud::ACCENT, 2.4 * k);
            }
            y = r.bottom() + 60.0 * k;
            // ---- what to do
            let entry = |ui: &mut egui::Ui, y: &mut f32, label: &str, size: f32, id: &str| -> bool {
                let rect = Rect::from_min_size(pos2(x - 18.0 * k, *y), vec2(430.0 * k, size * 1.75));
                let resp = ui.interact(rect, egui::Id::new(("inicio", id)), Sense::click());
                let on = resp.hovered();
                if on {
                    p.rect_filled(rect, 0.0, tint(hud::ACCENT.gamma_multiply(0.09)));
                    p.line_segment([rect.left_top(), rect.left_bottom()], Stroke::new(3.0 * k, tint(hud::ACCENT)));
                }
                text(pos2(x, *y + size * 0.3), label, size, if on { hud::TEXT } else { Color32::from_rgb(204, 216, 224) }, size * 0.16);
                *y += size * 1.75 + 4.0 * k;
                resp.clicked()
            };
            if entry(ui, &mut y, if self.resumed { "CONTINUAR" } else { "JUGAR" }, 34.0 * k, "jugar") {
                action = Some(Action::Play);
            }
            // ---- where to start
            y += 10.0 * k;
            text(pos2(x, y), "EMPEZAR", 12.5 * k, hud::ACCENT, 3.2 * k);
            y += 24.0 * k;
            for (i, place) in self.places.iter().enumerate() {
                let rect = Rect::from_min_size(pos2(x - 18.0 * k, y), vec2(430.0 * k, 27.0 * k));
                let resp = ui.interact(rect, egui::Id::new(("inicio_lugar", i)), Sense::click());
                let on = i == self.chosen;
                if resp.hovered() {
                    p.rect_filled(rect, 0.0, tint(hud::ACCENT.gamma_multiply(0.07)));
                }
                let c = pos2(x + 6.0 * k, rect.center().y);
                let d = 5.0 * k;
                let mark = vec![c + vec2(0.0, -d), c + vec2(d, 0.0), c + vec2(0.0, d), c + vec2(-d, 0.0)];
                p.add(egui::Shape::convex_polygon(mark, if on { tint(hud::ACCENT) } else { Color32::TRANSPARENT }, Stroke::new(1.2, tint(if on { hud::ACCENT } else { hud::DIM }))));
                let r = text(pos2(x + 24.0 * k, y + 4.0 * k), &place.name, 16.5 * k, if on || resp.hovered() { hud::TEXT } else { Color32::from_rgb(190, 202, 212) }, 0.6 * k);
                text(pos2(r.right() + 12.0 * k, y + 7.5 * k), &place.what, 12.5 * k, hud::DIM, 0.4 * k);
                if resp.clicked() {
                    self.chosen = i;
                }
                // (twice on it: start there)
                if resp.double_clicked() {
                    action = Some(Action::Play);
                }
                y += 27.0 * k;
            }
            y += 16.0 * k;
            // ---- with others: the server and the name, and joining it (or leaving)
            if net.is_some() || crate::MULTI || !crate::DEMO {
                text(pos2(x, y), "CON OTROS", 12.5 * k, hud::ACCENT, 3.2 * k);
                y += 22.0 * k;
                let joined = net.is_some();
                for (label, value, width, chars) in [("SERVIDOR", &mut self.server, 190.0, 64usize), ("NOMBRE", &mut self.name, 130.0, NAME)] {
                    let r = text(pos2(x, y + 5.0 * k), label, 11.0 * k, hud::DIM, 1.6 * k);
                    let field = Rect::from_min_size(pos2(x + 84.0 * k, y), vec2(width * k, 24.0 * k));
                    ui.put(field, egui::TextEdit::singleline(value).char_limit(chars).font(egui::FontId::monospace(13.0 * k)).interactive(!joined));
                    let _ = r;
                    y += 28.0 * k;
                }
                if entry(ui, &mut y, if joined { "DESCONECTAR" } else { "CONECTAR" }, 16.0 * k, "conectar") {
                    action = Some(if joined { Action::Disconnect } else { Action::Connect });
                }
                y += 6.0 * k;
            }
            if entry(ui, &mut y, "OPCIONES Y CONTROLES", 20.0 * k, "opciones") {
                action = Some(Action::Options);
            }
            if entry(ui, &mut y, "SALIR", 20.0 * k, "salir") {
                action = Some(Action::Quit);
            }
            // ---- its foot
            let foot = screen.bottom() - 40.0 * k;
            let mut fx = x;
            for (key, what) in [("Intro", "jugar"), ("Flechas", "dónde empezar"), ("Esc", "opciones")] {
                let g = p.layout_no_wrap(key.to_string(), egui::FontId::proportional(13.0 * k), tint(hud::ACCENT));
                let cap = Rect::from_min_size(pos2(fx, foot), vec2(g.size().x + 14.0 * k, g.size().y + 6.0 * k));
                p.rect(cap, egui::CornerRadius::same(2), tint(Color32::from_rgb(18, 26, 34)), Stroke::new(1.0, tint(Color32::from_rgb(66, 86, 100))), egui::StrokeKind::Inside);
                p.galley(cap.center() - g.size() * 0.5, g, tint(hud::ACCENT));
                let r = text(pos2(cap.right() + 10.0 * k, foot + 3.0 * k), what, 12.5 * k, hud::DIM, 0.5 * k);
                fx = r.right() + 26.0 * k;
            }
            let mut job = egui::text::LayoutJob::default();
            job.append(&format!("{} {}  ·  {build}  ·  {adapter}", crate::GAME, env!("CARGO_PKG_VERSION")), 0.0, egui::TextFormat { font_id: egui::FontId::proportional(11.5 * k), color: tint(hud::DIM), extra_letter_spacing: 1.6 * k, ..Default::default() });
            let g = p.layout_job(job);
            p.galley(pos2(screen.right() - g.size().x - 40.0 * k, foot + 4.0 * k), g, tint(hud::DIM));
        });
        ctx.request_repaint();
        action
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn menu() -> Start {
        let p = |name: &str, at| Place { name: name.into(), what: String::new(), at };
        Start { places: vec![p("base", Where::Spawn), p("nave", Where::Beside(7)), p("asiento", Where::Seat(7, 0))], chosen: 0, t: 0.0, veil: 0.0, subject: None, lift: 0.0, resumed: false, server: SERVER.into(), name: "Ana".into() }
    }

    #[test]
    fn the_arrows_go_round_the_places() {
        let mut m = menu();
        assert_eq!(m.place(), Some(Where::Spawn));
        m.step(1);
        m.step(1);
        assert_eq!(m.place(), Some(Where::Seat(7, 0)));
        m.step(1);
        assert_eq!(m.place(), Some(Where::Spawn));
        m.step(-1);
        assert_eq!(m.place(), Some(Where::Seat(7, 0)));
    }

    #[test]
    fn it_is_drawn_at_any_size_and_says_what_is_clicked() {
        // (as the HUD's test: the menu laid out in windows small and large, nothing panics)
        for size in [(640.0, 400.0), (1600.0, 900.0), (3840.0, 2160.0)] {
            let ctx = egui::Context::default();
            let mut m = menu();
            let input = egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(size.0, size.1))), ..Default::default() };
            let mut got = None;
            let mut out = ctx.run_ui(input, |ui| {
                let ctx = ui.ctx().clone();
                got = m.draw(&ctx, "V0", "prueba", Some("2 jug. · 12 ms"));
            });
            out.textures_delta.clear();
            assert_eq!(got, None, "nothing was clicked");
            assert!(!out.shapes.is_empty());
        }
    }
}
