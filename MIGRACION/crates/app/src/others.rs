//! The other players, as bodies drawn here from what the server's snapshots tell of each
//! (`PlayerState`, `lunar_play::online`). Each is made the first time it is told of, moved from
//! what is told and animated here, and forgotten when it is told of no more.
use crate::{
    body::{Body, Stance},
    rig::Rig,
    ships::Ships,
};
use glam::{DVec3, Vec3};
use lunar_core::{anim::BodyScene, body::BodyRegistry, structure::set::Structures};
use lunar_net::{PlayerState, flag};

/// What the others' bodies are made from: the suit's rig, as measured once (each body a copy),
/// and its mesh in the renderer, whole (seen from outside).
pub struct BodySource {
    pub rig: Rig,
    pub whole: Option<u16>,
}

/// Another player, as drawn here.
pub struct Other {
    pub id: u32,
    body: Body,
    pub stance: Stance,
    /// Riding a ship: where they were in its frame last frame (their speed over its deck is
    /// taken from how that changes: what is told of their speed is not fresh while they ride).
    local: Option<Vec3>,
    /// Told of this frame (the ones not told of are gone).
    here: bool,
}

#[derive(Default)]
pub struct Others {
    pub list: Vec<Other>,
}

/// The way a body faces from its turn (rad from `north` toward the right) where `up` is up.
pub fn facing(north: DVec3, up: DVec3, yaw: f64) -> DVec3 {
    north * yaw.cos() + north.cross(up) * yaw.sin()
}

impl Others {
    /// Where each other's eyes are as last drawn, and who they are (what their name goes over).
    pub fn eyes(&self) -> impl Iterator<Item = (DVec3, u32)> + '_ {
        self.list.iter().map(|o| (o.stance.eye, o.id))
    }

    /// The others as `states` tell them, `ahead` s old (each on its own carried on by its speed:
    /// what rides or floats by a ship is placed by our copy of it), drawn into `out`. `net(k)`:
    /// our structure for the thing a state names `k`.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(&mut self, states: &[(u32, PlayerState)], ahead: f64, dt: f64, net: impl Fn(u64) -> Option<u64>, source: &BodySource, set: &Structures, bodies: &BodyRegistry, ships: &Ships, out: &mut BodyScene) {
        for o in &mut self.list {
            o.here = false;
        }
        for (id, p) in states {
            let by = p.ride.and_then(&net).and_then(|id| set.get(id));
            // (riding a ship, or floating by one: where it is in our copy of the ship)
            let mut eye = by.map_or(p.pos + p.vel.as_dvec3() * ahead, |s| s.to_world(p.local));
            let ride = by.filter(|_| p.flags & flag::BESIDE == 0);
            let body = bodies.get(usize::from(p.body).min(bodies.len().saturating_sub(1)) as lunar_core::body::BodyId);
            let mut up = body.up(eye);
            let mut ahead_way = facing(body.turn_from(up), up, f64::from(p.yaw));
            // seated: the seat's eyes and the way it faces, up the ship's up
            let seat = p.seat.and_then(|(k, i)| {
                let structure = net(k)?;
                let (s, n) = (set.get(structure)?, ships.by_structure(structure)?);
                let d = &ships.list[n].kind.seats.get(usize::from(i))?.def;
                let h = d.rumbo.to_radians();
                Some((s.to_world(Vec3::from_array(d.ojos)), (s.rot * Vec3::Y).as_dvec3(), (s.rot * Vec3::new(h.sin(), 0.0, h.cos())).as_dvec3()))
            });
            if let Some((at, u, a)) = seat {
                (eye, up, ahead_way) = (at, u, a);
            }
            let known = self.list.iter().position(|o| o.id == *id);
            // (over a ship's deck: how fast they go is how fast their place in it changes)
            let vel = match (ride, known.and_then(|k| self.list[k].local)) {
                (Some(s), Some(was)) if dt > 1e-6 => (s.rot * ((p.local - was) / dt as f32)).as_dvec3().clamp_length_max(12.0),
                (Some(_), None) => DVec3::ZERO,
                _ => p.vel.as_dvec3(),
            };
            let stance = Stance {
                eye,
                up,
                ahead: (ahead_way - up * ahead_way.dot(up)).normalize_or(up.any_orthonormal_vector()),
                eye_h: if seat.is_some() { 1.2 } else { f64::from(p.eye_h) },
                vel,
                grounded: p.flags & flag::GROUNDED != 0,
                g: lunar_core::structure::weight::felt(bodies.field(eye).pull, eye, ride, true, ride.is_some_and(|s| s.in_rooms(s.to_local(eye)))).length(),
                ride: ride.map(|s| s.id),
                seated: seat.is_some(),
                inside: false,
                own_eyes: false,
            };
            let k = match known {
                Some(k) => k,
                None => {
                    self.list.push(Other { id: *id, body: Body::new(source.rig.clone(), source.whole, source.whole), stance, local: None, here: true });
                    self.list.len() - 1
                }
            };
            let o = &mut self.list[k];
            (o.stance, o.here, o.local) = (stance, true, ride.map(|_| p.local));
            o.body.update(dt, &o.stance, set, bodies, [None, None]);
            o.body.show(out, &o.stance);
        }
        self.list.retain(|o| o.here);
    }
}
