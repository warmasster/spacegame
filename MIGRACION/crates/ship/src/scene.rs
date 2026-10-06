//! What a ship shows besides its structure, rebuilt every frame from its state: every control of
//! every panel as its model posed as it stands (toggles thrown, knobs turned, levers pulled, covers
//! lifted, breakers popped), every indicator (lamps, needles with their swing, bars, displays,
//! screens with their pages, annunciators), the silkscreen, the lenses and light of every lamp, and
//! the rods of every ram as far as it extends. Near panels in full, farther ones without text, far
//! ones only their lights.
use crate::{kind::ShipKind, panels::Panels, ship::Ship};
use glam::{Affine3A, DVec3, Quat, Vec3};
use lunar_controls::{
    def::{ControlDef, named_color},
    indicator::IndKind,
    layout::{Rect, scale_of},
};
use lunar_core::{
    font::{Font, Placed},
    props::{BOX, CONE, CYLINDER, DecalQuad, GlyphQuad, Lamp, Prop, PropFrame, PropScene, SPHERE},
    structure::state::Structure,
};
use std::f32::consts::FRAC_PI_2;

/// Distances (m) under which panels show text, controls, and anything at all.
const TEXT: f32 = 6.0;
const CONTROLS: f32 = 22.0;
/// Panels inside, seen from outside the ship: only this near (m).
const OUTSIDE: f32 = 12.0;

const DARK: [u8; 3] = [26, 27, 30];
const METAL: [u8; 3] = [168, 170, 176];
const PAINT: [u8; 3] = [236, 234, 224];

/// Relief of silkscreen letters (moulded into the plate).
const RELIEF: f32 = 0.8;

/// Writes props of one panel in its plane (mm, z out of the face) into the scene.
struct Pen<'a> {
    out: &'a mut PropScene,
    frame: u16,
    /// Panel plane (m) → ship frame.
    f: Affine3A,
    rot: Quat,
    font: &'a Font,
    scratch: Vec<Placed>,
    /// Everything drawn is scaled this much about `origin` (mm): controls are modelled at their
    /// nominal size and drawn at the layout's.
    scale: f32,
    origin: Vec3,
}

fn mm(x: f32) -> f32 {
    x * 0.001
}

impl Pen<'_> {
    /// A primitive at (x, y, z) mm with size (mm), turned by `r` in the panel plane frame.
    fn prop(&mut self, mesh: u8, at: Vec3, size: Vec3, r: Quat, color: [u8; 3], emissive: f32, rough: u8, metal: u8) {
        let at = self.origin + (at - self.origin) * self.scale;
        let pos = self.f.transform_point3(at * 0.001);
        self.out.props.push(Prop { frame: self.frame, mesh, pos, rot: self.rot * r, size: size * 0.001 * self.scale, color, emissive, rough, metal });
    }

    /// Draw what follows at `scale` about (x, y) mm (1: as laid out).
    fn zoom(&mut self, x: f32, y: f32, scale: f32) {
        self.origin = Vec3::new(x, y, 0.0);
        self.scale = scale;
    }

    /// A cylinder standing out of the panel at (x, y), from z0 to z1 mm.
    fn post(&mut self, x: f32, y: f32, z0: f32, z1: f32, d: f32, color: [u8; 3], emissive: f32, metal: u8) {
        self.prop(CYLINDER, Vec3::new(x, y, (z0 + z1) * 0.5), Vec3::new(d, z1 - z0, d), Quat::from_rotation_x(FRAC_PI_2), color, emissive, 120, metal);
    }

    /// Text centred (or aligned) at (x, y) mm, `h` mm tall, on the panel at height z.
    fn text(&mut self, s: &str, x: f32, y: f32, z: f32, h: f32, color: [u8; 3], emissive: f32, align: f32) {
        if s.is_empty() {
            return;
        }
        self.scratch.clear();
        self.font.layout(s, align, &mut self.scratch);
        let (u, v) = (self.f.transform_vector3(Vec3::X), self.f.transform_vector3(Vec3::Y));
        let o = self.origin + (Vec3::new(x, y, z) - self.origin) * self.scale;
        let h = h * self.scale;
        // lit letters (displays, screens) are light, not relief
        let relief = if emissive > 0.0 { 0.0 } else { RELIEF };
        for g in &self.scratch {
            let c = o + Vec3::new(g.center[0] * h, g.center[1] * h, 0.0);
            self.out.glyphs.push(GlyphQuad { frame: self.frame, center: self.f.transform_point3(c * 0.001), u: u * mm(g.half[0] * h), v: v * mm(g.half[1] * h), uv: g.uv, color, emissive, relief });
        }
    }

    /// A flat lit rectangle on a display (mm, at height z), in a display's light (no bloom).
    fn fill(&mut self, x: f32, y: f32, w: f32, h: f32, z: f32, color: [u8; 3], glow: f32) {
        if w <= 0.01 || h <= 0.01 {
            return;
        }
        // under what is written on it (text sits a little higher)
        self.prop(BOX, Vec3::new(x + w * 0.5, y + h * 0.5, z - 0.3), Vec3::new(w, h, 0.2), Quat::IDENTITY, color, -glow, 200, 0);
    }

    /// A lit line on a display from (x0, y0) to (x1, y1), `w` mm wide.
    #[allow(clippy::too_many_arguments)]
    fn line(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, w: f32, z: f32, color: [u8; 3], glow: f32) {
        let (dx, dy) = (x1 - x0, y1 - y0);
        let len = (dx * dx + dy * dy).sqrt();
        if len < 0.01 {
            return;
        }
        let r = Quat::from_rotation_z(dy.atan2(dx));
        self.prop(BOX, Vec3::new((x0 + x1) * 0.5, (y0 + y1) * 0.5, z), Vec3::new(len + w * 0.5, w, 0.2), r, color, -glow, 200, 0);
    }

    /// A lit rectangle outline on a display.
    #[allow(clippy::too_many_arguments)]
    fn frame_lit(&mut self, x: f32, y: f32, w: f32, h: f32, lw: f32, z: f32, color: [u8; 3], glow: f32) {
        self.line(x, y, x + w, y, lw, z, color, glow);
        self.line(x, y + h, x + w, y + h, lw, z, color, glow);
        self.line(x, y, x, y + h, lw, z, color, glow);
        self.line(x + w, y, x + w, y + h, lw, z, color, glow);
    }

    /// A rectangle outline (silkscreen frame), `w` mm wide lines.
    fn outline(&mut self, r: &Rect, w: f32, color: [u8; 3]) {
        let z = 0.3;
        self.prop(BOX, Vec3::new(r.x + r.w * 0.5, r.y + r.h - w * 0.5, z), Vec3::new(r.w, w, 0.4), Quat::IDENTITY, color, 0.0, 200, 0);
        self.prop(BOX, Vec3::new(r.x + r.w * 0.5, r.y + w * 0.5, z), Vec3::new(r.w, w, 0.4), Quat::IDENTITY, color, 0.0, 200, 0);
        self.prop(BOX, Vec3::new(r.x + w * 0.5, r.y + r.h * 0.5, z), Vec3::new(w, r.h, 0.4), Quat::IDENTITY, color, 0.0, 200, 0);
        self.prop(BOX, Vec3::new(r.x + r.w - w * 0.5, r.y + r.h * 0.5, z), Vec3::new(w, r.h, 0.4), Quat::IDENTITY, color, 0.0, 200, 0);
    }
}

/// How much what the hand is aimed at glows of its own (a control's parts), and the line round it.
const AIM_GLOW: f32 = 0.22;
const AIM_LINE: [u8; 3] = [255, 196, 110];

/// A thin warm line round what the hand is aimed at (a display's light: it does not bloom).
fn aim_frame(pen: &mut Pen, r: &Rect) {
    pen.frame_lit(r.x - 1.6, r.y - 1.6, r.w + 3.2, r.h + 3.2, 0.55, 0.5, AIM_LINE, 0.55);
}

/// A laid-out rectangle at the nominal size the models are written for (same centre).
fn nominal(r: &Rect, k: f32) -> Rect {
    let (w, h) = (r.w / k, r.h / k);
    Rect { x: r.x + (r.w - w) * 0.5, y: r.y + (r.h - h) * 0.5, w, h }
}

fn def_color(d: &ControlDef, or: [u8; 3]) -> [u8; 3] {
    d.color.as_ref().and_then(|c| c.rgb().ok()).unwrap_or(or)
}

