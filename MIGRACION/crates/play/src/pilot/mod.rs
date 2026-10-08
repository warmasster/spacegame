//! The player: walking, the suit's pack and free flight.
//!
//! Which way is up, what one weighs and what one may stand on are asked every step of where one
//! is now and of what carries one now — never of where one started, of how fast one goes or of
//! what happened before:
//! - carried by a structure (standing on it, or in the air in its rooms), that structure is the
//!   frame: one turns as it turns, and weighs what one weighs aboard it
//!   (`lunar_core::structure::weight`) — its own gravity where it makes any, toward its decks,
//!   however it lies and wherever it is; else what is left of the world's pull as it goes;
//! - on one's own, one is pulled as the world pulls there (`BodyRegistry::field`);
//! - where nothing weighs, the way up is the one one came with: nothing turns it.
//!
//! The body's own way up eases toward the one its weight gives, the faster the more it weighs
//! (`cuerpo.enderezar`), so going from one of these to another is never seen as a jump. How one
//! faces is a frame of one's own (`up`, `fore`) carried along as the way up turns: no axis of the
//! world is in it, so there is no place where it flips.
//!
//! Walking, the body is three spheres that the parts of any structure push out (decks, ramps,
//! bulkheads, consoles: `walk`). What you stand on carries you: aboard a ship you keep your place
//! in its frame wherever it goes and however it turns. Seated, you are fixed to the seat and look
//! round in the ship's frame. In the air the pack's jets take over the walking (`pack`).
//!
//! The player has no clock of their own. The world steps them as it steps everything that lives
//! among its structures (`lunar_core::structure::schedule::Among`): slice by slice, right after
//! the structures of that slice. So the player and every structure are always of the same
//! instant — to the eye and to what is bumped into, at any speed, anywhere, at any frame rate —
//! and nothing here knows or makes up for how fast anything goes. What is touched stops you
//! against itself, however fast it goes (`walk::passing`). Every number of how the body moves is
//! the scenario's data (`PlayerDef`: `cuerpo`, `mochila`, `linterna`).
use glam::{DQuat, DVec3, Quat, Vec3};
use lunar_core::view::View;
use lunar_core::{
    body::{BodyId, BodyRegistry},
    scenario::PlayerDef,
    scene::Site,
    structure::{
        schedule::{Among, coasted},
        set::{Contact, Structures},
        state::Structure,
        weight,
    },
};
use std::sync::Arc;

#[cfg(test)]
mod frames;
mod pack;
#[cfg(test)]
mod shots;
mod state;
#[cfg(test)]
mod tests;
mod walk;

pub use pack::Hold;
pub use state::{Garbled, Summary};

#[derive(Default, Clone, Copy, Debug, PartialEq)]
pub struct Input {
    pub forward: f64,
    pub side: f64,
    pub vertical: f64,
    /// Shift
    pub run: bool,
    /// Alt
    pub boost: bool,
    pub jump: bool,
    /// C held: crouched (lower, slower, the air takes less of you).
    pub crouch: bool,
    /// Q and E floating with the pack on: roll left (−1) and right (1).
    pub roll: f64,
}

/// Player-tunable controls (Esc menu).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Controls {
    /// Flight speed factor while Shift is held.
    pub run_factor: f64,
    /// Extra flight factor while Alt is held (on top of Shift).
    pub boost_factor: f64,
    /// How much the mouse turns the view (1: as the suit's data says), and up for down.
    pub mouse: f64,
    pub invert_y: bool,
    /// How loud everything is (0..1.5).
    pub volume: f32,
}

impl Controls {
    pub fn new(def: &PlayerDef) -> Controls {
        Controls { run_factor: def.flight.run_factor, boost_factor: def.flight.boost_factor, mouse: 1.0, invert_y: false, volume: 0.8 }
    }

    /// Flight speed factor for the keys held.
    pub fn factor(&self, input: &Input) -> f64 {
        (if input.run { self.run_factor } else { 1.0 }) * if input.boost { self.boost_factor } else { 1.0 }
    }
}

/// The structure that carries the player.
#[derive(Clone, Copy, Debug)]
pub struct Ride {
    pub id: u64,
    /// The eye in its frame: where we are, however fast it goes.
    pub local: Vec3,
    /// Its turn at the last step: how much it turned since turns the player too.
    rot: Quat,
}

/// A seat: where the eyes go and where you stand when you get up (structure frame).
#[derive(Clone, Copy, Debug)]
pub struct Seat {
    pub structure: u64,
    /// Which seat of its ship.
    pub index: usize,
    pub eyes: Vec3,
    /// Way it faces (rad round +Y, 0 = +Z).
    pub heading: f32,
    pub exit: Vec3,
}

/// A place to stand at on leaving a seat (structure frame): where the feet go, how far under it
/// the floor may be (m) and the way to face there (rad round +Y, 0 = +Z), if it says one.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Place {
    pub feet: Vec3,
    pub drop: f32,
    pub heading: Option<f32>,
}

/// Nearer the way up than this (rad) the body is upright: it is set to it (the eye moves a
/// couple of micrometres).
const ALIGNED: f64 = 1e-6;

/// `v` laid level where `up` is the way up (unit); any level way if it has no level part.
fn level(v: DVec3, up: DVec3) -> DVec3 {
    (v - up * v.dot(up)).try_normalize().unwrap_or_else(|| up.any_orthonormal_vector())
}

