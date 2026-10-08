//! Explosions, shots and missiles in play. Each definition may name a key: pressing it fires where
//! the player aims, at whatever is nearer, ground or structure (core::effects runs the explosion,
//! core::rounds flies the shots that have a speed, core::missiles the missiles, the structures take
//! the damage). Every frame the particles, the rounds and missiles in flight (drawn as particles,
//! in the same draw) and the flashes go to the renderer; missiles leave their trail, the camera
//! shakes or follows a missile.
use crate::builds::Builds;
use glam::{DVec3, Vec3};
use lunar_core::{
    body::BodyRegistry,
    detonation::ChargeDef,
    effects::{EffectDefs, Effects, ExplosionDef, ShotDef},
    guided::{Aim, Flight as Guided, Hit},
    missiles::{Missiles, Strike, Target},
    particles::Particle,
    rounds::{self, Impact, Rounds},
    structure::{damage, motion::{Motion, Sweep}},
};
use lunar_core::view::View;

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

/// What can be let fly or set off: which of the game's definitions, by its place among them.
/// Every weapon there is or will be (a launcher in the hand, a ship's gun or rack, a test key,
/// a script) lets fly one of these, and nothing else knows what weapon it was.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum What {
    /// A shot of `shots.jsonc`: a round that flies, or one that strikes at once (no speed).
    Shot(u16),
    /// A ballistic missile (`missiles.jsonc`).
    Missile(u16),
    /// A guided missile (`guiados.jsonc`).
    Guided(u16),
    /// A decoy (`senuelos.jsonc`).
    Decoy(u16),
    /// An explosion of the effects' definitions, set off where it is.
    Boom(u16),
}

/// One thing let fly or set off: the one way anything is fired (`Blasts::launch`), and what the
/// other players' games are told of it (`Seen::Launch`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Launch {
    pub what: What,
    pub from: DVec3,
    /// Its own way (unit) and speed along it (m/s), besides `vel`: how fast what let it go was
    /// going there.
    pub dir: DVec3,
    pub speed: f32,
    pub vel: DVec3,
    /// What a guided missile follows.
    pub target: Option<u64>,
    /// The structure it was let go from (a ship's gun, the ship a player rides): it is told in
    /// that structure's frame, and a guided missile cannot strike it before it is armed.
    pub by: Option<u64>,
}

/// What another player's game must be told of what is let fly here (`multi` tells it; theirs
/// does it with `Blasts::show`). What any of it does to the structures is not here: the game it
/// was fired in decides it and tells it (`Builds::take_strikes`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Seen {
    /// Something let fly or set off here, numbered `tag` among ours.
    Launch { tag: u32, launch: Launch },
    /// Where something of ours ended (`tag`; 0: what struck at once): `what` it was, at `at`
    /// going along `dir` at `vel` (what it struck was going at), on structure `on` (none: the
    /// ground, or nothing), with `extra` J more in its blast. Elsewhere it ends there, so: what
    /// every game sees go off is where the damage was done.
    End { tag: u32, what: What, at: DVec3, dir: DVec3, vel: DVec3, on: Option<u64>, extra: f32 },
    /// Where a guided missile of ours is now, and what its seeker and motor push it with: the
    /// others' copies have no seeker of their own; they are led by this, and brought to it.
    Track { tag: u32, pos: DVec3, vel: DVec3, push: DVec3 },
}

/// The mark, in a tag, of what was let fly in another player's game: it flies and is seen here,
/// does nothing, and ends where its game says it ended (`Seen::End`).
const FOREIGN: u32 = 1 << 31;
/// The mark, in a tag told back to the game that let it fly, of its own number for it: where it
/// ends (and where a guided one is) is told so, and it is not started again here.
pub const OWN: u32 = 1 << 30;
/// A guided missile's own push off the rail (m/s).
pub const RAIL: f32 = 25.0;
/// How often where our guided missiles are is told (s); and how long the number another game
/// gave something is remembered (s: longer than anything flies).
const TRACK_EVERY: f64 = 0.1;
/// A copy of another game's guided missile off by less than this (m) is put where it is told.
const TRACK_SNAP: f64 = 10.0;
const FORGET: f64 = 300.0;
/// What was told longer ago than this (s) is not started here (it is over).
const STALE: f32 = 2.0;