/// Height of a guard cover over its panel (mm): over the short lever of a toggle it guards.
use lunar_controls::mech::GUARD_H;
/// Lever of a toggle (mm): a full one, and the short bat under a guard cover.
const LEVER: f32 = 16.0;
const LEVER_GUARDED: f32 = 8.0;

/// The model of a control kind, posed. `guarded`: a cover goes over it.
#[allow(clippy::too_many_arguments)]
fn control(p: &mut Pen, d: &ControlDef, r: &Rect, pose: &lunar_controls::Pose, value: f64, text: &str, powered: bool, near: bool, guarded: bool) {
    let (cx, cy) = (r.x + r.w * 0.5, r.y + r.h * 0.5);
    let e = pose.e;
    match d.kind.as_str() {
        "pulsador" => {
            let seta = d.modo.as_deref() == Some("seta");
            let cap = def_color(d, if seta { [200, 30, 24] } else { [60, 62, 66] });
            p.post(cx, cy, 0.0, 3.0, r.w * 0.96, DARK, 0.0, 60);
            let lit = d.luz.is_some() && value >= 0.5 && powered;
            let glow = if lit { -2.5 } else { 0.0 };
            if seta {
                p.post(cx, cy, 3.0, 10.0 - e[0] * 1000.0, r.w * 0.34, METAL, 0.0, 200);
                p.prop(SPHERE, Vec3::new(cx, cy, 12.0 - e[0] * 1000.0), Vec3::new(r.w * 0.9, r.w * 0.9, 9.0), Quat::IDENTITY, cap, glow, 90, 0);
            } else {
                p.post(cx, cy, 3.0, 9.0 - e[0] * 1000.0, r.w * 0.7, cap, glow, 0);
            }
        }
        "interruptor" => {
            p.prop(BOX, Vec3::new(cx, cy, 1.0), Vec3::new(r.w * 0.8, r.h * 0.75, 2.0), Quat::IDENTITY, DARK, 0.0, 160, 60);
            p.post(cx, cy, 2.0, 7.0, 8.0, METAL, 0.0, 220);
            // the lever leans up and down (about the panel's x)
            let lean = Quat::from_rotation_x(-e[0]);
            let pivot = Vec3::new(cx, cy, 7.0);
            let axis = lean * Vec3::Z;
            let len = if guarded { LEVER_GUARDED } else { LEVER };
            p.prop(CYLINDER, pivot + axis * len * 0.5, Vec3::new(3.0, len, 3.0), lean * Quat::from_rotation_x(FRAC_PI_2), METAL, 0.0, 70, 240);
            p.prop(SPHERE, pivot + axis * len, Vec3::splat(5.0), Quat::IDENTITY, def_color(d, METAL), 0.0, 70, 200);
            if near && d.posiciones.len() >= 2 {
                let n = d.posiciones.len();
                p.text(&d.posiciones[n - 1], cx, r.y + r.h - 1.5, 0.3, 2.6, PAINT, 0.0, 0.5);
                p.text(&d.posiciones[0], cx, r.y + 1.5, 0.3, 2.6, PAINT, 0.0, 0.5);
            }
        }
        "selector" | "llave" => {
            let key = d.kind == "llave";
            p.post(cx, cy, 0.0, 2.0, r.w * 0.86, [52, 54, 58], 0.0, 120);
            let turn = Quat::from_rotation_z(-e[0]);
            if key {
                p.post(cx, cy, 2.0, 6.0, r.w * 0.6, METAL, 0.0, 230);
                p.prop(BOX, Vec3::new(cx, cy, 9.0), Vec3::new(3.0, r.w * 0.5, 6.0), turn, [190, 160, 60], 0.0, 80, 230);
            } else {
                p.post(cx, cy, 2.0 + e[1] * 1000.0, 12.0 + e[1] * 1000.0, r.w * 0.58, DARK, 0.0, 30);
                p.prop(BOX, turn * Vec3::new(0.0, r.w * 0.17, 0.0) + Vec3::new(cx, cy, 12.4 + e[1] * 1000.0), Vec3::new(1.6, r.w * 0.3, 0.8), turn, PAINT, 0.0, 200, 0);
            }
            if near {
                let n = d.posiciones.len().max(2);
                let wrap = d.vuelta.as_deref() == Some("libre");
                let travel = d.recorrido.unwrap_or(if wrap { 360.0 } else { 270.0 }).to_radians() as f32;
                let step = travel / if wrap { n as f32 } else { (n - 1) as f32 };
                let mid = if wrap { 0.0 } else { (n - 1) as f32 * 0.5 };
                for (i, l) in d.posiciones.iter().enumerate() {
                    let a = (i as f32 - mid) * step;
                    let rad = r.w * 0.5 + 2.0;
                    let (x, y) = (cx + a.sin() * rad, cy + a.cos() * rad);
                    p.prop(BOX, Vec3::new(cx + a.sin() * (r.w * 0.47), cy + a.cos() * (r.w * 0.47), 0.3), Vec3::new(0.7, 2.2, 0.4), Quat::from_rotation_z(-a), PAINT, 0.0, 200, 0);
                    p.text(
                        l,
                        x,
                        y + 1.5 * a.cos().signum(),
                        0.3,
                        2.3,
                        PAINT,
                        0.0,
                        if a.sin() > 0.3 {
                            0.0
                        } else if a.sin() < -0.3 {
                            1.0
                        } else {
                            0.5
                        },
                    );
                }
            }
        }
        "rueda" => {
            p.post(cx, cy, 0.0, 2.0, r.w * 0.96, [44, 46, 50], 0.0, 100);
            p.post(cx, cy, 2.0, 14.0, r.w * 0.78, DARK, 0.0, 40);
            // grip ribs and its index turn with it
            let turn = Quat::from_rotation_z(-e[0]);
            for k in 0..12 {
                let a = k as f32 / 12.0 * std::f32::consts::TAU;
                let q = turn * Quat::from_rotation_z(a);
                p.prop(BOX, q * Vec3::new(0.0, r.w * 0.39, 0.0) + Vec3::new(cx, cy, 8.0), Vec3::new(1.4, 1.2, 11.0), q, [40, 40, 44], 0.0, 120, 30);
            }
            p.prop(BOX, turn * Vec3::new(0.0, r.w * 0.24, 0.0) + Vec3::new(cx, cy, 14.2), Vec3::new(1.5, r.w * 0.22, 0.6), turn, [240, 120, 30], 0.0, 200, 0);
            if near {
                let sweep = d.recorrido.unwrap_or(300.0).to_radians() as f32;
                for k in 0..=10 {
                    let a = (k as f32 / 10.0 - 0.5) * sweep;
                    p.prop(BOX, Vec3::new(cx + a.sin() * r.w * 0.5, cy + a.cos() * r.w * 0.5, 0.3), Vec3::new(0.6, if k % 5 == 0 { 3.0 } else { 1.6 }, 0.4), Quat::from_rotation_z(-a), PAINT, 0.0, 200, 0);
                }
            }
        }
        "volante" => {
            // a valve's hand wheel: a bolted flange on the plate, the stem out of it (further out
            // the more open the valve is: it is read from across the room), and on the stem the
            // wheel — a hub, five spokes, a rim and its spinner — turned as far as it is
            let rad = r.w * 0.5;
            let paint = def_color(d, [214, 168, 28]);
            let open = value.clamp(0.0, 1.0) as f32;
            p.post(cx, cy, 0.0, 3.0, r.w * 0.46, [52, 54, 58], 0.0, 140);
            for k in 0..6 {
                let a = k as f32 / 6.0 * std::f32::consts::TAU;
                p.post(cx + a.sin() * r.w * 0.18, cy + a.cos() * r.w * 0.18, 3.0, 4.4, 3.4, METAL, 0.0, 220);
            }
            p.post(cx, cy, 3.0, 8.0, 12.0, [70, 72, 78], 0.0, 200);
            let lift = 12.0 + 9.0 * open;
            p.post(cx, cy, 8.0, lift + 5.0, 6.0, METAL, 0.0, 240);
            // (it opens to the left, as valves do)
            let turn = Quat::from_rotation_z(e[0]);
            let hub = Vec3::new(cx, cy, lift + 1.2);
            p.post(cx, cy, lift - 1.5, lift + 3.6, r.w * 0.2, paint, 0.0, 60);
            for k in 0..5 {
                let q = turn * Quat::from_rotation_z(k as f32 / 5.0 * std::f32::consts::TAU);
                p.prop(BOX, q * Vec3::new(0.0, rad * 0.5, 0.0) + hub, Vec3::new(3.4, rad * 0.84, 2.6), q, paint, 0.0, 110, 40);
            }
            // the rim: short lengths end to end
            const RIM: usize = 20;
            let (ring, thick) = (rad * 0.9, 4.6);
            for k in 0..RIM {
                let q = turn * Quat::from_rotation_z(k as f32 / RIM as f32 * std::f32::consts::TAU);
                p.prop(CYLINDER, q * Vec3::new(0.0, ring, 0.0) + hub, Vec3::new(thick, std::f32::consts::TAU * ring / RIM as f32 * 1.16, thick), q * Quat::from_rotation_z(FRAC_PI_2), paint, 0.0, 110, 40);
            }
            // its spinner: a knob on the rim to turn it by
            p.prop(CYLINDER, turn * Vec3::new(0.0, ring, 0.0) + hub + Vec3::Z * 6.0, Vec3::new(5.4, 10.0, 5.4), Quat::from_rotation_x(FRAC_PI_2), DARK, 0.0, 150, 20);
        }
        "palanca" => {
            if d.ejes == Some(2) {
                p.post(cx, cy, 0.0, 9.0, r.w * 0.72, [30, 30, 32], 0.0, 0);
                let tilt = Quat::from_rotation_x(-e[0]) * Quat::from_rotation_y(e[1]);
                let pivot = Vec3::new(cx, cy, 6.0);
                let up = tilt * Vec3::Z;
                p.prop(CYLINDER, pivot + up * 22.0, Vec3::new(7.0, 44.0, 7.0), tilt * Quat::from_rotation_x(FRAC_PI_2), METAL, 0.0, 80, 220);
                p.prop(CYLINDER, pivot + up * 66.0, Vec3::new(22.0, 50.0, 22.0), tilt * Quat::from_rotation_x(FRAC_PI_2), [36, 36, 38], 0.0, 170, 20);
                p.prop(SPHERE, pivot + up * 92.0, Vec3::splat(10.0), Quat::IDENTITY, [190, 30, 26], 0.0, 120, 0);
            } else {
                p.prop(BOX, Vec3::new(cx, cy, 1.5), Vec3::new(r.w * 0.55, r.h * 0.92, 3.0), Quat::IDENTITY, DARK, 0.0, 170, 60);
                let lean = Quat::from_rotation_x(-e[0]);
                let pivot = Vec3::new(cx, cy, -18.0);
                let up = lean * Vec3::Z;
                p.prop(BOX, pivot + up * 36.0, Vec3::new(5.0, 5.0, 46.0), lean, METAL, 0.0, 80, 220);
                let grip = def_color(d, [36, 36, 40]);
                p.prop(BOX, pivot + up * 62.0, Vec3::new(r.w * 0.85, 13.0, 13.0), lean, grip, 0.0, 150, 30);
                if near {
                    // detent marks along the slot
                    let (lo, hi) = match &d.rango {
                        Some([a, b]) => (a.si().unwrap_or(0.0), b.si().unwrap_or(1.0)),
                        None => (0.0, 1.0),
                    };
                    let sweep = d.recorrido.unwrap_or(70.0).to_radians() as f32;
                    for det in &d.retenes {
                        let at = det.en.si().unwrap_or(0.0);
                        let a = (((at - lo) / (hi - lo).max(1e-9)) as f32 - 0.5) * sweep;
                        let y = cy + a.sin() * 54.0;
                        p.prop(BOX, Vec3::new(cx + r.w * 0.33, y, 0.3), Vec3::new(4.0, 0.7, 0.4), Quat::IDENTITY, PAINT, 0.0, 200, 0);
                        if let Some(n) = &det.nombre {
                            p.text(n, cx + r.w * 0.33 + 3.0, y, 0.3, 2.4, PAINT, 0.0, 0.0);
                        }
                    }
                }
            }
        }
        "tapa" => {
            // a box over what it guards, over the short lever under it, hinged at its top edge on
            // a bracket: it lifts toward the viewer and lies back over the top (never into the
            // panel; the layout keeps that room free)
            let lift = Quat::from_rotation_x(-e[0]);
            let pivot = Vec3::new(cx, r.y + r.h, GUARD_H);
            let at = |v: Vec3| pivot + lift * v;
            let col = def_color(d, [196, 36, 28]);
            p.prop(BOX, Vec3::new(cx, r.y + r.h - 1.2, (GUARD_H - 1.6) * 0.5), Vec3::new(r.w * 0.56, 2.4, GUARD_H - 1.6), Quat::IDENTITY, [40, 40, 44], 0.0, 120, 120);
            // lid, the two sides and the front, open at the back (the panel) and nowhere else
            p.prop(BOX, at(Vec3::new(0.0, -r.h * 0.5, 0.0)), Vec3::new(r.w, r.h, 1.6), lift, col, 0.0, 110, 30);
            for x in [r.w * 0.5 - 0.8, -r.w * 0.5 + 0.8] {
                p.prop(BOX, at(Vec3::new(x, -r.h * 0.5, -GUARD_H * 0.5)), Vec3::new(1.6, r.h, GUARD_H), lift, col, 0.0, 110, 30);
            }
            p.prop(BOX, at(Vec3::new(0.0, -r.h + 0.8, -GUARD_H * 0.5)), Vec3::new(r.w, 1.6, GUARD_H), lift, col, 0.0, 110, 30);
            // the hinge
            p.prop(CYLINDER, pivot, Vec3::new(3.0, r.w * 0.9, 3.0), Quat::from_rotation_z(FRAC_PI_2), METAL, 0.0, 80, 220);
            if e[1] < 0.5 {
                // the seal: a wire from the front to the panel
                p.prop(BOX, Vec3::new(cx, r.y + 0.8, GUARD_H * 0.5), Vec3::new(0.6, 0.6, GUARD_H + 2.0), Quat::IDENTITY, [200, 40, 40], 0.0, 100, 200);
            }
        }
        "bisel" => {
            // the housing round the glass, and its buttons on the strip (the held one in)
            let st = lunar_controls::mfd::STRIP;
            let per = d.botones.unwrap_or(lunar_controls::mfd::PER_SIDE).max(1);
            let housing = [38, 40, 44];
            for (x, y, w, h) in [(r.x, r.y, st, r.h), (r.x + r.w - st, r.y, st, r.h), (r.x + st, r.y, r.w - 2.0 * st, st), (r.x + st, r.y + r.h - st, r.w - 2.0 * st, st)] {
                p.prop(BOX, Vec3::new(x + w * 0.5, y + h * 0.5, 3.0), Vec3::new(w, h, 6.0), Quat::IDENTITY, housing, 0.0, 150, 60);
            }
            let (gw, gh) = (r.w - 2.0 * st, r.h - 2.0 * st);
            let held = e[0].round() as u32;
            for k in 0..per * 4 {
                let (_, [bx, by]) = lunar_controls::mfd::slot_pos(k, per, gw, gh, st);
                let side = k / per;
                let along = if side < 2 { gh } else { gw } / per as f32;
                let (w, h) = if side < 2 { (st * 0.55, along * 0.55) } else { (along * 0.55, st * 0.55) };
                let out = if held == k + 1 { 1.0 } else { 2.5 };
                p.prop(BOX, Vec3::new(r.x + st + bx, r.y + st + by, 6.0 + out * 0.5), Vec3::new(w, h, out), Quat::IDENTITY, [58, 60, 64], 0.0, 120, 40);
            }
        }
        "disyuntor" => {
            p.post(cx, cy, 0.0, 3.0, 11.0, DARK, 0.0, 40);
            let out = e[0] * 1000.0;
            p.post(cx, cy, 3.0, 8.0 + out, 8.0, [28, 28, 30], 0.0, 20);
            if e[1] > 0.5 {
                p.post(cx, cy, 6.5, 6.5 + out, 8.3, [240, 240, 236], 0.0, 0);
            }
        }
        "teclado" => {
            p.prop(BOX, Vec3::new(cx, cy, 3.0), Vec3::new(r.w, r.h, 6.0), Quat::IDENTITY, [48, 50, 54], 0.0, 160, 60);
            // display
            p.prop(BOX, Vec3::new(cx, r.y + r.h * 0.87, 6.2), Vec3::new(r.w * 0.86, r.h * 0.17, 0.6), Quat::IDENTITY, [10, 14, 10], 0.0, 40, 0);
            if powered {
                p.text(text, r.x + r.w * 0.9, r.y + r.h * 0.87, 6.7, r.h * 0.09, [255, 140, 40], 3.0, 1.0);
            }
            let keys = ["1", "2", "3", "4", "5", "6", "7", "8", "9", ".", "0", "CLR"];
            let held = e[0] as usize;
            for (k, label) in keys.iter().enumerate() {
                let (col, row) = (k % 3, k / 3);
                let x = r.x + r.w * (0.18 + 0.32 * col as f32);
                let y = r.y + r.h * (0.69 - 0.152 * row as f32);
                let down = if held == k + 1 { 1.5 } else { 0.0 };
                p.prop(BOX, Vec3::new(x, y, 7.0 - down), Vec3::new(r.w * 0.26, r.h * 0.12, 3.0), Quat::IDENTITY, [70, 72, 78], 0.0, 150, 20);
                if near {
                    p.text(label, x, y, 8.6 - down, r.h * 0.05, PAINT, 0.0, 0.5);
                }
            }
            let down = if held == 13 { 1.5 } else { 0.0 };
            p.prop(BOX, Vec3::new(cx, r.y + r.h * 0.07, 7.0 - down), Vec3::new(r.w * 0.86, r.h * 0.1, 3.0), Quat::IDENTITY, [40, 110, 50], 0.0, 150, 20);
            if near {
                p.text("ENTER", cx, r.y + r.h * 0.07, 8.6 - down, r.h * 0.05, PAINT, 0.0, 0.5);
            }
        }
        _ => {
            p.prop(BOX, Vec3::new(cx, cy, 2.0), Vec3::new(r.w, r.h, 4.0), Quat::IDENTITY, DARK, 0.0, 160, 0);
        }
    }
}

