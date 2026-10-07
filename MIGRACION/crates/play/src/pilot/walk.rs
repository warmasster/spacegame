//! The body among the structures: three spheres one over another (`PlayerDef::cuerpo`) that the
//! parts of any structure push out — a floor holds them up, a wall stops them, what is no higher
//! than a step is stepped on to. What is touched stops the body against itself (`passing`): its
//! own speed there is what counts, however fast it and we go in the world.
use super::Pilot;
use glam::DVec3;
use lunar_core::structure::set::Structures;

// The slack of the tests below (m unless said). Not tuning: what counts as touching, in, or on.
/// At or under the ground by this much is on it.
pub(super) const ON_GROUND: f64 = 0.01;
/// A ray that meets a face this near its start started inside it.
const INSIDE: f64 = 2e-3;
/// Rays down for what is underfoot start this far over the feet.
const OVER_FEET: f64 = 0.1;
/// Deeper than this in something (the feet's own sphere in what is not floor): no room there.
const IN_THE_WAY: f64 = 0.02;
/// A step's top is looked for from this far over the highest one taken, and is one if it is at
/// least this high; the foot goes that much over it, and what would then be in the body deeper
/// than `ON_A_STEP` leaves no room on it.
const STEP_MARGIN: f64 = 0.02;
const STEP_LIFT: f64 = 0.01;
const ON_A_STEP: f64 = 0.03;
/// Standing up: the spheres tried a little thinner, something in them deeper than this that is
/// not floor (its normal less than this much up) is in the way.
const HEAD_THIN: f32 = 0.95;
const HEAD_IN: f64 = 0.01;
const HEAD_FLOOR: f64 = 0.3;
/// Going at something at least this much (cosine with its face, the other way) is walking into it.
const INTO: f64 = -0.2;
/// The eyes this far under standing height: crouched.
const CROUCHED: f64 = 0.05;
/// Times the body is pushed out of what it is in, at most, per step.
const PASSES: usize = 3;

impl Pilot {
    /// Where the feet would stand at `at` (world): the floor under it no further down than `drop`
    /// (a deck, a step, the ground), if a body standing there touches nothing.
    pub fn room(&mut self, set: &Structures, at: DVec3, up: DVec3, drop: f64) -> Option<DVec3> {
        let (shape, over) = (self.def.cuerpo, self.def.escalon);
        let deck = set.raycast_solid(at + up * over, -up, over + drop, shape.menudo).filter(|(_, t, n)| *t > INSIDE && n.dot(up) > shape.apoyo).map(|(_, t, _)| at + up * (over - t));
        let feet = deck.or_else(|| {
            // no deck: the ground, if there is any there and it is that near under it
            let below = self.bodies.get(self.bodies.field(at).ground?).altitude(at);
            (-over..=drop).contains(&below).then(|| at - up * below)
        })?;
        for (k, h) in shape.esferas.iter().enumerate() {
            self.contacts.clear();
            set.sphere_contacts(feet + up * *h, shape.radio, shape.menudo, &mut self.contacts);
            // (the feet's own touch the floor they are on)
            if self.contacts.iter().any(|c| f64::from(c.depth) > IN_THE_WAY && (k > 0 || c.normal.dot(up) < shape.apoyo)) {
                return None;
            }
        }
        Some(feet)
    }

    /// Push the body's spheres out of every structure part; the structure stood on, if any.
    /// `going`: the way we go over the ground (level, unit or zero); `afoot`: on our feet, so
    /// what is in the way of a foot and no higher than a step is stepped on to, not bumped into.
    pub(super) fn collide(&mut self, set: &Structures, up: DVec3, going: DVec3, afoot: bool) -> Option<usize> {
        let shape = self.def.cuerpo;
        let mut on = None;
        let feet = self.eye_h;
        let spheres = self.spheres();
        let step = self.def.escalon;
        let mut tried = false;
        for _ in 0..PASSES {
            let mut moved = false;
            for (k, h) in spheres.iter().enumerate() {
                let c = self.position - up * (feet - h);
                self.contacts.clear();
                set.sphere_contacts(c, shape.radio, shape.menudo, &mut self.contacts);
                for i in 0..self.contacts.len() {
                    let ct = self.contacts[i];
                    let n = ct.normal;
                    let depth = f64::from(ct.depth);
                    let rise = n.dot(up);
                    // (what is touched stops us against itself: how fast it goes, as our own
                    // speeds are counted)
                    let its = self.passing(set, ct.structure);
                    let its_up = its.dot(up);
                    if rise > shape.apoyo && k == 0 {
                        // ground: straight up, so slopes hold you
                        let lift = depth / rise;
                        if lift <= step {
                            self.position += up * lift;
                            self.vertical_velocity = self.vertical_velocity.max(its_up);
                            on = Some(ct.structure);
                            moved = true;
                            continue;
                        }
                    }
                    // in the way of a foot: a step, if there is somewhere to stand on it
                    if k == 0 && afoot && !tried && rise.abs() <= shape.apoyo && !self.crouched() {
                        let side = (n - up * rise).normalize_or_zero();
                        if side != DVec3::ZERO && going.dot(side) < INTO {
                            tried = true;
                            if let Some((lift, s)) = self.step_on(set, up, -side) {
                                self.position += up * lift;
                                self.vertical_velocity = self.vertical_velocity.max(its_up);
                                self.sink += lift;
                                self.stepped = lift;
                                on = Some(s);
                                moved = true;
                                break;
                            }
                        }
                    }
                    if rise < -shape.apoyo {
                        // a ceiling
                        self.position += n * depth;
                        self.vertical_velocity = self.vertical_velocity.min(its_up);
                    } else {
                        // a wall: sideways only; what slid us into it stops
                        let side = (n - up * rise).normalize_or(n);
                        self.position += side * depth;
                        self.drift -= side * (self.drift - its).dot(side).min(0.0);
                    }
                    moved = true;
                }
            }
            if !moved {
                break;
            }
        }
        on
    }

