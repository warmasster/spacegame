//! Explosions, shots and missiles in play. Each definition may name a key: pressing it fires where
//! the player aims, at whatever is nearer, ground or structure (core::effects runs the explosion,
//! core::rounds flies the shots that have a speed, core::missiles the missiles, the structures take
//! the damage). Every frame the particles, the rounds and missiles in flight (drawn as particles,
//! in the same draw) and the flashes go to the renderer; missiles leave their trail, the camera
//! shakes or follows a missile.
use crate::builds::Builds;
use glam::DVec3;
use lunar_core::{
    body::BodyRegistry,
    detonation::ChargeDef,
    effects::{EffectDefs, Effects, ExplosionDef, ShotDef},
    guided::{Aim, Flight as Guided, Hit},
    missiles::{Missiles, Strike, Target},
    particles::Particle,
    rounds::{self, Impact, Rounds},
};
use lunar_render::{Light, Renderer, View};
use winit::keyboard::KeyCode;

/// How far the aim reaches (m), and for missiles.

const AIM_RANGE: f64 = 3000.0;
const MISSILE_RANGE: f64 = 300_000.0;
/// Rounds in flight at once (a space battle's worth).
const ROUNDS: usize = 20_000;

/// A shot the menu changes in play (`ShotDef::menu`): charge, speed, size, and their limits.
#[derive(Clone, Debug, PartialEq)]
pub struct ShotEdit {
    pub index: usize,
    pub name: String,
    pub key: String,
    /// kg of TNT.
    pub tnt: f32,
    pub speed: f32,
    pub size: f32,
    pub max_tnt: f32,
    pub max_speed: f32,
}

#[derive(Clone, Debug, PartialEq)]
enum Action {
    Explode(String),
    Shoot(usize),
    Launch(usize),
}

/// Something this game fired or set off that the other players' games must show too (`multi`
/// tells them; they show it with `Blasts::show`). What it does to the structures is not here:
/// that is decided once, by whoever simulates each one.
#[derive(Clone, Debug, PartialEq)]
pub enum Seen {
    /// A round of shot `shot` (its place in `shots.jsonc`) leaving `from` along `dir` at `speed` m/s.
    Round { shot: u16, from: DVec3, dir: DVec3, speed: f32 },
    /// A missile of kind `kind` launched from `from` at `to`.
    Missile { kind: u16, from: DVec3, to: DVec3 },
    /// Explosion `id` going off at `at`, `scale` times as big, with `extra` J more.
    Boom { id: String, at: DVec3, scale: f32, extra: f32 },
}

/// The mark, in a round's kind, of one fired in another player's game: it flies and is seen
/// here, and where it lands it does nothing (its shooter's game says what it struck).
const FOREIGN: u16 = 0x8000;

pub struct Blasts {
    pub fx: Effects,
    pub missiles: Missiles,
    /// Trail style, warhead (explosion id) and look in flight of each missile kind.
    trails: Vec<Option<u8>>,
    warheads: Vec<String>,
    missile_looks: Vec<Option<(u8, f32)>>,
    /// Shots in flight, how each shot kind is drawn, where they landed this step, and every round
    /// and missile as a particle for the renderer (all reused).
    pub rounds: Rounds,
    shot_looks: Vec<(u8, f32)>,
    impacts: Vec<Impact>,
    looks: Vec<Particle>,
    strikes: Vec<Strike>,
    /// The camera rides behind the newest missile.
    pub follow: bool,
    shots: Vec<(String, ShotDef)>,
    keys: Vec<(KeyCode, Action)>,
    /// To fire on the next update, and whether at the aim (else on the ground ahead).
    queued: Option<(Action, bool)>,
    /// An automatic shot whose key is held: (key, shot, rounds owed).
    held: Option<(KeyCode, usize, f64)>,
    /// How far ahead `fire_ahead` blasts land (m).
    ahead: f64,
    lights: Vec<Light>,
    time: f64,
    shots_fired: u64,
    /// With other players: what was fired and set off here since it was last taken (`Seen`), kept
    /// only while `tell` is on (whoever takes them sets it).
    pub seen: Vec<Seen>,
    pub tell: bool,
    /// The missiles launched in other players' games: they fly here to be seen, and strike nothing.
    guests: Missiles,
    guest_strikes: Vec<Strike>,
    /// Guided missiles and decoys in flight (`lunar_core::guided`): the explosion, the look and
    /// the trail of each missile kind, the puffs of each decoy kind.
    pub guided: Guided,
    guided_warheads: Vec<String>,
    guided_looks: Vec<Option<(u8, f32)>>,
    guided_trails: Vec<Option<u8>>,
    decoy_looks: Vec<Option<u8>>,
    /// Where what the guided missiles follow is and how it shines, as whoever knows last said
    /// (`tactics`): by id.
    pub aims: Vec<(u64, Aim)>,
    hits: Vec<Hit>,
}