pub struct Pilot {
    pub position: DVec3,
    /// The look: turned `yaw` rad from the way `fore` points, round the way up toward the
    /// right, and raised `pitch`. (Seated: the head's turn from the way the seat faces.)
    pub yaw: f64,
    pub pitch: f64,
    pub flying: bool,
    /// Base flight speed (m/s); Shift and Alt multiply it.
    pub speed: f64,
    pub grounded: bool,
    /// The body whose surface is nearest (whose ground is drawn under the player), and the one
    /// whose ground may be stood on: none past every body's reach.
    pub body: BodyId,
    pub ground: Option<BodyId>,
    /// The body's own frame: its way up, and the level way its turn is counted from. Both are
    /// carried along as the way up turns and as what carries us turns; nothing of them comes
    /// from an axis of the world.
    up: DVec3,
    fore: DVec3,
    /// What we weigh now (world, m/s²: how we speed up for it, in what our speeds are counted
    /// in).
    weight: DVec3,
    vertical_velocity: f64,
    def: PlayerDef,
    bodies: Arc<BodyRegistry>,
    spawn: DVec3,
    pub ride: Option<Ride>,
    pub seat: Option<Seat>,
    /// Seated, the look goes all the way round (the view from outside); else the head turns as
    /// far as a head does.
    pub free_look: bool,
    contacts: Vec<Contact>,
    /// (another, for what is looked at while those are gone through)
    probe: Vec<Contact>,
    /// What the keys ask and what the menu says, for the slices the world gives us this frame
    /// (`begin`).
    input: Input,
    controls: Controls,
    /// What the air on the move does to us (m/s², world): set by the owner each frame.
    pub wind: DVec3,
    /// How fast we go level (m/s; aboard, in what carries us; else in the world): what we had on
    /// leaving what we stood on, what the pack and the air gave us. The boots brake it.
    drift: DVec3,
    /// How fast we went at the last step (m/s; aboard, in what carries us; else in the world).
    vel: DVec3,
    /// Height of the eye over the feet now (m): standing, or crouched (`cuerpo.agachado`).
    eye_h: f64,
    /// Helmet lamps on (L).
    pub lamps: bool,
    /// The jet pack: switched on (off, Space is only a jump), its gas (0..1) and how hard it is
    /// pushing now (0..1, for whoever shows it).
    pub pack_on: bool,
    pub fuel: f64,
    pub jet: f64,
    /// What the pack pushes us with now (world), in shares of its full push up: up 1, down
    /// `mochila.abajo`, sideways and steadying what they burn. Its jets leave the other way
    /// (`plumes`).
    pub push: DVec3,
    /// How we were walking when we last stood on something (m/s): kept as we leave it.
    walked: DVec3,
    /// Off the ground for real — on purpose (a jump, a push of the pack) or for longer than a
    /// step off a sill — and for how long (s): only then do the pack's side jets take over the
    /// walking.
    aloft: bool,
    air: f64,
    /// The ship whose rooms we are in, if any (set by the owner each frame): in the air inside
    /// it we go with it, boarded before or not; outside its rooms, in the air, we are on our own.
    pub cabin: Option<u64>,
    /// The pack steadies us when the keys are let go (default); off, it lets us coast.
    pub steady: bool,
    /// What it steadies us to (`pack`).
    hold: Hold,
    /// How far under where they are the eyes are shown (m): a step is taken at once and the eyes
    /// come up after it.
    sink: f64,
    /// A landing taken on the knees: how far the eyes are down for it (m) and how fast they go
    /// (a spring: down with the blow, up again in a moment).
    dip: f64,
    dip_v: f64,
    /// How we met the ground in the frame's slices (`begin` forgets it): how fast we came down
    /// on it (m/s; 0 unless we landed), and how high a step we took up (m; 0 if none). For
    /// whoever animates the body and for the sound of it.
    pub landed: f64,
    pub stepped: f64,
}

impl Pilot {
    pub fn new(bodies: Arc<BodyRegistry>, site: &Site, def: PlayerDef) -> Self {
        let spawn = bodies.get(site.body).above_ground(site.dir, def.eye_height);
        Pilot::at(bodies, def, spawn)
    }

    fn at(bodies: Arc<BodyRegistry>, def: PlayerDef, spawn: DVec3) -> Pilot {
        let here = bodies.field(spawn);
        let up = here.up().unwrap_or(DVec3::Y);
        // (the look's turn at the start is counted from the north of the ground stood on)
        let fore = here.ground.map_or_else(|| up.any_orthonormal_vector(), |g| level(bodies.get(g).turn_from(up), up));
        Pilot {
            position: spawn,
            yaw: def.yaw,
            pitch: def.pitch,
            flying: false,
            speed: def.flight.speed,
            grounded: true,
            body: here.nearest,
            ground: here.ground,
            up,
            fore,
            weight: here.pull,
            vertical_velocity: 0.,
            def,
            bodies,
            spawn,
            ride: None,
            seat: None,
            free_look: false,
            contacts: Vec::new(),
            probe: Vec::new(),
            input: Input::default(),
            controls: Controls::new(&def),
            wind: DVec3::ZERO,
            drift: DVec3::ZERO,
            vel: DVec3::ZERO,
            eye_h: def.eye_height,
            lamps: false,
            pack_on: false,
            fuel: 1.0,
            jet: 0.0,
            push: DVec3::ZERO,
            walked: DVec3::ZERO,
            aloft: false,
            air: 0.0,
            cabin: None,
            steady: true,
            hold: Hold::Still,
            sink: 0.0,
            dip: 0.0,
            dip_v: 0.0,
            landed: 0.0,
            stepped: 0.0,
        }
    }

