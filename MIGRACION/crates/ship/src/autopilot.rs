//! The autopilot: what flies the ship when the pilot asks it to (`docs/COMBATE.md`).
//!
//! Three things a panel sets, from the least to the most that it takes over:
//! - **holds**, that stack (`ap.altura`, `ap.rumbo`, `ap.velocidad`, each a switch and a wheel):
//!   a height over the ground, a compass heading, a speed. Any of them on and the computer flies
//!   the engines: what is not held is left as it goes, and the translation keys still push;
//! - a **program** (`ap.nav`, a selector: `NAV`): stop, follow the track chosen, take off, land,
//!   or just point somewhere. A program uses the holds that make sense with it (FRENAR and SEGUIR
//!   at the height held);
//! - a **combat mode** (`ap.combate`, a selector: `COMBAT`), which wins over the other two.
//!
//! A mode is an entry in `NAV` or `COMBAT`: a name and a function from what the ship knows now
//! (`Sight`) to what it wants of the flight computer (`Demand`: a turn rate, and what to gain
//! over the ground). The flight computer does the rest (`flight.rs`): it shares the push between
//! the engines — swinging them itself where they swing — and the thrusters, so a mode works on any
//! ship, on any body and with none.
//!
//! Everything is in the ship's own frame: nothing here knows where the ship is.
//!
//! How it turns: where something weighs, by the horizon — across first, then up and down, its
//! top kept up — so that it never ends on its back; where nothing weighs, about its own axes
//! and without rolling.
//!
//! The pilot's stick always wins: while it is off its centre the mode's turn is not asked.
use crate::{
    flight::Flight,
    ship::{Body, MachineRt},
    tactical::Tactical,
};
use glam::{Quat, Vec3};
use lunar_core::structure::state::Structure;
use lunar_signals::{SignalId, Store, Writer};

/// What a mode has to fly by (ship frame).
#[derive(Clone, Copy, Debug)]
pub struct Sight {
    /// Ship to world.
    pub rot: Quat,
    pub spin: Vec3,
    /// Its velocity over the body under it, and that body's pull (exactly zero past every body).
    pub vel: Vec3,
    pub gravity: Vec3,
    /// The way its engines push as they stand (zero: it has none), what they give at full
    /// throttle (m/s²), and whether the computer swings them itself (`Flight::vectoring`).
    pub along: Vec3,
    pub accel: f32,
    pub vectoring: bool,
    /// How fast it may turn (rad/s per axis).
    pub rates: Vec3,
    /// Height of its lowest point over the ground (m; huge where there is none), the compass
    /// heading of its nose (rad from north toward east; none where there is no north), and
    /// whether it stands on the ground.
    pub altitude: f32,
    pub heading: Option<f32>,
    pub grounded: bool,
    /// The translation keys as they are held (−1..1).
    pub keys: Vec3,
    /// What is held: a height (m), a heading (rad), a speed (m/s).
    pub hold_height: Option<f32>,
    pub hold_heading: Option<f32>,
    pub hold_speed: Option<f32>,
    /// The track chosen: where it is and how it moves (relative to the ship).
    pub target: Option<(Vec3, Vec3)>,
    /// Where to point to hit it, and the way to the worst threat.
    pub lead: Option<Vec3>,
    pub threat: Option<Vec3>,
    /// The range to keep (m).
    pub standoff: f32,
    pub t: f64,
}

/// What a mode wants of the flight computer.
#[derive(Clone, Copy, Debug, Default)]
pub struct Demand {
    /// Turn rates (rad/s, ship frame): none, the turn is the pilot's.
    pub rate: Option<Vec3>,
    /// What to gain over the ground (m/s², ship frame; its weight is the computer's to carry):
    /// none, the engines and the thrusters are the pilot's.
    pub accel: Option<Vec3>,
}

/// What a mode keeps from tick to tick.
#[derive(Clone, Copy, Debug, Default)]
pub struct Memory {
    /// The attitude it was engaged at (ship to world).
    pub held: Option<Quat>,
    /// When it was engaged (ship time).
    pub since: f64,
}

pub struct Mode {
    pub id: &'static str,
    /// As a panel writes it on its selector.
    pub name: &'static str,
    /// It flies on the track chosen: with none it does nothing (`ap.sin_objetivo`).
    pub target: bool,
    pub fly: fn(&Sight, &mut Memory) -> Demand,
}

/// Programs, in the order `ap.nav` counts them (1 is the first; 0 is none: the holds alone).
pub const NAV: &[Mode] = &[
    Mode { id: "actitud", name: "ACTITUD", target: false, fly: attitude },
    Mode { id: "progrado", name: "PROGR.", target: false, fly: prograde },
    Mode { id: "retrogrado", name: "RETRO", target: false, fly: retrograde },
    Mode { id: "frenar", name: "FRENAR", target: false, fly: brake },
    Mode { id: "seguir", name: "SEGUIR", target: true, fly: follow },
    Mode { id: "despegar", name: "DESPEG.", target: false, fly: take_off },
    Mode { id: "aterrizar", name: "ATERRIZ.", target: false, fly: land },
];