/// The model of an indicator, as it reads now.
fn indicator(p: &mut Pen, d: &ControlDef, r: &Rect, ind: &lunar_controls::Indicator, st: &lunar_controls::IndState, near: bool, plots: &crate::plots::Plots) {
    let (cx, cy) = (r.x + r.w * 0.5, r.y + r.h * 0.5);
    match &ind.kind {
        IndKind::Lamp { rules } => {
            p.post(cx, cy, 0.0, 3.0, r.w, [40, 41, 44], 0.0, 160);
            let off = rules.first().map_or([60, 255, 90], |r| r.color).map(|c| (f32::from(c) * 0.22) as u8);
            let (col, glow) = if st.lit > 0.0 { (st.color, -2.0 * st.lit) } else { (off, 0.0) };
            p.post(cx, cy, 3.0, 5.5, r.w * 0.72, col, glow, 0);
        }
        IndKind::Needle { sweep, lo, hi, zones, .. } => {
            let rad = r.w * 0.5;
            p.post(cx, cy, 0.0, 3.0, r.w, [70, 72, 76], 0.0, 200);
            p.post(cx, cy, 3.0, 4.0, r.w * 0.92, [14, 14, 16], 0.0, 0);
            if near {
                for k in 0..=10 {
                    let a = (k as f32 / 10.0 - 0.5) * sweep;
                    p.prop(BOX, Vec3::new(cx + a.sin() * rad * 0.8, cy + a.cos() * rad * 0.8, 4.3), Vec3::new(0.7, if k % 5 == 0 { 4.0 } else { 2.2 }, 0.3), Quat::from_rotation_z(-a), PAINT, 0.0, 200, 0);
                }
                for &(z0, z1, c) in zones {
                    let (u0, u1) = (((z0 - lo) / (hi - lo)) as f32, ((z1 - lo) / (hi - lo)) as f32);
                    let n = ((u1 - u0) * 24.0).ceil().max(1.0) as usize;
                    for k in 0..n {
                        let u = u0 + (u1 - u0) * (k as f32 + 0.5) / n as f32;
                        let a = (u - 0.5) * sweep;
                        let seg = (u1 - u0) * sweep * rad * 0.66 / n as f32;
                        p.prop(BOX, Vec3::new(cx + a.sin() * rad * 0.66, cy + a.cos() * rad * 0.66, 4.3), Vec3::new(seg * 1.05, 1.8, 0.3), Quat::from_rotation_z(-a), c, 0.0, 200, 0);
                    }
                }
                let unit = d.unidad.as_deref().unwrap_or("");
                p.text(unit, cx, cy - rad * 0.38, 4.3, rad * 0.16, PAINT, 0.0, 0.5);
                if let Some(t) = &d.rotulo {
                    p.text(t, cx, cy + rad * 0.3, 4.3, rad * 0.13, PAINT, 0.0, 0.5);
                }
            }
            let a = st.x;
            let turn = Quat::from_rotation_z(-a);
            p.prop(BOX, turn * Vec3::new(0.0, rad * 0.36, 0.0) + Vec3::new(cx, cy, 5.0), Vec3::new(1.4, rad * 0.86, 0.6), turn, [255, 150, 40], 0.4, 120, 0);
            p.post(cx, cy, 4.0, 6.0, 5.0, [30, 30, 30], 0.0, 200);
        }
        IndKind::Bar { .. } => {
            p.prop(BOX, Vec3::new(cx, cy, 1.5), Vec3::new(r.w, r.h, 3.0), Quat::IDENTITY, [16, 16, 18], 0.0, 120, 0);
            let h = (r.h - 4.0) * st.x;
            if h > 0.1 {
                p.prop(BOX, Vec3::new(cx, r.y + 2.0 + h * 0.5, 3.1), Vec3::new(r.w * 0.6, h, 0.4), Quat::IDENTITY, st.color, -1.5 * st.lit, 120, 0);
            }
        }
        IndKind::Seven { color, .. } => {
            p.prop(BOX, Vec3::new(cx, cy, 1.5), Vec3::new(r.w, r.h, 3.0), Quat::IDENTITY, [12, 8, 6], 0.0, 40, 0);
            if st.lit > 0.0 {
                p.text(&st.text, r.x + r.w - 3.0, cy, 3.2, r.h * 0.62, *color, 3.0, 1.0);
            }
        }
        IndKind::Counter { .. } => {
            p.prop(BOX, Vec3::new(cx, cy, 1.5), Vec3::new(r.w, r.h, 3.0), Quat::IDENTITY, [12, 12, 12], 0.0, 120, 0);
            p.text(&st.text, cx, cy, 3.2, r.h * 0.6, PAINT, 0.0, 0.5);
        }
        IndKind::Screen { color, .. } => {
            p.prop(BOX, Vec3::new(cx, cy, 3.0), Vec3::new(r.w, r.h, 6.0), Quat::IDENTITY, [34, 35, 38], 0.0, 170, 80);
            let (sw, sh) = (r.w * 0.9, r.h * 0.84);
            p.prop(BOX, Vec3::new(cx, cy, 6.1), Vec3::new(sw, sh, 0.4), Quat::IDENTITY, [4, 8, 10], if st.lit > 0.0 { -0.15 } else { 0.0 }, 30, 0);
            if st.lit > 0.0 && near {
                let rows = (st.lines.len() + 2) as f32;
                let lh = sh / rows;
                let x0 = cx - sw * 0.46;
                p.text(&st.text, x0, cy + sh * 0.5 - lh * 0.8, 6.4, lh * 0.62, *color, 2.6, 0.0);
                for (i, l) in st.lines.iter().enumerate() {
                    p.text(l, x0, cy + sh * 0.5 - lh * (2.0 + i as f32), 6.4, lh * 0.55, *color, 2.2, 0.0);
                }
            }
        }
        IndKind::Mfd(m) => mfd(p, d, r, m, st, near, plots),
        IndKind::Annunciator { cells, columns, .. } => {
            let cols = *columns as usize;
            let rows = cells.len().div_ceil(cols.max(1));
            let (cw, ch) = ((r.w - 4.0) / cols as f32, (r.h - 4.0) / rows.max(1) as f32);
            p.prop(BOX, Vec3::new(cx, cy, 1.0), Vec3::new(r.w, r.h, 2.0), Quat::IDENTITY, [20, 20, 22], 0.0, 120, 0);
            // every cell lettered at one size: the one at which a full line (the widest letters)
            // fits across a cell and two lines fit down it
            let widest = p.font.width(&"M".repeat(lunar_controls::indicator::CELL_LETTERS));
            let size = ((cw - 3.0) / widest.max(1e-3)).min(ch * 0.3);
            for (k, c) in cells.iter().enumerate() {
                let (col, row) = (k % cols, k / cols);
                let x = r.x + 2.0 + cw * (col as f32 + 0.5);
                let y = r.y + r.h - 2.0 - ch * (row as f32 + 0.5);
                let cs = st.cells.get(k).copied().unwrap_or_default();
                let base = if c.level >= 2 { [255, 40, 30] } else { [255, 170, 20] };
                let (col_rgb, glow) = if cs.lit > 0.0 { (base, -2.2) } else { (base.map(|v| (f32::from(v) * 0.16) as u8), 0.0) };
                p.prop(BOX, Vec3::new(x, y, 3.0), Vec3::new(cw - 1.2, ch - 1.2, 2.0), Quat::IDENTITY, col_rgb, glow, 60, 0);
                if near {
                    let txt = if cs.lit > 0.0 { [30, 16, 8] } else { [120, 110, 96] };
                    let n = c.lines.len() as f32;
                    for (i, line) in c.lines.iter().enumerate() {
                        p.text(line, x, y + size * 1.25 * ((n - 1.0) * 0.5 - i as f32), 4.1, size, txt, 0.0, 0.5);
                    }
                }
            }
        }
    }
}