    /// Standing with the feet at `feet` (world), `up` the way up there: still, on nothing yet.
    pub fn put(&mut self, feet: DVec3, up: DVec3) {
        self.position = feet + up * self.eye_h;
        self.set_up(up);
        (self.vertical_velocity, self.drift, self.vel, self.hold) = (0.0, DVec3::ZERO, DVec3::ZERO, Hold::Still);
        (self.ride, self.seat, self.grounded, self.sink) = (None, None, true, 0.0);
        (self.dip, self.dip_v) = (0.0, 0.0);
    }

    /// Standing with the feet at `at` of structure `id` (its frame), the way up the one we
    /// weigh by there, and still to it however it goes: aboard it if that is in its rooms. It is
    /// taken as it is at the world's moment: one stepped now and then, or asleep, is on along its
    /// path from where it was last stepped (`coasted`; else, at orbital speed, hundreds of metres
    /// behind it), and is caught up there as whoever is aboard wakes it.
    pub fn put_on(&mut self, set: &Structures, id: u64, at: Vec3) {
        let Some(s) = set.get(id) else { return };
        let (pos, vel, rot) = coasted(s, self.bodies.field(s.pos).pull, set.now - s.clock);
        // (the way up as it is in its own frame where it was last stepped, turned as it is now)
        let up = (rot * s.rot.inverse()).as_dquat() * self.up_aboard(s, s.to_world(at));
        self.put(pos + (rot * at).as_dvec3(), up);
        // (aboard, how we go is how we go in it: still; beside it, as fast as it goes there)
        let local = rot.inverse() * (self.position - pos).as_vec3();
        if s.in_rooms(local) {
            self.ride = Some(Ride { id, local, rot });
            return;
        }
        let com = pos + (rot * s.com).as_dvec3();
        let v = vel + s.spin.cross((self.position - com).as_vec3()).as_dvec3();
        (self.vertical_velocity, self.drift) = (v.dot(up), v - up * v.dot(up));
        self.hold = Hold::Beside { id, vel, gains: DVec3::ZERO };
    }

    /// Still to structure `id` where we are (set there by a script, by a ship under way): as
    /// fast as it goes there.
    pub fn still_to(&mut self, set: &Structures, id: u64) {
        let Some(s) = set.get(id) else { return };
        let (v, up) = (s.velocity_at(self.position), self.up);
        (self.vertical_velocity, self.drift) = (v.dot(up), v - up * v.dot(up));
        self.hold = Hold::Beside { id, vel: s.vel, gains: DVec3::ZERO };
    }

    /// Back to the spawn, as at the start.
    pub fn reset(&mut self) {
        *self = Pilot::at(self.bodies.clone(), self.def, self.spawn);
    }

    pub fn directions(&self) -> (DVec3, DVec3, DVec3) {
        let up = self.up;
        let forward = self.fore * self.yaw.cos() + self.fore.cross(up) * self.yaw.sin();
        let right = forward.cross(up);
        (forward * self.pitch.cos() + up * self.pitch.sin(), right, up)
    }

    /// The way up is `up` now: how we face goes over to it by the shortest turn.
    fn set_up(&mut self, up: DVec3) {
        self.fore = level(DQuat::from_rotation_arc(self.up, up) * self.fore, up);
        self.up = up;
    }

    /// What carries us turned by `turn`: we turn with it, the way up, how we face and how we
    /// go in it.
    fn turned(&mut self, turn: Quat) {
        let turn = turn.as_dquat();
        self.up = (turn * self.up).normalize();
        self.fore = level(turn * self.fore, self.up);
        self.drift = turn * self.drift;
    }

    /// What carries us (`s`) is somewhere else than when we last looked: we are where we were
    /// in it, turned as it turned.
    fn taken_along(&mut self, s: &Structure) {
        let Some(mut r) = self.ride else { return };
        self.position = s.to_world(r.local);
        self.turned(s.rot * r.rot.inverse());
        r.rot = s.rot;
        self.ride = Some(r);
    }

    /// What carries us was put somewhere else at a stroke (a script's doing): we go with it
    /// now, not at the world's next step.
    pub fn moved_with(&mut self, set: &Structures) {
        if let Some(s) = self.ride.and_then(|r| set.get(r.id)) {
            self.taken_along(s);
        }
    }

    /// The body rights itself toward `want`, the way up that a weight of `g` m/s² gives, over
    /// `dt` s: by the angle still to go, no faster than it can, and as much slower as the weight
    /// is small (`cuerpo.enderezar`). Whether it is upright now (its way up is `want`, exactly).
    fn right(&mut self, want: DVec3, g: f64, dt: f64) -> bool {
        let e = self.def.cuerpo.enderezar;
        let cross = self.up.cross(want);
        let angle = cross.length().atan2(self.up.dot(want));
        let hard = (g / e.peso).min(1.0);
        if angle <= ALIGNED {
            if hard >= 1.0 {
                self.set_up(want);
            }
            return hard >= 1.0;
        }
        let step = (angle * (1.0 - (-e.ritmo * dt).exp())).min(e.giro * dt).min(angle) * hard;
        // (head over heels, any level axis does: over the side)
        let axis = cross.try_normalize().unwrap_or(self.fore);
        let turn = DQuat::from_axis_angle(axis, step);
        self.up = (turn * self.up).normalize();
        self.fore = level(turn * self.fore, self.up);
        false
    }