/// A key name of the definitions ("B", "7"...).
fn key_code(name: &str) -> Option<KeyCode> {
    use KeyCode::*;
    const LETTERS: [KeyCode; 26] = [
        KeyA, KeyB, KeyC, KeyD, KeyE, KeyF, KeyG, KeyH, KeyI, KeyJ, KeyK, KeyL, KeyM, KeyN, KeyO, KeyP, KeyQ, KeyR, KeyS, KeyT, KeyU, KeyV, KeyW, KeyX, KeyY,
        KeyZ,
    ];
    const DIGITS: [KeyCode; 10] = [Digit0, Digit1, Digit2, Digit3, Digit4, Digit5, Digit6, Digit7, Digit8, Digit9];
    let c = name.trim().to_ascii_uppercase();
    let mut ch = c.chars();
    match (ch.next(), ch.next()) {
        (Some(l @ 'A'..='Z'), None) => Some(LETTERS[l as usize - 'A' as usize]),
        (Some(d @ '0'..='9'), None) => Some(DIGITS[d as usize - '0' as usize]),
        _ => None,
    }
}

impl Blasts {
    pub fn new(defs: &EffectDefs, missiles: Missiles, capacity: usize) -> Result<Blasts, String> {
        let mut fx = Effects::new(defs, capacity);
        let mut keys = Vec::new();
        let mut trails = Vec::new();
        let mut warheads = Vec::new();
        let mut missile_looks = Vec::new();
        for (i, (id, m)) in missiles.defs.iter().enumerate() {
            let warhead = match (&m.warhead, m.charge) {
                (Some(w), _) => w.clone(),
                (None, Some(c)) => {
                    let w = format!("misil:{id}");
                    fx.add_explosion(&w, ExplosionDef { name: m.name.clone(), charge: Some(c), ..Default::default() })?;
                    w
                }
                (None, None) => return Err(format!("missile {id}: needs a warhead or a charge")),
            };
            if !fx.has(&warhead) {
                return Err(format!("missile {id}: unknown warhead '{warhead}'"));
            }
            warheads.push(warhead);
            missile_looks.push(m.look.as_ref().map(|l| fx.style(&l.style).map(|s| (s, l.size)).ok_or_else(|| format!("missile {id}: unknown style '{}'", l.style))).transpose()?);
            let trail = m.trail.as_deref().map(|t| fx.style(t).ok_or_else(|| format!("missile {id}: unknown trail '{t}'"))).transpose()?;
            trails.push(trail);
            if let Some(k) = &m.key {
                keys.push((key_code(k).ok_or_else(|| format!("{id}: unknown key '{k}'"))?, Action::Launch(i)));
            }
        }
        let bind = |k: &str, what: &str| key_code(k).ok_or_else(|| format!("{what}: unknown key '{k}'"));
        for (id, e) in &defs.explosions {
            if let Some(k) = &e.key {
                keys.push((bind(k, id)?, Action::Explode(id.clone())));
            }
        }
        let mut shot_looks = Vec::new();
        for (i, (id, s)) in defs.shots.iter().enumerate() {
            if let Some(k) = &s.key {
                keys.push((bind(k, id)?, Action::Shoot(i)));
            }
            // validated with the definitions: the style exists
            shot_looks.push(s.look.as_ref().map_or((0, 0.0), |l| (fx.style(&l.style).unwrap_or(0), l.size)));
        }
        // one thing per key, and none the player's own keys
        for (i, (k, _)) in keys.iter().enumerate() {
            if crate::input::taken(*k) {
                return Err(format!("la tecla {k:?} ya es del jugador (mover, agacharse, linterna...)"));
            }
            if keys[..i].iter().any(|(o, _)| o == k) {
                return Err(format!("la tecla {k:?} está asignada dos veces (disparos, misiles, explosiones)"));
            }
        }
        let guests = Missiles::new(missiles.defs.clone());
        Ok(Blasts {
            fx,
            missiles,
            seen: Vec::new(),
            tell: false,
            guests,
            guest_strikes: Vec::new(),
            guided: Guided::new(Vec::new(), Vec::new()),
            guided_warheads: Vec::new(),
            guided_looks: Vec::new(),
            guided_trails: Vec::new(),
            decoy_looks: Vec::new(),
            aims: Vec::new(),
            hits: Vec::new(),
            trails,
            warheads,
            missile_looks,
            rounds: Rounds::new(ROUNDS),
            shot_looks,
            impacts: Vec::with_capacity(256),
            looks: Vec::with_capacity(ROUNDS),
            strikes: Vec::new(),
            follow: false,
            shots: defs.shots.clone(),
            keys,
            queued: None,
            held: None,
            ahead: 20.0,
            lights: Vec::with_capacity(lunar_core::effects::MAX_FLASHES),
            time: 0.0,
            shots_fired: 0,
        })
    }