/// Combat modes, in the order `ap.combate` counts them.
pub const COMBAT: &[Mode] = &[
    Mode { id: "perseguir", name: "PERSEG.", target: true, fly: pursue },
    Mode { id: "apuntar", name: "APUNTAR", target: true, fly: point },
    Mode { id: "distancia", name: "DISTAN.", target: true, fly: keep_range },
    Mode { id: "evadir", name: "EVADIR", target: false, fly: evade },
    Mode { id: "separar", name: "SEPARAR", target: true, fly: extend },
];

/// How hard it turns toward where it wants to look (rad/s per rad off), and the turning it
/// counts on to stop in time (rad/s²): it slows down as √(2·α·angle) on the way in.
const TURN: f32 = 0.9;
const TURN_ACCEL: f32 = 0.25;
/// How far ahead (s) it reckons its own turning when it aims: a hull answers late, and aiming
/// at where the nose is now it would swing past.
const TURN_LEAD: f32 = 0.5;
/// Seconds in which it means to gain the speed it lacks.
const GAIN: f32 = 1.5;
/// Under this pull (m/s²) nothing weighs: there is no horizon to fly by.
const WEIGHS: f32 = 0.05;
/// How far off its heading (rad) it starts to hold back its pitch: across first.
const ACROSS_FIRST: f32 = 0.5;
/// Rolled further than this (rad) it rights itself before anything else.
const RIGHT_FIRST: f32 = 0.7;
/// Climb and sink it flies a height by: per metre off (1/s), the most up (m/s), and down: so
/// much plus a share of its height, and never more than the last. And the share of what it has
/// to stop with that it counts on: going up only its weight stops it, going down its engines.
const CLIMB: (f32, f32, f32, f32, f32) = (0.4, 50.0, 2.0, 0.15, 40.0);
const CLIMB_STOP: f32 = 0.5;
/// How fast ATERRIZ. comes down (m/s): at the ground, more per metre over it, and at most.
const LANDING: (f32, f32, f32) = (0.3, 0.15, 3.5);
/// The height DESPEG. climbs to with no height held (m).
const TAKE_OFF: f32 = 50.0;
/// What the keys ask with a hold on: m/s up and down, m/s² across.
const KEYS: (f32, f32) = (8.0, 3.0);
/// A push across its nose of more than this (m/s²) and it turns to take it on its engines.
const TURN_TO_PUSH: f32 = 1.5;
/// The most it closes on a track at (m/s).
const CLOSING: f32 = 300.0;
/// Lacking more than this across the line to a track (m/s) it flies to be with it before it
/// keeps its nose on it: its engines push fore and aft, and a nose held on the track while it
/// slides past would have it circling.
const WITH_IT: f32 = 6.0;

/// Up, where something weighs.
fn up(s: &Sight) -> Option<Vec3> {
    (s.gravity.length() > WEIGHS).then(|| -s.gravity.normalize())
}

/// The rate (rad/s) that takes out `angle` in good time, no faster than `top`.
fn slew(angle: f32, top: f32) -> f32 {
    angle.signum() * (TURN * angle.abs()).min((2.0 * TURN_ACCEL * angle.abs()).sqrt()).min(top.max(0.02))
}

/// The turn that puts the nose on `d` (ship frame, unit). Where something weighs it is flown by
/// the horizon: about the vertical to the bearing, then (once near it) up or down to the
/// elevation, the top kept up all the while; rolled far over, it rights itself first. Where
/// nothing weighs: about its own axes, across and then up, and no roll.
fn aim(d: Vec3, s: &Sight) -> Vec3 {
    let Some(u) = up(s) else {
        let (az, el) = (d.x.atan2(d.z), d.y.clamp(-1.0, 1.0).asin());
        let hold = (1.0 - (az.abs() - ACROSS_FIRST) / ACROSS_FIRST).clamp(0.2, 1.0);
        return Vec3::new(-slew(el + s.spin.x * TURN_LEAD, s.rates.x) * hold, slew(az - s.spin.y * TURN_LEAD, s.rates.y), 0.0);
    };
    // the nose and where it must go, each as a bearing on the horizon and a height over it
    let level = |v: Vec3| (v - u * v.dot(u)).try_normalize();
    let (nose, to) = (level(Vec3::Z), level(d));
    let az = match (nose, to) {
        (Some(n), Some(t)) => n.cross(t).dot(u).atan2(n.dot(t)),
        _ => 0.0,
    };
    let el = d.dot(u).clamp(-1.0, 1.0).asin() - u.z.clamp(-1.0, 1.0).asin();
    // (its nose straight up or down: there is no bank to speak of)
    let bank = if nose.is_some() { u.x.atan2(u.y) } else { 0.0 };
    let upright = (1.0 - (bank.abs() - RIGHT_FIRST) / RIGHT_FIRST).clamp(0.15, 1.0);
    let across_first = (1.0 - (az.abs() - ACROSS_FIRST) / ACROSS_FIRST).clamp(0.2, 1.0);
    // (to its left on the horizon: turning about it puts the nose down)
    let left = nose.map_or(Vec3::X, |n| u.cross(n));
    // (each as it will be in a moment, by how it is turning now)
    let (az, el, bank) = (az - s.spin.dot(u) * TURN_LEAD, el + s.spin.dot(left) * TURN_LEAD, bank + s.spin.z * TURN_LEAD);
    u * (slew(az, s.rates.y) * upright) - left * (slew(el, s.rates.x) * upright * across_first) - Vec3::Z * slew(bank, s.rates.z)
}