    /// The way up for whoever stands at `at` (world) carried by `s`: the one their weight there
    /// gives; weighing nothing, the structure's own.
    fn up_aboard(&self, s: &Structure, at: DVec3) -> DVec3 {
        let w = weight::felt(self.bodies.field(at).pull, at, Some(s), true, s.in_rooms(s.to_local(at)));
        if w.length() > self.def.cuerpo.sin_peso { -w.normalize() } else { (s.rot * Vec3::Y).as_dvec3() }
    }

    /// The look turned to `heading` rad from north toward east and raised `pitch`: north of the
    /// ground under us; where there is none (past every body's reach, at a pole), from the way
    /// our own turn is counted from.
    pub fn look_by(&mut self, heading: f64, pitch: f64) {
        if let Some(n) = self.ground.map(|g| self.bodies.get(g).north_at(self.up).0).filter(|n| *n != DVec3::ZERO) {
            self.fore = level(n, self.up);
        }
        (self.yaw, self.pitch) = (heading, pitch);
    }

    /// What we weigh now (world, m/s²), and how much of it presses on what is under our feet.
    pub fn weight(&self) -> DVec3 {
        self.weight
    }

    pub fn weighs(&self) -> f64 {
        (-self.weight.dot(self.up)).max(0.0)
    }

    /// The way the body faces as a turn (rad, toward the right) from the level way everyone
    /// counts from at the nearest body (`Body::turn_from`): how it is told to who has no frame
    /// of ours to say it in (another player's game).
    pub fn told_heading(&self) -> f64 {
        let b = self.bodies.get(self.body);
        let up = b.up(self.position);
        let (h, north) = (self.heading(), b.turn_from(up));
        h.dot(north.cross(up)).atan2(h.dot(north))
    }

    /// Radians the look turns per count of the mouse.
    pub fn mouse(&self) -> f64 {
        self.def.mouse
    }

    pub fn look(&mut self, x: f64, y: f64) {
        // floating where nothing weighs with the pack on, the mouse turns the whole body, every
        // way and without end (`floating`)
        if self.floating() {
            self.fold_look();
            let (fore, up) = (self.fore, self.up);
            let right = fore.cross(up);
            let turn = DQuat::from_axis_angle(right, -y * self.def.mouse) * DQuat::from_axis_angle(-up, x * self.def.mouse);
            self.up = (turn * up).normalize();
            self.fore = level(turn * fore, self.up);
            return;
        }
        let head = self.def.cuerpo.cabeza;
        self.yaw += x * self.def.mouse;
        self.pitch = (self.pitch - y * self.def.mouse).clamp(-head.arriba, head.arriba);
        if self.seat.is_some() && !self.free_look {
            // seated the head turns, the body does not
            self.yaw = self.yaw.clamp(-head.sentado, head.sentado);
        }
    }

    /// Floating with the pack on where nothing weighs (in the air, on our own): the body turns
    /// whole with the mouse, rolls with Q and E, and keeps however it is turned once the pack is
    /// off. Where something weighs the body rights itself toward it, the pack on or not.
    pub fn floating(&self) -> bool {
        self.pack_on && self.def.mochila.is_some() && self.seat.is_none() && !self.flying && !self.grounded && self.weight.length() <= self.def.cuerpo.sin_peso
    }

    /// The body's own frame: its way up and the level way its turn is counted from (what the
    /// mouse turns while it floats: `look`).
    /// How far up and down the head looks at most (rad).
    pub fn look_limit(&self) -> f64 {
        self.def.cuerpo.cabeza.arriba
    }

    pub fn body_frame(&self) -> (DVec3, DVec3) {
        (self.up, self.fore)
    }

    /// The body's frame as another game says it (a player's own, whose mouse turned it).
    pub fn set_body_frame(&mut self, up: DVec3, fore: DVec3) {
        if let Some(up) = up.try_normalize() {
            self.up = up;
            self.fore = level(fore, up);
        }
    }

    /// The look made the body's: it faces where we look, its way up the look's, the head
    /// straight on it.
    fn fold_look(&mut self) {
        if self.yaw == 0.0 && self.pitch == 0.0 {
            return;
        }
        let (forward, right, _) = self.directions();
        let up = right.cross(forward).normalize();
        (self.up, self.fore, self.yaw, self.pitch) = (up, level(forward, up), 0.0, 0.0);
    }

    /// Rolled `angle` rad to the right about where we look (floating).
    fn roll(&mut self, angle: f64) {
        self.fold_look();
        let turn = DQuat::from_axis_angle(self.fore, angle);
        self.up = (turn * self.up).normalize();
        self.fore = level(self.fore, self.up);
    }

    /// Sit on `seat` of its structure: eyes on the seat, looking the way it faces.
    pub fn sit(&mut self, set: &Structures, seat: Seat) {
        let Some(s) = set.get(seat.structure) else { return };
        self.seat = Some(seat);
        self.ride = Some(Ride { id: seat.structure, local: seat.eyes, rot: s.rot });
        self.position = s.to_world(seat.eyes);
        self.flying = false;
        (self.vertical_velocity, self.vel, self.hold) = (0.0, DVec3::ZERO, Hold::Still);
        self.yaw = 0.0;
        self.pitch = 0.0;
    }