    /// The guided missiles and the decoys there are (`guiados.jsonc`, `senuelos.jsonc`): each
    /// missile's explosion is made from its charge, its look and its trail are particle styles.
    pub fn set_guided(&mut self, flight: Guided) -> Result<(), String> {
        let style = |fx: &Effects, id: &str, s: &str| fx.style(s).ok_or_else(|| format!("{id}: estilo de partícula desconocido '{s}'"));
        (self.guided_warheads, self.guided_looks, self.guided_trails, self.decoy_looks) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        for (id, d) in &flight.defs {
            let w = format!("guiado:{id}");
            if !self.fx.has(&w) {
                self.fx.add_explosion(&w, ExplosionDef { name: d.name.clone(), charge: Some(d.charge), ..Default::default() })?;
            }
            self.guided_warheads.push(w);
            self.guided_looks.push(d.look.as_ref().map(|l| style(&self.fx, id, &l.style).map(|s| (s, l.size))).transpose()?);
            self.guided_trails.push(d.trail.as_deref().map(|t| style(&self.fx, id, t)).transpose()?);
        }
        for (id, d) in &flight.decoy_defs {
            self.decoy_looks.push(d.look.as_deref().map(|l| style(&self.fx, id, l)).transpose()?);
        }
        self.guided = flight;
        Ok(())
    }

    /// The speed shot `id` leaves at (m/s) and how far it reaches (m): what a gun's sight is
    /// worked out with.
    pub fn shot_speed(&self, id: &str) -> Option<(f32, f32)> {
        self.shots.iter().find(|(s, _)| s == id).and_then(|(_, s)| s.speed.map(|v| (v, s.range)))
    }

    /// A round of shot `id` fired from something that moves: leaving `from` along `dir`, with
    /// the speed `vel` of what fired it besides its own. False if there is no such shot (or it
    /// is one that strikes at once: a mounted gun fires rounds that fly).
    pub fn fire_round(&mut self, id: &str, from: DVec3, dir: DVec3, vel: DVec3, bodies: &BodyRegistry) -> bool {
        let Some(i) = self.shots.iter().position(|(s, _)| s == id) else { return false };
        let s = &self.shots[i].1;
        let Some(speed) = s.speed else { return false };
        let (range, spread) = (s.range, s.spread);
        self.shots_fired += 1;
        let up = dir.any_orthonormal_vector();
        let dir = self.scatter(&View { eye: from, forward: dir, up, fov_y: 1.0, near: 0.1 }, spread);
        let (style, size) = self.shot_looks[i];
        let seed = (self.shots_fired % 997) as f32 / 997.0;
        let mut r = rounds::round(i as u16, from, dir, speed, range, bodies.dominant(from), style, size, seed);
        r.vel += vel;
        if !self.rounds.fire(r) {
            return false;
        }
        self.told(Seen::Round { shot: i as u16, from, dir: r.vel.normalize_or(dir), speed: r.vel.length() as f32 });
        true
    }