/// The props of control `i` of panel `plan` posed `pose`, in the panel's plane (m, origin at its
/// bottom-left corner, x right, y up, z out of the face): what the checks of covers and reach
/// measure.
pub fn control_props(plan: &crate::kind::PanelPlan, i: usize, pose: &lunar_controls::Pose, font: &Font) -> Vec<Prop> {
    let mut out = PropScene::default();
    let d = &plan.def.mandos[i];
    let k = scale_of(d, plan.layout.scale);
    let rect = plan.layout.controls[i];
    let (r, [x, y]) = (nominal(&rect, k), rect.center());
    let mut pen = Pen { out: &mut out, frame: 0, f: Affine3A::IDENTITY, rot: Quat::IDENTITY, font, scratch: Vec::new(), scale: 1.0, origin: Vec3::ZERO };
    pen.zoom(x, y, k);
    let guarded = lunar_controls::layout::guard_of(&plan.def, i).is_some();
    control(&mut pen, d, &r, pose, 0.0, "", true, false, guarded);
    out.props
}

/// Where a hand works a control: the place on its moving part a finger presses or a hand takes
/// hold of, as that part stands now. For whoever puts a hand on it (the game's bodies).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Handle {
    /// The point of its moving part a hand works: a button's cap, a toggle's tip, the top of a
    /// knob, the grip of a lever, a wheel's hub, the lip of a cover.
    pub at: Vec3,
    /// The way out of it there (unit): where a finger comes from; a lever's own length.
    pub out: Vec3,
    /// The way that part lies or goes, square to `out` (unit): a toggle's travel, the bar of a
    /// T handle, a knob's index.
    pub axis: Vec3,
    /// How far it is turned about `out` (rad): knobs, wheels, keys.
    pub turn: f32,
    /// Half what a hand closes round there: a knob, a wheel's rim, a grip (0: a point).
    pub radius: f32,
    /// The plate it stands on: a point of its face under the control and the way out of it.
    /// Nothing of a hand goes behind that.
    pub plate: Vec3,
    pub normal: Vec3,
}

