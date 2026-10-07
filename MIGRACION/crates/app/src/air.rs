//! What the air of a decompressing ship looks like and does to whoever is in it (the web client's
//! decompression.ts on lunar-ship's atmosphere): vapour and ice crystals pouring out of every
//! breach and dust racing to it inside, the room fogging as the air left behind cools, a plate
//! creaking under more pressure than it holds, the burst when a room blows; the pull of the flow
//! on the player (inside, and in the plumes outside) and the shake.
use crate::ships::Ships;
use glam::{DVec3, Vec3};
use lunar_core::{body::BodyRegistry, effects::Effects, structure::set::Structures};
use lunar_ship::{
    atmos::{self, Vent},
    kind::SPACE,
};
use std::collections::HashMap;

/// Ships farther than this (m) show no air.
const SHOW_REACH: f64 = 200.0;

pub struct AirFx {
    vapour: Option<u8>,
    ice: Option<u8>,
    motes: Option<u8>,
    sparks: Option<u8>,
    /// Emission left over from the last frame, per (ship, opening or room).
    carry: HashMap<(u64, u64), f32>,
    /// Bursts already shown, per ship: (compartment, seconds left when seen).
    seen: HashMap<u64, Vec<(usize, f64)>>,
    rng: u64,
}

/// Two unit vectors across `d`.
fn basis(d: Vec3) -> (Vec3, Vec3) {
    let e = if d.y.abs() > 0.9 { Vec3::X } else { Vec3::Y };
    let e1 = e.cross(d).normalize_or(Vec3::X);
    (e1, d.cross(e1).normalize_or(Vec3::Z))
}

/// An outlet this damaged (0..1) arcs.
const SPARKS_FROM: f32 = 0.45;

impl AirFx {
    pub fn new(fx: &Effects) -> AirFx {
        AirFx { vapour: fx.style("vaho"), ice: fx.style("escarcha"), motes: fx.style("motas"), sparks: fx.style("chispas"), carry: HashMap::new(), seen: HashMap::new(), rng: 0x2545_f491_4f6c_dd1d }
    }

    fn rnd(&mut self) -> f32 {
        self.rng ^= self.rng >> 12;
        self.rng ^= self.rng << 25;
        self.rng ^= self.rng >> 27;
        (self.rng.wrapping_mul(0x2545_f491_4f6c_dd1d) >> 40) as f32 / (1u64 << 24) as f32
    }

    /// Whole particles owed for `key` at `rate` per second this frame.
    fn owed(&mut self, key: (u64, u64), rate: f32, dt: f32) -> usize {
        let n = self.carry.get(&key).copied().unwrap_or(0.0) + rate * dt;
        let whole = n.floor();
        self.carry.insert(key, n - whole);
        whole as usize
    }

    /// This frame's effects; how much the view should shake (0..1).
    pub fn frame(&mut self, dt: f32, ships: &Ships, set: &Structures, bodies: &BodyRegistry, fx: &mut Effects, eye: DVec3) -> f32 {
        let mut shake = 0.0;
        for sh in &ships.list {
            let Some(s) = set.get(sh.structure) else { continue };
            if s.to_world(s.center).distance(eye) - f64::from(s.radius) > SHOW_REACH {
                continue;
            }
            let id = sh.structure;
            let a = &sh.atmos;
            let mine = atmos::room_of(&sh.kind, s.to_local(eye));
            for v in &a.vents {
                self.jet(id, v, sh, s, bodies, fx, dt);
            }
            // the air left behind cools as it expands: the room fogs, whoever is in it feels it
            for c in 0..a.shock.len() {
                let k = a.shock[c] as f32;
                if k <= 0.02 {
                    continue;
                }
                self.fog(id, c, k, sh, s, bodies, fx, dt);
                if mine == Some(c) {
                    shake += k * dt * 3.0;
                }
            }
            // plates past what they hold creak and spit
            for &p in a.yielding() {
                let n = self.owed((id, 1 << 40 | u64::from(p)), 9.0, dt);
                if n == 0 {
                    continue;
                }
                let part = &s.parts[p as usize];
                let at = s.to_world(part.center);
                let d = at.distance(eye);
                if d < 6.0 {
                    shake += 0.06 * (1.0 - d / 6.0) as f32;
                }
                self.creak(at, part.radius, s.rot * (part.center - s.center).normalize_or(Vec3::Y), bodies, fx);
            }
            // outlets and junction boxes badly hurt, and still fed, arc: the worse, the more often
            if sh.panels.panels.iter().any(|p| p.powered) {
                for &p in &sh.kind.sparks {
                    let part = &s.parts[p as usize];
                    let hurt = part.damage();
                    if !part.alive || hurt < SPARKS_FROM {
                        continue;
                    }
                    if self.owed((id, 2 << 40 | u64::from(p)), 0.5 + 3.0 * (hurt - SPARKS_FROM), dt) > 0 {
                        let at = s.to_world(part.center);
                        let _ = fx.explode_scaled("chispas", bodies, bodies.dominant(at), at, 0.6);
                    }
                }
            }
            // a room that just blew: the burst
            let seen = self.seen.entry(id).or_default();
            seen.retain(|(c, _)| a.bursts.iter().any(|b| b.0 == *c));
            let fresh: Vec<(usize, f64)> = a.bursts.iter().filter(|b| !seen.iter().any(|x| x.0 == b.0)).map(|b| (b.0, b.1)).collect();
            for (c, k) in fresh {
                self.seen.entry(id).or_default().push((c, k));
                if let Some(v) = a.vents.iter().filter(|v| v.up == c).max_by(|x, y| x.mdot.total_cmp(&y.mdot)) {
                    let at = s.to_world(v.at);
                    self.bang(at, s.rot * v.dir, k as f32, bodies, fx);
                    shake += (k as f32) * (1.0 - (at.distance(eye) / 14.0) as f32).max(0.0);
                }
            }
        }
        shake.min(1.0)
    }

