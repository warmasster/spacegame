//! Gestures: what a player says with the hands — a wave, pointing where one looks, a thumb up,
//! "OK", "stop", a salute, "come here", a shrug. Each is data (`assets/defs/gestos.jsonc`): for
//! each arm, where the hand is at each moment (a place by name: so far from the shoulder or
//! from the eyes, the way the palm faces, the way the fingers point, the way the elbow goes)
//! and the pose of its fingers there; how long it lasts; whether it is held until it is let go.
//! Nothing here knows one gesture from another.
//!
//! A gesture is told to the other players as its number in the file (from 1; 0: none): the
//! same number makes the same gesture on any body of the rig, so what is seen of the others is
//! made here from one byte.
//!
//! The player chooses with the wheel (`Wheel`): the key held, the mouse toward a gesture (or
//! its number), the key let go.
use crate::handwork::{Arm, Target, held};
use glam::{DVec3, Vec3};
use serde::Deserialize;
use std::collections::BTreeMap;

/// What a place is measured from.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    /// The arm's own shoulder.
    #[default]
    Hombro,
    /// The eyes.
    Ojos,
}

/// A place a hand is held at. Axes: x toward the arm's own side (out from the body's middle),
/// y up, z ahead: the same place serves either arm.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlaceDef {
    #[serde(default)]
    pub desde: Origin,
    #[serde(default)]
    pub en: [f32; 3],
    /// Instead of `en`: this far from the shoulder along the way the player looks (m), the
    /// fingers along it.
    #[serde(default)]
    pub mirada: Option<f32>,
    /// The way the palm faces, and the way the fingers point.
    pub palma: [f32; 3],
    #[serde(default = "ahead")]
    pub dedos: [f32; 3],
    /// The way the elbow goes, if not down and out.
    #[serde(default)]
    pub codo: Option<[f32; 3]>,
}

fn ahead() -> [f32; 3] {
    [0.0, 0.0, 1.0]
}

/// A gesture as written. `der`, `izq`: for each arm, keys of time (s), place and hand pose.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GestureDef {
    pub id: String,
    /// As the wheel names it.
    pub nombre: String,
    pub dura: f32,
    /// Held at its last key until it is let go (else it ends when its time is up).
    #[serde(default)]
    pub mantener: bool,
    #[serde(default)]
    pub der: Vec<(f32, String, String)>,
    #[serde(default)]
    pub izq: Vec<(f32, String, String)>,
}

/// `gestos.jsonc`. The order of `gestos` is what the others are told (a gesture's number): new
/// ones go at the end.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GestureSet {
    pub puntos: BTreeMap<String, PlaceDef>,
    pub gestos: Vec<GestureDef>,
}

#[derive(Clone, Copy, Debug)]
struct Place {
    from: Origin,
    at: Vec3,
    look: Option<f32>,
    palm: Vec3,
    fingers: Vec3,
    pole: Option<Vec3>,
}

/// A key of an arm: when, where, the fingers.
#[derive(Clone, Copy, Debug)]
struct Key {
    t: f32,
    place: usize,
    pose: [f32; 5],
}

#[derive(Clone, Debug)]
pub struct Gesture {
    pub id: String,
    pub name: String,
    pub secs: f32,
    pub hold: bool,
    /// Left, right.
    arms: [Vec<Key>; 2],
}

/// `GestureSet` ready to use: its places and poses found.
#[derive(Clone, Debug)]
pub struct GestureKit {
    places: Vec<Place>,
    pub list: Vec<Gesture>,
}

impl GestureKit {
    pub fn new(def: &GestureSet, poses: &BTreeMap<String, [f32; 5]>) -> Result<GestureKit, String> {
        if def.gestos.len() > 254 {
            return Err("gestos: caben 254".into());
        }
        let names: Vec<&String> = def.puntos.keys().collect();
        let places = def.puntos.values().map(|p| Place { from: p.desde, at: Vec3::from(p.en), look: p.mirada, palm: Vec3::from(p.palma), fingers: Vec3::from(p.dedos), pole: p.codo.map(Vec3::from) }).collect();
        let mut list = Vec::new();
        for g in &def.gestos {
            let arm = |keys: &[(f32, String, String)]| -> Result<Vec<Key>, String> {
                if keys.windows(2).any(|w| w[1].0 < w[0].0) {
                    return Err(format!("gesto {}: las claves van en orden de tiempo", g.id));
                }
                keys.iter()
                    .map(|(t, place, pose)| {
                        Ok(Key {
                            t: *t,
                            place: names.iter().position(|n| *n == place).ok_or_else(|| format!("gesto {}: no hay punto '{place}'", g.id))?,
                            pose: poses.get(pose).copied().ok_or_else(|| format!("gesto {}: no hay pose de mano '{pose}' en el esqueleto", g.id))?,
                        })
                    })
                    .collect()
            };
            let arms = [arm(&g.izq)?, arm(&g.der)?];
            if !(g.dura > 0.0) || (arms[0].is_empty() && arms[1].is_empty()) {
                return Err(format!("gesto {}: dura algo y mueve una mano al menos", g.id));
            }
            list.push(Gesture { id: g.id.clone(), name: g.nombre.clone(), secs: g.dura, hold: g.mantener, arms });
        }
        Ok(GestureKit { places, list })
    }