/// The `Handle` of a control drawn by `control` in rectangle `r` posed `pose` (the panel's
/// plane, mm at the nominal size, as `control` draws it), on its sub-element `elem` (a bezel's
/// button, a keypad's key). What `control` draws and what this says must agree: a test of the
/// game holds every control of every ship to it.
pub fn handle(d: &ControlDef, r: &Rect, pose: &lunar_controls::Pose, value: f64, elem: u8, guarded: bool) -> Handle {
    let (cx, cy) = (r.x + r.w * 0.5, r.y + r.h * 0.5);
    let e = pose.e;
    let straight = |at: Vec3, radius: f32| Handle { at, out: Vec3::Z, axis: Vec3::Y, turn: 0.0, radius, plate: Vec3::ZERO, normal: Vec3::Z };
    let mut h = match d.kind.as_str() {
        "pulsador" if d.modo.as_deref() == Some("seta") => straight(Vec3::new(cx, cy, 16.5 - e[0] * 1000.0), r.w * 0.45),
        "pulsador" => straight(Vec3::new(cx, cy, 9.0 - e[0] * 1000.0), r.w * 0.35),
        "interruptor" => {
            let lean = Quat::from_rotation_x(-e[0]);
            let len = if guarded { LEVER_GUARDED } else { LEVER };
            Handle { at: Vec3::new(cx, cy, 7.0) + lean * Vec3::Z * len, out: lean * Vec3::Z, axis: lean * Vec3::Y, radius: 2.5, ..straight(Vec3::ZERO, 0.0) }
        }
        "selector" | "llave" | "rueda" => {
            let (top, radius) = match d.kind.as_str() {
                "selector" => (12.0 + e[1] * 1000.0, r.w * 0.29),
                "llave" => (12.0, r.w * 0.25),
                _ => (14.0, r.w * 0.39),
            };
            Handle { axis: Quat::from_rotation_z(-e[0]) * Vec3::Y, turn: -e[0], ..straight(Vec3::new(cx, cy, top), radius) }
        }
        "volante" => {
            let lift = 12.0 + 9.0 * value.clamp(0.0, 1.0) as f32;
            Handle { axis: Quat::from_rotation_z(e[0]) * Vec3::Y, turn: e[0], ..straight(Vec3::new(cx, cy, lift + 1.2), r.w * 0.45) }
        }
        "palanca" if d.ejes == Some(2) => {
            let tilt = Quat::from_rotation_x(-e[0]) * Quat::from_rotation_y(e[1]);
            Handle { at: Vec3::new(cx, cy, 6.0) + tilt * Vec3::Z * 66.0, out: tilt * Vec3::Z, axis: tilt * Vec3::Y, radius: 11.0, ..straight(Vec3::ZERO, 0.0) }
        }
        "palanca" => {
            let lean = Quat::from_rotation_x(-e[0]);
            Handle { at: Vec3::new(cx, cy, -18.0) + lean * Vec3::Z * 62.0, out: lean * Vec3::Z, axis: Vec3::X, radius: 6.5, ..straight(Vec3::ZERO, 0.0) }
        }
        "tapa" => {
            // its front lip, the way a finger gets under it: out of the panel and from below it
            let lift = Quat::from_rotation_x(-e[0]);
            let out = (Vec3::Z * 0.8 + lift * Vec3::NEG_Y * 0.6).normalize();
            Handle { at: Vec3::new(cx, r.y + r.h, GUARD_H) + lift * Vec3::new(0.0, -r.h, 0.0), out, axis: Vec3::X, ..straight(Vec3::ZERO, 0.0) }
        }
        "bisel" => {
            let st = lunar_controls::mfd::STRIP;
            let per = d.botones.unwrap_or(lunar_controls::mfd::PER_SIDE).max(1);
            if u32::from(elem) < per * 4 {
                let (_, [bx, by]) = lunar_controls::mfd::slot_pos(u32::from(elem), per, r.w - 2.0 * st, r.h - 2.0 * st, st);
                let out = if e[0].round() as u32 == u32::from(elem) + 1 { 1.0 } else { 2.5 };
                straight(Vec3::new(r.x + st + bx, r.y + st + by, 6.0 + out), 0.0)
            } else {
                straight(Vec3::new(cx, cy, 6.4), 0.0)
            }
        }
        "disyuntor" => straight(Vec3::new(cx, cy, 8.0 + e[0] * 1000.0), 4.0),
        "teclado" => {
            let down = if e[0] as usize == usize::from(elem) + 1 { 1.5 } else { 0.0 };
            let (x, y) = if elem < 12 { (r.x + r.w * (0.18 + 0.32 * f32::from(elem % 3)), r.y + r.h * (0.69 - 0.152 * f32::from(elem / 3))) } else { (cx, r.y + r.h * 0.07) };
            straight(Vec3::new(x, y, 8.5 - down), 0.0)
        }
        _ => straight(Vec3::new(cx, cy, 4.0), 0.0),
    };
    h.plate = Vec3::new(cx, cy, 0.0);
    h
}

impl Ship {
    /// How control `k` stands now, for its model.
    pub fn control_pose(&self, k: usize) -> lunar_controls::Pose {
        let mut pose = lunar_controls::Pose::default();
        if let Some(c) = self.panels.controls.get(k) {
            c.mech.pose(&c.st, &mut pose);
        }
        pose
    }