/// The turn that takes the ship's axis `from` to `to` (both ship frame, unit) the short way: for
/// a hull that must turn to put its engines where it pushes.
fn turn(from: Vec3, to: Vec3, s: &Sight) -> Vec3 {
    let (cross, dot) = (from.cross(to), from.dot(to).clamp(-1.0, 1.0));
    let angle = dot.acos();
    if angle < 1e-4 {
        return Vec3::ZERO;
    }
    // (straight behind: any way round is as good)
    let axis = cross.try_normalize().unwrap_or_else(|| from.any_orthonormal_vector());
    axis * slew(angle, s.rates.dot(axis.abs()))
}

/// What it can be asked to gain: its height first (it may fall no faster than it weighs, and
/// climbs with what its engines have over its weight), the rest across.
fn within(a: Vec3, s: &Sight) -> Vec3 {
    let cap = s.accel * 0.95;
    let Some(u) = up(s) else { return a.clamp_length_max(cap) };
    let g = s.gravity.length();
    let v = a.dot(u).clamp(-0.9 * g, ((cap - g) * 0.8).max(0.0));
    let room = (cap * cap - (v + g) * (v + g)).max(0.0).sqrt();
    u * v + (a - u * a.dot(u)).clamp_length_max(room)
}

/// Fly: gain `a` over the ground (m/s²) with the nose toward `face` if it is given. A ship that
/// swings its engines keeps its hull level where it weighs (turning, if the push is across its
/// nose, to take it on its engines) and puts its nose along the push where it does not; one that
/// cannot turns its hull to put its engines where it must push.
fn drive(a: Vec3, face: Option<Vec3>, s: &Sight) -> Demand {
    let a = within(a, s);
    let push = a - s.gravity;
    let rate = match (face, up(s)) {
        (Some(d), _) => aim(d, s),
        (None, Some(u)) if s.vectoring => {
            let nose = (Vec3::Z - u * u.z).try_normalize().unwrap_or(Vec3::Z);
            let across = a - u * a.dot(u);
            // (its engines swing fore and aft: either end of the nose will do)
            let to = if across.length() > TURN_TO_PUSH { across.normalize() * across.dot(nose).signum() } else { nose };
            aim(if to == Vec3::ZERO { nose } else { to }, s)
        }
        (None, None) if s.vectoring => match push.try_normalize() {
            Some(d) if push.length() > 0.05 => aim(d, s),
            _ => Vec3::ZERO,
        },
        _ => match push.try_normalize() {
            Some(d) if push.length() > 0.05 && s.along != Vec3::ZERO => turn(s.along, d, s),
            _ => Vec3::ZERO,
        },
    };
    Demand { rate: Some(rate), accel: Some(a) }
}

fn idle() -> Demand {
    Demand::default()
}

/// The climb (m/s) that takes it to `height` over the ground: slower down than up, and slower
/// the lower it is.
fn climb_to(height: f32, s: &Sight) -> f32 {
    let off = height - s.altitude;
    let g = s.gravity.length();
    // (no faster than it can stop in what is left)
    let (up, down) = ((2.0 * CLIMB_STOP * g * off.abs()).sqrt(), (2.0 * CLIMB_STOP * (s.accel - g).max(0.1) * off.abs()).sqrt());
    (off * CLIMB.0).clamp(-(CLIMB.2 + CLIMB.3 * s.altitude.max(0.0)).min(CLIMB.4).min(down), CLIMB.1.min(up))
}

/// Its nose on the horizon, turned to heading `to` (rad) if the ship knows its own.
fn course(to: Option<f32>, u: Vec3, s: &Sight) -> Vec3 {
    let nose = (Vec3::Z - u * u.z).try_normalize().unwrap_or(Vec3::Z);
    match (to, s.heading) {
        // (east of north is to the right: about the vertical, the other way)
        (Some(to), Some(now)) => Quat::from_axis_angle(u, -(to - now + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) + std::f32::consts::PI) * nose,
        _ => nose,
    }
}

/// Over the ground: gain what it lacks of `across` (a velocity on the horizon; none: as it goes,
/// and the keys push) and of `climb` (m/s), its nose toward `face`.
fn fly_level(across: Option<Vec3>, climb: f32, face: Option<Vec3>, u: Vec3, s: &Sight) -> Demand {
    let rise = s.vel.dot(u);
    let level = |v: Vec3| v - u * v.dot(u);
    let a = match across {
        Some(want) => (level(want) - level(s.vel)) / GAIN,
        None => level(s.keys) * KEYS.1,
    };
    drive(a + u * ((climb - rise) / GAIN), face, s)
}