    /// A gesture's number by its id (0: there is none such).
    pub fn number(&self, id: &str) -> u8 {
        self.list.iter().position(|g| g.id == id).map_or(0, |k| k as u8 + 1)
    }

    fn target(&self, key: &Key, arm: &Arm, look: Vec3, eyes: DVec3) -> Target {
        let p = &self.places[key.place];
        let out = -arm.inward();
        let (up, ahead) = (arm.up(), arm.ahead());
        let world = |v: Vec3| out * v.x + up * v.y + ahead * v.z;
        let (at, fingers) = match p.look {
            Some(far) => (arm.shoulder + (look * far).as_dvec3(), look),
            None => (if p.from == Origin::Ojos { eyes } else { arm.shoulder } + world(p.at).as_dvec3(), world(p.fingers)),
        };
        let own = if arm.side == 0 { 1.0 } else { -1.0 };
        held(arm, at, world(p.palm), fingers, key.pose, p.pole.map(|v| Vec3::new(v.x * own, v.y, v.z)))
    }
}

/// The gesture a body is making.
#[derive(Clone, Copy, Debug, Default)]
pub struct Playing {
    /// Its number (from 1; 0: none).
    pub id: u8,
    pub t: f32,
    /// What was last told (`set`): told again the same, nothing starts again.
    told: u8,
    /// Each arm doing the other's part (its own hand is not free).
    swap: bool,
}

/// Seconds from one key to the next are eased (slow out, slow in).
fn ease(k: f32) -> f32 {
    let k = k.clamp(0.0, 1.0);
    k * k * (3.0 - 2.0 * k)
}

impl Playing {
    /// Make gesture `id` (0: none), as told: it starts when what is told changes.
    pub fn set(&mut self, kit: &GestureKit, id: u8) {
        if id == self.told {
            return;
        }
        self.told = id;
        (self.id, self.t) = (if usize::from(id) <= kit.list.len() { id } else { 0 }, 0.0);
    }

    /// Make gesture `id` now, from its start (the player's own choice: again if it is the same).
    pub fn start(&mut self, kit: &GestureKit, id: u8) {
        self.told = 0;
        self.set(kit, id);
        self.told = self.id;
    }

    /// No gesture any more.
    pub fn stop(&mut self) {
        *self = Playing::default();
    }

    /// A frame of it: where each hand (left, right) is asked to be. `look`: the way the player
    /// looks; `eyes`: where its eyes are; `free`: the hands with nothing else in them.
    pub fn update(&mut self, dt: f32, kit: &GestureKit, arms: &[Arm; 2], look: Vec3, eyes: DVec3, free: [bool; 2]) -> [Option<Target>; 2] {
        let Some(g) = usize::from(self.id).checked_sub(1).and_then(|k| kit.list.get(k)) else { return [None, None] };
        if self.t == 0.0 {
            // one arm's gesture goes to the other if its own hand is taken
            let one = g.arms[0].is_empty() != g.arms[1].is_empty();
            let own = usize::from(g.arms[0].is_empty());
            self.swap = one && !free[own] && free[1 - own];
        }
        self.t += dt;
        if self.t >= g.secs && !g.hold {
            (self.id, self.told, self.t) = (0, 0, 0.0);
            return [None, None];
        }
        let t = self.t.min(g.secs);
        std::array::from_fn(|i| {
            let keys = &g.arms[if self.swap { 1 - i } else { i }];
            if keys.is_empty() || !free[i] {
                return None;
            }
            let next = keys.iter().position(|k| k.t > t).unwrap_or(keys.len());
            let (a, b) = (&keys[next.saturating_sub(1)], &keys[next.min(keys.len() - 1)]);
            let k = if b.t > a.t { ease((t - a.t) / (b.t - a.t)) } else { 0.0 };
            let (ta, tb) = (kit.target(a, &arms[i], look, eyes), kit.target(b, &arms[i], look, eyes));
            let pole = match (ta.pole, tb.pole) {
                (None, None) => None,
                (pa, pb) => {
                    let usual = crate::body::ELBOW_POLE * Vec3::new(if i == 0 { 1.0 } else { -1.0 }, 1.0, 1.0);
                    Some(pa.unwrap_or(usual).lerp(pb.unwrap_or(usual), k))
                }
            };
            Some(Target { at: ta.at.lerp(tb.at, f64::from(k)), rot: ta.rot.slerp(tb.rot, k), fingers: std::array::from_fn(|f| ta.fingers[f] + (tb.fingers[f] - ta.fingers[f]) * k), pole })
        })
    }
}