    /// Where a hand works control `k` (its sub-element `elem`) in the ship's frame (m), posed
    /// `pose` (`control_pose`: as it stands; or as whoever follows it has it). None if there is
    /// no such control or its panel is gone.
    pub fn handle(&self, s: &Structure, k: usize, elem: u8, pose: &lunar_controls::Pose) -> Option<Handle> {
        let c = self.panels.controls.get(k)?;
        if !self.panels.panels[c.panel].alive {
            return None;
        }
        let plan = &self.kind.panels[c.panel];
        let d = &plan.def.mandos[c.index];
        let k = scale_of(d, plan.layout.scale);
        let [x, y] = c.rect.center();
        let h = handle(d, &nominal(&c.rect, k), pose, c.mech.value(&c.st), elem, lunar_controls::layout::guard_of(&plan.def, c.index).is_some());
        let f = Panels::frame(&self.kind, s, plan);
        // (drawn bigger than nominal about its own middle on the plate: `Pen::zoom`)
        let origin = Vec3::new(x, y, 0.0);
        let point = |p: Vec3| f.transform_point3((origin + (p - origin) * k) * 0.001);
        let dir = |v: Vec3| f.transform_vector3(v).normalize_or(Vec3::Z);
        Some(Handle { at: point(h.at), out: dir(h.out), axis: dir(h.axis), turn: h.turn, radius: h.radius * k * 0.001, plate: point(h.plate), normal: dir(h.normal) })
    }
}

/// A picture of the ship's systems (`plots`) on a display: its middle at (ox, oy), `h` mm from
/// there to its edge.
fn plot(p: &mut Pen, pl: &crate::plots::Plot, ox: f32, oy: f32, h: f32, z: f32, near: bool) {
    use crate::plots::Shape;
    for l in &pl.lines {
        p.line(ox + l.a[0] * h, oy + l.a[1] * h, ox + l.b[0] * h, oy + l.b[1] * h, 0.22 + 0.3 * l.weight, z, l.color, 0.7 + l.weight);
    }
    for m in &pl.marks {
        let (cx, cy, s) = (ox + m.at[0] * h, oy + m.at[1] * h, m.size * h);
        let mut seg = |a: [f32; 2], b: [f32; 2]| p.line(cx + a[0] * s, cy + a[1] * s, cx + b[0] * s, cy + b[1] * s, 0.5, z + 0.1, m.color, 1.9);
        match m.shape {
            Shape::Dot => {
                seg([-0.4, 0.0], [0.4, 0.0]);
                seg([0.0, -0.4], [0.0, 0.4]);
            }
            Shape::Square => {
                for k in 0..4 {
                    let c = [[-1.0, -1.0], [1.0, -1.0], [1.0, 1.0], [-1.0, 1.0]];
                    seg(c[k], c[(k + 1) % 4]);
                }
            }
            Shape::Diamond => {
                for k in 0..4 {
                    let c = [[0.0, -1.3], [1.3, 0.0], [0.0, 1.3], [-1.3, 0.0]];
                    seg(c[k], c[(k + 1) % 4]);
                }
            }
            Shape::Circle => {
                for k in 0..8 {
                    let at = |k: usize| {
                        let a = k as f32 * std::f32::consts::FRAC_PI_4;
                        [a.cos() * 1.1, a.sin() * 1.1]
                    };
                    seg(at(k), at(k + 1));
                }
            }
            Shape::Triangle => {
                for k in 0..3 {
                    let c = [[0.0, 1.3], [1.15, -0.8], [-1.15, -0.8]];
                    seg(c[k], c[(k + 1) % 3]);
                }
            }
            Shape::Cross => {
                seg([-1.0, 0.0], [-0.3, 0.0]);
                seg([0.3, 0.0], [1.0, 0.0]);
                seg([0.0, -1.0], [0.0, -0.3]);
                seg([0.0, 0.3], [0.0, 1.0]);
            }
            Shape::Caret => {
                // a chevron pointing out from the middle of the picture
                let out = glam::Vec2::from_array(m.at).normalize_or(glam::Vec2::Y);
                let side = out.perp();
                seg((side * 0.9 - out * 0.7).to_array(), (out * 0.7).to_array());
                seg((-side * 0.9 - out * 0.7).to_array(), (out * 0.7).to_array());
            }
        }
        if m.boxed {
            for k in 0..4 {
                let c = [[-1.9, -1.9], [1.9, -1.9], [1.9, 1.9], [-1.9, 1.9]];
                p.line(cx + c[k][0] * s, cy + c[k][1] * s, cx + c[(k + 1) % 4][0] * s, cy + c[(k + 1) % 4][1] * s, 0.3, z + 0.1, [235, 240, 240], 1.6);
            }
        }
        if m.tail != [0.0; 2] {
            p.line(cx, cy, cx + m.tail[0] * h, cy + m.tail[1] * h, 0.3, z + 0.1, m.color, 1.4);
        }
    }
    if near {
        for w in &pl.words {
            p.text(w.text.as_str(), ox + w.at[0] * h, oy + w.at[1] * h, z + 0.2, (w.size * h).max(1.6), w.color, 1.7, w.align);
        }
    }
}

