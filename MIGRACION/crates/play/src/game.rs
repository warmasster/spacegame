//! The game, one step at a time. Every machine that plays — the server, which has the truth, and
//! each player's game, which predicts its own with it — runs the same steps: exactly `STEP` s each,
//! numbered from the start (`Game::step`), whatever the frames of whoever draws it. Nothing in a
//! step reads a clock, a key or the picture: what each player asks for is handed over as their
//! `Input` before it, and what happened is read after it.
//!
//! The order of a step is the one the world has always had (`docs/MOVIMIENTO.md`): the players
//! take what they ask for; what flies is flown; the ships' sensors look, their systems run and
//! their weapons fire; then the structures and everything that lives among them (the players,
//! the rounds in flight) slice by slice; then where rounds landed, and the hands.
//!
//! Whoever draws it draws between the last two steps (`present` / `restore`): everything of the
//! same instant, a share of a step behind the newest.
use crate::{
    blasts::Blasts,
    builds::{Builds, Strike},
    defs::Defs,
    hands::Hands,
    pilot::{Controls, Input, Pilot},
    ships::Ships,
    tactics::Tactics,
};
use glam::{Affine3A, DQuat, DVec3, Quat};
use lunar_core::{
    body::BodyRegistry,
    scenario::PlayerDef,
    scene::Site,
    structure::{motion::Motion, schedule::Among, set::Structures},
    traffic::Traffic,
    view::View,
};
use std::{path::Path, sync::Arc};

/// How long a step is (s), and how many there are in a second.
pub const HZ: u32 = 60;
pub const STEP: f64 = 1.0 / HZ as f64;

/// Someone in the game: their body and what they ask of it, their bare hands, and how they like
/// their controls. The game steps them all alike; who they are (here, over a network, a bot) is
/// whoever holds them.
pub struct Player {
    pub pilot: Pilot,
    pub hands: Hands,
    /// What the keys ask of the next step (`Game::tick` hands it to the body); a jump is asked
    /// once and taken by the step that does it.
    pub input: Input,
    pub controls: Controls,
    /// The view the player acts along (what the hands reach for, what a test key fires at): their
    /// eyes, or, seen from outside, what is under the middle of the picture. None: the body's own.
    pub aim: Option<View>,
    /// Not in the world yet (a player whose game has not said what it asks): no step moves them,
    /// and they watch nothing.
    pub away: bool,
    /// How far from where it is the body is drawn (world; and aboard, in what carries it): what
    /// is left to go of a correction, taken out over a moment so that the eye does not jump
    /// (`online`). Only `present` uses it.
    pub offset: (DVec3, glam::Vec3),
    /// Where the body was as the last step began, to draw it between steps (`present`).
    was: Option<(DVec3, Option<(u64, glam::Vec3)>)>,
    shown: Option<(DVec3, Option<glam::Vec3>)>,
}

impl AsMut<Player> for Player {
    fn as_mut(&mut self) -> &mut Player {
        self
    }
}

impl Player {
    pub fn new(bodies: Arc<BodyRegistry>, site: &Site, def: PlayerDef) -> Player {
        Player { pilot: Pilot::new(bodies, site, def), hands: Hands::new(def.manos), input: Input::default(), controls: Controls::new(&def), aim: None, away: false, offset: (DVec3::ZERO, glam::Vec3::ZERO), was: None, shown: None }
    }

    /// The view the player acts along now (`aim`, else their eyes where they are).
    pub fn acting_view(&self, set: &Structures) -> View {
        self.aim.unwrap_or_else(|| self.pilot.view_aboard(set).unwrap_or_else(|| self.pilot.view()))
    }
}

/// Every player as one thing that lives among the structures (`Among`): each stepped, in turn,
/// with every slice (where they are is watched through `Game::seen_from`).
struct Crowd<'a, P>(&'a mut [P], DVec3);