    /// A key went down: true when it fires something (on the next update); an automatic shot
    /// keeps firing until the key goes up.
    pub fn key(&mut self, key: KeyCode) -> bool {
        match self.keys.iter().find(|(k, _)| *k == key) {
            Some((_, a)) => {
                if let Action::Shoot(i) = a
                    && self.shots[*i].1.rate.is_some()
                {
                    // the first round at once
                    self.held = Some((key, *i, 1.0));
                } else {
                    self.queued = Some((a.clone(), true));
                }
                true
            }
            None => false,
        }
    }

    /// Every key up (the window lost the focus).
    pub fn release_all(&mut self) {
        self.held = None;
    }

    /// A key went up.
    pub fn release(&mut self, key: KeyCode) {
        if self.held.is_some_and(|(k, _, _)| k == key) {
            self.held = None;
        }
    }

    /// Fire shot `id` along the view on the next update (scripts). False if there is none.
    pub fn shoot_named(&mut self, id: &str) -> bool {
        match self.shots.iter().position(|(s, _)| s == id) {
            Some(i) => {
                self.queued = Some((Action::Shoot(i), true));
                true
            }
            None => false,
        }
    }

    /// Shot `id` fired now from `from` along `dir` (anyone's, not the camera's). False if there is
    /// no such shot.
    pub fn fire_from(&mut self, id: &str, from: DVec3, dir: DVec3, bodies: &BodyRegistry, builds: &mut Builds) -> bool {
        let Some(i) = self.shots.iter().position(|(s, _)| s == id) else { return false };
        let up = dir.any_orthonormal_vector();
        self.fire(Action::Shoot(i), true, bodies, &View { eye: from, forward: dir, up, fov_y: 1.0, near: 0.1 }, builds);
        true
    }

    /// The shots the menu may change, as they are now.
    pub fn editable(&self) -> Vec<ShotEdit> {
        let mut out = Vec::new();
        for (i, (_, s)) in self.shots.iter().enumerate() {
            let (Some(m), Some(c), Some(speed)) = (s.menu, s.charge, s.speed) else { continue };
            let key = s.key.clone().unwrap_or_default();
            out.push(ShotEdit { index: i, name: s.name.clone(), key, tnt: c.tnt, speed, size: self.shot_looks[i].1, max_tnt: m.max_tnt, max_speed: m.max_speed });
        }
        out
    }

    /// Change a shot from the menu: its next rounds fly and blow up with the new values (its blast
    /// is made again from the new charge).
    pub fn set_shot(&mut self, e: &ShotEdit) {
        let Some((_, s)) = self.shots.get_mut(e.index) else { return };
        let Some(m) = s.menu else { return };
        let c = ChargeDef { tnt: e.tnt.clamp(1e-3, m.max_tnt), casing: s.charge.map_or(0.0, |c| c.casing) };
        s.charge = Some(c);
        s.speed = Some(e.speed.clamp(1.0, m.max_speed));
        self.shot_looks[e.index].1 = e.size.max(0.01);
        // only the blast made for this shot (never a shared, written one)
        if let Some(fx) = s.impact.as_ref().filter(|id| id.starts_with("disparo:")) {
            let def = ExplosionDef { name: s.name.clone(), charge: Some(c), ..Default::default() };
            if let Err(err) = self.fx.add_explosion(fx, def) {
                eprintln!("{err}");
            }
        }
    }

    /// Blow up `id` on the ground `metres` ahead of the camera (command line, demos).
    pub fn fire_ahead(&mut self, id: &str, metres: f64) {
        self.queued = Some((Action::Explode(id.to_string()), false));
        self.ahead = metres;
    }

    /// The keys and what they fire, for the help line.
    pub fn help(&self) -> String {
        let name = |a: &Action| match a {
            Action::Explode(id) => id.clone(),
            Action::Shoot(i) => self.shots[*i].0.clone(),
            Action::Launch(i) => self.missiles.defs[*i].0.clone(),
        };
        self.keys.iter().map(|(k, a)| format!("{}: {}", format!("{k:?}").trim_start_matches("Key"), name(a))).collect::<Vec<_>>().join("  ")
    }