    /// Get up from the seat: standing at the first of `places` with a floor under it and room for
    /// the body (the seat's own exit if none has), facing the way the place says, or the way you
    /// looked.
    pub fn stand(&mut self, set: &Structures, places: &[Place]) {
        let Some(seat) = self.seat.take() else { return };
        let Some(s) = set.get(seat.structure) else {
            self.ride = None;
            return;
        };
        let (fwd, _) = self.seat_look(&seat);
        self.eye_h = self.def.eye_height;
        self.free_look = false;
        // (which way is up at each place is what one weighs there aboard it)
        let found = places.iter().find_map(|p| {
            let at = s.to_world(p.feet);
            let up = self.up_aboard(s, at);
            self.room(set, at, up, f64::from(p.drop)).map(|feet| (feet, p.heading))
        });
        let (feet, heading) = found.unwrap_or_else(|| (s.to_world(seat.exit), None));
        let up = self.up_aboard(s, feet);
        self.position = feet + up * self.def.eye_height;
        self.ride = Some(Ride { id: seat.structure, local: s.to_local(self.position), rot: s.rot });
        (self.vertical_velocity, self.drift, self.vel, self.sink) = (0.0, DVec3::ZERO, DVec3::ZERO, 0.0);
        let fwd = heading.map_or(fwd, |h| Vec3::new(h.sin(), 0.0, h.cos()));
        let f = (s.rot * fwd).as_dvec3();
        let most = self.def.cuerpo.cabeza.arriba;
        // facing that way: the turn is counted from it
        (self.up, self.fore, self.yaw) = (up, level(f, up), 0.0);
        self.pitch = f.dot(up).clamp(-1.0, 1.0).asin().clamp(-most, most);
    }

    /// Seated: the look (forward, up) in the structure frame.
    fn seat_look(&self, seat: &Seat) -> (Vec3, Vec3) {
        let h = seat.heading - self.yaw as f32;
        let p = self.pitch as f32;
        let (sh, ch, sp, cp) = (h.sin(), h.cos(), p.sin(), p.cos());
        (Vec3::new(sh * cp, sp, ch * cp), Vec3::new(-sh * sp, cp, -ch * sp))
    }

    /// Looks at a point of the world.
    pub fn look_at(&mut self, target: DVec3) {
        let d = (target - self.position).normalize_or_zero();
        if d == DVec3::ZERO {
            return;
        }
        self.yaw = d.dot(self.fore.cross(self.up)).atan2(d.dot(self.fore));
        self.pitch = d.dot(self.up).clamp(-1.0, 1.0).asin();
    }

    /// Mouse wheel notches change the base flight speed.
    pub fn wheel(&mut self, notches: f64) {
        let f = &self.def.flight;
        self.speed = (self.speed * f.wheel_step.powf(notches)).clamp(f.min_speed, f.max_speed);
    }

    /// How fast we go (m/s; aboard, in what carries us; else in the world), by the last step.
    pub fn velocity(&self) -> DVec3 {
        self.vel
    }

    /// How fast we go in the world (m/s), aboard or not: what leaves us there (the pack's gas)
    /// starts with it.
    pub fn velocity_in(&self, set: &Structures) -> DVec3 {
        self.vel + self.ride.and_then(|r| set.get(r.id)).map_or(DVec3::ZERO, |s| s.velocity_at(self.position))
    }

    pub fn motion_in(&self, set: &Structures) -> lunar_core::structure::motion::Motion {
        lunar_core::structure::motion::Motion { at: self.position, vel: self.velocity_in(set), spin: self.ride.and_then(|ride| set.get(ride.id)).map_or(DVec3::ZERO, |structure| structure.spin.as_dvec3()) }
    }

    /// Speed up (m/s) on foot or in the air.
    pub fn vertical_speed(&self) -> f64 {
        self.vertical_velocity
    }

    pub fn toggle_flight(&mut self) {
        self.flying = !self.flying;
        self.vertical_velocity = 0.;
    }

    /// A frame begins: what the keys ask and what the menu says, for the slices the world will
    /// give us in it; what happened in the last frame's is forgotten.
    pub fn begin(&mut self, input: Input, controls: Controls) {
        (self.input, self.controls) = (input, controls);
        (self.landed, self.stepped) = (0.0, 0.0);
    }

