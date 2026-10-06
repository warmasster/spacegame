//! The stress-test world as data (structure of arrays): a fleet of ships, parked or flying circuits,
//! and a crowd of NPCs wandering on the ground. Updates run in parallel (rayon); no allocation per
//! frame. Positions are f64 (world); the renderer converts them to f32 near its origin. Every
//! object keeps the body it was placed on: its vertical and its ground come from there.
use crate::{
    body::{Body, BodyId, BodyRegistry},
    noise::Random,
    scenario::{CrowdDef, FleetDef, Layout, SiteDef, lerp_range},
};
use glam::{DQuat, DVec3, Quat, Vec3};
use rayon::prelude::*;

/// A tangent frame on a body's ground.
#[derive(Clone, Copy, Debug)]
pub struct Site {
    pub body: BodyId,
    pub radius: f64,
    pub dir: DVec3,
    pub east: DVec3,
    pub north: DVec3,
}

impl Site {
    /// The frame where `dir` is the way out from the centre of body `body` (`b`): north is
    /// the body's own (at its poles, where there is none, any level way).
    pub fn new(body: BodyId, b: &Body, dir: DVec3) -> Site {
        let dir = dir.normalize();
        let north = b.turn_from(dir);
        Site { body, radius: b.radius, dir, east: north.cross(dir), north }
    }

    pub fn from_def(d: &SiteDef, bodies: &BodyRegistry) -> Result<Site, String> {
        let id = bodies.find(&d.body).ok_or_else(|| format!("site: unknown body '{}'", d.body))?;
        let b = bodies.get(id);
        let base = Site::new(id, b, DVec3::from_array(d.dir));
        Ok(Site::new(id, b, base.at(d.east, d.north)))
    }

    /// Unit direction (from the body's centre) x m east and z m north of the site.
    pub fn at(&self, x: f64, z: f64) -> DVec3 {
        (self.dir * self.radius + self.east * x + self.north * z).normalize()
    }

    /// Point `k` of `n` on a sunflower spiral round the site: (east, north) m and the angle.
    pub fn spiral(layout: &Layout, k: usize, n: usize) -> (f64, f64, f64) {
        let r = layout.inner + (layout.radius - layout.inner).max(10.) * ((k as f64 + 0.5) / n as f64).sqrt();
        let a = k as f64 * 2.39996;
        (a.cos() * r, a.sin() * r, a)
    }
}

/// Orientation with +Y along `up` and +Z along `fwd` (projected).
pub fn basis(up: DVec3, fwd: DVec3) -> Quat {
    let z = (fwd - up * fwd.dot(up)).normalize_or(up.any_orthonormal_vector());
    let x = up.cross(z);
    DQuat::from_mat3(&glam::DMat3::from_cols(x, up, z)).as_quat()
}

#[derive(Clone, Copy, Debug)]
pub struct Circuit {
    pub center: DVec3,
    pub up: DVec3,
    pub east: DVec3,
    pub north: DVec3,
    pub radius: f64,
    pub altitude: f64,
    pub omega: f64,
    pub phase: f64,
    pub figure_eight: bool,
}

impl Circuit {
    fn at(&self, t: f64) -> (DVec3, DVec3) {
        let a = self.phase + self.omega * t;
        let (s, c) = a.sin_cos();
        let (x, z, dx, dz) = if self.figure_eight {
            // lemniscate of Gerono
            (c, s * c, -s, c * c - s * s)
        } else {
            (c, s, -s, c)
        };
        let p = self.center + self.up * self.altitude + (self.east * x + self.north * z) * self.radius;
        (p, self.east * dx + self.north * dz)
    }
}

pub struct Fleet {
    /// Index into the fleet definition's model list.
    pub kind: Vec<u8>,
    pub body: Vec<BodyId>,
    pub pos: Vec<DVec3>,
    pub rot: Vec<Quat>,
    /// Ships `parked..` fly their circuit (index `k - parked`).
    pub parked: usize,
    pub circuits: Vec<Circuit>,
    bank: (f64, f64),
}