impl<P: AsMut<Player>> Among for Crowd<'_, P> {
    fn wake(&mut self, set: &mut Structures, dt: f64) {
        for p in self.0.iter_mut().map(|p| p.as_mut()).filter(|p| !p.away) {
            p.pilot.wake(set, dt);
        }
    }

    fn before(&mut self, set: &Structures) {
        for p in self.0.iter_mut().map(|p| p.as_mut()).filter(|p| !p.away) {
            p.pilot.before(set);
        }
    }

    fn slice(&mut self, set: &Structures, bodies: &BodyRegistry, dt: f64) {
        for p in self.0.iter_mut().map(|p| p.as_mut()).filter(|p| !p.away) {
            p.pilot.slice(set, bodies, dt);
        }
    }

    fn at(&self) -> DVec3 {
        self.1
    }
}

/// Who has the say over what happens in this game.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Say {
    /// All of it, and nobody else plays: what is decided is done.
    #[default]
    Alone,
    /// All of it, and players play over a network: what is done to a structure is done here in
    /// one order (`Out::strikes`, with its dice) for every player's game to do the same.
    Server,
    /// None of it: a player's game that predicts its own body over a server (`online`). What it
    /// decides about a structure is not done (the server's word is), the ships' weapons are the
    /// server's to fire.
    Client,
}

/// What a step told, for whoever reads it after it (emptied at the start of each).
#[derive(Default)]
pub struct Out {
    /// What the ships told whoever works them: the ship's structure, about what, the line and
    /// its level (0 normal, 1 caution, 2 warning).
    pub said: Vec<(u64, String, String, u8)>,
    /// Said by the server (`Say::Server`): what was done to structures this step, in the order it
    /// was done, each with its dice and, for what has articulations, how they were posed (bones
    /// past the root; none: as they are).
    pub strikes: Vec<(Strike, u64, Vec<Affine3A>)>,
    /// What came into being this step (pieces off what was struck, what a ship let go of), by id.
    pub made: Vec<u64>,
}

pub struct Game {
    pub bodies: Arc<BodyRegistry>,
    pub site: Site,
    pub builds: Builds,
    pub ships: Ships,
    pub blasts: Blasts,
    pub tactics: Tactics,
    /// Ships going about between fields: where each is is a formula of the time, the same
    /// wherever it is worked out (none in a bench).
    pub traffic: Option<Traffic>,
    /// Toward the sun (world, unit): what lights the ships and warms them.
    pub sun: DVec3,
    /// Steps taken since the start.
    pub step: u64,
    /// Where someone looks from besides the players (a camera, a script's eye): what is round
    /// them is simulated in full too. Set by whoever runs the game.
    pub watchers: Vec<DVec3>,
    /// Ships to run in full whatever their distance (the one a tool works on, a script's).
    pub awake: Vec<u64>,
    pub out: Out,
    pub say: Say,
    /// How many strikes were done here since the start (each one's dice follow from it).
    pub struck: u64,
    /// What a step works with (reused).
    seen_from: Vec<DVec3>,
    awake_now: Vec<u64>,
    people: Vec<(DVec3, f32)>,
}

impl Game {
    /// The scenario of `defs` (its structures and ships round its site), with the guided
    /// missiles and decoys of `dir` (`assets/defs`); room for `particles` particles (whoever does
    /// not draw them needs few).
    pub fn new(defs: &Defs, dir: &Path, particles: usize, wanted: impl Fn(&str) -> bool) -> Result<Game, String> {
        let bodies = defs.system.bodies.clone();
        let sc = &defs.scenario;
        let site = Site::from_def(&sc.site, &bodies)?;
        let mut blasts = Blasts::new(&defs.effects, lunar_core::missiles::Missiles::new(defs.missiles.clone()), particles)?;
        blasts.set_guided(lunar_core::guided::Flight::load(dir).map_err(|e| e.to_string())?)?;
        let effects: Vec<&str> = defs.effects.explosions.iter().map(|(id, _)| id.as_str()).collect();
        let mut builds = Builds::new(defs.structures.clone(), sc, &site, &bodies, &effects)?;
        let mut ships = Ships::new(defs.ships.clone(), defs.font.0.clone());
        Tactics::check(&ships, &blasts)?;
        for a in sc.ships.iter().filter(|a| wanted(&a.ship)) {
            ships.spawn(&mut builds, &bodies, &a.ship, site.body, site.at(a.east, a.north), a.yaw.to_radians())?;
        }
        Ok(Game {
            bodies,
            site,
            builds,
            ships,
            blasts,
            tactics: Tactics::default(),
            traffic: None,
            sun: DVec3::Y,
            step: 0,
            watchers: Vec::new(),
            awake: Vec::new(),
            out: Out::default(),
            say: Say::Alone,
            struck: 0,
            seen_from: Vec::new(),
            awake_now: Vec::new(),
            people: Vec::new(),
        })
    }