    /// Where the aim lands first, ground or structure, within `max` m.
    fn aim(bodies: &BodyRegistry, builds: &Builds, from: DVec3, dir: DVec3, max: f64) -> Option<DVec3> {
        let ground = bodies.raycast(from, dir, max).map(|(_, p)| p);
        let reach = ground.map_or(max, |p| p.distance(from));
        builds.set.raycast(from, dir, reach).map(|(_, _, p)| p).or(ground)
    }

    fn fire(&mut self, action: Action, aim: bool, bodies: &BodyRegistry, view: &View, builds: &mut Builds) {
        match action {
            Action::Explode(id) => {
                let at = if aim {
                    Blasts::aim(bodies, builds, view.eye, view.forward, AIM_RANGE)
                } else {
                    let b = bodies.get(bodies.dominant(view.eye));
                    let up = b.up(view.eye);
                    let ahead = (view.forward - up * view.forward.dot(up)).normalize_or(view.forward);
                    // straight down to the ground there, however high the camera is
                    let probe = b.altitude(view.eye).max(0.0) + 50.0 + AIM_RANGE;
                    bodies.raycast(view.eye + ahead * self.ahead + up * 50.0, -up, probe).map(|(_, p)| p)
                };
                let Some(at) = at else { return };
                // pulled back a little toward the shooter: the blast goes off at the surface, not in it
                let at = at - view.forward * 0.3;
                match self.fx.explode(&id, bodies, bodies.dominant(at), at) {
                    Ok(Some(d)) => builds.blast(at, d),
                    Ok(None) => {}
                    Err(e) => {
                        eprintln!("{e}");
                        return;
                    }
                }
                self.told(Seen::Boom { id, at, scale: 1.0, extra: 0.0 });
            }
            Action::Launch(i) => {
                let Some(to) = Blasts::aim(bodies, builds, view.eye, view.forward, MISSILE_RANGE) else { return };
                let id = self.missiles.defs[i].0.clone();
                let from = view.eye + view.forward * 2.0;
                match self.missiles.launch(&id, from, to, bodies) {
                    Ok(_) => self.told(Seen::Missile { kind: i as u16, from, to }),
                    Err(e) => eprintln!("{e}"),
                }
            }
            Action::Shoot(i) => {
                let s = self.shots[i].1.clone();
                self.shots_fired += 1;
                let dir = self.scatter(view, s.spread);
                if let Some(speed) = s.speed {
                    let from = view.eye + view.forward * 1.0;
                    let (style, size) = self.shot_looks[i];
                    let seed = (self.shots_fired % 997) as f32 / 997.0;
                    if self.rounds.fire(rounds::round(i as u16, from, dir, speed, s.range, bodies.dominant(from), style, size, seed)) {
                        self.told(Seen::Round { shot: i as u16, from, dir, speed });
                    }
                    return;
                }
                let range = f64::from(s.range);
                let ground = bodies.raycast(view.eye, view.forward, range).map(|(_, p)| p);
                let reach = ground.map_or(range, |p| p.distance(view.eye));
                let at = builds.shoot(view.eye, view.forward, &s, reach, self.shots_fired).or(ground);
                if let (Some(at), Some(fx)) = (at, &s.impact) {
                    let at = at - view.forward * 0.1;
                    if self.fx.explode(fx, bodies, bodies.dominant(at), at).is_ok() {
                        self.told(Seen::Boom { id: fx.clone(), at, scale: 1.0, extra: 0.0 });
                    }
                }
            }
        }
    }

    /// Something fired or set off here, kept for the other players' games while there are any.
    fn told(&mut self, what: Seen) {
        if self.tell {
            self.seen.push(what);
        }
    }

    /// Something fired or set off in another player's game `age` s ago, shown here: a round
    /// flies on from where it is by now, a missile is launched, an explosion goes off. None of
    /// it does anything to the structures here.
    pub fn show(&mut self, what: &Seen, age: f32, bodies: &BodyRegistry) {
        match what {
            Seen::Round { shot, from, dir, speed } => {
                let Some((_, s)) = self.shots.get(usize::from(*shot)) else { return };
                let (style, size) = self.shot_looks[usize::from(*shot)];
                // (as far along as it has flown while the word of it came)
                let flown = f64::from(*speed) * f64::from(age.clamp(0.0, 0.25));
                let from = *from + *dir * flown;
                self.rounds.fire(rounds::round(*shot | FOREIGN, from, *dir, *speed, (s.range - flown as f32).max(1.0), bodies.dominant(from), style, size, 0.5));
            }
            Seen::Missile { kind, from, to } => {
                if let Some((id, _)) = self.guests.defs.get(usize::from(*kind)).cloned() {
                    let _ = self.guests.launch(&id, *from, *to, bodies);
                }
            }
            Seen::Boom { id, at, scale, extra } => {
                let _ = self.fx.explode_with(id, bodies, bodies.dominant(*at), *at, *scale, *extra);
            }
        }
    }