    /// Vapour and ice out of one opening, and inside, dust racing to it.
    #[allow(clippy::too_many_arguments)]
    fn jet(&mut self, id: u64, v: &Vent, sh: &lunar_ship::Ship, s: &lunar_core::structure::state::Structure, bodies: &BodyRegistry, fx: &mut Effects, dt: f32) {
        let (Some(vapour), Some(ice), Some(motes)) = (self.vapour, self.ice, self.motes) else { return };
        let key = v.part.map_or_else(|| (v.at.x * 100.0) as i64 as u64 ^ ((v.at.z * 100.0) as i64 as u64) << 20, u64::from);
        let rate = (40.0 * (v.mdot as f32).powf(0.6)).min(500.0) * v.fade as f32;
        let count = self.owed((id, key), rate, dt);
        if count == 0 {
            return;
        }
        let vac = v.down == SPACE;
        let at = s.to_world(v.at);
        let body = bodies.dominant(at);
        let dir = s.rot * v.dir;
        let (e1, e2) = basis(dir);
        let speed = v.speed as f32;
        let vs = if vac { 4.0 + (speed * 0.08).min(30.0) } else { 1.5 + (speed * 0.03).min(10.0) };
        let spread = if vac { 0.5 } else { 0.25 };
        let r0 = v.r0 as f32;
        let size = (r0 * 1.5).clamp(0.4, 1.4);
        for _ in 0..count {
            let ang = self.rnd() * std::f32::consts::TAU;
            let rad = self.rnd().sqrt() * r0 * 0.8;
            let pos = at + (e1 * ang.cos() * rad + e2 * ang.sin() * rad + dir * 0.05).as_dvec3();
            let vel = (dir + e1 * (self.rnd() - 0.5) * 2.0 * spread + e2 * (self.rnd() - 0.5) * 2.0 * spread) * vs * (0.6 + 0.8 * self.rnd());
            if self.rnd() < 0.15 {
                let life = 0.3 + self.rnd() * 0.8;
                fx.puff(ice, bodies, body, pos, vel, 0.02, life);
            } else {
                let (sz, life) = ((0.05 + self.rnd() * 0.22) * size, if vac { 0.5 + self.rnd() } else { 0.3 + self.rnd() * 0.6 });
                fx.puff(vapour, bodies, body, pos, vel, sz, life);
            }
        }
        // inside: dust and scraps racing to the opening across the room
        if v.mdot < 0.5 {
            return;
        }
        for _ in 0..(count as f32 * 0.35).ceil() as usize {
            let r = 0.4 + self.rnd() * 2.6;
            let from = v.at - v.dir * r + basis(v.dir).0 * (self.rnd() - 0.5) * r * 1.2 + basis(v.dir).1 * (self.rnd() - 0.5) * r * 1.2;
            if atmos::room_of(&sh.kind, from) != Some(v.up) {
                continue;
            }
            let to = v.at - from;
            let dist = to.length().max(1e-3);
            let sp = (2.0 + speed * r0 * r0 / dist.max(0.45).powi(2) * 0.25).min(14.0);
            let size = 0.015 + self.rnd() * 0.035;
            fx.puff(motes, bodies, body, s.to_world(from), s.rot * (to / dist * sp), size, (dist / sp + 0.1).min(1.5));
        }
    }