    /// `dt` s on (more than none), among the structures as they are now: what the world's slice
    /// does to us (`Among`). `set` none: on the bare ground.
    pub fn step(&mut self, dt: f64, input: Input, controls: &Controls, set: Option<&Structures>) {
        if dt <= 0.0 {
            return;
        }
        let shape = self.def.cuerpo;
        (self.landed, self.stepped) = (0.0, 0.0);
        self.push = DVec3::ZERO;
        // the eyes come up after a step taken at once, and after a landing taken on the knees
        // (critically damped, stepped exactly: `lunar_core::anim::spring`)
        self.sink *= (-shape.tras_escalon * dt).exp();
        if self.sink.abs() < 1e-4 {
            self.sink = 0.0;
        }
        let y = 2.0 * std::f64::consts::LN_2 / shape.rodillas.vida;
        let (j1, e) = (self.dip_v + self.dip * y, (-y * dt).exp());
        self.dip = e * (self.dip + j1 * dt);
        self.dip_v = e * (self.dip_v - j1 * y * dt);
        // what carries us took us along (and turned us with it: it is our frame)
        if let Some(r) = self.ride {
            match set.and_then(|set| set.get(r.id)) {
                Some(s) => self.taken_along(s),
                None => {
                    self.ride = None;
                    self.seat = None;
                }
            }
        }
        if let Some(seat) = self.seat {
            if let (Some(r), Some(s)) = (&mut self.ride, set.and_then(|set| set.get(seat.structure))) {
                r.local = seat.eyes;
                self.position = s.to_world(seat.eyes);
                // (what holds where the seat is now, for whoever asks of us while we sit)
                let here = self.bodies.field(self.position);
                (self.body, self.ground) = (here.nearest, here.ground);
                self.weight = weight::felt(here.pull, self.position, Some(s), true, s.in_rooms(seat.eyes));
            }
            self.grounded = true;
            (self.vertical_velocity, self.vel, self.jet) = (0.0, DVec3::ZERO, 0.0);
            return;
        }
        let from = self.position;
        let bodies = self.bodies.clone();
        // what holds where we are now: what pulls, whose ground is under us (if any), and what
        // we weigh for it and for what carries us
        let here = bodies.field(self.position);
        (self.body, self.ground) = (here.nearest, here.ground);
        let ground = here.ground.map(|g| bodies.get(g));
        // (aboard what carries us: in its rooms or not by where we are in it. In the air on our
        // own in a ship's rooms: aboard it too, though we do not go with it yet — and there by
        // what whoever owns us found as the frame began, when it and we were of one instant:
        // now it has moved and we have not)
        let aboard = |id: u64| set.and_then(|set| set.get(id));
        let (by, carried, inside) = match self.ride.and_then(|r| aboard(r.id).map(|s| (s, r.local))) {
            Some((s, local)) => (Some(s), true, s.in_rooms(local)),
            None => (self.cabin.and_then(aboard), false, true),
        };
        let weighs = if self.flying { DVec3::ZERO } else { weight::felt(here.pull, self.position, by, carried, inside) };
        self.weight = weighs;
        // (how we go is one speed: as the way up turns it is said again by it, whole — `drift`
        // its level part, `vertical_velocity` the rest — so going round a body, however fast,
        // or righting ourselves, turns nothing of it)
        let ours = self.drift + self.up * self.vertical_velocity;
        // (upright: all of the weight is along the body's own way up, and none of it level)
        let mut upright = false;
        if self.flying {
            // free flight: the way up follows the pull, where there is one
            if let Some(want) = here.up() {
                let k = 1.0 - (-self.def.flight.up_follow * dt).exp();
                let up = (self.up + (want - self.up) * k).try_normalize().unwrap_or(want);
                self.set_up(up);
            }
        } else {
            // the way up is the way we weigh; weighing nothing, the one we came with
            let was = self.up;
            if let Some(want) = (-weighs).try_normalize() {
                upright = self.right(want, weighs.length(), dt);
            }
            if self.up != was {
                self.vertical_velocity = ours.dot(self.up);
                self.drift = ours - self.up * self.vertical_velocity;
            }
        }
        if input.roll != 0.0
            && self.floating()
            && let Some(j) = self.def.mochila
        {
            self.roll(input.roll.clamp(-1.0, 1.0) * j.alabeo * dt);
            self.vertical_velocity = ours.dot(self.up);
            self.drift = ours - self.up * self.vertical_velocity;
        }
        let (forward, right, up) = self.directions();
        let tangent = (forward - up * forward.dot(up)).normalize_or(right.cross(up));
        // what of our weight presses on what is under our feet; too little of it and the boots
        // hold nothing: we float
        let g = if upright { weighs.length() } else { (-weighs.dot(up)).max(0.0) };
        let heavy = g > shape.sin_peso;
        if !heavy && !self.flying {
            self.grounded = false;
            self.aloft = true;
        }
        let was_grounded = self.grounded;
        if self.flying {
            let wish = forward * input.forward + right * input.side + up * input.vertical;
            // no cap on the product: Shift x Alt can reach tens of km/s
            self.position += wish.normalize_or_zero() * self.speed * controls.factor(&input) * dt;
        } else {
            let wish = (tangent * input.forward + right * input.side).normalize_or_zero();
            // (the pack's jets push where we look: up or down too if we look so)
            let aim = (forward * input.forward + right * input.side).normalize_or_zero();
            let pace = if self.crouched() {
                shape.agachado.paso
            } else if input.run {
                self.def.run_speed
            } else {
                self.def.walk_speed
            };
            // (how fast we go, level, from what the pack keeps us to, before we do anything
            // about it)
            let apart = self.aside(up).length();
            // on our feet, walking; in the air with the pack on, its jets
            self.legs_or_jets(dt, &input, up, g, wish, aim, pace);
            // our weight, whichever way it is (while the body rights itself, not all of it is
            // along its own way up)
            if upright {
                self.vertical_velocity -= g * dt;
            } else {
                let down = weighs.dot(up);
                self.vertical_velocity += down * dt;
                self.drift += (weighs - up * down) * dt;
            }
            // the air on the move: it lifts you off the deck, and slides you over it past what
            // your boots hold
            let a = self.wind;
            let lift = a.dot(up);
            self.vertical_velocity += lift * dt;
            self.drift += (a - up * lift) * dt;
            if self.grounded {
                // on your feet: the boots' friction and the legs stepping against it brake what
                // speed you came down with over what you stand on; against a gale there is no
                // bracing, only the boots
                let gale = (a - up * lift).length() > shape.vendaval * g;
                let hold = if gale { shape.vendaval * g } else { self.def.agarre * g + self.def.frenada } * dt;
                let side = self.aside(up);
                let l = side.length();
                self.drift -= if l <= hold { side } else { side * (hold / l) };
            }
            // no faster than `deriva` from what we stand on or keep to; in the air, by our own
            // doing only: if it is that which leaves us, it leaves us
            let most = if self.grounded { shape.deriva } else { apart.max(shape.deriva) };
            let side = self.aside(up);
            self.drift -= side - side.clamp_length_max(most);
            self.position += (up * self.vertical_velocity + self.drift) * dt;
        }
        // (from here we are where we are at this instant, as the structures are: whatever is
        // asked of them about us is asked now, not before we moved)
        // crouching and standing up (if there is room over the head): the feet stay put
        let crouch = input.crouch && !self.flying;
        let mut want = if crouch { shape.agachado.ojos } else { self.def.eye_height };
        if want > self.eye_h + 1e-9 && set.is_some_and(|set| !self.headroom(set, up)) {
            want = self.eye_h;
        }
        let rise = (want - self.eye_h).clamp(-shape.agachado.ritmo * dt, shape.agachado.ritmo * dt);
        self.eye_h += rise;
        self.position += up * rise;
        // how fast we come down, and which way we go over the ground, before anything stops us
        let coming = self.vertical_velocity;
        let going = self.position - from;
        let going = (going - up * going.dot(up)).normalize_or_zero();
        let rising = self.vertical_velocity > 1e-6 || input.jump || (input.vertical > 0.0 && self.pack_on);
        // the ground under the feet, where there is any: the way out of it there, and how far
        // under it the feet are (m; over it: less than nothing)
        let under_ground = |p: &Pilot| {
            ground.map(|b| {
                let feet = p.position - up * p.eye_h;
                let dir = b.up(feet);
                (dir, b.radius + b.height(dir) - (feet - b.center).length())
            })
        };
        // on the ground: at it or under it; going up (a jump, the pack) only if under it
        let mut on_ground = false;
        if let Some((dir, deep)) = under_ground(self)
            && deep >= -if self.vertical_velocity > 0.0 { 0.0 } else { walk::ON_GROUND }
        {
            on_ground = true;
            self.position += dir * deep;
            self.vertical_velocity = self.vertical_velocity.max(0.);
        }
        let mut on = match set {
            Some(set) if !self.flying => self.collide(set, up, going, was_grounded || on_ground),
            _ => None,
        };
        // the ground dropped away under a foot on it (a step down, a ramp, a slope): the legs
        // reach down to it, as far as a step, and we stay on our feet
        let (mut reaching, mut reached) = (false, None);
        if !self.flying && was_grounded && !on_ground && on.is_none() && !rising {
            let over = under_ground(self);
            let gap = over.map_or(f64::INFINITY, |(_, deep)| -deep);
            let under = self.under(set, up, gap);
            if under.1 <= self.def.escalon {
                let down = under.1.min(shape.piernas * dt).max(0.0);
                self.position -= up * down;
                // (still to what is under the feet: a deck under way goes as it goes)
                self.vertical_velocity = match (under.0, set) {
                    (Some(i), Some(set)) => self.passing(set, i).dot(up),
                    _ => 0.0,
                };
                (reaching, reached) = (true, under.0);
                if under.0.is_none()
                    && under.1 - down < walk::ON_GROUND
                    && let Some((dir, deep)) = under_ground(self)
                {
                    self.position += dir * deep;
                    on_ground = true;
                }
                if let Some(set) = set {
                    on = self.collide(set, up, going, false);
                }
            }
        }
        // on our feet only where there is weight on them: what we weigh on what is under them
        // (on something that falls away as fast as we fall, nothing)
        let pressed = match (on.or(reached), set) {
            (Some(i), Some(set)) if !on_ground => {
                let s = &set.list[i];
                (-weight::felt(here.pull, self.position, Some(s), true, s.in_rooms(s.to_local(self.position))).dot(up)).max(0.0)
            }
            _ => g,
        };
        self.grounded = (pressed > shape.sin_peso || self.flying) && (on_ground || on.is_some() || reaching);
        if self.grounded && !was_grounded {
            // (on to something that moves: how fast we came down on it)
            let under = match (on, set) {
                (Some(i), Some(set)) if !on_ground => self.passing(set, i).dot(up),
                _ => 0.0,
            };
            let falling = (under - coming).max(0.0);
            self.landed = falling;
            // the knees take it
            self.dip_v += (falling * shape.rodillas.hundimiento).min(shape.rodillas.golpe);
        }
        if self.grounded {
            (self.aloft, self.air) = (false, 0.0);
        } else {
            self.air += dt;
            self.aloft |= self.def.mochila.is_some_and(|j| self.air > j.despegue);
        }
        // what carries us now: what we stand on; in the air, the ship whose rooms we are in
        // (whether we ever stood in it or flew into it just now)
        let ride = match (on, set) {
            (Some(i), Some(set)) => {
                let s = &set.list[i];
                Some(Ride { id: s.id, local: s.to_local(self.position), rot: s.rot })
            }
            _ if (on_ground && heavy) || self.flying => None,
            (None, Some(set)) => self.cabin.and_then(|id| {
                let s = set.get(id)?;
                Some(Ride { id, local: s.to_local(self.position), rot: s.rot })
            }),
            _ => None,
        };
        // (as our speeds were counted while we moved: aboard, in what carried us)
        self.vel = (self.position - from) / dt;
        // off what carried us, its speed there is ours now, and the pack keeps us beside it; on
        // to something, we move with it
        let (was, is) = (self.ride.map(|r| r.id), ride.map(|r| r.id));
        if was != is
            && let Some(set) = set
        {
            let at = self.position;
            if let Some(s) = was.and_then(|id| set.get(id)) {
                let v = s.velocity_at(at);
                self.vertical_velocity += v.dot(up);
                self.drift += v - up * v.dot(up);
                self.vel += v;
                self.hold = Hold::Beside { id: s.id, vel: s.vel, gains: DVec3::ZERO };
            }
            if let Some(s) = is.and_then(|id| set.get(id)) {
                let v = s.velocity_at(at);
                self.vertical_velocity -= v.dot(up);
                self.drift -= v - up * v.dot(up);
                self.vel -= v;
                self.hold = Hold::Still;
            }
        }
        if (on_ground && heavy) || self.flying {
            self.hold = Hold::Still;
        }
        // down on our feet with speed: what the boots hold of it as they take the blow (their
        // friction times how fast we came down on them); the rest the legs brake, step by step
        if self.landed > 0.0 {
            let hold = self.def.agarre * self.landed;
            let l = self.drift.length();
            self.drift = if l <= hold { DVec3::ZERO } else { self.drift * (1.0 - hold / l) };
        }
        self.ride = ride;
        // in the air on our own: what the pack keeps us to, looked at again from where we are
        // now (the structures are of this instant, and so are we only once we have moved)
        if let (None, false, false, Some(set)) = (self.ride, self.grounded, self.flying, set) {
            self.refer(set, dt);
        }
    }

