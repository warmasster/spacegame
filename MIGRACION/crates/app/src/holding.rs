//! How a tool is held and how it moves in the hands: all of it data (`gear.jsonc`, a tool's
//! `"sujecion"`), nothing here knows a welder from a launcher.
//!
//! A tool is held at a place of the eye's frame. It has weight: it lags behind the look and
//! settles (`inercia`), it bobs with the steps (`paso`), it kicks when it fires (`retroceso`).
//! It is made of pieces — its body and what moves on it (a hatch, a trigger, the rocket it is
//! loaded with) — and has named points: where each hand holds it, where it is loaded, its
//! muzzle. And it has clips (`lunar_core::anim::clip`): movements written key by key — taking it
//! out, firing, reloading — that move the tool itself (`herramienta.en`, `herramienta.giro`),
//! its pieces (`<pieza>.giro` about its axis, `<pieza>.en`, `<pieza>.ver`), say where each hand
//! is at each moment (`manos`: a point of the tool, or `cuerpo:<point>` of the body, with the
//! hand's pose there) and what a hand carries (`lleva`: a piece goes with a hand from a moment
//! on, held by one of its points, until it is back on the tool).
use glam::{Quat, Vec3};
use lunar_core::anim::{
    Xf,
    clip::{ChannelDef, Clip, ClipDef, Playing},
    skeleton::frame_turn,
    spring::{Spring3, Sway3, hz_of},
};
use serde::Deserialize;
use std::collections::BTreeMap;

fn x_axis() -> [f32; 3] {
    [1.0, 0.0, 0.0]
}

/// A piece besides the body: where its origin is on the tool (its pivot) and the axis it turns
/// about; `oculta`: it shows only while a clip says so.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PieceDef {
    pub en: [f32; 3],
    #[serde(default = "x_axis")]
    pub eje: [f32; 3],
    #[serde(default)]
    pub oculta: bool,
}

/// A place on the tool (its frame: x left, y up, z ahead). For a hand: the middle of the palm,
/// the way the palm faces and the way from index to little finger. `pieza`: it goes with that
/// piece.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolPoint {
    pub en: [f32; 3],
    #[serde(default)]
    pub palma: Option<[f32; 3]>,
    #[serde(default)]
    pub traves: Option<[f32; 3]>,
    #[serde(default)]
    pub pieza: Option<String>,
}