    /// Condensation fog through a room decompressing violently (k: its shock 0..1).
    #[allow(clippy::too_many_arguments)]
    fn fog(&mut self, id: u64, c: usize, k: f32, sh: &lunar_ship::Ship, s: &lunar_core::structure::state::Structure, bodies: &BodyRegistry, fx: &mut Effects, dt: f32) {
        let Some(vapour) = self.vapour else { return };
        let plan = &sh.kind.compartments[c];
        let count = self.owed((id, 2 << 40 | c as u64), (150.0 * k * (plan.volume / 20.0).sqrt()).min(400.0), dt);
        let out: Vec<&Vent> = sh.atmos.vents.iter().filter(|v| v.up == c).collect();
        for _ in 0..count {
            let [lo, hi] = plan.boxes[(self.rnd() * plan.boxes.len() as f32) as usize % plan.boxes.len()];
            let l = lo + (hi - lo) * Vec3::new(self.rnd(), self.rnd(), self.rnd());
            if atmos::room_of(&sh.kind, l) != Some(c) {
                continue;
            }
            let mut vel = Vec3::new(self.rnd() - 0.5, (self.rnd() - 0.5) * 0.75, self.rnd() - 0.5) * 0.4;
            if !out.is_empty() {
                // it drifts toward the openings the room is emptying through
                let v = out[(self.rnd() * out.len() as f32) as usize % out.len()];
                let d = v.at - l;
                let dist = d.length().max(1e-3);
                vel += d / dist * (1.0 + 6.0 / dist.max(0.5)).min(6.0) * (0.5 + self.rnd());
            }
            let pos = s.to_world(l);
            let (size, life) = (0.2 + self.rnd() * 0.4, 0.6 + self.rnd());
            fx.puff(vapour, bodies, bodies.dominant(pos), pos, s.rot * vel, size, life);
        }
    }

    /// A plate tearing: flakes and a hiss from its edge, now and then a spark.
    fn creak(&mut self, at: DVec3, radius: f32, out: Vec3, bodies: &BodyRegistry, fx: &mut Effects) {
        let (Some(motes), Some(sparks)) = (self.motes, self.sparks) else { return };
        let (e1, e2) = basis(out);
        let ang = self.rnd() * std::f32::consts::TAU;
        let p = at + ((e1 * ang.cos() + e2 * ang.sin()) * radius * 0.7).as_dvec3();
        let body = bodies.dominant(p);
        for _ in 0..4 {
            let vel = out * (0.5 + self.rnd() * 2.0) + Vec3::new(self.rnd() - 0.5, self.rnd() - 0.5, self.rnd() - 0.5) * 1.2;
            let (size, life) = (0.02 + self.rnd() * 0.05, 0.2 + self.rnd() * 0.4);
            fx.puff(motes, bodies, body, p, vel, size, life);
        }
        if self.rnd() < 0.35 {
            for _ in 0..3 {
                let vel = Vec3::new(self.rnd() - 0.5, self.rnd() - 0.5, self.rnd() - 0.5).normalize_or(Vec3::Y) * (1.0 + self.rnd() * 2.0);
                let life = 0.1 + self.rnd() * 0.25;
                fx.puff(sparks, bodies, body, p, vel, 0.015, life);
            }
        }
    }

    /// The burst of vapour out of a room that blew (k 0..1).
    fn bang(&mut self, at: DVec3, dir: Vec3, k: f32, bodies: &BodyRegistry, fx: &mut Effects) {
        let Some(vapour) = self.vapour else { return };
        let (e1, e2) = basis(dir);
        let body = bodies.dominant(at);
        for _ in 0..(160.0 * k) as usize {
            let vel = (dir + e1 * (self.rnd() - 0.5) * 1.6 + e2 * (self.rnd() - 0.5) * 1.6) * (6.0 + self.rnd() * 30.0) * k;
            let pos = at + ((e1 * (self.rnd() - 0.5) + e2 * (self.rnd() - 0.5)) * 0.8).as_dvec3();
            let (size, life) = (0.1 + self.rnd() * 0.45, 0.4 + self.rnd() * 1.2);
            fx.puff(vapour, bodies, body, pos, vel, size, life);
        }
    }
}