/// A multi-function display as it reads: its glass, the legends of its bezel buttons (the page
/// shown boxed), the page's title and its instruments, each in its zone's colour.
fn mfd(p: &mut Pen, d: &ControlDef, r: &Rect, m: &lunar_controls::mfd::Mfd, st: &lunar_controls::IndState, near: bool, plots: &crate::plots::Plots) {
    use lunar_controls::mfd::{STRIP, WKind, slot_pos};
    let (gx, gy, gw, gh) = (r.x + STRIP, r.y + STRIP, r.w - 2.0 * STRIP, r.h - 2.0 * STRIP);
    let z = 6.4;
    // the glass, dark (a little light when on)
    p.prop(BOX, Vec3::new(gx + gw * 0.5, gy + gh * 0.5, 3.0), Vec3::new(gw, gh, 6.0), Quat::IDENTITY, [3, 6, 8], if st.lit > 0.0 { -0.05 } else { 0.0 }, 25, 0);
    if st.lit <= 0.0 {
        return;
    }
    let ms = &st.mfd;
    let page = &m.pages[ms.page.min(m.pages.len() - 1)];
    let dim = |c: [u8; 3]| c.map(|x| (f32::from(x) * 0.4) as u8);
    let base = m.color;
    let txt = [210, 235, 240];
    // legends by their buttons; the page shown boxed
    let lh = (gh * 0.055).clamp(2.5, 7.0);
    for (k, pg) in m.pages.iter().enumerate() {
        let ([lx, ly], _) = slot_pos(k as u32, m.per_side, gw, gh, STRIP);
        let side = k as u32 / m.per_side;
        let (x, align) = match side {
            0 => (gx + lx + 2.0, 0.0),
            1 => (gx + lx - 2.0, 1.0),
            _ => (gx + lx, 0.5),
        };
        let y = gy
            + ly
            + match side {
                2 => lh,
                3 => -lh,
                _ => 0.0,
            };
        let shown = k == ms.page;
        if near {
            p.text(&pg.button, x, y, z + 0.2, lh, if shown { [10, 14, 12] } else { base }, 1.6, align);
        }
        if shown {
            let w = (pg.button.chars().count() as f32 * 0.62 + 0.6) * lh;
            let x0 = x - w * align;
            p.fill(x0 - 0.8, y - lh * 0.65, w + 1.6, lh * 1.3, z, base, 1.4);
        }
    }
    // the title
    let (cx0, cy0, cw, ch) = (gx + gw * 0.15, gy + gh * 0.08, gw * 0.7, gh * 0.78);
    if near {
        p.text(&page.title, gx + gw * 0.5, gy + gh * 0.93, z, lh * 1.25, base, 1.8, 0.5);
    }
    p.line(cx0, cy0 + ch + 0.5, cx0 + cw, cy0 + ch + 0.5, 0.35, z, dim(base), 1.2);
    // the instruments
    for (wi, (w, ws)) in page.widgets.iter().zip(ms.widgets.iter()).enumerate() {
        let pad = 0.04;
        let (x, y, ww, wh) = (cx0 + cw * (w.cell[0] + w.cell[2] * pad), cy0 + ch * (w.cell[1] + w.cell[3] * pad), cw * w.cell[2] * (1.0 - 2.0 * pad), ch * w.cell[3] * (1.0 - 2.0 * pad));
        let col = ws.color;
        let small = (wh * 0.15).clamp(3.5, 9.0).min(ww * 0.12);
        match &w.kind {
            WKind::Value => {
                if near {
                    p.text(&w.label, x + ww * 0.5, y + wh * 0.8, z, small, txt, 1.2, 0.5);
                    p.text(&ws.text, x + ww * 0.5, y + wh * 0.38, z, (wh * 0.28).min(ww / (ws.text.chars().count().max(1) as f32 * 0.62)), col, 2.0, 0.5);
                }
                p.frame_lit(x, y, ww, wh, 0.3, z, dim(col), 1.0);
            }
            WKind::Bar => {
                let (bx, by, bw, bh) = (x + ww * 0.05, y + wh * 0.3, ww * 0.9, wh * 0.28);
                p.frame_lit(bx, by, bw, bh, 0.35, z, dim(col), 1.2);
                p.fill(bx + 0.6, by + 0.6, (bw - 1.2) * ws.u, bh - 1.2, z, col, 1.5);
                if near {
                    p.text(&w.label, x + ww * 0.05, y + wh * 0.8, z, small, txt, 1.2, 0.0);
                    p.text(&ws.text, x + ww * 0.95, y + wh * 0.8, z, small, col, 1.6, 1.0);
                }
            }
            WKind::Tank => {
                // a tank: its shell, its level, marks at quarters
                let (tw, th) = (ww * 0.55, wh * 0.66);
                let (tx, ty) = (x + (ww - tw) * 0.5, y + wh * 0.14);
                p.frame_lit(tx, ty, tw, th, 0.45, z, txt, 0.9);
                p.line(tx + tw * 0.25, ty + th, tx + tw * 0.75, ty + th + th * 0.06, 0.45, z, txt, 0.9);
                p.line(tx + tw * 0.75, ty + th + th * 0.06, tx + tw * 0.25, ty + th + th * 0.06, 0.45, z, txt, 0.9);
                p.fill(tx + 0.8, ty + 0.8, tw - 1.6, (th - 1.6) * ws.u, z, col, 1.5);
                for q in 1..4 {
                    let yy = ty + th * q as f32 / 4.0;
                    p.line(tx + tw, yy, tx + tw + tw * 0.12, yy, 0.3, z, dim(txt), 0.8);
                }
                if near {
                    p.text(&w.label, x + ww * 0.5, y + wh * 0.06, z, small, txt, 1.2, 0.5);
                    p.text(&ws.text, x + ww * 0.5, ty + th * 0.5, z, small * 1.1, [10, 14, 12], 1.6, 0.5);
                    p.text(&ws.text, x + ww * 0.5, y + wh * 0.94, z, small * 1.1, col, 1.6, 0.5);
                }
            }
            WKind::Dial => {
                let rad = (ww.min(wh) * 0.42).max(2.0);
                let (ccx, ccy) = (x + ww * 0.5, y + wh * 0.52);
                let sweep = 1.5 * std::f32::consts::PI;
                let segs = 30;
                for k in 0..segs {
                    let (u0, u1) = (k as f32 / segs as f32, (k + 1) as f32 / segs as f32);
                    let v = w.lo + (w.hi - w.lo) * f64::from((u0 + u1) * 0.5);
                    let zc = w.zones.iter().find(|(a, b, _)| v >= a.min(*b) && v <= a.max(*b)).map_or(dim(txt), |z| z.2);
                    let a0 = std::f32::consts::PI * 1.25 - u0 * sweep;
                    let a1 = std::f32::consts::PI * 1.25 - u1 * sweep;
                    p.line(ccx + a0.cos() * rad, ccy + a0.sin() * rad, ccx + a1.cos() * rad, ccy + a1.sin() * rad, 0.7, z, zc, 1.3);
                }
                let a = std::f32::consts::PI * 1.25 - ws.u * sweep;
                p.line(ccx, ccy, ccx + a.cos() * rad * 0.9, ccy + a.sin() * rad * 0.9, 0.6, z + 0.1, col, 1.8);
                if near {
                    p.text(&ws.text, ccx, ccy - rad * 0.55, z, small, col, 1.6, 0.5);
                    p.text(&w.label, ccx, y + wh * 0.02 + small * 0.5, z, small, txt, 1.2, 0.5);
                }
            }
            WKind::Graph => {
                let (fx, fy, fw, fh) = (x + ww * 0.02, y + wh * 0.04, ww * 0.96, wh * 0.78);
                p.frame_lit(fx, fy, fw, fh, 0.3, z, dim(txt), 0.9);
                // the zones' limits as faint dashes
                for &(a, b, c) in &w.zones {
                    for v in [a, b] {
                        let u = ((v - w.lo) / (w.hi - w.lo)) as f32;
                        if u > 0.01 && u < 0.99 {
                            let y = fy + fh * u;
                            for k in 0..12 {
                                let x0 = fx + fw * k as f32 / 12.0;
                                p.line(x0, y, x0 + fw / 24.0, y, 0.3, z, c.map(|x| (f32::from(x) * 0.5) as u8), 0.9);
                            }
                        }
                    }
                }
                if let Some(h) = ms.history.get(ms.page).and_then(|hs| hs.get(wi)) {
                    let n = (w.window * 2.0).max(4.0) as usize;
                    let at = |i: usize, v: f32| (fx + fw * i as f32 / (n - 1).max(1) as f32, fy + fh * (((f64::from(v) - w.lo) / (w.hi - w.lo)).clamp(0.0, 1.0) as f32));
                    let start = n.saturating_sub(h.len());
                    for i in 1..h.len() {
                        let (a, b) = (at(start + i - 1, h[i - 1]), at(start + i, h[i]));
                        p.line(a.0, a.1, b.0, b.1, 0.45, z, col, 1.6);
                    }
                }
                if near {
                    p.text(&w.label, x + ww * 0.02, y + wh * 0.92, z, small, txt, 1.2, 0.0);
                    p.text(&ws.text, x + ww * 0.98, y + wh * 0.92, z, small, col, 1.6, 1.0);
                }
            }
            WKind::Text(_) => {
                if near {
                    let rows = ws.lines.len().max(1) as f32;
                    let lsize = (wh / rows * 0.6).min(ww / 26.0 * 1.6).max(1.5);
                    for (i, l) in ws.lines.iter().enumerate() {
                        p.text(l, x, y + wh - (i as f32 + 0.6) * (wh / rows), z, lsize, base, 1.6, 0.0);
                    }
                }
            }
            WKind::Plot(name) => {
                // a square in the middle of its cell, the picture's (−1..1) to its edges
                let h = ww.min(wh) * 0.5;
                let (ox, oy) = (x + ww * 0.5, y + wh * 0.5);
                if let Some(pl) = plots.get(name) {
                    plot(p, pl, ox, oy, h, z, near);
                } else if near {
                    p.text("SIN DATOS", ox, oy, z, small, dim(txt), 1.0, 0.5);
                }
            }
            WKind::Map(areas, rects) => {
                for (((name, _), rc), (c, v)) in areas.iter().zip(rects).zip(ws.areas.iter()) {
                    let (ax, ay, aw, ah) = (x + ww * rc[0], y + wh * rc[1], ww * rc[2], wh * rc[3]);
                    p.fill(ax, ay, aw, ah, z - 0.1, c.map(|x| (f32::from(x) * 0.16) as u8), 0.8);
                    p.frame_lit(ax, ay, aw, ah, 0.6, z, *c, 1.4);
                    if near {
                        let sz = (aw * 0.9 / (name.chars().count().max(4) as f32 * 0.62)).min(ah * 0.24).min(9.0);
                        p.text(name, ax + aw * 0.5, ay + ah * 0.66, z, sz, txt, 1.3, 0.5);
                        let vz = (aw * 0.9 / (v.chars().count().max(3) as f32 * 0.62)).min(ah * 0.3).min(12.0);
                        p.text(v, ax + aw * 0.5, ay + ah * 0.34, z, vz, *c, 1.8, 0.5);
                    }
                }
                if near {
                    p.text(&w.label, x + ww * 0.5, y - small * 0.2, z, small, txt, 1.0, 0.5);
                }
            }
        }
    }
    let _ = d;
}