/// The holds alone: a height, a heading, a speed along the nose (or along the heading held).
/// Where nothing weighs only the speed means anything: along the way it goes.
fn holds(s: &Sight, _: &mut Memory) -> Demand {
    let Some(u) = up(s) else {
        let Some(speed) = s.hold_speed else { return idle() };
        let way = s.vel.try_normalize().filter(|_| s.vel.length() > 0.5).unwrap_or(Vec3::Z);
        let lacks = way * speed - s.vel;
        // (there: it coasts, and burns again only once it is a little off)
        return if lacks.length() < 0.5 { Demand { rate: Some(Vec3::ZERO), accel: Some(Vec3::ZERO) } } else { drive(lacks / GAIN, None, s) };
    };
    let to = course(s.hold_heading, u, s);
    let climb = s.hold_height.map_or(s.keys.y * KEYS.0, |h| climb_to(h, s));
    fly_level(s.hold_speed.map(|v| to * v), climb, Some(to), u, s)
}

fn attitude(s: &Sight, m: &mut Memory) -> Demand {
    let held = *m.held.get_or_insert(s.rot);
    // what turns the ship back to it, in the ship's frame
    let (axis, angle) = (s.rot.inverse() * held).to_axis_angle();
    let angle = if angle > std::f32::consts::PI { angle - std::f32::consts::TAU } else { angle };
    Demand { rate: Some(axis * (TURN * angle).clamp(-0.5, 0.5)), ..idle() }
}

fn prograde(s: &Sight, _: &mut Memory) -> Demand {
    match s.vel.try_normalize() {
        Some(v) if s.vel.length() > 0.3 => Demand { rate: Some(aim(v, s)), ..idle() },
        _ => Demand { rate: Some(Vec3::ZERO), ..idle() },
    }
}

fn retrograde(s: &Sight, _: &mut Memory) -> Demand {
    match s.vel.try_normalize() {
        Some(v) if s.vel.length() > 0.3 => Demand { rate: Some(aim(-v, s)), ..idle() },
        _ => Demand { rate: Some(Vec3::ZERO), ..idle() },
    }
}

/// Stops over the body under it and stays there (at the height held, if one is; else where it
/// stops). Where nothing weighs: stops, and then lets go.
fn brake(s: &Sight, _: &mut Memory) -> Demand {
    match up(s) {
        Some(u) => fly_level(Some(Vec3::ZERO), s.hold_height.map_or(0.0, |h| climb_to(h, s)), None, u, s),
        None if s.vel.length() < 0.3 => Demand { rate: Some(Vec3::ZERO), accel: Some(Vec3::ZERO) },
        None => drive(-s.vel / GAIN, None, s),
    }
}

/// How fast to close on something `far` metres past the range kept: as fast as it can still
/// brake from, and backing off gently from inside it.
fn closing(far: f32, s: &Sight) -> f32 {
    if far >= 0.0 { (2.0 * s.accel * 0.3 * far).sqrt().min(CLOSING) } else { (far / 6.0).max(-40.0) }
}

/// What it lacks to be at the range kept from the track and going as it goes (m/s).
fn lacks(p: Vec3, v: Vec3, s: &Sight) -> Vec3 {
    let d = p.length().max(1.0);
    v + p / d * closing(d - s.standoff, s)
}

/// Goes to the track chosen and stays with it at the range kept, going as it goes. Over the
/// ground it flies level with its nose toward it (at the height held, if one is; else at the
/// track's).
fn follow(s: &Sight, _: &mut Memory) -> Demand {
    let Some((p, v)) = s.target else { return idle() };
    let lacks = lacks(p, v, s);
    match up(s) {
        Some(u) => {
            let to = (p - u * p.dot(u)).try_normalize().filter(|_| p.length() > 30.0);
            let climb = s.hold_height.map_or(s.vel.dot(u) + lacks.dot(u), |h| climb_to(h, s));
            // (its nose on it once it is with it; until then, where it must push)
            let face = if (lacks - u * lacks.dot(u)).length() > WITH_IT { None } else { to.or(Some(course(None, u, s))) };
            fly_level(Some(s.vel + lacks), climb, face, u, s)
        }
        // (with it: its nose on it; else on the way it must push)
        None if lacks.length() < 0.5 => Demand { rate: Some(aim(p.normalize_or(Vec3::Z), s)), accel: Some(Vec3::ZERO) },
        None => drive(lacks / GAIN, None, s),
    }
}

/// Straight up to the height held (`TAKE_OFF` with none), going nowhere, and stays there.
fn take_off(s: &Sight, _: &mut Memory) -> Demand {
    let Some(u) = up(s) else { return idle() };
    fly_level(Some(Vec3::ZERO), climb_to(s.hold_height.unwrap_or(TAKE_OFF), s), Some(course(s.hold_heading, u, s)), u, s)
}

/// Straight down where it is, slower the lower, until it stands; then it lets its engines go.
fn land(s: &Sight, _: &mut Memory) -> Demand {
    let Some(u) = up(s) else { return idle() };
    if s.grounded {
        // (standing: nothing asked of the engines — its weight is the ground's)
        return Demand { rate: None, accel: Some(s.gravity) };
    }
    fly_level(Some(Vec3::ZERO), -(LANDING.0 + LANDING.1 * s.altitude.max(0.0)).min(LANDING.2), Some(course(None, u, s)), u, s)
}