    /// The time of the game (s since it began): of the step last taken.
    pub fn time(&self) -> f64 {
        self.step as f64 * STEP
    }

    /// One step, with `players` as they ask (each one's `input`).
    pub fn tick<P: AsMut<Player>>(&mut self, players: &mut [P]) {
        let dt = STEP;
        self.out.said.clear();
        self.out.strikes.clear();
        self.out.made.clear();
        self.builds.set.sun = self.sun;
        self.mark();
        let bodies = self.bodies.clone();
        let made_from = self.builds.set.next_free();
        // as each begins: what the air pulls them with, whose rooms they are in, what they ask
        for p in players.iter_mut().map(|p| p.as_mut()).filter(|p| !p.away) {
            prepare(&self.ships, &self.builds.set, p);
            p.was = Some((p.pilot.position, p.pilot.ride.map(|r| (r.id, r.local))));
        }
        self.builds.set.begin_step();
        // what flies is flown (and what a test key asked is fired, along the first player's view)
        let (view, motion) = match players.first_mut() {
            Some(p) => {
                let p = p.as_mut();
                (p.acting_view(&self.builds.set), p.pilot.motion_in(&self.builds.set))
            }
            None => (View { eye: DVec3::ZERO, forward: DVec3::Z, up: DVec3::Y, fov_y: 1.0, near: 0.1 }, Motion::default()),
        };
        self.blasts.update(dt, &bodies, view, motion, &mut self.builds);
        // who watches: whoever looks from somewhere, and every player
        self.seen_from.clear();
        self.seen_from.extend_from_slice(&self.watchers);
        self.awake_now.clear();
        self.awake_now.extend_from_slice(&self.awake);
        self.people.clear();
        for p in players.iter_mut().map(|p| p.as_mut()).filter(|p| !p.away) {
            let pilot = &p.pilot;
            self.seen_from.push(pilot.position);
            // in full: the ships the players ride, and what whoever runs the game asks
            self.awake_now.extend(pilot.ride.map(|r| r.id));
            pilot.body_into(&mut self.people);
        }
        self.tactics.look(&mut self.ships, &self.builds, &bodies, self.traffic.as_ref(), &mut self.blasts);
        self.ships.update(dt, &mut self.builds, &bodies, &mut self.blasts.fx, self.sun, &self.seen_from, &self.awake_now, &self.people);
        // (a player's game over a server does not fire the ships' weapons: what they fire is told)
        if self.say != Say::Client {
            self.tactics.fire(&mut self.ships, &mut self.builds, &bodies, &mut self.blasts);
        }
        // a seat on something that moves takes whoever sits in it along
        for p in players.iter_mut() {
            if let Some(seat) = &mut p.as_mut().pilot.seat
                && let Some(n) = self.ships.by_structure(seat.structure)
            {
                seat.eyes = self.ships.list[n].seat_eyes(seat.index);
            }
        }
        for sh in &mut self.ships.list {
            for (about, text, level) in sh.said.drain(..) {
                self.out.said.push((sh.structure, about, text, level));
            }
        }
        // the world on, and the players with it: slice by slice among its structures, so they are
        // always of the same instant (`lunar_core::structure::schedule::Among`)
        {
            let (effects, mut flight) = self.blasts.flight();
            let at = players.first_mut().map_or(DVec3::ZERO, |p| p.as_mut().pilot.position);
            let mut crowd = Crowd(players, at);
            self.builds.update(dt, &bodies, effects, &self.seen_from, &mut [&mut crowd, &mut flight]);
        }
        self.blasts.land_rounds(&bodies, &mut self.builds);
        // bare hands on what is loose: pulled toward where each player's look holds it
        for p in players.iter_mut().map(|p| p.as_mut()).filter(|p| !p.away) {
            let view = p.acting_view(&self.builds.set);
            let motion = p.pilot.motion_in(&self.builds.set);
            p.hands.update(dt, &mut self.builds, &view, motion);
        }
        self.strike();
        self.out.made.extend(self.builds.set.list.iter().filter(|s| s.id >= made_from).map(|s| s.id));
        self.step += 1;
        if let Some(t) = &mut self.traffic {
            t.update(&bodies, self.step as f64 * STEP);
        }
    }