// ---------------------------------------------------------------- choosing one

/// How far the mouse goes from the wheel's middle before it chooses (in the wheel's own
/// measure: 1 is its rim), and how much of that a count of the mouse is.
const DEAD: f32 = 0.3;
const MOUSE: f32 = 0.006;

/// The wheel of gestures, while its key is held.
#[derive(Clone, Copy, Debug, Default)]
pub struct Wheel {
    pub open: bool,
    /// Where the mouse has gone from its middle (right, up; 1: the rim).
    at: [f32; 2],
    pub pick: Option<usize>,
}

impl Wheel {
    /// The key held or let go: on letting go, the gesture chosen (its place in the list), if
    /// one was.
    pub fn hold(&mut self, held: bool) -> Option<usize> {
        if held && !self.open {
            *self = Wheel { open: true, at: [0.0; 2], pick: None };
        }
        if !held && self.open {
            self.open = false;
            return self.pick;
        }
        None
    }

    /// The mouse moved (counts to the right and down) over a wheel of `n` gestures: true when
    /// the wheel took it.
    pub fn mouse(&mut self, dx: f64, dy: f64, n: usize) -> bool {
        if !self.open {
            return false;
        }
        self.at = [self.at[0] + dx as f32 * MOUSE, self.at[1] - dy as f32 * MOUSE];
        let far = (self.at[0] * self.at[0] + self.at[1] * self.at[1]).sqrt();
        if far > 1.0 {
            self.at = [self.at[0] / far, self.at[1] / far];
        }
        self.pick = (far >= DEAD && n > 0).then(|| slot(self.at, n));
        true
    }

    /// A number key (from 0) while it is open: that gesture. True when the wheel took it.
    pub fn number(&mut self, k: usize, n: usize) -> bool {
        if self.open && k < n {
            self.pick = Some(k);
            let a = angle(k, n);
            self.at = [a.sin() * 0.8, a.cos() * 0.8];
        }
        self.open
    }

    /// Where the mouse is in it (right, up; 1: the rim).
    pub fn at(&self) -> [f32; 2] {
        self.at
    }
}

/// Where slot `k` of `n` is round the wheel (rad from the top, clockwise): the first at the top.
pub fn angle(k: usize, n: usize) -> f32 {
    k as f32 / n.max(1) as f32 * std::f32::consts::TAU
}

/// The slot of `n` that `at` (right, up) points to.
fn slot(at: [f32; 2], n: usize) -> usize {
    let a = at[0].atan2(at[1]).rem_euclid(std::f32::consts::TAU);
    ((a / std::f32::consts::TAU * n as f32).round() as usize) % n
}

/// The wheel as the HUD shows it: the gestures' names, the one chosen, where the mouse is.
#[derive(Clone, Debug, Default)]
pub struct WheelView {
    pub names: Vec<String>,
    pub pick: Option<usize>,
    pub at: [f32; 2],
}