pub struct Blasts {
    pub fx: Effects,
    pub missiles: Missiles,
    /// Trail style, warhead (explosion id) and look in flight of each missile kind.
    trails: Vec<Option<u8>>,
    warheads: Vec<String>,
    pub missile_looks: Vec<Option<(u8, f32)>>,
    /// Shots in flight, how each shot kind is drawn, where they landed this step, and every round
    /// and missile as a particle for the renderer (all reused).
    pub rounds: Rounds,
    shot_looks: Vec<(u8, f32)>,
    impacts: Vec<Impact>,
    sweep: Sweep,
    pub impact_count: u64,
    pub last_impact: Option<Impact>,
    pub looks: Vec<Particle>,
    strikes: Vec<Strike>,
    /// The camera rides behind the newest missile.
    pub follow: bool,
    shots: Vec<(String, ShotDef)>,
    /// The test keys (a letter or a digit, `key_name`) and what each fires.
    keys: Vec<(char, Action)>,
    /// To fire on the next update, and whether at the aim (else on the ground ahead).
    queued: Option<(Action, bool)>,
    /// An automatic shot whose key is held: (key, shot, rounds owed).
    held: Option<(char, usize, f64)>,
    /// How far ahead `fire_ahead` blasts land (m).
    ahead: f64,
    pub time: f64,
    shots_fired: u64,
    /// With other players: what was fired and set off here since it was last taken (`Seen`), kept
    /// only while `tell` is on (whoever takes them sets it).
    pub seen: Vec<Seen>,
    pub tell: bool,
    /// The missiles launched in other players' games: they fly here to be seen, and strike nothing.
    pub guests: Missiles,
    guest_strikes: Vec<Strike>,
    /// Guided missiles and decoys in flight (`lunar_core::guided`): the explosion, the look and
    /// the trail of each missile kind, the puffs of each decoy kind.
    pub guided: Guided,
    guided_warheads: Vec<String>,
    pub guided_looks: Vec<Option<(u8, f32)>>,
    guided_trails: Vec<Option<u8>>,
    decoy_looks: Vec<Option<u8>>,
    /// Where what the guided missiles follow is and how it shines, as whoever knows last said
    /// (`tactics`): by id.
    pub aims: Vec<(u64, Aim)>,
    hits: Vec<Hit>,
    /// The explosions a `What::Boom` names, in the order of their definitions.
    booms: Vec<String>,
    /// The number the next thing we let fly gets; what other games let fly, by (their player,
    /// their number), with the number it has here and when it came.
    next_tag: u32,
    foreign: Vec<(u64, u32, f64)>,
    /// Until where our guided missiles are is told again (s).
    track_in: f64,
    /// How many things have been let fly or set off here (ours and the others' copies).
    pub started: u64,
    /// How many things have ended here (ours and the others'); and, kept only while `log_ends`
    /// is on (the tests), each: what it was and on what, where in its frame.
    pub ends: u64,
    pub log_ends: bool,
    pub ended: Vec<(What, Option<(u64, Vec3)>)>,
    /// While the picture is drawn between steps (`Game::present`) what is let fly is kept here,
    /// as it was asked from where things are drawn, to be let fly from where they are.
    pub hold: bool,
    pub kept: Vec<Launch>,
}