    /// The aim scattered by `spread` mrad (a cheap gaussian from the shot count).
    fn scatter(&self, view: &View, spread: f32) -> DVec3 {
        if spread <= 0.0 {
            return view.forward;
        }
        let h = |k: u64| {
            let x = (self.shots_fired.wrapping_mul(0x9E37_79B9_7F4A_7C15).wrapping_add(k.wrapping_mul(0xBF58_476D_1CE4_E5B9)) >> 11) as f64;
            x / (1u64 << 53) as f64
        };
        let (u1, u2) = (h(1).max(1e-9), h(2));
        let r = (-2.0 * u1.ln()).sqrt() * f64::from(spread) * 1e-3;
        let a = u2 * std::f64::consts::TAU;
        let right = view.forward.cross(view.up).normalize_or(DVec3::X);
        let up = right.cross(view.forward);
        (view.forward + right * (r * a.cos()) + up * (r * a.sin())).normalize()
    }

    /// Rounds fly; where one lands it cuts into the structure it struck and sets off its impact.
    fn fly_rounds(&mut self, dt: f64, bodies: &BodyRegistry, builds: &mut Builds) {
        let set = &builds.set;
        self.rounds.step(dt as f32, bodies, |from, dir, len| set.raycast(from, dir, len).map(|(_, _, p)| p), &mut self.impacts);
        for k in 0..self.impacts.len() {
            let i = self.impacts[k];
            // (fired in another player's game: theirs says what it struck)
            if i.kind & FOREIGN != 0 {
                continue;
            }
            let s = &self.shots[usize::from(i.kind)].1;
            self.shots_fired += 1;
            if i.structure {
                // the cut goes on through the structure from just in front of where it struck
                let from = i.at - i.dir * 0.5;
                let _ = builds.shoot(from, i.dir, s, 2.0, self.shots_fired);
            }
            if let Some(fx) = &s.impact {
                let at = i.at - i.dir * 0.1;
                let blast = self.fx.explode(fx, bodies, bodies.dominant(at), at);
                if self.tell && blast.is_ok() {
                    self.seen.push(Seen::Boom { id: fx.clone(), at, scale: 1.0, extra: 0.0 });
                }
                if let Ok(Some(d)) = blast {
                    builds.blast(at, d);
                }
            }
        }
        self.impacts.clear();
    }

    /// Missiles fly; their trails are left behind; strikes blow their warhead (plus their speed).
    fn fly(&mut self, dt: f64, bodies: &BodyRegistry, builds: &mut Builds) {
        self.missiles.update(dt, bodies, &mut builds.set, &mut self.strikes);
        for s in std::mem::take(&mut self.strikes) {
            let at = s.at - s.vel.normalize_or_zero() * 0.5;
            // the impact's energy: a bigger blast for a charge, a harder hit for anything else
            let extra = (s.energy * 0.5) as f32;
            let blast = self.fx.explode_with(&self.warheads[s.kind], bodies, bodies.dominant(at), at, 1.0, extra);
            if self.tell && blast.is_ok() {
                self.seen.push(Seen::Boom { id: self.warheads[s.kind].clone(), at, scale: 1.0, extra });
            }
            if let Ok(Some(d)) = blast {
                builds.blast(at, d);
            }
        }
        // (the others' missiles fly on to be seen; what they strike is their launcher's to say)
        if !self.guests.list.is_empty() {
            self.guests.update(dt, bodies, &mut builds.set, &mut self.guest_strikes);
            self.guest_strikes.clear();
        }
        for k in 0..self.missiles.list.len() + self.guests.list.len() {
            let m = if k < self.missiles.list.len() { self.missiles.list[k] } else { self.guests.list[k - self.missiles.list.len()] };
            let (Some(style), rate) = (self.trails[m.kind], f64::from(self.missiles.defs[m.kind].1.trail_rate)) else { continue };
            let n = ((m.t * rate).floor() - ((m.t - dt) * rate).floor()).max(0.0) as usize;
            let back = -m.vel.normalize_or_zero();
            for j in 0..n.min(8) {
                let p = m.pos + back * (m.vel.length() * dt * j as f64 / n as f64);
                self.fx.puff(style, bodies, bodies.dominant(p), p, (back * 6.0).as_vec3(), 0.5, 4.0);
            }
        }
    }