    /// The helmet lamps' light, if on: from the brow, the way you look.
    pub fn lamp(&self, view: &View) -> Option<lunar_core::props::Lamp> {
        if !self.lamps {
            return None;
        }
        let l = self.def.linterna;
        let pos = view.eye + view.up * l.en[0] + view.forward * l.en[1];
        Some(lunar_core::props::Lamp { pos, color: l.color, range: l.alcance, dir: view.forward.as_vec3(), cone: l.cono, inside: self.ride.is_some() && !self.flying })
    }

    /// Where the eyes are shown (world).
    pub fn eye(&self) -> DVec3 {
        self.position - self.up * self.sunk()
    }

    /// How far under where they are the eyes are shown (after a step taken at once, a landing).
    fn sunk(&self) -> f64 {
        self.sink + self.dip.clamp(0.0, self.def.cuerpo.rodillas.tope)
    }

    /// How high the shown eyes are over the feet (after a step taken at once they are still
    /// coming up).
    pub fn eye_over_feet(&self) -> f64 {
        self.eye_h - self.sunk()
    }

    /// The way the body faces (level, unit): where one looks, on the ground's plane.
    pub fn heading(&self) -> DVec3 {
        let (forward, right, up) = self.directions();
        (forward - up * forward.dot(up)).normalize_or(up.cross(right))
    }