/// Lead pursuit: the nose where the rounds must go, and to the range kept from the track.
fn pursue(s: &Sight, _: &mut Memory) -> Demand {
    let Some((p, v)) = s.target else { return idle() };
    let lacks = lacks(p, v, s);
    // (sliding fast across the line to it, it first flies to be with it: `WITH_IT`)
    let d = p.normalize_or(Vec3::Z);
    let sliding = (lacks - d * lacks.dot(d)).length() > WITH_IT * 4.0;
    drive(lacks / GAIN, (!sliding).then(|| s.lead.unwrap_or(d)), s)
}

/// The nose where to fire and nothing else: the engines are the pilot's.
fn point(s: &Sight, _: &mut Memory) -> Demand {
    let Some((p, _)) = s.target else { return idle() };
    Demand { rate: Some(aim(s.lead.unwrap_or(p.normalize_or(Vec3::Z)), s)), ..idle() }
}

/// The nose on the track itself, and the range kept.
fn keep_range(s: &Sight, _: &mut Memory) -> Demand {
    let Some((p, v)) = s.target else { return idle() };
    drive(lacks(p, v, s) / GAIN, Some(p.normalize_or(Vec3::Z)), s)
}

/// Across the way to whatever threatens it, at full power, the other way every few seconds.
fn evade(s: &Sight, m: &mut Memory) -> Demand {
    let Some(threat) = s.threat.or(s.target.map(|(p, _)| p.normalize_or(Vec3::Z))) else {
        return Demand { rate: Some(Vec3::ZERO), ..idle() };
    };
    let flip = if ((s.t - m.since) / 2.5) as u64 % 2 == 0 { 1.0 } else { -1.0 };
    let side = threat.cross(up(s).unwrap_or(Vec3::Y)).try_normalize().unwrap_or(Vec3::X) * flip;
    let over = threat.cross(side).normalize_or(Vec3::Y);
    drive((side * 0.8 + over * 0.6).normalize_or(side) * s.accel.max(1.0), None, s)
}

fn extend(s: &Sight, _: &mut Memory) -> Demand {
    let Some((p, _)) = s.target else { return idle() };
    drive(-p.normalize_or(Vec3::Z) * s.accel.max(1.0), None, s)
}

/// A hold: its switch and its wheel.
#[derive(Clone, Copy)]
struct Hold {
    on: SignalId,
    set: SignalId,
}

impl Hold {
    fn new(store: &mut Store, name: &str, unit: &str) -> Result<Hold, String> {
        Ok(Hold { on: store.define(&format!("ap.{name}")), set: store.define_unit(&format!("ap.{name}_sel"), unit, 0.0).map_err(|e| e.0)? })
    }

    fn get(&self, store: &Store) -> Option<f32> {
        store.on(self.on).then(|| store.get(self.set) as f32)
    }
}

pub struct Autopilot {
    nav: SignalId,
    combat: SignalId,
    standoff: SignalId,
    limit: Option<SignalId>,
    height: Hold,
    heading: Hold,
    speed: Hold,
    /// What the ship is told of the world: its height over the ground and its compass heading
    /// (degrees; negative where there is no north).
    s_height: SignalId,
    s_heading: SignalId,
    o_on: SignalId,
    o_mode: SignalId,
    o_idle: SignalId,
    /// The mode it flies (0 the holds alone, 1 a program, 2 a combat mode; its index) and what
    /// that keeps.
    flying: Option<(u8, usize)>,
    memory: Memory,
}

impl Autopilot {
    /// The autopilot of a ship of `kind` that flies (none on one that does not).
    pub fn new(kind: &crate::kind::ShipKind, store: &mut Store) -> Result<Option<Autopilot>, String> {
        let Some(f) = &kind.def.vuelo else { return Ok(None) };
        let mut world = |name: &str| -> Result<SignalId, String> {
            let id = store.define(name);
            store.claim(id, Writer::World)?;
            Ok(id)
        };
        let (o_on, o_mode, o_idle) = (world("ap.activo")?, world("ap.modo")?, world("ap.sin_objetivo")?);
        let standoff = store.define_unit("ap.distancia", "m", 0.0).map_err(|e| e.0)?;
        // (the heading is told and set in degrees, as a compass reads)
        let (height, heading, speed) = (Hold::new(store, "altura", "m")?, Hold { on: store.define("ap.rumbo"), set: store.define("ap.rumbo_sel") }, Hold::new(store, "velocidad", "m/s")?);
        Ok(Some(Autopilot {
            nav: store.define("ap.nav"),
            combat: store.define("ap.combate"),
            standoff,
            limit: f.limite.as_ref().map(|n| store.define(n)),
            height,
            heading,
            speed,
            s_height: store.define("nave.altura"),
            s_heading: store.define("nave.rumbo"),
            o_on,
            o_mode,
            o_idle,
            flying: None,
            memory: Memory::default(),
        }))
    }

    /// The signals it reads (its selectors, its holds, the range to keep, the limit on its
    /// engines).
    pub fn reads(&self) -> Vec<SignalId> {
        let holds = [self.height, self.heading, self.speed].into_iter().flat_map(|h| [h.on, h.set]);
        [self.nav, self.combat, self.standoff].into_iter().chain(holds).chain(self.limit).collect()
    }