/// Draws the wheel: a ring of names low on the screen over the tool slots (the middle of the
/// picture stays free), the one chosen lit, a dot where the mouse is.
pub fn draw_wheel(painter: &egui::Painter, screen: egui::Rect, k: f32, w: &WheelView) {
    use crate::hud::{ACCENT, DIM, TEXT, plate};
    use egui::{Align2, Color32, FontId, Stroke, pos2, vec2};
    let n = w.names.len().max(1);
    // (wider than tall: the names lie side by side along its top and its bottom)
    let radius = 88.0 * k;
    let c = pos2(screen.center().x, screen.bottom() - 150.0 * k - radius);
    painter.circle_filled(c, radius * 0.5, Color32::from_rgba_premultiplied(7, 10, 14, 150));
    painter.circle_stroke(c, radius * 0.5, Stroke::new(1.0, Color32::from_rgba_premultiplied(58, 72, 84, 150)));
    painter.circle_stroke(c, radius * DEAD * 0.5, Stroke::new(1.0, Color32::from_rgba_premultiplied(58, 72, 84, 90)));
    painter.text(c, Align2::CENTER_CENTER, "GESTOS", FontId::proportional(9.5 * k), DIM);
    for (i, name) in w.names.iter().enumerate() {
        let a = angle(i, n);
        let at = c + vec2(a.sin() * 2.3, -a.cos()) * radius;
        let chosen = w.pick == Some(i);
        let size = vec2(98.0, 22.0) * k;
        let r = egui::Rect::from_center_size(at, size);
        plate(painter, r, k * 0.7, chosen.then_some(ACCENT), if chosen { 1.0 } else { 0.8 });
        painter.text(r.left_center() + vec2(9.0 * k, 0.0), Align2::LEFT_CENTER, format!("{}", i + 1), FontId::proportional(10.0 * k), if chosen { ACCENT } else { DIM });
        painter.text(r.center() + vec2(7.0 * k, 0.0), Align2::CENTER_CENTER, name, FontId::proportional(11.5 * k), if chosen { TEXT } else { DIM });
    }
    painter.circle_filled(c + vec2(w.at[0], -w.at[1]) * radius * 0.5, 3.2 * k, ACCENT);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kit() -> GestureKit {
        let set: GestureSet = lunar_core::defs::parse(
            "gestos",
            r#"{ "puntos": {
                    "alto": { "en": [0.1, 0.2, 0.3], "palma": [0, 0, 1], "dedos": [0, 1, 0] },
                    "lado": { "en": [0.3, 0.2, 0.3], "palma": [0, 0, 1], "dedos": [0, 1, 0], "codo": [1, 0, 0] },
                    "frente": { "mirada": 0.6, "palma": [0, -1, 0] },
                    "ceja": { "desde": "ojos", "en": [0.1, 0.05, 0.1], "palma": [0, -1, 0], "dedos": [-1, 0, 0] } },
                 "gestos": [
                    { "id": "hola", "nombre": "HOLA", "dura": 1.0, "der": [[0.2, "alto", "abierta"], [0.6, "lado", "abierta"], [1.0, "alto", "abierta"]] },
                    { "id": "senalar", "nombre": "SEÑALAR", "dura": 0.3, "mantener": true, "der": [[0.3, "frente", "apuntar"]] },
                    { "id": "nose", "nombre": "NO SÉ", "dura": 1.0, "der": [[0.5, "lado", "abierta"]], "izq": [[0.5, "lado", "abierta"]] } ] }"#,
        )
        .unwrap();
        let poses: BTreeMap<String, [f32; 5]> = [("abierta", [-0.8; 5]), ("apuntar", [0.6, -0.9, 0.9, 0.9, 0.9])].into_iter().map(|(k, v)| (k.to_string(), v)).collect();
        GestureKit::new(&set, &poses).unwrap()
    }

    #[test]
    fn a_gesture_is_its_number_and_a_wrong_one_is_none() {
        let kit = kit();
        assert_eq!((kit.number("hola"), kit.number("senalar"), kit.number("nada")), (1, 2, 0));
        let mut p = Playing::default();
        p.set(&kit, 9);
        assert_eq!(p.id, 0);
        p.set(&kit, 2);
        assert_eq!(p.id, 2);
        // told the same again: it does not start again
        p.t = 0.2;
        p.set(&kit, 2);
        assert_eq!(p.t, 0.2);
        // ... but chosen again by its own player it does
        p.start(&kit, 2);
        assert!(p.t == 0.0 && p.id == 2);
        p.set(&kit, 0);
        assert_eq!(p.id, 0);
    }

    #[test]
    fn the_wheel_chooses_by_where_the_mouse_goes_and_gives_it_on_letting_go() {
        let mut w = Wheel::default();
        assert!(!w.mouse(100.0, 0.0, 8) && w.hold(false).is_none());
        assert!(w.hold(true).is_none() && w.open);
        // a little: nothing chosen yet; far to the right: the one at three o'clock of eight
        assert!(w.mouse(10.0, 0.0, 8) && w.pick.is_none());
        w.mouse(200.0, 0.0, 8);
        assert_eq!(w.pick, Some(2));
        // up: the first; down: the one opposite
        w.mouse(-400.0, -400.0, 8);
        w.mouse(210.0, -200.0, 8);
        assert_eq!(w.pick, Some(0), "{:?}", w.at());
        w.mouse(0.0, 600.0, 8);
        assert_eq!(w.pick, Some(4));
        assert!((w.at()[0].powi(2) + w.at()[1].powi(2)).sqrt() <= 1.0 + 1e-5);
        // its number chooses too; letting go gives it, once
        assert!(w.number(6, 8));
        assert_eq!(w.hold(false), Some(6));
        assert!(w.hold(false).is_none() && !w.number(1, 8));
        // let go with nothing chosen: nothing
        w.hold(true);
        assert!(w.hold(false).is_none());
        assert_eq!((slot([0.0, 1.0], 8), slot([-1.0, 0.0], 8), slot([-0.1, 1.0], 8)), (0, 6, 0));
    }
}