    /// Where the feet are (world), and the way up.
    pub fn feet(&self) -> (DVec3, DVec3) {
        (self.position - self.up * self.eye_h, self.up)
    }

    pub fn view(&self) -> View {
        let (forward, _, up) = self.directions();
        View { eye: self.eye(), forward, up, fov_y: self.def.fov.to_radians(), near: self.def.near }
    }

    /// The view aboard what carries us, where that is now (None when nothing does): seated, the
    /// look is the ship's.
    pub fn view_aboard(&self, set: &Structures) -> Option<View> {
        let r = self.ride?;
        let s = set.get(r.id)?;
        let eye = s.to_world(r.local) - self.up * self.sunk();
        let (forward, up) = match &self.seat {
            Some(seat) => {
                let (f, u) = self.seat_look(seat);
                ((s.rot * f).as_dvec3(), (s.rot * u).as_dvec3())
            }
            None => {
                let (f, _, u) = self.directions();
                (f, u)
            }
        };
        Some(View { eye, forward, up, fov_y: self.def.fov.to_radians(), near: self.def.near })
    }

    /// How high the feet are over the nearest ground, however far that is (m).
    pub fn altitude(&self) -> f64 {
        self.bodies.get(self.body).altitude(self.position) - self.def.eye_height
    }
}

/// The world steps the player with everything that lives among its structures: with what the
/// keys asked as the frame began (`Pilot::begin`), and keeping what happened in any of the
/// frame's slices.
impl Among for Pilot {
    fn slice(&mut self, set: &Structures, _: &BodyRegistry, dt: f64) {
        let (landed, stepped) = (self.landed, self.stepped);
        let (input, controls) = (self.input, self.controls);
        self.step(dt, input, &controls, Some(set));
        // (a jump is asked once)
        self.input.jump = false;
        (self.landed, self.stepped) = (self.landed.max(landed), self.stepped.max(stepped));
    }
}