    /// Over a network every structure is done in one order everywhere (`Structure::shared`):
    /// the server has the say (`owned`), a player's game none (`remote`). What has no name yet is
    /// named by its id (the server's: the same in every game). Alone, nothing is marked.
    fn mark(&mut self) {
        let (owned, remote) = match self.say {
            Say::Alone => return,
            Say::Server => (true, false),
            Say::Client => (false, true),
        };
        for s in &mut self.builds.set.list {
            if s.lineage == 0 {
                s.lineage = s.id;
            }
            (s.shared, s.owned, s.remote) = (true, owned, remote);
        }
    }

    /// What was decided this step to be done to structures: done now by the server, in order,
    /// each with its dice (`Out::strikes`, for every player's game to do the same); in a
    /// player's game over a server, forgotten (the server's word comes).
    fn strike(&mut self) {
        let mut strikes = std::mem::take(&mut self.out.strikes);
        let mut list = Vec::new();
        self.builds.take_strikes(&mut list);
        if self.say == Say::Server {
            for s in list {
                let id = match s {
                    Strike::Hit { id, .. } | Strike::Blow { id, .. } => id,
                };
                self.struck += 1;
                let seed = self.struck.wrapping_mul(0x9e37_79b9_7f4a_7c15);
                let pose = self.builds.pose_of(id).to_vec();
                self.builds.strike_done(&s, seed, true, &pose);
                strikes.push((s, seed, pose));
            }
        }
        self.out.strikes = strikes;
    }

    /// One player's body stepped alone, as `tick` steps it, among the structures as they are
    /// now (they do not move): what a player's game does again for the steps since the one the
    /// server put it right at (`online`), with what was asked at each.
    pub fn step_alone(&self, p: &mut Player) {
        prepare(&self.ships, &self.builds.set, p);
        p.pilot.slice(&self.builds.set, &self.bodies, STEP);
    }

    /// Everything as it was a share `alpha` (0..1) of the way from the step before the last to
    /// the last: what is drawn between steps, all of the same instant. Undone by `restore`, which
    /// must come before the next step.
    pub fn present<P: AsMut<Player>>(&mut self, alpha: f64, players: &mut [P]) {
        self.builds.set.present(alpha);
        self.blasts.hold = true;
        // what the particles' time is ahead of the picture: what is made while it is drawn goes
        // there (`Particles::ahead`)
        self.blasts.fx.particles.ahead = (1.0 - alpha) * STEP;
        if let Some(t) = &mut self.traffic {
            t.update(&self.bodies, (self.step as f64 - (1.0 - alpha)) * STEP);
        }
        for p in players.iter_mut() {
            let p = p.as_mut();
            p.shown = None;
            let Some((pos, ride)) = p.was else { continue };
            let pilot = &mut p.pilot;
            let local = match (ride, pilot.ride.as_mut()) {
                // aboard the same as before: between where we were in it and where we are
                (Some((id, before)), Some(r)) if r.id == id => {
                    let now = r.local;
                    r.local = before.lerp(now, alpha as f32) + p.offset.1;
                    Some(now)
                }
                _ => None,
            };
            p.shown = Some((pilot.position, local));
            pilot.position = pos.lerp(pilot.position, alpha) + p.offset.0;
        }
    }

    /// Back to the last step, as `present` found it; what was let fly meanwhile, from where what
    /// it left is at the step (it was asked from where that was drawn).
    pub fn restore<P: AsMut<Player>>(&mut self, players: &mut [P]) {
        let mut held = std::mem::take(&mut self.blasts.kept);
        for l in &mut held {
            unpresent(&self.builds.set, l, players);
        }
        self.blasts.hold = false;
        self.blasts.fx.particles.ahead = 0.0;
        if let Some(t) = &mut self.traffic {
            t.update(&self.bodies, self.step as f64 * STEP);
        }
        self.builds.set.restore();
        for p in players.iter_mut() {
            let p = p.as_mut();
            let Some((pos, local)) = p.shown.take() else { continue };
            p.pilot.position = pos;
            if let (Some(l), Some(r)) = (local, p.pilot.ride.as_mut()) {
                r.local = l;
            }
        }
        let bodies = self.bodies.clone();
        for l in held.drain(..) {
            self.blasts.launch(l, &bodies, &mut self.builds);
        }
        self.blasts.kept = held;
    }
}