impl Fleet {
    /// `n` ships round the site, `flying` of them spread evenly in the air.
    pub fn new(bodies: &BodyRegistry, site: &Site, def: &FleetDef, layout: &Layout, n: usize, flying: usize) -> Fleet {
        let flying = flying.min(n);
        let kinds = def.models.len().max(1);
        let cd = def.circuit;
        let placed: Vec<(u8, BodyId, DVec3, Quat, Option<Circuit>)> = (0..n)
            .into_par_iter()
            .map(|k| {
                let (x, z, a) = Site::spiral(layout, k, n);
                let dir = site.at(x, z);
                let ground = bodies.get(site.body).above_ground(dir, 0.0);
                let body = bodies.dominant(ground);
                let heading = site.east * a.cos() + site.north * a.sin();
                let rot = basis(dir, heading);
                let flying = n > 0 && (k + 1) * flying / n > k * flying / n;
                let circuit = flying.then(|| {
                    let mut rng = Random(k as u32 * 7919 + 13);
                    let local = Site::new(body, bodies.get(body), dir);
                    Circuit {
                        center: ground,
                        up: dir,
                        east: local.east,
                        north: local.north,
                        radius: lerp_range(cd.radius, rng.next_f64()),
                        altitude: lerp_range(cd.altitude, rng.next_f64()),
                        omega: lerp_range(cd.omega, rng.next_f64()) * if rng.next_f64() < 0.5 { -1. } else { 1. },
                        phase: rng.next_f64() * std::f64::consts::TAU,
                        figure_eight: rng.next_f64() < cd.figure_eight,
                    }
                });
                ((k % kinds) as u8, body, ground, rot, circuit)
            })
            .collect();
        // parked first, flying last: the moving range is one contiguous upload
        let mut fleet = Fleet { kind: Vec::new(), body: Vec::new(), pos: Vec::new(), rot: Vec::new(), parked: 0, circuits: Vec::new(), bank: (cd.bank, cd.bank_max) };
        for pass in [false, true] {
            for (kind, body, p, rot, c) in &placed {
                if c.is_some() == pass {
                    fleet.kind.push(*kind);
                    fleet.body.push(*body);
                    fleet.pos.push(*p);
                    fleet.rot.push(*rot);
                    if let Some(c) = c {
                        fleet.circuits.push(*c);
                    }
                }
            }
            if !pass {
                fleet.parked = fleet.kind.len();
            }
        }
        fleet
    }

    pub fn len(&self) -> usize {
        self.kind.len()
    }

    pub fn is_empty(&self) -> bool {
        self.kind.is_empty()
    }

    pub fn update(&mut self, bodies: &BodyRegistry, t: f64) {
        let parked = self.parked;
        let (bank, bank_max) = self.bank;
        self.pos[parked..].par_iter_mut().zip(self.rot[parked..].par_iter_mut()).zip(self.circuits.par_iter().zip(self.body[parked..].par_iter())).for_each(|((p, r), (c, body))| {
            let (pos, vel) = c.at(t);
            *p = pos;
            let up = bodies.get(*body).up(pos);
            // bank into the turn a little
            let (_, ahead) = c.at(t + 0.5);
            let turn = (ahead - vel).dot(vel.cross(up).normalize_or_zero());
            *r = basis(up, vel) * Quat::from_rotation_z((turn * bank).clamp(-bank_max, bank_max) as f32);
        });
    }
}

/// NPC animation state (matches the baked clips).
pub const ANIM_IDLE: u8 = 0;
pub const ANIM_WALK: u8 = 1;

pub struct Crowd {
    pub body: Vec<BodyId>,
    pub pos: Vec<DVec3>,
    pub home: Vec<DVec3>,
    pub fwd: Vec<Vec3>,
    pub anim: Vec<u8>,
    /// Clip phase (cycles).
    pub phase: Vec<f32>,
    pub timer: Vec<f32>,
    pub turn: Vec<f32>,
    pub rng: Vec<u32>,
    /// Ground height at the last sample, its slope along the path, metres since.
    pub ground: Vec<[f64; 3]>,
    def: CrowdDef,
}