/// A key name of the definitions ("B", "7"...).
/// The key a definition names (`"key": "b"`), as one upper-case letter or digit: what the keys of
/// whoever plays are matched against (the client turns its key codes into these).
pub fn key_name(name: &str) -> Option<char> {
    let c = name.trim().to_ascii_uppercase();
    let mut ch = c.chars();
    match (ch.next(), ch.next()) {
        (Some(l @ ('A'..='Z' | '0'..='9')), None) => Some(l),
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
                keys.push((key_name(k).ok_or_else(|| format!("{id}: unknown key '{k}'"))?, Action::Launch(i)));
            }
        }
        let bind = |k: &str, what: &str| key_name(k).ok_or_else(|| format!("{what}: unknown key '{k}'"));
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
        // one thing per key (that none is the player's own is for whoever reads the keys: `keys`)
        for (i, (k, _)) in keys.iter().enumerate() {
            if keys[..i].iter().any(|(o, _)| o == k) {
                return Err(format!("la tecla {k} está asignada dos veces (disparos, misiles, explosiones)"));
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
            booms: defs.explosions.iter().map(|(id, _)| id.clone()).collect(),
            next_tag: 0,
            foreign: Vec::new(),
            track_in: 0.0,
            started: 0,
            ends: 0,
            log_ends: false,
            ended: Vec::new(),
            hold: false,
            kept: Vec::new(),
            trails,
            warheads,
            missile_looks,
            rounds: Rounds::new(ROUNDS),
            sweep: Sweep::default(),
            impact_count: 0,
            last_impact: None,
            shot_looks,
            impacts: Vec::with_capacity(ROUNDS),
            looks: Vec::with_capacity(ROUNDS),
            strikes: Vec::new(),
            follow: false,
            shots: defs.shots.clone(),
            keys,
            queued: None,
            held: None,
            ahead: 20.0,
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

    /// What the definitions call `id`: a shot, a missile, a guided missile, a decoy or an
    /// explosion (looked for in that order).
    pub fn what(&self, id: &str) -> Option<What> {
        let at = |i: Option<usize>| i.map(|i| i as u16);
        at(self.shots.iter().position(|(s, _)| s == id))
            .map(What::Shot)
            .or_else(|| at(self.missiles.defs.iter().position(|(m, _)| m == id)).map(What::Missile))
            .or_else(|| self.guided.kind(id).map(What::Guided))
            .or_else(|| self.guided.decoy_kind(id).map(What::Decoy))
            .or_else(|| at(self.booms.iter().position(|b| b == id)).map(What::Boom))
    }

    /// Everything the definitions say can be let fly or set off, by name.
    pub fn every(&self) -> Vec<(String, What)> {
        let named = |list: &mut Vec<(String, What)>, ids: &mut dyn Iterator<Item = &String>, what: fn(u16) -> What| list.extend(ids.enumerate().map(|(i, id)| (id.clone(), what(i as u16))));
        let mut out = Vec::new();
        named(&mut out, &mut self.shots.iter().map(|s| &s.0), What::Shot);
        named(&mut out, &mut self.missiles.defs.iter().map(|m| &m.0), What::Missile);
        named(&mut out, &mut self.guided.defs.iter().map(|g| &g.0), What::Guided);
        named(&mut out, &mut self.guided.decoy_defs.iter().map(|d| &d.0), What::Decoy);
        named(&mut out, &mut self.booms.iter(), What::Boom);
        out
    }

    /// Whether anything of ours is still flying (rounds, missiles, guided missiles).
    pub fn flying(&self) -> bool {
        self.rounds.list.iter().any(|r| r.tag & FOREIGN == 0) || !self.missiles.list.is_empty() || self.guided.list.iter().any(|m| m.tag & FOREIGN == 0)
    }

    /// How many rounds a second shot kind `i` fires while its key is held (none: one a press).
    pub fn shot_rate(&self, i: u16) -> Option<f32> {
        self.shots.get(usize::from(i)).and_then(|(_, s)| s.rate)
    }

    /// The number of shot `id` (`What::Shot`), if there is one.
    pub fn shot_index(&self, id: &str) -> Option<u16> {
        self.shots.iter().position(|(s, _)| s == id).map(|i| i as u16)
    }

    /// The speed shot `id` leaves at (m/s) and how far it reaches (m): what a gun's sight is
    /// worked out with.
    pub fn shot_speed(&self, id: &str) -> Option<(f32, f32)> {
        self.shots.iter().find(|(s, _)| s == id).and_then(|(_, s)| s.speed.map(|v| (v, s.range)))
    }

    /// Shot `id` leaving `from` along `dir` (scattered as the shot is), from something going at
    /// `vel` there, structure `by`'s if it is one's: what `launch` lets fly. None if there is no
    /// such shot.
    pub fn shot(&mut self, id: &str, from: DVec3, dir: DVec3, vel: DVec3, by: Option<u64>) -> Option<Launch> {
        let i = self.shots.iter().position(|(s, _)| s == id)?;
        let s = &self.shots[i].1;
        let (speed, spread) = (s.speed.unwrap_or(0.0), s.spread);
        self.shots_fired += 1;
        let dir = self.scatter(&View { eye: from, forward: dir, up: dir.any_orthonormal_vector(), fov_y: 1.0, near: 0.1 }, spread);
        Some(Launch { what: What::Shot(i as u16), from, dir, speed, vel, target: None, by })
    }

    /// Shot `id` fired now (`shot`, then `launch`). False if there is no such shot or no room.
    #[allow(clippy::too_many_arguments)]
    pub fn fire_from(&mut self, id: &str, from: DVec3, dir: DVec3, vel: DVec3, by: Option<u64>, bodies: &BodyRegistry, builds: &mut Builds) -> bool {
        self.shot(id, from, dir, vel, by).is_some_and(|l| self.launch(l, bodies, builds))
    }

    /// The one way anything is let fly or set off here, whatever fired it: it flies (or goes off)
    /// and does what it does here, and the other games are told of it (`Seen::Launch`; where it
    /// ends, `Seen::End`). False if there was no room for it.
    pub fn launch(&mut self, l: Launch, bodies: &BodyRegistry, builds: &mut Builds) -> bool {
        // (asked while the picture is drawn between steps: kept until the world is back at its
        // step, and let fly from there: `Game::restore`)
        if self.hold {
            self.kept.push(l);
            return true;
        }
        self.next_tag = (self.next_tag + 1) % OWN;
        let tag = self.next_tag.max(1);
        if !self.start(&l, tag, 0.0, bodies, builds) {
            return false;
        }
        // (what struck at once told its end already: there is nothing in flight to tell of)
        if !matches!(l.what, What::Shot(i) if self.shots[usize::from(i)].1.speed.is_none()) {
            self.told(Seen::Launch { tag, launch: l });
        }
        true
    }

    /// `l` started, `age` s after it was let go: ours (it does what it does), or another game's
    /// (`tag` marked `FOREIGN`: it is seen, and does nothing).
    fn start(&mut self, l: &Launch, tag: u32, age: f32, bodies: &BodyRegistry, builds: &mut Builds) -> bool {
        let started = self.start_one(l, tag, age, bodies, builds);
        self.started += u64::from(started);
        started
    }

    fn start_one(&mut self, l: &Launch, tag: u32, age: f32, bodies: &BodyRegistry, builds: &mut Builds) -> bool {
        let ours = tag & FOREIGN == 0;
        let vel = l.vel + l.dir * f64::from(l.speed);
        let (from, age64) = (l.from + vel * f64::from(age), f64::from(age));
        match l.what {
            What::Shot(i) => {
                let Some((_, s)) = self.shots.get(usize::from(i)) else { return false };
                if s.speed.is_none() {
                    return ours && self.strike_at_once(usize::from(i), l, bodies, builds);
                }
                let (style, size) = self.shot_looks[usize::from(i)];
                let seed = (self.shots_fired % 997) as f32 / 997.0;
                let mut r = rounds::round(i, from, l.dir, l.speed, s.range, bodies.dominant(from), style, size, seed);
                (r.vel, r.tag) = (vel, tag);
                r.left -= age;
                self.rounds.fire(r)
            }
            What::Missile(k) => {
                if usize::from(k) >= self.missiles.defs.len() {
                    return false;
                }
                let pool = if ours { &mut self.missiles } else { &mut self.guests };
                pool.fire(usize::from(k), from, vel, tag);
                true
            }
            What::Guided(k) => {
                // (from where it left, flown on as it flies: it steers and burns from the start)
                if usize::from(k) >= self.guided.defs.len() || !self.guided.launch(k, l.from, vel, l.target, l.by.unwrap_or(0)) {
                    return false;
                }
                let aims = &self.aims;
                self.guided.catch_up(age64, bodies, |id| aims.iter().find(|a| a.0 == id).map(|a| a.1));
                // (ours: where it is and how it steers told from its first step; another game's
                // steers by its own seeker until that game says how it steers, then is led by it)
                if ours {
                    self.track_in = 0.0;
                }
                if let Some(m) = self.guided.list.last_mut() {
                    m.tag = tag;
                }
                true
            }
            What::Decoy(k) => usize::from(k) < self.guided.decoy_defs.len() && self.guided.release(k, from, vel),
            What::Boom(e) => {
                let Some(id) = self.booms.get(usize::from(e)).cloned() else { return false };
                match self.fx.explode_moving(&id, bodies, bodies.dominant(from), Motion { at: from, vel: l.vel, spin: DVec3::ZERO }, 1.0, 0.0) {
                    Ok(Some(d)) if ours => builds.blast(from, d),
                    Ok(_) => {}
                    Err(e) => {
                        eprintln!("{e}");
                        return false;
                    }
                }
                true
            }
        }
    }

    /// A shot of ours that strikes at once (no speed): what it strikes along its way, ground or
    /// structure, and its end told.
    fn strike_at_once(&mut self, i: usize, l: &Launch, bodies: &BodyRegistry, builds: &mut Builds) -> bool {
        let s = self.shots[i].1.clone();
        let range = f64::from(s.range);
        let ground = bodies.raycast(l.from, l.dir, range).map(|(_, p)| p);
        let reach = ground.map_or(range, |p| p.distance(l.from));
        let on = builds.set.raycast(l.from, l.dir, reach).map(|(k, _, _)| builds.set.list[k].id);
        let at = builds.shoot(l.from, l.dir, &s, reach, self.shots_fired).or(ground);
        if let Some(at) = at {
            let vel = on.and_then(|id| builds.set.get(id)).map_or(DVec3::ZERO, |st| st.velocity_at(at));
            self.end(0, What::Shot(i as u16), at, l.dir, vel, on, 0.0, bodies, builds);
        }
        true
    }

    /// Where something ended: what goes off there goes off (ours: with what it does to what is
    /// round it, `builds`), and ours is told.
    #[allow(clippy::too_many_arguments)]
    fn end(&mut self, tag: u32, what: What, at: DVec3, dir: DVec3, vel: DVec3, on: Option<u64>, extra: f32, bodies: &BodyRegistry, builds: &mut Builds) {
        // (a little back along its way: it goes off at the surface, not in it)
        let (id, back, extra) = match what {
            What::Shot(i) => match self.shots.get(usize::from(i)).and_then(|(_, s)| s.impact.clone()) {
                Some(fx) => (fx, 0.1, 0.0),
                None => (String::new(), 0.0, 0.0),
            },
            What::Missile(k) => (self.warheads.get(usize::from(k)).cloned().unwrap_or_default(), 0.5, extra),
            What::Guided(k) => (self.guided_warheads.get(usize::from(k)).cloned().unwrap_or_default(), 0.5, 0.0),
            What::Decoy(_) | What::Boom(_) => (String::new(), 0.0, 0.0),
        };
        let ours = tag & FOREIGN == 0;
        self.ends += 1;
        if self.log_ends {
            // (in the frame of what it struck as it is now: one behind the world carried on to it)
            let set = &builds.set;
            let local = on.and_then(|id| set.get(id)).map(|s| (s.id, s.rot.inverse() * (at - (s.pos + s.vel * (set.now - s.clock).max(0.0))).as_vec3()));
            self.ended.push((what, local));
        }
        if !id.is_empty() {
            let at = at - dir * back;
            if let (Ok(Some(d)), true) = (self.fx.explode_moving(&id, bodies, bodies.dominant(at), Motion { at, vel, spin: DVec3::ZERO }, 1.0, extra), ours) {
                builds.blast(at, d);
            }
        }
        if ours {
            self.told(Seen::End { tag, what, at, dir, vel, on, extra });
        }
    }

    /// A key went down: true when it fires something (on the next update); an automatic shot
    /// keeps firing until the key goes up.
    pub fn key(&mut self, key: char) -> bool {
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
    pub fn release(&mut self, key: char) {
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

    /// What a test key (or a script) fires, as a launch from the view.
    fn fire(&mut self, action: Action, aim: bool, bodies: &BodyRegistry, view: &View, motion: Motion, builds: &mut Builds) {
        let launch = match action {
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
                let (Some(at), Some(e)) = (at, self.booms.iter().position(|b| *b == id)) else { return };
                // pulled back a little toward the shooter: the blast goes off at the surface, not in it
                Launch { what: What::Boom(e as u16), from: at - view.forward * 0.3, dir: view.forward, speed: 0.0, vel: DVec3::ZERO, target: None, by: None }
            }
            Action::Launch(i) => {
                let Some(to) = Blasts::aim(bodies, builds, view.eye, view.forward, MISSILE_RANGE) else { return };
                let from = view.eye + view.forward * 2.0;
                let Some(v) = self.missiles.solve(i, from, to, bodies) else {
                    eprintln!("{}: fuera de alcance", self.missiles.defs[i].1.name);
                    return;
                };
                Launch { what: What::Missile(i as u16), from, dir: v.normalize_or(view.forward), speed: v.length() as f32, vel: DVec3::ZERO, target: None, by: None }
            }
            Action::Shoot(i) => {
                let id = self.shots[i].0.clone();
                // (what flies leaves a little ahead of the eye; what strikes at once, from it)
                let from = if self.shots[i].1.speed.is_some() { view.eye + view.forward * 1.0 } else { view.eye };
                let Some(l) = self.shot(&id, from, view.forward, motion.velocity_at(from), None) else { return };
                l
            }
        };
        self.launch(launch, bodies, builds);
    }

    /// Something for the other players' games, kept while there are any.
    fn told(&mut self, what: Seen) {
        if self.tell {
            self.seen.push(what);
        }
    }

    /// What flies here now that is this game's own (rounds, missiles, guided missiles, decoys;
    /// not the copies of another game's), and the number the next thing let fly gets: what a game
    /// kept on disk keeps of it (`save`), to the last bit. `take_up` puts it back.
    pub fn keep(&self, out: &mut Vec<u8>) {
        fn v(out: &mut Vec<u8>, p: DVec3) {
            p.to_array().iter().for_each(|x| out.extend_from_slice(&x.to_le_bytes()));
        }
        let mine = |tag: u32| tag & FOREIGN == 0;
        out.extend_from_slice(&self.next_tag.to_le_bytes());
        out.extend_from_slice(&(self.rounds.list.iter().filter(|r| mine(r.tag)).count() as u32).to_le_bytes());
        for r in self.rounds.list.iter().filter(|r| mine(r.tag)) {
            (v(out, r.pos), v(out, r.vel), v(out, r.nose));
            out.extend_from_slice(&r.kind.to_le_bytes());
            out.extend_from_slice(&r.body.to_le_bytes());
            out.extend_from_slice(&r.left.to_le_bytes());
            out.push(r.style);
            out.extend_from_slice(&r.size.to_le_bytes());
            out.extend_from_slice(&r.seed.to_le_bytes());
            out.extend_from_slice(&r.tag.to_le_bytes());
        }
        out.extend_from_slice(&(self.missiles.list.len() as u32).to_le_bytes());
        for m in &self.missiles.list {
            out.extend_from_slice(&(m.kind as u32).to_le_bytes());
            out.extend_from_slice(&m.tag.to_le_bytes());
            (v(out, m.pos), v(out, m.vel));
            out.extend_from_slice(&m.t.to_le_bytes());
            out.extend_from_slice(&m.next_look.to_le_bytes());
        }
        out.extend_from_slice(&(self.guided.list.iter().filter(|g| mine(g.tag) && g.led.is_none()).count() as u32).to_le_bytes());
        for g in self.guided.list.iter().filter(|g| mine(g.tag) && g.led.is_none()) {
            out.extend_from_slice(&g.kind.to_le_bytes());
            (v(out, g.pos), v(out, g.vel));
            out.extend_from_slice(&g.t.to_le_bytes());
            out.extend_from_slice(&g.target.map_or(u64::MAX, |t| t).to_le_bytes());
            out.extend_from_slice(&g.shooter.to_le_bytes());
            out.extend_from_slice(&g.blind.to_le_bytes());
            out.extend_from_slice(&g.look_in.to_le_bytes());
            out.extend_from_slice(&g.id.to_le_bytes());
            out.extend_from_slice(&g.tag.to_le_bytes());
            v(out, g.push);
        }
        out.extend_from_slice(&(self.guided.decoys.len() as u32).to_le_bytes());
        for d in &self.guided.decoys {
            out.extend_from_slice(&d.kind.to_le_bytes());
            (v(out, d.pos), v(out, d.vel));
            out.extend_from_slice(&d.age.to_le_bytes());
            out.extend_from_slice(&d.id.to_le_bytes());
        }
    }

    /// What `keep` kept, flying again (on top of whatever flies here: a game just made has
    /// nothing). An error if it is not what `keep` writes, or names what there is not.
    pub fn take_up(&mut self, r: &mut lunar_net::Reader) -> Result<(), lunar_net::WireError> {
        use lunar_net::WireError::Value;
        fn v(r: &mut lunar_net::Reader) -> Result<DVec3, lunar_net::WireError> {
            Ok(DVec3::new(r.f64()?, r.f64()?, r.f64()?))
        }
        self.next_tag = self.next_tag.max(r.u32()?);
        for _ in 0..r.u32()? {
            let (pos, vel, nose) = (v(r)?, v(r)?, v(r)?);
            let (kind, body, left, style, size, seed, tag) = (r.u16()?, r.u16()?, r.f32()?, r.u8()?, r.f32()?, r.f32()?, r.u32()?);
            if usize::from(kind) >= self.shots.len() {
                return Err(Value);
            }
            self.rounds.list.push(rounds::Round { pos, vel, nose, kind, body, left, style, size, seed, tag });
        }
        for _ in 0..r.u32()? {
            let (kind, tag) = (r.u32()? as usize, r.u32()?);
            let (pos, vel, t, next_look) = (v(r)?, v(r)?, r.f64()?, r.f64()?);
            if kind >= self.missiles.defs.len() {
                return Err(Value);
            }
            self.missiles.list.push(lunar_core::missiles::Missile { kind, tag, pos, vel, t, predicted: None, next_look });
        }
        for _ in 0..r.u32()? {
            let kind = r.u16()?;
            let (pos, vel, t) = (v(r)?, v(r)?, r.f64()?);
            let target = Some(r.u64()?).filter(|t| *t != u64::MAX);
            let (shooter, blind, look_in, id, tag, push) = (r.u64()?, r.f32()?, r.f32()?, r.u32()?, r.u32()?, v(r)?);
            if usize::from(kind) >= self.guided.defs.len() {
                return Err(Value);
            }
            self.guided.put_back(lunar_core::guided::Guided { kind, pos, vel, t, target, shooter, blind, look_in, id, tag, push, led: None });
        }
        for _ in 0..r.u32()? {
            let kind = r.u16()?;
            let (pos, vel, age, id) = (v(r)?, v(r)?, r.f32()?, r.u32()?);
            if usize::from(kind) >= self.guided.decoy_defs.len() {
                return Err(Value);
            }
            self.guided.put_back_decoy(lunar_core::guided::Decoy { kind, pos, vel, age, id });
        }
        Ok(())
    }

    /// What was let fly here as `tag` gone, as if it never was (the server did not let it fly).
    pub fn unfire(&mut self, tag: u32) {
        let tag = tag & !OWN;
        // (what strikes at once has no number: it struck already)
        if tag == 0 {
            return;
        }
        self.rounds.list.retain(|r| r.tag != tag);
        self.missiles.list.retain(|m| m.tag != tag);
        self.guided.list.retain(|m| m.tag != tag);
    }

    /// What player `from`'s game told, `age` s ago, done here: what they let fly flies here from
    /// where it is by now (seen, doing nothing), and ends where theirs ended; their guided
    /// missiles are brought to where theirs are.
    pub fn show(&mut self, from: u32, what: &Seen, age: f32, bodies: &BodyRegistry, builds: &mut Builds) {
        let key = |tag: u32| (u64::from(from) << 32) | u64::from(tag);
        match *what {
            // (ours, told back: it flies here already)
            Seen::Launch { tag, .. } if tag & OWN != 0 => {}
            // (ours, told back: still flying here, it ends now where it did there; ended here
            // already, it is not seen ending twice)
            Seen::End { tag, what, at, dir, vel, on, extra } if tag & OWN != 0 => {
                let mine = tag & !OWN;
                let flying = self.rounds.list.iter().any(|r| r.tag == mine) || self.missiles.list.iter().any(|m| m.tag == mine) || self.guided.list.iter().any(|m| m.tag == mine);
                if flying {
                    self.rounds.list.retain(|r| r.tag != mine);
                    self.missiles.list.retain(|m| m.tag != mine);
                    self.guided.list.retain(|m| m.tag != mine);
                    if age < STALE {
                        self.end(FOREIGN, what, at, dir, vel, on, extra, bodies, builds);
                    }
                }
            }
            Seen::Track { tag, pos, vel, .. } if tag & OWN != 0 => {
                // (ours, as it flies there: brought to it, as another's is)
                if let Some(m) = self.guided.list.iter_mut().find(|m| m.tag == tag & !OWN) {
                    let age = f64::from(age.min(STALE));
                    let want = pos + vel * age;
                    let k = if m.pos.distance(want) < TRACK_SNAP { 1.0 } else { 0.5 };
                    m.pos += (want - m.pos) * k;
                    m.vel += (vel - m.vel) * k;
                }
            }
            Seen::Launch { tag, launch } => {
                if age > STALE {
                    return;
                }
                self.next_tag = (self.next_tag + 1) % OWN;
                let here = self.next_tag.max(1) | FOREIGN;
                self.foreign.push((key(tag), here, self.time));
                self.start(&launch, here, age, bodies, builds);
            }
            Seen::End { tag, what, at, dir, vel, on, extra } => {
                // (still flying here: it ends now, where theirs did)
                if let Some(here) = self.foreign.iter().position(|f| f.0 == key(tag)).map(|k| self.foreign.swap_remove(k).1) {
                    self.rounds.list.retain(|r| r.tag != here);
                    self.guests.list.retain(|m| m.tag != here);
                    self.guided.list.retain(|m| m.tag != here);
                }
                if age < STALE {
                    self.end(FOREIGN, what, at, dir, vel, on, extra, bodies, builds);
                }
            }
            Seen::Track { tag, pos, vel, push } => {
                let Some(here) = self.foreign.iter().find(|f| f.0 == key(tag)).map(|f| f.1) else { return };
                if let Some(m) = self.guided.list.iter_mut().find(|m| m.tag == here) {
                    // where theirs is by now, carried on as it goes (what pulls it too)
                    let age = f64::from(age.min(STALE));
                    let a = push + bodies.field(pos).pull;
                    let (want, want_v) = (pos + vel * age + a * (0.5 * age * age), vel + a * age);
                    // (off by less than it flies in a frame: there at once, it cannot be seen;
                    // further: half the way each time, no jump)
                    let k = if m.pos.distance(want) < TRACK_SNAP { 1.0 } else { 0.5 };
                    m.pos += (want - m.pos) * k;
                    m.vel += (want_v - m.vel) * k;
                    m.led = Some(push);
                }
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
    pub fn flight(&mut self) -> (&mut Effects, rounds::Flight<'_>) {
        (&mut self.fx, rounds::Flight { rounds: &mut self.rounds, impacts: &mut self.impacts, sweep: &mut self.sweep })
    }

    pub fn land_rounds(&mut self, bodies: &BodyRegistry, builds: &mut Builds) {
        for k in 0..self.impacts.len() {
            let i = self.impacts[k];
            // (another game's: it ends where that game says, `show`)
            if i.tag & FOREIGN != 0 {
                continue;
            }
            let Some((_, s)) = self.shots.get(usize::from(i.kind)) else { continue };
            let (energy, area) = (s.energy, s.area);
            self.impact_count += 1;
            self.last_impact = Some(i);
            self.shots_fired += 1;
            let (at, dir, vel, on) = match i.surface {
                Some(hit) => {
                    let Some(structure) = builds.set.get(hit.id) else { continue };
                    let at = structure.to_world(hit.point);
                    let dir = (structure.rot * hit.dir).as_dvec3();
                    let vel = structure.velocity_at(at);
                    builds.hit(hit.id, &damage::Hit { point: hit.point - hit.dir * 0.05, dir: hit.dir, energy, radius: 0.0, area });
                    (at, dir, vel, Some(hit.id))
                }
                None => (i.at, i.dir, DVec3::ZERO, None),
            };
            self.end(i.tag, What::Shot(i.kind), at, dir, vel, on, 0.0, bodies, builds);
        }
        self.impacts.clear();
    }

    /// Missiles fly; their trails are left behind; strikes blow their warhead (plus their speed).
    fn fly(&mut self, dt: f64, bodies: &BodyRegistry, builds: &mut Builds) {
        self.missiles.update(dt, bodies, &mut builds.set, &mut self.strikes);
        for s in std::mem::take(&mut self.strikes) {
            let on = match s.target {
                Target::Structure(id) => Some(id),
                Target::Ground => None,
            };
            let vel = on.and_then(|id| builds.set.get(id)).map_or(DVec3::ZERO, |st| st.velocity_at(s.at));
            // the impact's energy: a bigger blast for a charge, a harder hit for anything else
            self.end(s.tag, What::Missile(s.kind as u16), s.at, s.vel.normalize_or_zero(), vel, on, (s.energy * 0.5) as f32, bodies, builds);
        }
        // (the others' missiles fly on to be seen; they end where their game says)
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
            // (another game's: it ends where that game says, `show`)
            if h.tag & FOREIGN != 0 {
                continue;
            }
            let vel = h.structure.and_then(|id| builds.set.get(id)).map_or(DVec3::ZERO, |st| st.velocity_at(h.at));
            // (what it was doing relative to what it struck is not known here: its charge alone)
            self.end(h.tag, What::Guided(h.kind), h.at, h.vel.normalize_or_zero(), vel, h.structure, 0.0, bodies, builds);
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
    /// The picture behind the newest missile when the camera follows it (`follow`), else `view`.
    pub fn follow_view(&self, bodies: &BodyRegistry, view: View) -> View {
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
    /// `dt` s on: what a test key or a script asked fired from `view` (going as `motion`), what
    /// flies flown, the effects run.
    pub fn update(&mut self, dt: f64, bodies: &BodyRegistry, view: View, motion: Motion, builds: &mut Builds) {
        self.time += dt;
        if let Some((action, aim)) = self.queued.take() {
            self.fire(action, aim, bodies, &view, motion, builds);
        }
        // automatic fire: as many rounds as the rate owes this frame
        if let Some((key, i, owed)) = self.held {
            let rate = f64::from(self.shots[i].1.rate.unwrap_or(10.0));
            let mut owed = owed + rate * dt;
            let mut n = 0;
            while owed >= 1.0 && n < 8 {
                owed -= 1.0;
                n += 1;
                self.fire(Action::Shoot(i), true, bodies, &view, motion, builds);
            }
            self.held = Some((key, i, owed.min(1.0)));
        }
        self.fly(dt, bodies, builds);
        self.fly_guided(dt, bodies, builds);
        self.fx.update(dt as f32, bodies);
        // now and then: where our guided missiles are, for the others; and the numbers of the
        // others' things no longer in flight here forgotten
        self.track_in -= dt;
        if self.track_in <= 0.0 {
            self.track_in = TRACK_EVERY;
            if self.tell {
                for m in &self.guided.list {
                    if m.tag != 0 && m.tag & FOREIGN == 0 {
                        self.seen.push(Seen::Track { tag: m.tag, pos: m.pos, vel: m.vel, push: m.push });
                    }
                }
            }
            if !self.foreign.is_empty() {
                let mut flying: Vec<u32> = self.rounds.list.iter().map(|r| r.tag).chain(self.guests.list.iter().map(|m| m.tag)).chain(self.guided.list.iter().map(|m| m.tag)).filter(|t| t & FOREIGN != 0).collect();
                flying.sort_unstable();
                let t = self.time;
                self.foreign.retain(|f| t - f.2 < FORGET && flying.binary_search(&f.1).is_ok());
            }
        }
    }

    /// The test keys there are (to be checked against the player's own).
    pub fn keys(&self) -> impl Iterator<Item = char> + '_ {
        self.keys.iter().map(|(k, _)| *k)
    }

    pub fn shaken(&self, view: View) -> View {
        let a = f64::from(self.fx.shake(view.eye));
        if a <= 0.0 {
            return view;
        }
        let right = view.forward.cross(view.up).normalize_or(DVec3::X);
        let t = self.time;
        let jitter = right * (t * 41.0).sin() + view.up * (t * 33.0 + 1.3).sin() * 0.8;
        View { forward: (view.forward + jitter * a).normalize(), ..view }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_handheld_round_starts_at_the_muzzle() {
        let defs = crate::defs::Defs::load(&crate::root().join("assets/defs")).unwrap();
        let bodies = &defs.system.bodies;
        let site = lunar_core::scene::Site::from_def(&defs.scenario.site, bodies).unwrap();
        let effects: Vec<&str> = defs.effects.explosions.iter().map(|(id, _)| id.as_str()).collect();
        let mut builds = Builds::new(defs.structures.clone(), &defs.scenario, &site, bodies, &effects).unwrap();
        let mut blasts = Blasts::new(&defs.effects, Missiles::new(defs.missiles.clone()), 1000).unwrap();
        let from = DVec3::new(4e6, 3e6, 2e6);
        blasts.tell = true;
        for speed in [0.0, 30.0, 300.0, 1600.0, 7800.0] {
            blasts.rounds.list.clear();
            let inherited = DVec3::new(speed, -speed * 0.3, speed * 0.2);
            assert!(blasts.fire_from("cohete", from, DVec3::Z, inherited, None, bodies, &mut builds));
            let round = blasts.rounds.list[0];
            assert!(round.pos.distance(from) < 0.001, "el cohete nace a {} m de la boca", round.pos.distance(from));
            assert!(round.vel.distance(inherited + DVec3::Z * 150.0) < 1e-8);
            let seen = blasts.seen.pop().unwrap();
            blasts.show(1, &seen, 0.1, bodies, &mut builds);
            let remote = blasts.rounds.list[1];
            assert!((remote.left - (round.left - 0.1)).abs() < 1e-5);
            assert_eq!(remote.nose, round.nose);
            assert_eq!(remote.vel, round.vel);
            blasts.rounds.looks(&mut blasts.looks);
            assert_eq!(blasts.looks[0].vel, (DVec3::Z * 150.0).as_vec3());
        }
    }

    #[test]
    fn key_names() {
        assert_eq!(key_name("b"), Some('B'));
        assert_eq!(key_name("7"), Some('7'));
        assert_eq!(key_name("F1"), None);
    }

    #[test]
    fn every_key_does_one_thing() {
        // the real definitions: no shot, missile or explosion shares a key (that none takes the
        // player's is the client's to check: `input::taken`)
        let d = crate::defs::Defs::load(&crate::root().join("assets/defs")).unwrap_or_else(|e| panic!("{e}"));
        let b = Blasts::new(&d.effects, lunar_core::missiles::Missiles::new(d.missiles.clone()), 1000).unwrap_or_else(|e| panic!("{e}"));
        assert!(b.keys.iter().any(|(k, a)| *k == 'U' && matches!(a, Action::Shoot(i) if b.shots[*i].0 == "metralleta")));
    }
}