    /// Guided missiles steer and decoys drift; a missile that goes off blows its charge there
    /// (against what it struck, or beside what it passed); motors and decoys leave their puffs.
    fn fly_guided(&mut self, dt: f64, bodies: &BodyRegistry, builds: &mut Builds) {
        if self.guided.list.is_empty() && self.guided.decoys.is_empty() {
            return;
        }
        let (set, aims) = (&builds.set, &self.aims);
        self.guided.update(dt, bodies, |id| aims.iter().find(|a| a.0 == id).map(|a| a.1), |from, dir, len| set.raycast(from, dir, len).map(|(i, _, p)| (p, set.list[i].id)), &mut self.hits);
        for h in std::mem::take(&mut self.hits) {
            let at = h.at - h.vel.normalize_or_zero() * 0.5;
            let w = &self.guided_warheads[usize::from(h.kind)];
            // (what it was doing relative to what it struck is not known here: its charge alone)
            let blast = self.fx.explode(w, bodies, bodies.dominant(at), at);
            if self.tell && blast.is_ok() {
                self.seen.push(Seen::Boom { id: w.clone(), at, scale: 1.0, extra: 0.0 });
            }
            if let Ok(Some(d)) = blast {
                builds.blast(at, d);
            }
        }
        // how many puffs a thing that has flown `t` s owes this frame, at `rate` a second
        let owed = |t: f64, rate: f64| ((t * rate).floor() - ((t - dt).max(0.0) * rate).floor()).max(0.0) as usize;
        for m in &self.guided.list {
            let def = &self.guided.defs[usize::from(m.kind)].1;
            let (Some(style), true) = (self.guided_trails[usize::from(m.kind)], m.t < def.burn) else { continue };
            let n = owed(m.t, f64::from(def.trail_rate)).min(6);
            let back = -m.vel.normalize_or_zero();
            for j in 0..n {
                let p = m.pos + back * (m.vel.length() * dt * j as f64 / n as f64);
                self.fx.puff(style, bodies, bodies.dominant(p), p, (m.vel + back * 40.0).as_vec3(), 0.35, 1.6);
            }
        }
        for d in &self.guided.decoys {
            let def = &self.guided.decoy_defs[usize::from(d.kind)].1;
            let Some(style) = self.decoy_looks[usize::from(d.kind)] else { continue };
            // a cloud that opens: each puff leaves it a little its own way
            for j in 0..owed(f64::from(d.age), f64::from(def.rate)).min(4) {
                let h = (u64::from(d.id).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ ((f64::from(d.age) * 977.0) as u64 + j as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9)) >> 11;
                let r = |k: u32| ((h >> (k * 13)) & 0x1fff) as f64 / 4096.0 - 1.0;
                let out = DVec3::new(r(0), r(1), r(2)) * f64::from(def.salida) * 0.25;
                self.fx.puff(style, bodies, bodies.dominant(d.pos), d.pos, (d.vel + out).as_vec3(), def.size, def.life);
            }
        }
    }

    /// Behind the newest missile, looking where it flies (when following one).
    fn follow_view(&self, bodies: &BodyRegistry, view: View) -> View {
        let Some(m) = self.missiles.list.last().filter(|_| self.follow) else { return view };
        let up = bodies.get(bodies.dominant(m.pos)).up(m.pos);
        let dir = m.vel.normalize_or(view.forward);
        let eye = m.pos - dir * 30.0 + up * 8.0;
        View { eye, forward: (m.pos + dir * 40.0 - eye).normalize(), up, ..view }
    }