impl Ship {
    /// This frame's props, glyphs and lamps of the ship (on structure `s`, seen from `eye`).
    pub fn scene(&self, s: &Structure, cat: &lunar_core::structure::catalog::Catalog, font: &Font, eye: DVec3, out: &mut PropScene, lamps: &mut Vec<Lamp>) {
        let kind: &ShipKind = &self.kind;
        // two frames: what is outside, and what is inside its hull (lit by its own lamps only)
        let frame = out.frames.len() as u16;
        out.frames.push(PropFrame { pos: s.pos, rot: s.rot, inside: false });
        out.frames.push(PropFrame { pos: s.pos, rot: s.rot, inside: true });
        let eye_local = s.to_local(eye);
        // from outside, its inside panels only near (an open door shows them)
        let eye_in = crate::atmos::room_of(kind, eye_local).is_some();
        // ---- panels ----
        for (pi, plan) in kind.panels.iter().enumerate() {
            let prt = &self.panels.panels[pi];
            if !prt.alive {
                continue;
            }
            let f = Panels::frame(kind, s, plan);
            let size = Vec3::new(plan.layout.size[0], plan.layout.size[1], 0.0) * 0.001;
            let center = f.transform_point3(size * 0.5);
            let dist = center.distance(eye_local);
            // seen from behind: nothing to draw
            let facing = f.transform_vector3(Vec3::Z).dot(eye_local - center) > 0.0;
            if dist > CONTROLS || !facing || (!eye_in && dist > OUTSIDE && crate::atmos::room_of(kind, center).is_some()) {
                continue;
            }
            let near = dist < TEXT;
            let (_, rot, _) = f.to_scale_rotation_translation();
            let frame = frame + u16::from(crate::atmos::room_of(kind, center + f.transform_vector3(Vec3::Z) * 0.1).is_some());
            let mut pen = Pen { out: &mut *out, frame, f, rot, font, scratch: Vec::new(), scale: 1.0, origin: Vec3::ZERO };
            if near {
                for r in &plan.layout.frames {
                    pen.outline(r, 0.7, PAINT);
                }
                for t in &plan.layout.texts {
                    pen.text(&t.text, t.at[0], t.at[1], 0.3, t.size, PAINT, 0.0, 0.5);
                }
            }
            let mut buf = String::new();
            for (ck, c) in self.panels.controls.iter().enumerate().filter(|(_, c)| c.panel == pi) {
                let d = &plan.def.mandos[c.index];
                let first = pen.out.props.len();
                let mut pose = lunar_controls::Pose::default();
                c.mech.pose(&c.st, &mut pose);
                buf.clear();
                c.mech.text(&c.st, &mut buf);
                let k = scale_of(d, plan.layout.scale);
                let (r, [x, y]) = (nominal(&c.rect, k), c.rect.center());
                pen.zoom(x, y, k);
                let guarded = lunar_controls::layout::guard_of(&plan.def, c.index).is_some();
                control(&mut pen, d, &r, &pose, c.mech.value(&c.st), &buf, prt.powered, near, guarded);
                pen.zoom(0.0, 0.0, 1.0);
                // the one the hand is aimed at: a faint glow of its own and a thin warm line round it
                if self.panels.aimed == Some(crate::panels::Aimed::Control(ck)) {
                    for p in &mut pen.out.props[first..] {
                        if p.emissive >= 0.0 {
                            p.emissive = p.emissive.max(AIM_GLOW);
                        }
                    }
                    aim_frame(&mut pen, &c.rect);
                }
            }
            for (ik, i) in self.panels.indicators.iter().enumerate().filter(|(_, i)| i.panel == pi) {
                let d = &plan.def.mandos[i.index];
                let k = scale_of(d, plan.layout.scale);
                let (r, [x, y]) = (nominal(&i.rect, k), i.rect.center());
                pen.zoom(x, y, k);
                indicator(&mut pen, d, &r, &i.ind, &i.st, near, &self.plots);
                pen.zoom(0.0, 0.0, 1.0);
                if self.panels.aimed == Some(crate::panels::Aimed::Indicator(ik)) {
                    aim_frame(&mut pen, &i.rect);
                }
            }
        }
        // ---- decals: with their part, gone with it ----
        for d in &kind.decals {
            let mut m = Affine3A::IDENTITY;
            if let Some(p) = d.part {
                let part = &s.parts[p as usize];
                if !part.alive {
                    continue;
                }
                if part.bone > 0 {
                    m = s.bones.get(usize::from(part.bone)).copied().unwrap_or(Affine3A::IDENTITY);
                }
            }
            let frame = frame + u16::from(crate::atmos::room_of(kind, d.center).is_some());
            out.decals.push(DecalQuad { frame, center: m.transform_point3(d.center), u: m.transform_vector3(d.u), v: m.transform_vector3(d.v), uv: d.uv, tint: d.tint, emissive: d.emissive });
        }
        // ---- what is written on its parts: outside in its own frame, inside in its inside's ----
        let mut placed: Vec<Placed> = Vec::new();
        lunar_core::structure::labels::show(s, cat, font, eye_local, |at| frame + u16::from(crate::atmos::room_of(kind, at).is_some()), &mut placed, out);
        // ---- lamps: their light and their lens ----
        for (k, m) in self.machines.iter().enumerate() {
            let Some(l) = &kind.machines[k].light else {
                continue;
            };
            let Some(p) = m.part else { continue };
            let part = &s.parts[p as usize];
            if !part.alive {
                continue;
            }
            let mut b = m.m.light();
            if let Some([hz, duty]) = l.parpadeo {
                b *= f32::from(u8::from((self.t as f32 * hz).fract() < duty));
            }
            let at = part.local.transform_point3(Vec3::from_array(l.en));
            let lin = |c: u8| (f32::from(c) / 255.0).powf(2.2);
            if let Some(r) = l.lente {
                let n = l.dir.map_or(Vec3::Y, |d| part.local.transform_vector3(Vec3::from_array(d)).normalize_or(Vec3::Y));
                let q = Quat::from_rotation_arc(Vec3::Y, n);
                out.props.push(Prop { frame, mesh: CYLINDER, pos: at, rot: q, size: Vec3::new(r * 2.0, 0.01, r * 2.0), color: l.color, emissive: 4.0 * b * l.intensidad.min(3.0), rough: 40, metal: 0 });
            }
            if b > 0.02 {
                let dir = l.dir.map_or(Vec3::ZERO, |d| s.rot * part.local.transform_vector3(Vec3::from_array(d)).normalize_or_zero());
                let inside = kind.machines[k].lit_inside;
                lamps.push(Lamp {
                    pos: s.to_world(at),
                    color: [lin(l.color[0]) * l.intensidad * b, lin(l.color[1]) * l.intensidad * b, lin(l.color[2]) * l.intensidad * b],
                    range: l.alcance,
                    dir,
                    cone: l.cono.map_or(0.0, |c| (c * 0.5).to_radians().cos()),
                    inside,
                });
            }
        }
        // ---- rams and rods: as far out as they are ----
        for (k, a) in self.actuators.iter().enumerate() {
            let plan = &kind.actuators[k];
            let j = &self.joints[plan.joint];
            let Some((a0, b0)) = a.link.ends(j) else {
                continue;
            };
            let parent = kind.joints[plan.joint].parent.map_or(Affine3A::IDENTITY, |p| self.poses[p]);
            let (pa, pb) = (parent.transform_point3(Vec3::from_array(a0.map(|x| x as f32))), parent.transform_point3(Vec3::from_array(b0.map(|x| x as f32))));
            let len = pa.distance(pb);
            if len < 0.02 || pa.distance(eye_local) > 60.0 {
                continue;
            }
            let dir = (pb - pa) / len;
            let q = Quat::from_rotation_arc(Vec3::Y, dir);
            let (bore, color) = match a.drive {
                lunar_machines::actuator::drive::Drive::Hydraulic { area, .. } => ((area / std::f64::consts::PI).sqrt() as f32 * 2.0, [70, 74, 80]),
                lunar_machines::actuator::drive::Drive::Pneumatic { area, .. } => ((area / std::f64::consts::PI).sqrt() as f32 * 2.0, [60, 110, 160]),
                _ => (0.05, [90, 90, 96]),
            };
            let barrel = len * 0.55;
            out.props.push(Prop { frame, mesh: CYLINDER, pos: pa + dir * barrel * 0.5, rot: q, size: Vec3::new(bore + 0.02, barrel, bore + 0.02), color, emissive: 0.0, rough: 120, metal: 160 });
            let rod = len * 0.5;
            out.props.push(Prop { frame, mesh: CYLINDER, pos: pb - dir * rod * 0.5, rot: q, size: Vec3::new(bore * 0.5, rod, bore * 0.5), color: [215, 218, 222], emissive: 0.0, rough: 25, metal: 250 });
            out.props.push(Prop { frame, mesh: SPHERE, pos: pa, rot: q, size: Vec3::splat(bore + 0.03), color, emissive: 0.0, rough: 120, metal: 160 });
            out.props.push(Prop { frame, mesh: SPHERE, pos: pb, rot: q, size: Vec3::splat(bore * 0.8), color, emissive: 0.0, rough: 120, metal: 160 });
        }
        let _ = (CONE, named_color("verde"));
    }
}