    /// What the mode chosen wants this tick (none: no mode, no computer, or a mode with nothing
    /// to fly on).
    #[allow(clippy::too_many_arguments)]
    pub fn fly(&mut self, store: &mut Store, flight: &Flight, machines: &[MachineRt], s: &Structure, b: &Body, tac: Option<&Tactical>, t: f64) -> Option<Demand> {
        let (nav, combat) = (store.get(self.nav).round() as i64, store.get(self.combat).round() as i64);
        let held = [self.height, self.heading, self.speed].iter().any(|h| store.on(h.on));
        let chosen = if combat >= 1 && (combat as usize) <= COMBAT.len() {
            Some((2, combat as usize - 1))
        } else if nav >= 1 && (nav as usize) <= NAV.len() {
            Some((1, nav as usize - 1))
        } else if held {
            Some((0, 0))
        } else {
            None
        };
        let chosen = chosen.filter(|_| flight.computer_on(store));
        if chosen != self.flying {
            self.flying = chosen;
            self.memory = Memory { held: None, since: t };
        }
        let Some((family, k)) = chosen else {
            if store.get(self.o_on) != 0.0 {
                store.set(self.o_on, 0.0);
                store.set(self.o_mode, 0.0);
                store.set(self.o_idle, 0.0);
            }
            return None;
        };
        const HOLDS: Mode = Mode { id: "retenciones", name: "", target: false, fly: holds };
        let mode = match family {
            2 => &COMBAT[k],
            1 => &NAV[k],
            _ => &HOLDS,
        };
        let target = tac.and_then(Tactical::target).map(|tr| (tr.pos, tr.vel));
        store.set(self.o_on, if family == 2 { 2.0 } else { 1.0 });
        store.set(self.o_mode, if family == 0 { 0.0 } else { (k + 1) as f64 });
        store.set(self.o_idle, if mode.target && target.is_none() { 1.0 } else { 0.0 });
        if mode.target && target.is_none() {
            return None;
        }
        let mass = (b.mass as f32).max(1.0);
        let mut accel = flight.push() / mass;
        // (the pilot's limit on what it may ask of the engines, in g)
        if let Some(l) = self.limit {
            let g = store.get(l) as f32;
            if g > 0.0 {
                accel = accel.min(g * 9.806_65);
            }
        }
        let standoff = store.get(self.standoff) as f32;
        let heading = store.get(self.s_heading) as f32;
        let sight = Sight {
            rot: s.rot,
            spin: b.spin,
            vel: b.vel,
            gravity: b.gravity,
            along: flight.axis(machines, s),
            accel,
            vectoring: flight.vectoring(),
            rates: flight.rates(store),
            altitude: store.get(self.s_height) as f32,
            heading: (heading >= 0.0).then(|| heading.to_radians()),
            grounded: s.grounded,
            keys: flight.keys(store),
            hold_height: self.height.get(store),
            hold_heading: self.heading.get(store).map(f32::to_radians),
            hold_speed: self.speed.get(store),
            target,
            lead: tac.and_then(|x| x.lead),
            threat: tac.and_then(|x| x.threat),
            standoff: if standoff > 0.0 { standoff } else { 800.0 },
            t,
        };
        Some((mode.fly)(&sight, &mut self.memory))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MOON: Vec3 = Vec3::new(0.0, -1.62, 0.0);

    fn sight() -> Sight {
        Sight {
            rot: Quat::IDENTITY,
            spin: Vec3::ZERO,
            vel: Vec3::ZERO,
            gravity: Vec3::ZERO,
            along: Vec3::Z,
            accel: 10.0,
            vectoring: true,
            rates: Vec3::splat(0.6),
            altitude: 100.0,
            heading: Some(0.0),
            grounded: false,
            keys: Vec3::ZERO,
            hold_height: None,
            hold_heading: None,
            hold_speed: None,
            target: None,
            lead: None,
            threat: None,
            standoff: 800.0,
            t: 0.0,
        }
    }

    /// The ship turned by `q` from level over the Moon: what it then sees of its weight.
    fn turned(q: Quat) -> Sight {
        Sight { rot: q, gravity: q.inverse() * MOON, ..sight() }
    }

    #[test]
    fn every_mode_has_its_own_name() {
        for list in [NAV, COMBAT] {
            for (k, m) in list.iter().enumerate() {
                assert!(list[..k].iter().all(|o| o.id != m.id && o.name != m.name), "{} dos veces", m.id);
                assert!(m.name.chars().count() <= 8, "{}: no cabe en un selector", m.name);
            }
        }
    }

    /// Flies `aim` at world direction `to` from attitude `q` by its own rates until it is there,
    /// and says how far over it ever rolled and where it ended.
    fn slewed(mut q: Quat, to: Vec3, weighs: bool) -> (f32, Quat) {
        let mut worst = 0.0f32;
        for _ in 0..3000 {
            let s = if weighs { turned(q) } else { Sight { rot: q, ..sight() } };
            let rate = aim(q.inverse() * to, &s);
            q = (q * Quat::from_scaled_axis(rate * 0.02)).normalize();
            worst = worst.max((q * Vec3::Y).dot(Vec3::Y).clamp(-1.0, 1.0).acos());
        }
        (worst, q)
    }

    #[test]
    fn it_turns_to_a_track_across_and_then_up_and_never_ends_on_its_back() {
        // tracks all round it and over it: level to begin with, it gets its nose on each of them
        // leaning no further than it must look up, and ends with its top up
        for (k, to) in [Vec3::new(1.0, 0.2, 0.3), Vec3::new(-1.0, 0.5, -1.0), Vec3::new(0.0, 0.3, -1.0), Vec3::new(0.3, -0.6, -0.4), Vec3::new(0.0, 0.9, 0.2)].into_iter().enumerate() {
            let to = to.normalize();
            let (worst, q) = slewed(Quat::IDENTITY, to, true);
            let looks_up = to.y.abs().asin();
            assert!((q * Vec3::Z).dot(to) > 0.999, "{k}: no llega: {:?}", q * Vec3::Z);
            assert!(worst < looks_up + 0.12, "{k}: se inclina {:.0}° para mirar {:.0}° arriba", worst.to_degrees(), looks_up.to_degrees());
            assert!((q * Vec3::X).y.abs() < 0.03, "{k}: acaba alabeada: {:?}", q * Vec3::X);
        }
        // left on its back, it rights itself on the way
        let (_, q) = slewed(Quat::from_rotation_z(3.0), Vec3::new(1.0, 0.0, 1.0).normalize(), true);
        assert!((q * Vec3::Y).y > 0.99 && (q * Vec3::Z).dot(Vec3::new(1.0, 0.0, 1.0).normalize()) > 0.999, "{:?}", q * Vec3::Y);
        // where nothing weighs it gets there too, and never rolls about its nose
        for to in [Vec3::new(1.0, 0.2, 0.3), Vec3::new(-0.2, 0.5, -1.0), Vec3::new(0.0, -1.0, 0.1)] {
            let (_, q) = slewed(Quat::IDENTITY, to.normalize(), false);
            assert!((q * Vec3::Z).dot(to.normalize()) > 0.999, "{to:?}: {:?}", q * Vec3::Z);
        }
        let s = sight();
        assert_eq!(aim(Vec3::new(0.4, 0.3, 0.8).normalize(), &s).z, 0.0);
    }

    #[test]
    fn a_lead_to_port_is_a_turn_to_port() {
        // +X is to port: a turn about +Y, weighing or not
        for s in [sight(), turned(Quat::IDENTITY)] {
            let r = aim(Vec3::new(0.2, 0.0, 0.98).normalize(), &s);
            assert!(r.y > 0.05 && r.x.abs() < 1e-3 && r.z.abs() < 1e-3, "{r:?}");
        }
    }

    #[test]
    fn braking_where_it_weighs_ends_holding_still_and_level() {
        let mut s = turned(Quat::IDENTITY);
        s.vel = Vec3::new(0.0, -2.0, 30.0);
        let d = brake(&s, &mut Memory::default());
        // against its speed, and up against its fall
        let a = d.accel.unwrap();
        assert!(a.z < -5.0 && a.y > 1.0, "{a:?}");
        s.vel = Vec3::ZERO;
        let d = brake(&s, &mut Memory::default());
        assert!(d.accel.unwrap().length() < 1e-3 && d.rate.unwrap().length() < 1e-3);
    }

    #[test]
    fn braking_where_nothing_weighs_points_against_the_speed() {
        let mut s = sight();
        s.vel = Vec3::new(0.0, 0.0, 50.0);
        let d = brake(&s, &mut Memory::default());
        // going the way its nose looks: it turns round, asking all the while to lose its speed
        assert!(d.rate.unwrap().length() > 0.1 && d.accel.unwrap().z < -5.0);
        // a hull that cannot swing its engines turns them, wherever they look
        s.vectoring = false;
        s.along = Vec3::Y;
        let d = brake(&s, &mut Memory::default());
        assert!(d.rate.unwrap().x < -0.1, "{:?}", d.rate);
    }

    #[test]
    fn a_height_held_is_climbed_to_and_no_faster_down_than_it_can_stop() {
        let mut s = turned(Quat::IDENTITY);
        s.hold_height = Some(300.0);
        let d = holds(&s, &mut Memory::default());
        assert!(d.accel.unwrap().y > 1.0);
        // there, and going neither up nor down: nothing asked
        s.altitude = 300.0;
        assert!(holds(&s, &mut Memory::default()).accel.unwrap().length() < 1e-3);
        // far above it, low: it comes down gently
        s.altitude = 10.0;
        s.hold_height = Some(0.0);
        assert!(climb_to(0.0, &s) > -4.0);
    }

    #[test]
    fn a_speed_held_goes_along_the_nose_and_a_heading_held_turns_it() {
        let mut s = turned(Quat::IDENTITY);
        s.hold_speed = Some(120.0);
        let d = holds(&s, &mut Memory::default());
        let a = d.accel.unwrap();
        // forward, with what its engines have left over its weight
        assert!(a.z > 5.0 && a.x.abs() < 1e-3 && a.y.abs() < 1e-3, "{a:?}");
        assert!((a - s.gravity).length() <= s.accel * 0.95 + 1e-3);
        // heading north, told to head east: to starboard (−X), about the vertical
        s.hold_heading = Some(std::f32::consts::FRAC_PI_2);
        let d = holds(&s, &mut Memory::default());
        assert!(d.rate.unwrap().y < -0.1, "{:?}", d.rate);
        // with no north to go by it keeps its nose where it is
        s.heading = None;
        assert!(holds(&s, &mut Memory::default()).rate.unwrap().length() < 1e-3);
    }

    #[test]
    fn with_a_hold_on_the_keys_still_push() {
        let mut s = turned(Quat::IDENTITY);
        s.hold_height = Some(100.0);
        s.keys = Vec3::new(0.0, 0.0, 1.0);
        assert!(holds(&s, &mut Memory::default()).accel.unwrap().z > 1.0);
        s.hold_height = None;
        s.hold_heading = Some(0.0);
        s.keys = Vec3::new(0.0, 1.0, 0.0);
        assert!(holds(&s, &mut Memory::default()).accel.unwrap().y > 1.0);
    }

    #[test]
    fn following_closes_on_the_track_and_then_goes_as_it_goes() {
        let mut s = turned(Quat::IDENTITY);
        // 5 km off to port and ahead, going away at 40 m/s
        let p = Vec3::new(3000.0, 0.0, 4000.0);
        s.target = Some((p, p.normalize() * 40.0));
        let d = follow(&s, &mut Memory::default());
        assert!(d.accel.unwrap().dot(p.normalize()) > 3.0, "{:?}", d.accel);
        assert!(d.rate.unwrap().y > 0.1, "el morro hacia ella: {:?}", d.rate);
        // at its range, going as it goes: nothing to gain
        let p = Vec3::new(0.0, 0.0, 800.0);
        s.target = Some((p, Vec3::ZERO));
        assert!(follow(&s, &mut Memory::default()).accel.unwrap().length() < 1e-3);
        // inside it: it backs off
        s.target = Some((Vec3::new(0.0, 0.0, 300.0), Vec3::ZERO));
        assert!(follow(&s, &mut Memory::default()).accel.unwrap().z < -1.0);
    }

    #[test]
    fn landing_comes_down_slower_the_lower_and_lets_go_on_the_ground() {
        let mut s = turned(Quat::IDENTITY);
        let sink = |h: f32, s: &mut Sight| {
            s.altitude = h;
            -land(s, &mut Memory::default()).accel.unwrap().y * GAIN
        };
        let (high, low) = (sink(100.0, &mut s), sink(1.0, &mut s));
        assert!(high > low && low > 0.2 && low < 0.6, "{high} {low}");
        s.grounded = true;
        let d = land(&s, &mut Memory::default());
        assert!(d.rate.is_none() && (d.accel.unwrap() - s.gravity).length() < 1e-6);
    }

    #[test]
    fn pursuit_puts_the_nose_on_the_lead_and_closes_to_its_range() {
        let mut s = sight();
        s.target = Some((Vec3::new(0.0, 0.0, 5000.0), Vec3::ZERO));
        s.lead = Some(Vec3::new(0.2, 0.0, 0.98).normalize());
        let d = pursue(&s, &mut Memory::default());
        assert!(d.rate.unwrap().y > 0.05, "{:?}", d.rate);
        assert!(d.accel.unwrap().z > 9.0);
        // at its range and not closing: nothing to gain
        s.target = Some((Vec3::new(0.0, 0.0, 800.0), Vec3::ZERO));
        assert!(pursue(&s, &mut Memory::default()).accel.unwrap().length() < 1e-3);
    }

    #[test]
    fn a_mode_on_a_track_does_nothing_without_one() {
        let s = sight();
        for m in NAV.iter().chain(COMBAT).filter(|m| m.target) {
            let d = (m.fly)(&s, &mut Memory::default());
            assert!(d.rate.is_none() && d.accel.is_none(), "{}", m.id);
        }
    }

    #[test]
    fn evading_goes_across_the_threat_and_changes_side() {
        let mut s = sight();
        s.threat = Some(Vec3::Z);
        let mut m = Memory::default();
        let a = evade(&s, &mut m).accel.unwrap();
        s.t = 3.0;
        let b = evade(&s, &mut m).accel.unwrap();
        assert!(a.length() > 5.0 && a.dot(Vec3::Z).abs() < 1e-3 && a.x * b.x < 0.0, "{a:?} {b:?}");
    }

    #[test]
    fn nothing_is_asked_that_the_engines_cannot_give_and_its_weight_comes_first() {
        let s = turned(Quat::IDENTITY);
        let a = within(Vec3::new(0.0, 50.0, 500.0), &s);
        assert!((a - s.gravity).length() <= s.accel * 0.95 + 1e-3 && a.y > 1.0 && a.z > 1.0, "{a:?}");
        // it may fall, but not be thrown down
        assert!(within(Vec3::new(0.0, -50.0, 0.0), &s).y >= -1.62 * 0.9 - 1e-3);
    }
}