    /// The newest missile in flight: distance, what it will strike and when.
    pub fn status(&self, builds: &Builds) -> String {
        let Some(m) = self.missiles.list.last() else { return String::new() };
        let what = match m.predicted {
            Some((Target::Structure(id), t)) => format!("{} en {:.0} s", builds.set.get(id).map_or("estructura", |s| &*s.name), t - m.t),
            Some((Target::Ground, t)) => format!("suelo en {:.0} s", t - m.t),
            None => "sin impacto previsto".into(),
        };
        format!("misil: {:.0} m/s, {}", m.vel.length(), what)
    }

    /// Fire what was asked and simulate; returns the view with the camera shake. The particles
    /// and the lights are handed to the renderer by `draw`, once it is known where the picture
    /// is taken from.
    pub fn update(&mut self, dt: f64, bodies: &BodyRegistry, view: View, builds: &mut Builds) -> View {
        self.time += dt;
        if let Some((action, aim)) = self.queued.take() {
            self.fire(action, aim, bodies, &view, builds);
        }
        // automatic fire: as many rounds as the rate owes this frame
        if let Some((key, i, owed)) = self.held {
            let rate = f64::from(self.shots[i].1.rate.unwrap_or(10.0));
            let mut owed = owed + rate * dt;
            let mut n = 0;
            while owed >= 1.0 && n < 8 {
                owed -= 1.0;
                n += 1;
                self.fire(Action::Shoot(i), true, bodies, &view, builds);
            }
            self.held = Some((key, i, owed.min(1.0)));
        }
        self.fly_rounds(dt, bodies, builds);
        self.fly(dt, bodies, builds);
        self.fly_guided(dt, bodies, builds);
        self.fx.update(dt as f32, bodies);
        self.rounds.looks(&mut self.looks);
        for m in self.missiles.list.iter().chain(&self.guests.list) {
            if let Some((style, size)) = self.missile_looks[m.kind] {
                self.looks.push(Particle { pos: m.pos, vel: m.vel.as_vec3(), age: 0.0, life: 1.0, size, seed: 0.5, ground: 0.0, height: 1e6, body: bodies.dominant(m.pos), style });
            }
        }
        for m in &self.guided.list {
            if let Some((style, size)) = self.guided_looks[usize::from(m.kind)] {
                self.looks.push(Particle { pos: m.pos, vel: m.vel.as_vec3(), age: 0.0, life: 1.0, size, seed: 0.5, ground: 0.0, height: 1e6, body: bodies.dominant(m.pos), style });
            }
        }
        let view = self.follow_view(bodies, view);
        self.lights.clear();
        self.lights.extend(self.fx.flashes().iter().map(|f| {
            let i = f.now();
            Light { pos: f.pos, color: [f.color.x * i, f.color.y * i, f.color.z * i], range: f.range, ..Light::default() }
        }));
        let a = f64::from(self.fx.shake(view.eye));
        if a <= 0.0 {
            return view;
        }
        let right = view.forward.cross(view.up).normalize_or(DVec3::X);
        let t = self.time;
        let jitter = right * (t * 41.0).sin() + view.up * (t * 33.0 + 1.3).sin() * 0.8;
        View { forward: (view.forward + jitter * a).normalize(), ..view }
    }

    /// This frame's particles and lights to the renderer, seen from `eye`: where the picture is
    /// taken from in the end (the player's eyes, or a camera outside: the particles are sent
    /// relative to it, so the eye of `update` will not do).
    pub fn draw(&self, r: &mut Renderer, eye: DVec3) {
        r.set_effects(&self.fx.particles, &self.looks, &self.lights, eye);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_names() {
        assert_eq!(key_code("b"), Some(KeyCode::KeyB));
        assert_eq!(key_code("7"), Some(KeyCode::Digit7));
        assert_eq!(key_code("F1"), None);
    }

    #[test]
    fn every_key_does_one_thing() {
        // the real definitions: no shot, missile or explosion shares a key or takes the player's
        let d = crate::content::Defs::load(&crate::root().join("assets/defs")).unwrap_or_else(|e| panic!("{e}"));
        let b = Blasts::new(&d.effects, lunar_core::missiles::Missiles::new(d.missiles.clone()), 1000).unwrap_or_else(|e| panic!("{e}"));
        assert!(b.keys.iter().any(|(k, a)| *k == KeyCode::KeyQ && matches!(a, Action::Shoot(i) if b.shots[*i].0 == "metralleta")));
    }
}