    /// How fast structure `i` of `set` goes where we are (m/s), as our own speeds are counted:
    /// aboard, from what carries us (itself: not at all); in the air, in the world.
    pub(super) fn passing(&self, set: &Structures, i: usize) -> DVec3 {
        let s = &set.list[i];
        match self.ride {
            Some(r) if r.id == s.id => DVec3::ZERO,
            Some(r) => s.velocity_at(self.position) - set.get(r.id).map_or(DVec3::ZERO, |c| c.velocity_at(self.position)),
            None => s.velocity_at(self.position),
        }
    }

    /// What is under the feet within a step: a deck (its structure) if one is no further down
    /// than the ground (`gap` m under them), and how far down it is (m).
    pub(super) fn under(&self, set: Option<&Structures>, up: DVec3, gap: f64) -> (Option<usize>, f64) {
        let shape = self.def.cuerpo;
        let feet = self.position - up * self.eye_h;
        let deck = set.and_then(|set| set.raycast_solid(feet + up * OVER_FEET, -up, self.def.escalon + OVER_FEET, shape.menudo)).filter(|h| h.1 > INSIDE && h.2.dot(up) > shape.apoyo).map(|h| (h.0, h.1 - OVER_FEET));
        match deck {
            Some((i, d)) if d <= gap => (Some(i), d),
            _ => (None, gap),
        }
    }

    /// Something no higher than a step `ahead` (level, unit) of the feet, with room to stand on
    /// it: how far up the feet go to be on it (m) and the structure it is of.
    fn step_on(&mut self, set: &Structures, up: DVec3, ahead: DVec3) -> Option<(f64, usize)> {
        let shape = self.def.cuerpo;
        let step = self.def.escalon;
        let feet = self.position - up * self.eye_h;
        // its top, where the foot would be set down: the highest of a near and a far try
        let mut top: Option<(f64, usize)> = None;
        for past in shape.pisada {
            let from = feet + ahead * (f64::from(shape.radio) + past) + up * (step + STEP_MARGIN);
            if let Some((s, t, n)) = set.raycast_solid(from, -up, step + STEP_MARGIN, shape.menudo) {
                let h = step + STEP_MARGIN - t;
                // (a ray that starts inside it: it is higher than a step)
                if t > INSIDE && n.dot(up) > shape.apoyo && h > STEP_MARGIN && top.is_none_or(|x| h > x.0) {
                    top = Some((h, s));
                }
            }
        }
        let (h, s) = top?;
        let lift = h + STEP_LIFT;
        // room for the body: where it is, raised, and where it will be standing
        let heights = self.spheres();
        for at in [feet + up * lift, feet + up * lift + ahead * (f64::from(shape.radio) + shape.pisada[0])] {
            for hh in heights {
                self.probe.clear();
                set.sphere_contacts(at + up * hh, shape.radio, shape.menudo, &mut self.probe);
                if self.probe.iter().any(|c| f64::from(c.depth) > ON_A_STEP && c.normal.dot(up) <= shape.apoyo) {
                    return None;
                }
            }
        }
        Some((lift, s))
    }

    /// The body as spheres in the world (none in free flight or seated): what a door, a ramp or
    /// a hoist stops at.
    pub fn body(&self) -> Vec<(DVec3, f32)> {
        let mut out = Vec::new();
        self.body_into(&mut out);
        out
    }

    /// The same, added to `out` (what a step does for every player, with nothing new made).
    pub fn body_into(&self, out: &mut Vec<(DVec3, f32)>) {
        if self.flying || self.seat.is_some() {
            return;
        }
        let feet = self.position - self.up * self.eye_h;
        out.extend(self.spheres().iter().map(|h| (feet + self.up * *h, self.def.cuerpo.radio)));
    }

    /// Crouched (or on the way down or up).
    pub fn crouched(&self) -> bool {
        self.eye_h < self.def.eye_height - CROUCHED
    }

    /// The body's spheres (heights over the feet) as tall as it stands now.
    pub(super) fn spheres(&self) -> [f64; 3] {
        let shape = self.def.cuerpo;
        let k = ((self.def.eye_height - self.eye_h) / (self.def.eye_height - shape.agachado.ojos)).clamp(0.0, 1.0);
        std::array::from_fn(|i| shape.esferas[i] + (shape.agachado.esferas[i] - shape.esferas[i]) * k)
    }

    /// Room over the head to stand up: the standing body's top sphere touches nothing.
    pub(super) fn headroom(&mut self, set: &Structures, up: DVec3) -> bool {
        let shape = self.def.cuerpo;
        let feet = self.position - up * self.eye_h;
        for h in [shape.esferas[1], shape.esferas[2]] {
            self.contacts.clear();
            set.sphere_contacts(feet + up * h, shape.radio * HEAD_THIN, shape.menudo, &mut self.contacts);
            if self.contacts.iter().any(|c| f64::from(c.depth) > HEAD_IN && c.normal.dot(up) < HEAD_FLOOR) {
                return false;
            }
        }
        true
    }
}