/// A player as a step begins for them: what the air on the move pulls them with, whose rooms
/// they are in, and what they ask (a jump is asked once and taken by the step that does it).
fn prepare(ships: &Ships, set: &Structures, p: &mut Player) {
    let pilot = &mut p.pilot;
    pilot.wind = if pilot.seat.is_some() || pilot.flying {
        DVec3::ZERO
    } else {
        let drag = if pilot.crouched() { lunar_ship::atmos::DRAG_CROUCHED } else { lunar_ship::atmos::DRAG_STANDING };
        crate::air::wind(ships, set, pilot.position, drag)
    };
    // in the air inside a ship's rooms we go with it; outside them we are on our own
    pilot.cabin = match pilot.ride {
        Some(r) => set.get(r.id).filter(|s| s.in_rooms(r.local)).map(|s| s.id),
        None => set.rooms_at(pilot.position),
    };
    pilot.begin(p.input, p.controls);
    p.input.jump = false;
}

/// A launch asked from where things are drawn (`present`), put where it is from at the step: in
/// the frame of what it left (`by`) as that is drawn and as it is; else moved as the player
/// nearest to it was moved.
fn unpresent<P: AsMut<Player>>(set: &Structures, l: &mut crate::blasts::Launch, players: &mut [P]) {
    if let Some(by) = l.by
        && let (Some(s), Some((pos, rot))) = (set.get(by), set.true_pose(by))
    {
        let local = s.rot.inverse().as_dquat() * (l.from - s.pos);
        let turn = (rot * s.rot.inverse()).as_dquat();
        l.from = pos + rot.as_dquat() * local;
        l.dir = turn * l.dir;
        l.vel = turn * l.vel;
        return;
    }
    let mut nearest: Option<(DVec3, DVec3)> = None;
    for p in players.iter_mut() {
        let p = p.as_mut();
        if let Some((pos, _)) = p.shown
            && nearest.is_none_or(|n| p.pilot.position.distance_squared(l.from) < n.1.distance_squared(l.from))
        {
            nearest = Some((pos, p.pilot.position));
        }
    }
    if let Some((pos, drawn)) = nearest {
        l.from += pos - drawn;
    }
}

/// A few bytes that say what the game is now, to compare two runs of it (the same steps on two
/// machines, a server and its clients): where every structure is (to a tenth of a millimetre)
/// and how it is turned, what is left of each and what each ship's systems keep
/// (`lunar_ship::sync::Digest`), and where each player is.
pub fn digest(game: &Game, players: &[&Player]) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    let mut eat = |v: u64| {
        for b in v.to_le_bytes() {
            h = (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3);
        }
    };
    let q = |x: f64| (x * 1e4).round() as i64 as u64;
    for s in &game.builds.set.list {
        eat(s.id);
        for v in s.pos.to_array() {
            eat(q(v));
        }
        for v in s.rot.to_array() {
            eat(q(f64::from(v)));
        }
        let ship = game.ships.list.iter().find(|sh| sh.structure == s.id);
        let d = lunar_ship::sync::Digest::of(ship, s);
        eat(u64::from(d.hash));
        for l in d.levels {
            eat(q(f64::from(l)));
        }
    }
    for p in players {
        for v in p.pilot.position.to_array() {
            eat(q(v));
        }
    }
    eat(game.step);
    h
}

/// Between two turns, a share `alpha` of the way, the short way round.
pub fn turn_between(a: Quat, b: Quat, alpha: f64) -> Quat {
    let (a, b) = (a.as_dquat(), b.as_dquat());
    let b = if a.dot(b) < 0.0 { -b } else { b };
    DQuat::slerp(a, b, alpha).normalize().as_quat()
}