/// A hand on the tool at rest: the point it holds and the pose of its fingers there.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HandOn {
    pub punto: String,
    pub pose: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TriggerDef {
    pub pieza: String,
    pub giro: f32,
    pub mano: usize,
    pub pose: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ToolClipDef {
    dura: f32,
    #[serde(default)]
    bucle: bool,
    #[serde(default)]
    canales: BTreeMap<String, ChannelDef>,
    #[serde(default)]
    eventos: Vec<(f32, String)>,
    /// Where each hand is (`izq`, `der`): keys of time, point (of the tool; `cuerpo:<name>` of
    /// the body; `libre`: nowhere) and hand pose.
    #[serde(default)]
    manos: BTreeMap<String, Vec<(f32, String, String)>>,
    /// What a hand carries: keys of time, piece, hand (`izq`, `der`; `` back on the tool) and
    /// the point of the piece it is held by.
    #[serde(default)]
    lleva: Vec<(f32, String, String, String)>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(try_from = "ToolClipDef")]
pub struct ToolClip {
    pub clip: Clip,
    /// Left, right.
    pub hands: [Vec<(f32, String, String)>; 2],
    pub carries: Vec<(f32, String, Option<usize>, String)>,
}

fn side(name: &str) -> Result<usize, String> {
    match name {
        "izq" => Ok(0),
        "der" => Ok(1),
        other => Err(format!("mano '{other}' (izq o der)")),
    }
}

impl TryFrom<ToolClipDef> for ToolClip {
    type Error = String;

    fn try_from(d: ToolClipDef) -> Result<ToolClip, String> {
        let clip = Clip::try_from(ClipDef { dura: d.dura, bucle: d.bucle, canales: d.canales, eventos: d.eventos })?;
        let mut hands: [Vec<(f32, String, String)>; 2] = Default::default();
        for (name, keys) in d.manos {
            if keys.windows(2).any(|w| w[1].0 < w[0].0) {
                return Err(format!("manos.{name}: las claves van en orden de tiempo"));
            }
            hands[side(&name)?] = keys;
        }
        let mut carries = Vec::new();
        for (t, piece, hand, point) in d.lleva {
            carries.push((t, piece, if hand.is_empty() { None } else { Some(side(&hand)?) }, point));
        }
        carries.sort_by(|a, b| a.0.total_cmp(&b.0));
        Ok(ToolClip { clip, hands, carries })
    }
}

fn inercia() -> f32 {
    0.03
}

fn paso() -> f32 {
    0.012
}

fn vida() -> f32 {
    0.08
}

fn rebote() -> f32 {
    0.5
}

fn arrastre() -> f32 {
    0.3
}

fn alabeo() -> f32 {
    2.5
}

fn one() -> f32 {
    1.0
}

/// What of the body carries a tool: a point of the rig (`rigs/*.jsonc`, `puntos`) it goes with
/// — up and down with a shoulder as the body strides, sags and lands — and how much of that
/// point's movement under the eyes it takes (1: all of it).
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RestDef {
    pub punto: String,
    #[serde(default = "one")]
    pub peso: f32,
}

/// How a tool is held (a tool's `"sujecion"`).
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HoldDef {
    /// Where its origin is in the eye's frame (x left, y up, z ahead; m), and how it is turned
    /// there (degrees: nose up, nose to the left, rolled).
    pub en: [f32; 3],
    #[serde(default)]
    pub giro: [f32; 3],
    /// Its weight in the hands: how far it lags behind the look (m per rad/s; it turns as much,
    /// in rad per m), how far it bobs with the steps (m), and the half-life it settles with.
    #[serde(default = "inercia")]
    pub inercia: f32,
    #[serde(default = "paso")]
    pub paso: f32,
    #[serde(default = "vida")]
    pub vida: f32,
    /// How it swings as it settles (1: it never goes past its place; 0.5: once past and back;
    /// less: livelier), how much of the holder's changes of speed leave it behind (a start, a
    /// stop, a jump, a landing: 0 none .. 1 all), and how far it rolls into a turn of the look
    /// (degrees per rad/s).
    #[serde(default = "rebote")]
    pub rebote: f32,
    #[serde(default = "arrastre")]
    pub arrastre: f32,
    #[serde(default = "alabeo")]
    pub alabeo: f32,
    /// The point it turns about as the look goes up and down (the eye's frame with the look
    /// level; none: the eye itself, so it keeps its place in the picture): a shoulder, for what
    /// rests on one.
    #[serde(default)]
    pub pivote: [f32; 3],
    /// What of the body carries it, if anything does.
    #[serde(default)]
    pub apoyo: Option<RestDef>,
    /// What it does when it fires: back (m), nose up (degrees), and the half-life it comes
    /// back with.
    #[serde(default)]
    pub retroceso: [f32; 3],
    #[serde(default)]
    pub piezas: BTreeMap<String, PieceDef>,
    #[serde(default)]
    pub puntos: BTreeMap<String, ToolPoint>,
    /// The hands on it at rest (left, right); none: that hand is free.
    #[serde(default)]
    pub manos: [Option<HandOn>; 2],
    /// Its trigger: the piece that turns while it is pulled and how far (degrees), the hand
    /// that pulls it (0 left, 1 right) and that hand's pose pulling.
    #[serde(default)]
    pub gatillo: Option<TriggerDef>,
    #[serde(default)]
    pub clips: BTreeMap<String, ToolClip>,
}

impl Default for HoldDef {
    fn default() -> HoldDef {
        HoldDef { en: [-0.2, -0.2, 0.4], giro: [0.0; 3], inercia: inercia(), paso: paso(), vida: vida(), rebote: rebote(), arrastre: arrastre(), alabeo: alabeo(), pivote: [0.0; 3], apoyo: None, retroceso: [0.0; 3], piezas: BTreeMap::new(), puntos: BTreeMap::new(), manos: [None, None], gatillo: None, clips: BTreeMap::new() }
    }
}

impl HoldDef {
    /// What its data names that is not there (a clip moving a piece it has not, a hand at a
    /// point it has not): for whoever loads it.
    pub fn check(&self) -> Result<(), String> {
        let point = |p: &str| p == "libre" || p.starts_with("cuerpo:") || self.puntos.contains_key(p);
        for h in self.manos.iter().flatten() {
            if !point(&h.punto) {
                return Err(format!("una mano en el punto '{}', que no hay", h.punto));
            }
        }
        if let Some(g) = &self.gatillo
            && (!self.piezas.contains_key(&g.pieza) || g.mano > 1)
        {
            return Err(format!("el gatillo es la pieza '{}', que no hay, o de una mano que no es 0 ni 1", g.pieza));
        }
        for (name, p) in &self.puntos {
            if p.pieza.as_ref().is_some_and(|x| !self.piezas.contains_key(x)) {
                return Err(format!("el punto '{name}' va con una pieza que no hay"));
            }
        }
        for (name, c) in &self.clips {
            for ch in c.clip.channels.keys() {
                let (what, how) = ch.split_once('.').ok_or_else(|| format!("clip '{name}': canal '{ch}' (es <pieza>.<giro|en|ver>)"))?;
                if !(what == "herramienta" || self.piezas.contains_key(what)) || !matches!(how, "giro" | "en" | "ver") {
                    return Err(format!("clip '{name}': canal '{ch}' de algo que no hay"));
                }
            }
            for keys in &c.hands {
                if let Some(k) = keys.iter().find(|k| !point(&k.1)) {
                    return Err(format!("clip '{name}': una mano va al punto '{}', que no hay", k.1));
                }
            }
            for k in &c.carries {
                if !self.piezas.contains_key(&k.1) || self.puntos.get(&k.3).is_none_or(|p| p.pieza.as_deref() != Some(k.1.as_str())) {
                    return Err(format!("clip '{name}': lleva '{}' por '{}', que no es un punto de esa pieza", k.1, k.3));
                }
            }
        }
        Ok(())
    }
}

/// Where a hand is asked to be: a point of the tool (its frame: place, the way the palm faces,
/// from index to little finger), or one of the body by its name, or nowhere.
#[derive(Clone, Debug, PartialEq)]
pub enum Place {
    Tool(Vec3, Vec3, Vec3),
    Body(String),
    Free,
}

/// A hand between two places (`k` of the way from the first to the second), its fingers by the
/// names of two poses likewise.
#[derive(Clone, Debug, PartialEq)]
pub struct HandAt {
    pub from: Place,
    pub to: Place,
    pub k: f32,
    pub poses: (String, String),
}

/// The tool as it is this frame.
#[derive(Clone, Debug, Default)]
pub struct ToolPose {
    /// The tool in the eye's frame.
    pub at: Xf,
    /// Each piece (in the order of the tool's `piezas`) in the tool's frame, and whether it
    /// shows.
    pub pieces: Vec<(Xf, bool)>,
    /// Each piece carried by a hand: the hand, and the piece's point it is held by (in the
    /// piece's own frame: place and turn).
    pub carried: Vec<Option<(usize, Xf)>>,
    pub hands: [Option<HandAt>; 2],
}

/// What the body of whoever holds it is doing: its steps (from the gait), how fast it goes
/// (world axes, m/s) and how far the point of it that carries the tool (`apoyo`) is from where
/// it would be with the body still under its eyes (the eye's frame, m).
#[derive(Clone, Copy, Debug, Default)]
pub struct Steps {
    pub phase: f32,
    pub walk: f32,
    pub vel: Vec3,
    pub carry: Vec3,
}

#[derive(Default)]
pub struct Holding {
    pub playing: Option<Playing>,
    /// Events of the clip due this frame.
    pub events: Vec<String>,
    /// Its weight: how far it is from its place (m) and how far it is turned from it (rad:
    /// nose up, nose to the left, rolled), each on its spring.
    sway: Sway3,
    lean: Sway3,
    kick: Spring3,
    /// Where its clip has it, as its weight follows that (m; degrees).
    clip_at: Sway3,
    clip_turn: Sway3,
    /// A clip started and has not moved it yet.
    fresh: bool,
    /// How far it has come in for hands that did not reach it (the eye's frame, m).
    pull: Vec3,
    carry: Spring3,
    last: Option<Quat>,
    vel: Option<Vec3>,
    clock: f32,
    /// How far the trigger is pulled (0..1).
    pulled: f32,
}

/// Hands that do not reach the tool: it comes in to them in about this long (s), and goes back
/// out to its place at this rate (1/s) when they do.
const PULL_IN: f32 = 0.05;
const PULL_BACK: f32 = 2.0;
/// The most it comes in (m), the most the body's point carries it (m), and the most of a change
/// of speed in a frame that is felt (m/s: more is a change of place, not a push).
const PULL_MAX: f32 = 0.45;
const CARRY_MAX: f32 = 0.3;
const SHOVE_MAX: f32 = 4.0;

/// The middle of a critically damped spring's swing when kicked: the kick that takes it `peak`
/// away (`spring::Spring`: x(t) = v t e^(-y t), most at t = 1/y).
fn kick_for(peak: f32, halflife: f32) -> f32 {
    peak * (2.0 * std::f32::consts::LN_2 / halflife.max(1e-4)) * std::f32::consts::E
}

/// A tool's turn from its angles (degrees: nose up, nose to the left, rolled).
pub fn turned(giro: [f32; 3]) -> Quat {
    Quat::from_rotation_y(giro[1].to_radians()) * Quat::from_rotation_x(-giro[0].to_radians()) * Quat::from_rotation_z(giro[2].to_radians())
}

impl Holding {
    /// Starts clip `name` (if the tool has it). Whether it did.
    pub fn play(&mut self, def: &HoldDef, name: &str) -> bool {
        let has = def.clips.contains_key(name);
        self.playing = has.then(|| Playing::start(name));
        self.fresh = has;
        has
    }

    /// A blow to it (a clip's `golpe:<left>,<up>,<ahead>[,<nose up>[,<roll>]]`: m/s and
    /// degrees/s): a latch sprung, a rocket seated, a hatch slapped shut.
    fn blow(&mut self, what: &str) {
        let mut n = [0.0f32; 5];
        for (k, v) in what.split(',').take(5).enumerate() {
            n[k] = v.trim().parse().unwrap_or(0.0);
        }
        self.sway.kick(Vec3::new(n[0], n[1], n[2]));
        self.lean.kick(Vec3::new(n[3].to_radians(), 0.0, n[4].to_radians()));
    }

    /// The clip being played, by name.
    pub fn clip(&self) -> Option<&str> {
        self.playing.as_ref().map(|p| p.clip.as_str())
    }

    /// It fired: the kick.
    pub fn fired(&mut self, def: &HoldDef) {
        let [back, up, half] = def.retroceso;
        let half = if half > 0.0 { half } else { def.vida };
        self.kick.kick(Vec3::new(kick_for(up.to_radians(), half), 0.0, kick_for(back, half)));
    }

    /// `dt` seconds on: the tool this frame. `look`: the eye's turn (world: x left, y up of the
    /// view, z where it looks); `level`: the same with the look level (what a `pivote` is given
    /// in); `short`: how far the hands fell short of it last frame (eye's frame: from where a
    /// hand got to, to where it was asked): it comes in to them; `trigger`: its trigger is held.
    pub fn update(&mut self, dt: f32, def: &HoldDef, look: Quat, level: Quat, steps: Steps, short: Vec3, trigger: bool) -> ToolPose {
        self.events.clear();
        self.clock += dt;
        self.pulled += ((if trigger { 1.0 } else { 0.0 }) - self.pulled) * (1.0 - (-dt / 0.035).exp());
        // the clip
        let mut over = false;
        let playing = self.playing.as_mut().and_then(|p| {
            let c = def.clips.get(&p.clip)?;
            over = !p.step(&c.clip, dt);
            self.events.extend(c.clip.events(p.before, p.t).map(str::to_string));
            Some((c, p.t.min(c.clip.secs)))
        });
        for k in 0..self.events.len() {
            if let Some(what) = self.events[k].strip_prefix("golpe:").map(str::to_string) {
                self.blow(&what);
            }
        }
        let value = |name: &str| playing.and_then(|(c, t)| c.clip.at(name, t));
        // its weight on the springs of the arms that hold it: so stiff, swinging so much
        let (hz, ratio) = (hz_of(def.vida.max(1e-3)), def.rebote);
        // behind the look as it turns, and rolled into the turn
        let turn = self.last.map_or(Vec3::ZERO, |l| (l.inverse() * look).to_scaled_axis() / dt.max(1e-4));
        self.last = Some(look);
        let lag = Vec3::new((-turn.y * def.inercia).clamp(-0.06, 0.06), (turn.x * def.inercia).clamp(-0.06, 0.06), 0.0);
        // left behind by every change of the holder's speed: a start, a stop, a jump, a landing
        if let Some(was) = self.vel {
            let shove = (look.inverse() * (steps.vel - was)).clamp_length_max(SHOVE_MAX);
            self.sway.kick(-shove * def.arrastre);
            // (and its nose goes the other way: up as it is left under, down as it is left over)
            self.lean.kick(Vec3::new(-shove.y * def.arrastre * 0.8, 0.0, shove.x * def.arrastre * 0.8));
        }
        self.vel = Some(steps.vel);
        let sway = self.sway.step(lag, hz, ratio, dt);
        // (left behind, it points behind too: to the side it lags to, and under the look)
        let roll = (turn.y * def.alabeo.to_radians()).clamp(-0.2, 0.2);
        let lean = self.lean.step(Vec3::new(0.0, 0.0, roll), hz, ratio, dt);
        // with the steps: side to side once a stride, down twice, its nose after them; and the
        // breath of whoever holds it
        let swing = std::f32::consts::TAU * steps.phase;
        let stride = def.paso * steps.walk;
        let breath = (self.clock * 1.1).sin();
        let bob = Vec3::new(swing.sin() * 0.6, -(2.0 * swing).cos() * 0.5, 0.0) * stride + Vec3::new((self.clock * 0.53).sin() * 0.001, breath * 0.002, 0.0);
        let rock = Vec3::new((2.0 * swing + 0.9).cos() * stride * 1.6 + breath * 0.003, swing.cos() * stride * 1.2, swing.sin() * stride * 1.8);
        let kick_half = if def.retroceso[2] > 0.0 { def.retroceso[2] } else { def.vida.max(1e-3) };
        let kick = self.kick.step(Vec3::ZERO, kick_half, dt);
        // hands that do not reach it: it comes in to them, and goes back out when they do
        let short = short.clamp_length_max(PULL_MAX);
        self.pull += -short * (1.0 - (-dt / PULL_IN).exp()) - self.pull * (1.0 - (-dt * PULL_BACK).exp());
        self.pull = self.pull.clamp_length_max(PULL_MAX);
        // what of the body carries it takes it along
        let carry = self.carry.step(steps.carry.clamp_length_max(CARRY_MAX) * def.apoyo.as_ref().map_or(0.0, |a| a.peso), 0.03, dt);
        // where its clip has it: its weight follows that too (it swings past where a hand
        // brings it and settles), from where the clip starts
        let want_at = value("herramienta.en").map_or(Vec3::ZERO, |v| Vec3::new(v[0], v[1], v[2]));
        let want_turn = value("herramienta.giro").map_or(Vec3::ZERO, |v| Vec3::new(v[0], v[1], v[2]));
        if std::mem::take(&mut self.fresh) {
            (self.clip_at, self.clip_turn) = (Sway3::at(want_at), Sway3::at(want_turn));
        }
        let clip_at = self.clip_at.step(want_at, hz * 1.6, (ratio + 0.1).min(1.0), dt);
        let clip_turn = self.clip_turn.step(want_turn, hz * 1.6, (ratio + 0.1).min(1.0), dt);
        let rot = turned(def.giro) * turned([clip_turn.x, clip_turn.y, clip_turn.z]) * Quat::from_rotation_y(sway.x * 2.0 + lean.y + rock.y) * Quat::from_rotation_x(-sway.y * 2.0 - kick.x - lean.x - rock.x) * Quat::from_rotation_z(lean.z + rock.z);
        let rot = rot.normalize();
        let held = Vec3::from(def.en) + clip_at + sway + bob + self.pull + carry - rot * Vec3::Z * kick.z;
        // it turns up and down with the look about its pivot (a shoulder), not about the eye
        let pivot = Vec3::from(def.pivote);
        let at = Xf::new(held + (look.inverse() * level) * pivot - pivot, rot);
        // its pieces
        let mut pieces = Vec::with_capacity(def.piezas.len());
        for (name, p) in &def.piezas {
            let turn = value(&format!("{name}.giro")).map_or(0.0, |v| v[0]) + def.gatillo.as_ref().filter(|g| g.pieza == *name).map_or(0.0, |g| g.giro * self.pulled);
            let moved = value(&format!("{name}.en")).map_or(Vec3::ZERO, |v| Vec3::new(v[0], v[1], v[2]));
            let shows = value(&format!("{name}.ver")).map_or(!p.oculta, |v| v[0] > 0.5);
            pieces.push((Xf::new(Vec3::from(p.en) + moved, Quat::from_axis_angle(Vec3::from(p.eje).normalize_or(Vec3::X), turn.to_radians())), shows));
        }
        // what a hand carries now
        let mut carried = vec![None; pieces.len()];
        if let Some((c, t)) = playing {
            for (kt, piece, hand, point) in &c.carries {
                if *kt > t {
                    break;
                }
                let (Some(i), Some(pt)) = (def.piezas.keys().position(|k| k == piece), def.puntos.get(point)) else { continue };
                carried[i] = hand.map(|h| {
                    let (n, a) = palm_axes(pt);
                    (h, Xf::new(Vec3::from(pt.en) - Vec3::from(def.piezas[piece].en), frame_turn(Vec3::Y, Vec3::X, n, a)))
                });
            }
        }
        // the hands
        let place = |name: &str| -> Place {
            if name == "libre" {
                return Place::Free;
            }
            if let Some(b) = name.strip_prefix("cuerpo:") {
                return Place::Body(b.to_string());
            }
            let Some(pt) = def.puntos.get(name) else { return Place::Free };
            let (n, a) = palm_axes(pt);
            // (a point of a piece goes with it)
            match pt.pieza.as_ref().and_then(|x| def.piezas.keys().position(|k| k == x)) {
                Some(i) => {
                    let home = Vec3::from(def.piezas[pt.pieza.as_ref().unwrap()].en);
                    let x = pieces[i].0;
                    Place::Tool(x.pos + x.rot * (Vec3::from(pt.en) - home), x.rot * n, x.rot * a)
                }
                None => Place::Tool(Vec3::from(pt.en), n, a),
            }
        };
        let hands = std::array::from_fn(|h| {
            let rest = def.manos[h].as_ref();
            let keys = playing.map(|(c, t)| (&c.hands[h], t)).filter(|(k, _)| !k.is_empty());
            match keys {
                Some((keys, t)) => {
                    let next = keys.iter().position(|k| k.0 > t).unwrap_or(keys.len());
                    let a = &keys[next.saturating_sub(1)];
                    let b = &keys[next.min(keys.len() - 1)];
                    let k = if b.0 > a.0 { ((t - a.0) / (b.0 - a.0)).clamp(0.0, 1.0) } else { 0.0 };
                    Some(HandAt { from: place(&a.1), to: place(&b.1), k: k * k * (3.0 - 2.0 * k), poses: (a.2.clone(), b.2.clone()) })
                }
                None => rest.map(|r| {
                    // (the finger goes to the trigger as it is pulled)
                    let pull = def.gatillo.as_ref().filter(|g| g.mano == h);
                    HandAt { from: place(&r.punto), to: place(&r.punto), k: pull.map_or(0.0, |_| self.pulled), poses: (r.pose.clone(), pull.map_or_else(|| r.pose.clone(), |g| g.pose.clone())) }
                }),
            }
        });
        if over {
            self.playing = None;
        }
        ToolPose { at, pieces, carried, hands }
    }
}

/// The way the palm faces and the way from index to little finger at a point (unit).
fn palm_axes(p: &ToolPoint) -> (Vec3, Vec3) {
    let n = Vec3::from(p.palma.unwrap_or([1.0, 0.0, 0.0])).normalize_or(Vec3::X);
    let a = Vec3::from(p.traves.unwrap_or([0.0, -1.0, 0.0]));
    (n, (a - n * a.dot(n)).normalize_or(n.any_orthonormal_vector()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def() -> HoldDef {
        serde_json::from_str(
            r#"{ "en": [-0.3, -0.1, 0.1], "retroceso": [0.06, 5, 0.07],
                 "piezas": { "cohete": { "en": [0, 0.02, 0.2], "oculta": true }, "puerta": { "en": [0.04, 0.05, 0.2], "eje": [0, 0, 1] } },
                 "puntos": {
                    "agarre_der": { "en": [0, -0.08, 0.1], "palma": [1, 0, 0], "traves": [0, -1, 0.2] },
                    "agarre_izq": { "en": [0, -0.07, 0.33], "palma": [-1, 0, 0], "traves": [0, -1, 0.2] },
                    "cohete_mano": { "en": [0, 0.02, 0.2], "palma": [0, -1, 0], "traves": [0, 0, -1], "pieza": "cohete" },
                    "boca": { "en": [0, 0, 0.45] }
                 },
                 "manos": [ { "punto": "agarre_izq", "pose": "agarre" }, { "punto": "agarre_der", "pose": "dedo_fuera" } ],
                 "clips": {
                    "recarga": { "dura": 2.0,
                        "canales": { "puerta.giro": [[0.2, 0], [0.5, 85], [1.5, 85], [1.8, 0]], "cohete.ver": { "curva": "salto", "claves": [[0, 0], [0.8, 1]] }, "herramienta.en": [[0, 0, 0, 0], [0.4, 0.1, -0.1, 0.1], [1.7, 0.1, -0.1, 0.1], [2.0, 0, 0, 0]] },
                        "manos": { "izq": [[0.2, "agarre_izq", "agarre"], [0.8, "cuerpo:bolsa_izq", "coger"], [1.4, "cohete_mano", "coger"], [1.9, "agarre_izq", "agarre"]] },
                        "lleva": [[0.8, "cohete", "izq", "cohete_mano"], [1.4, "cohete", "", "cohete_mano"]],
                        "eventos": [[1.8, "cargado"]] }
                 } }"#,
        )
        .unwrap()
    }

    #[test]
    fn what_is_written_is_checked_against_what_the_tool_has() {
        let d = def();
        d.check().unwrap();
        let mut bad = d.clone();
        bad.manos[0] = Some(HandOn { punto: "nada".into(), pose: "agarre".into() });
        assert!(bad.check().is_err());
        let mut bad = d.clone();
        bad.piezas.remove("puerta");
        assert!(bad.check().unwrap_err().contains("puerta.giro"));
        let mut bad = d;
        bad.puntos.get_mut("cohete_mano").unwrap().pieza = None;
        assert!(bad.check().is_err(), "a piece is carried by a point of its own");
    }

    #[test]
    fn at_rest_it_is_where_it_is_held_with_the_hands_on_its_grips() {
        let d = def();
        let mut h = Holding::default();
        let mut pose = ToolPose::default();
        for _ in 0..120 {
            pose = h.update(1.0 / 60.0, &d, Quat::IDENTITY, Quat::IDENTITY, Steps::default(), Vec3::ZERO, false);
        }
        // (but for the breath of whoever holds it: a couple of millimetres)
        assert!((pose.at.pos - Vec3::new(-0.3, -0.1, 0.1)).length() < 0.004 && pose.at.rot.angle_between(Quat::IDENTITY) < 0.006, "{:?}", pose.at);
        assert_eq!(pose.pieces.len(), 2);
        // (pieces in the order of their names: the rocket, hidden; the hatch, shut)
        assert!(!pose.pieces[0].1 && pose.pieces[1].1 && pose.pieces[1].0.rot.angle_between(Quat::IDENTITY) < 1e-6);
        assert!(pose.carried.iter().all(Option::is_none));
        let left = pose.hands[0].as_ref().unwrap();
        assert!(matches!(left.from, Place::Tool(p, n, _) if (p - Vec3::new(0.0, -0.07, 0.33)).length() < 1e-6 && n.x < -0.99) && left.poses.0 == "agarre");
        assert!(pose.hands[1].as_ref().is_some_and(|r| r.poses.0 == "dedo_fuera"));
    }

    #[test]
    fn it_lags_behind_the_look_and_kicks_when_it_fires_and_comes_back() {
        let d = def();
        let mut h = Holding::default();
        let dt = 1.0 / 120.0;
        // turning to the left at 2 rad/s: it is left behind, to the right
        let mut yaw = 0.0f32;
        let mut pose = ToolPose::default();
        for _ in 0..60 {
            yaw += 2.0 * dt;
            pose = h.update(dt, &d, Quat::from_rotation_y(yaw), Quat::from_rotation_y(yaw), Steps::default(), Vec3::ZERO, false);
        }
        assert!(pose.at.pos.x < -0.3 - 0.03, "it does not lag: {:?}", pose.at.pos);
        for _ in 0..240 {
            pose = h.update(dt, &d, Quat::from_rotation_y(yaw), Quat::from_rotation_y(yaw), Steps::default(), Vec3::ZERO, false);
        }
        assert!((pose.at.pos.x + 0.3).abs() < 0.003);
        // fired: back about as far as its data says, nose up, and home again
        h.fired(&d);
        let (mut back, mut up) = (0.0f32, 0.0f32);
        for _ in 0..240 {
            pose = h.update(dt, &d, Quat::from_rotation_y(yaw), Quat::from_rotation_y(yaw), Steps::default(), Vec3::ZERO, false);
            back = back.max(0.1 - pose.at.pos.z);
            up = up.max((pose.at.rot * Vec3::Z).y);
        }
        assert!((back - 0.06).abs() < 0.012, "kicked back {back:.3} m");
        assert!((up.asin().to_degrees() - 5.0).abs() < 1.0, "nose up {:.1} degrees", up.asin().to_degrees());
        assert!((pose.at.pos - Vec3::new(-0.3, -0.1, 0.1)).length() < 0.004);
    }

    #[test]
    fn hands_that_do_not_reach_it_bring_it_in_and_it_never_runs_from_them() {
        // (it used to go the other way: out by as much as the hands were short, which left them
        // shorter still — on a ramp, where the body is lower under the eyes, it flew off)
        let d = def();
        for fps in [30.0f32, 60.0, 144.0] {
            let mut h = Holding::default();
            // the hands reach no farther ahead than 0.02 m: it is held at 0.1
            let reach = 0.02;
            let mut short = Vec3::ZERO;
            let (mut farthest, mut z) = (f32::MIN, 0.0);
            for _ in 0..(3.0 * fps) as usize {
                let pose = h.update(1.0 / fps, &d, Quat::IDENTITY, Quat::IDENTITY, Steps::default(), short, false);
                z = pose.at.pos.z;
                farthest = farthest.max(z);
                // (from where a hand got to, to where it was asked)
                short = Vec3::new(0.0, 0.0, (z - reach).max(0.0));
            }
            assert!(farthest < 0.1 + 1e-3, "a {fps} fps se aleja hasta {farthest:.3} m");
            assert!(z - reach < 0.012, "a {fps} fps queda a {:.3} m de las manos", z - reach);
            // the hands reach again: back out to its place
            for _ in 0..(4.0 * fps) as usize {
                z = h.update(1.0 / fps, &d, Quat::IDENTITY, Quat::IDENTITY, Steps::default(), Vec3::ZERO, false).at.pos.z;
            }
            assert!((z - 0.1).abs() < 0.004, "a {fps} fps no vuelve a su sitio: {z:.3}");
        }
    }

    #[test]
    fn it_has_weight_a_change_of_speed_leaves_it_behind_and_it_swings_back_past_its_place() {
        let d = def();
        let mut h = Holding::default();
        let dt = 1.0 / 120.0;
        for _ in 0..60 {
            h.update(dt, &d, Quat::IDENTITY, Quat::IDENTITY, Steps::default(), Vec3::ZERO, false);
        }
        // a jump: 2 m/s up at once. It is left under its place, then comes up past it, and settles
        let up = Steps { vel: Vec3::new(0.0, 2.0, 0.0), ..Steps::default() };
        let (mut low, mut high) = (0.0f32, 0.0f32);
        let mut y = 0.0;
        for _ in 0..360 {
            y = h.update(dt, &d, Quat::IDENTITY, Quat::IDENTITY, up, Vec3::ZERO, false).at.pos.y + 0.1;
            (low, high) = (low.min(y), high.max(y));
        }
        assert!(low < -0.01 && low > -0.08, "se queda {low:.3} m por debajo");
        assert!(high > 0.002, "no vuelve pasándose: {high:.4}");
        assert!(y.abs() < 0.004, "no se asienta: {y:.4}");
        // the same at any speed held: only the change is felt (aboard a ship at 2 km/s, turning
        // the look does nothing to it)
        let fast = Steps { vel: Vec3::new(0.0, 2.0, 2000.0), ..Steps::default() };
        let mut h = Holding::default();
        for _ in 0..120 {
            h.update(dt, &d, Quat::IDENTITY, Quat::IDENTITY, fast, Vec3::ZERO, false);
        }
        let mut most = 0.0f32;
        for k in 0..240 {
            let look = Quat::from_rotation_y(k as f32 * 0.002);
            let pose = h.update(dt, &d, look, look, fast, Vec3::ZERO, false);
            most = most.max((pose.at.pos - Vec3::new(-0.3, -0.1, 0.1)).length());
        }
        assert!(most < 0.03, "girar la vista a 2 km/s lo mueve {most:.3} m");
        // with none of that asked of it (`rebote` 1, `arrastre` 0) it does neither
        let stiff = HoldDef { rebote: 1.0, arrastre: 0.0, ..d };
        let mut h = Holding::default();
        h.update(dt, &stiff, Quat::IDENTITY, Quat::IDENTITY, Steps::default(), Vec3::ZERO, false);
        let y = h.update(dt, &stiff, Quat::IDENTITY, Quat::IDENTITY, up, Vec3::ZERO, false).at.pos.y + 0.1;
        assert!(y.abs() < 0.003);
    }

    #[test]
    fn what_rests_on_a_shoulder_turns_about_it_and_goes_with_it() {
        let pivot = Vec3::new(-0.25, -0.17, -0.15);
        let d = HoldDef { pivote: pivot.into(), apoyo: Some(RestDef { punto: "hombro".into(), peso: 1.0 }), ..def() };
        let run = |h: &mut Holding, look: Quat, steps: Steps| {
            let mut pose = ToolPose::default();
            for _ in 0..240 {
                pose = h.update(1.0 / 120.0, &d, look, Quat::IDENTITY, steps, Vec3::ZERO, false);
            }
            pose
        };
        // looking level it is where it is held; the point of it that is on the pivot then...
        let level = run(&mut Holding::default(), Quat::IDENTITY, Steps::default());
        let on = level.at.inverse().point(pivot);
        // ... stays there as the look goes up and down (the world's frame: the look's turn times
        // where the tool is in the eye's)
        for pitch in [-50.0f32, -20.0, 30.0, 60.0] {
            let look = Quat::from_rotation_x(-pitch.to_radians());
            let pose = run(&mut Holding::default(), look, Steps::default());
            let at = look * pose.at.point(on);
            assert!((at - pivot).length() < 0.006, "mirando {pitch}°: el hombro del arma en {at:?}");
            // and it points where one looks
            assert!((look * (pose.at.rot * Vec3::Z)).dot(look * Vec3::Z) > 0.999);
        }
        // the shoulder 5 cm lower under the eyes (a stride, a ramp): so is it
        let low = run(&mut Holding::default(), Quat::IDENTITY, Steps { carry: Vec3::new(0.0, -0.05, 0.0), ..Steps::default() });
        assert!(((low.at.pos.y - level.at.pos.y) + 0.05).abs() < 0.004, "{:.3}", low.at.pos.y - level.at.pos.y);
        // what nothing of the body carries stays in its place in the picture
        let free = HoldDef { apoyo: None, ..d.clone() };
        let mut h = Holding::default();
        let mut pose = ToolPose::default();
        for _ in 0..240 {
            pose = h.update(1.0 / 120.0, &free, Quat::IDENTITY, Quat::IDENTITY, Steps { carry: Vec3::new(0.0, -0.05, 0.0), ..Steps::default() }, Vec3::ZERO, false);
        }
        assert!((pose.at.pos.y - level.at.pos.y).abs() < 0.004);
    }

    #[test]
    fn a_clips_blow_makes_it_jump_and_its_weight_follows_what_the_clip_asks() {
        let mut d = def();
        let clip: ToolClip = serde_json::from_str(r#"{ "dura": 1.0, "canales": { "herramienta.en": [[0, 0, 0, 0], [0.2, 0, -0.2, 0], [1.0, 0, -0.2, 0]] }, "eventos": [[0.6, "golpe:0,-0.4,0,-40"]] }"#).unwrap();
        d.clips.insert("baja".into(), clip);
        d.check().unwrap();
        let mut h = Holding::default();
        assert!(h.play(&d, "baja"));
        let dt = 1.0 / 120.0;
        let (mut t, mut lowest_before, mut lowest_after, mut nose) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
        while h.playing.is_some() {
            let pose = h.update(dt, &d, Quat::IDENTITY, Quat::IDENTITY, Steps::default(), Vec3::ZERO, false);
            t += dt;
            let y = pose.at.pos.y + 0.1;
            if t < 0.58 {
                lowest_before = lowest_before.min(y);
            } else {
                lowest_after = lowest_after.min(y);
                nose = nose.min((pose.at.rot * Vec3::Z).y);
            }
        }
        // brought down 0.2 m in a fifth of a second it goes a little past and comes back...
        assert!(lowest_before < -0.203 && lowest_before > -0.25, "baja hasta {lowest_before:.3}");
        // ... and the blow knocks it lower and its nose down
        assert!(lowest_after < -0.207, "el golpe no lo mueve: {lowest_after:.3}");
        assert!(nose < -0.01, "el golpe no le baja el morro: {nose:.3}");
    }

    #[test]
    fn a_clip_moves_its_pieces_takes_a_hand_away_and_gives_it_something_to_carry() {
        let d = def();
        let mut h = Holding::default();
        assert!(!h.play(&d, "nada") && h.play(&d, "recarga"));
        let dt = 1.0 / 60.0;
        let mut seen = Vec::new();
        let mut t = 0.0;
        let mut loaded = None;
        while h.playing.is_some() {
            let pose = h.update(dt, &d, Quat::IDENTITY, Quat::IDENTITY, Steps::default(), Vec3::ZERO, false);
            t += dt;
            if h.events.iter().any(|e| e == "cargado") {
                loaded = Some(t);
            }
            seen.push((t, pose));
            assert!(t < 3.0, "the clip never ends");
        }
        let at = |when: f32| &seen.iter().find(|s| s.0 >= when).unwrap().1;
        // the hatch opens and shuts (about z)
        assert!(at(1.0).pieces[1].0.rot.angle_between(Quat::from_rotation_z(85f32.to_radians())) < 1e-3);
        assert!(at(1.95).pieces[1].0.rot.angle_between(Quat::IDENTITY) < 1e-3);
        // the rocket shows once the hand has it, carried by the left hand until it is seated
        assert!(!at(0.5).pieces[0].1 && at(1.0).pieces[0].1 && at(1.9).pieces[0].1);
        assert!(at(0.5).carried[0].is_none() && at(1.0).carried[0].is_some_and(|c| c.0 == 0) && at(1.5).carried[0].is_none());
        // the left hand: off its grip to the body's pouch and back by the chamber; the right
        // stays where it was
        let l = at(0.5).hands[0].clone().unwrap();
        assert!(matches!(l.from, Place::Tool(..)) && l.to == Place::Body("bolsa_izq".into()) && l.k > 0.3 && l.k < 0.7 && l.poses == ("agarre".to_string(), "coger".to_string()));
        assert!(matches!(at(1.95).hands[0].clone().unwrap().to, Place::Tool(p, ..) if (p.z - 0.33).abs() < 1e-6));
        assert!(matches!(at(1.0).hands[1].clone().unwrap().from, Place::Tool(p, ..) if (p.z - 0.1).abs() < 1e-6));
        // the tool comes down to be loaded and goes back
        assert!((at(1.0).at.pos - Vec3::new(-0.2, -0.2, 0.2)).length() < 0.003);
        assert!(loaded.is_some_and(|t| (t - 1.8).abs() < 0.03), "{loaded:?}");
        assert!((t - 2.0).abs() < 0.03);
    }
}