impl Crowd {
    pub fn new(bodies: &BodyRegistry, site: &Site, def: &CrowdDef, layout: &Layout, n: usize) -> Crowd {
        let homes: Vec<(BodyId, DVec3, f64)> = (0..n)
            .into_par_iter()
            .map(|k| {
                let (x, z, _) = Site::spiral(layout, k, n);
                let dir = site.at(x + def.offset[0], z + def.offset[1]);
                let b = bodies.get(site.body);
                let h = b.height(dir);
                let p = b.center + dir * (b.radius + h);
                (bodies.dominant(p), p, h)
            })
            .collect();
        let mut c = Crowd {
            body: Vec::with_capacity(n),
            pos: Vec::with_capacity(n),
            home: Vec::with_capacity(n),
            fwd: Vec::with_capacity(n),
            anim: vec![ANIM_IDLE; n],
            phase: Vec::with_capacity(n),
            timer: Vec::with_capacity(n),
            turn: vec![0.; n],
            rng: Vec::with_capacity(n),
            ground: Vec::with_capacity(n),
            def: def.clone(),
        };
        for (k, (body, p, h)) in homes.into_iter().enumerate() {
            let mut rng = Random((k as u32).wrapping_mul(2654435761) ^ 0xa11ce);
            let b = bodies.get(body);
            let local = Site::new(body, b, p - b.center);
            let a = rng.next_f64() * std::f64::consts::TAU;
            c.body.push(body);
            c.pos.push(p);
            c.home.push(p);
            c.fwd.push((local.east * a.cos() + local.north * a.sin()).as_vec3());
            c.phase.push(rng.next_f64() as f32);
            c.timer.push((rng.next_f64() * def.idle_time[1]) as f32);
            c.rng.push(rng.0);
            c.ground.push([h, 0., 0.]);
        }
        c
    }

    pub fn len(&self) -> usize {
        self.pos.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pos.is_empty()
    }

    pub fn update(&mut self, bodies: &BodyRegistry, dt: f64) {
        let d = &self.def;
        (
            (self.pos.par_iter_mut(), self.home.par_iter(), self.body.par_iter()),
            self.fwd.par_iter_mut(),
            self.anim.par_iter_mut(),
            self.phase.par_iter_mut(),
            self.timer.par_iter_mut(),
            self.turn.par_iter_mut(),
            self.rng.par_iter_mut(),
            self.ground.par_iter_mut(),
        )
            .into_par_iter()
            .for_each(|((pos, home, body), fwd, anim, phase, timer, turn, seed, ground)| {
                let b: &Body = bodies.get(*body);
                let mut rng = Random(*seed);
                *timer -= dt as f32;
                if *timer <= 0. {
                    // alternate walking and standing; walks pick a new bearing
                    let walk = *anim == ANIM_IDLE;
                    *anim = if walk { ANIM_WALK } else { ANIM_IDLE };
                    *timer = lerp_range(if walk { d.walk_time } else { d.idle_time }, rng.next_f64()) as f32;
                    *turn = ((rng.next_f64() - 0.5) * d.wander) as f32;
                }
                let up = b.up(*pos);
                if *anim == ANIM_WALK {
                    let to_home = *home - *pos;
                    let mut f = fwd.as_dvec3();
                    let steer = if to_home.length() > d.leash {
                        // turn back toward home
                        f.cross(to_home).dot(up).signum() * d.home_turn
                    } else {
                        f64::from(*turn)
                    };
                    f = DQuat::from_axis_angle(up, steer * dt) * f;
                    f = (f - up * f.dot(up)).normalize();
                    *fwd = f.as_vec3();
                    let step = d.walk_speed * dt;
                    let moved = *pos + f * step;
                    let dir = (moved - b.center).normalize();
                    ground[2] += step;
                    if ground[2] >= d.resample {
                        let h = b.height(dir);
                        ground[1] = (h - ground[0]) / ground[2];
                        ground[0] = h;
                        ground[2] = 0.;
                    }
                    let h = ground[0] + ground[1] * ground[2];
                    *pos = b.center + dir * (b.radius + h);
                    *phase = (*phase + (step / d.stride) as f32).fract();
                } else {
                    *phase = (*phase + (dt * d.idle_rate) as f32).fract();
                }
                *seed = rng.0;
            });
    }
}
