//! Explosions running: a crater dug into the ground (after a moment, under the dust), a flash that
//! lights the scene, a camera shake and particle bursts. Anything (a key, a weapon, a crash, a
//! part breaking) calls `explode` with a definition id and a point. Definitions live in
//! `effect_defs`; an explosion that is a `charge` gets its bursts from `detonation` per blast (the
//! gravity there decides how far the ejecta fly).
pub use crate::effect_defs::*;
use crate::{
    body::{BodyId, BodyRegistry},
    deform::Crater,
    detonation::{Burst, DetonationRules},
    particles::{Particle, Particles},
};
use glam::{DVec3, Vec3};

/// Flashes lighting the scene at once (the renderer's light table).
pub const MAX_FLASHES: usize = 8;
/// Craters waiting for their dust to rise.
const MAX_PENDING: usize = 16;

/// A light the scene is lit by for a moment.
#[derive(Clone, Copy, Debug)]
pub struct Flash {
    pub pos: DVec3,
    pub color: Vec3,
    pub intensity: f32,
    pub range: f32,
    age: f32,
    duration: f32,
}

impl Flash {
    /// Intensity now: a sharp peak, then a fast fall.
    pub fn now(&self) -> f32 {
        let t = (self.age / self.duration).min(1.0);
        self.intensity * (1.0 - t) * (1.0 - t) * (1.0 - t)
    }
}

#[derive(Clone, Copy, Debug)]
struct Shake {
    pos: DVec3,
    def: ShakeDef,
    age: f32,
}

#[derive(Clone, Copy, Debug)]
struct PendingCrater {
    body: BodyId,
    crater: Crater,
    left: f32,
}

/// Explosions running: particles, flashes, shakes, craters still to dig.
pub struct Effects {
    pub particles: Particles,
    style_names: Vec<String>,
    /// Each explosion with its written emitters as bursts.
    explosions: Vec<(String, ExplosionDef, Vec<Burst>)>,
    rules: DetonationRules,
    layer_styles: Vec<u8>,
    flashes: Vec<Flash>,
    shakes: Vec<Shake>,
    pending: Vec<PendingCrater>,
    scratch: Vec<Burst>,
    rng: u64,
}

impl Effects {
    pub fn new(defs: &EffectDefs, capacity: usize) -> Effects {
        let style_names: Vec<String> = defs.styles.iter().map(|(n, _)| n.clone()).collect();
        let style = |name: &str| style_names.iter().position(|s| s == name).unwrap_or(0) as u8;
        let explosions = defs.explosions.iter().map(|(id, e)| (id.clone(), e.clone(), written(e, &style))).collect();
        let layer_styles = defs.rules.layers.iter().map(|l| style(&l.style)).collect();
        Effects {
            particles: Particles::new(defs.styles.iter().map(|(_, s)| *s).collect(), capacity),
            explosions,
            rules: defs.rules.clone(),
            layer_styles,
            style_names,
            flashes: Vec::with_capacity(MAX_FLASHES),
            shakes: Vec::with_capacity(MAX_FLASHES),
            pending: Vec::with_capacity(MAX_PENDING),
            scratch: Vec::with_capacity(32),
            rng: 0x9e37_79b9_7f4a_7c15,
        }
    }

    /// One more explosion (a missile's warhead charge...); its styles must exist.
    pub fn add_explosion(&mut self, id: &str, def: ExplosionDef) -> Result<(), String> {
        if let Some(em) = def.emitters.iter().find(|em| self.style(&em.style).is_none()) {
            return Err(format!("explosion {id}: unknown particle style '{}'", em.style));
        }
        let names = &self.style_names;
        let bursts = written(&def, &|n: &str| names.iter().position(|s| s == n).unwrap_or(0) as u8);
        self.explosions.retain(|(e, _, _)| e != id);
        self.explosions.push((id.to_string(), def, bursts));
        Ok(())
    }

    pub fn has(&self, id: &str) -> bool {
        self.explosions.iter().any(|(e, _, _)| e == id)
    }

    /// Index of a particle style by name.
    pub fn style(&self, name: &str) -> Option<u8> {
        self.style_names.iter().position(|s| s == name).map(|i| i as u8)
    }

    /// One particle of `style` at `pos` over `body` (trails, sparks of anything).
    #[allow(clippy::too_many_arguments)]
    pub fn puff(&mut self, style: u8, bodies: &BodyRegistry, body: BodyId, pos: DVec3, vel: Vec3, size: f32, life: f32) {
        let b = bodies.get(body);
        let d = b.up(pos);
        let ground = b.radius + b.height_at(d, f64::from(size).max(1.0));
        let seed = self.rnd();
        self.particles.spawn(Particle { pos, vel, age: 0.0, life, size, seed, ground, height: ((pos - b.center).length() - ground) as f32, body, style });
    }

    pub fn ids(&self) -> impl Iterator<Item = &str> {
        self.explosions.iter().map(|(id, _, _)| id.as_str())
    }

    fn rnd(&mut self) -> f32 {
        // xorshift64*
        self.rng ^= self.rng >> 12;
        self.rng ^= self.rng << 25;
        self.rng ^= self.rng >> 27;
        (self.rng.wrapping_mul(0x2545_f491_4f6c_dd1d) >> 40) as f32 / (1u64 << 24) as f32
    }

    fn range(&mut self, r: [f32; 2]) -> f32 {
        r[0] + (r[1] - r[0]) * self.rnd()
    }

    /// Blow up definition `id` at `at` over `body` (it digs its crater only near the ground); what
    /// it does to structures comes back for whoever holds them.
    pub fn explode(&mut self, id: &str, bodies: &BodyRegistry, body: BodyId, at: DVec3) -> Result<Option<BlastDef>, String> {
        self.explode_with(id, bodies, body, at, 1.0, 0.0)
    }

    /// `explode` with the written emitters grown by `scale` (spread `scale` times wider and, for
    /// bigger things, up to 3x the particles: a part `scale` times the effect's size breaking).
    pub fn explode_scaled(&mut self, id: &str, bodies: &BodyRegistry, body: BodyId, at: DVec3, scale: f32) -> Result<Option<BlastDef>, String> {
        self.explode_with(id, bodies, body, at, scale, 0.0)
    }

    /// `explode_scaled` with `extra` J more (an impact's speed): a charge grows by its TNT
    /// equivalent, anything else only hits structures harder.
    pub fn explode_with(&mut self, id: &str, bodies: &BodyRegistry, body: BodyId, at: DVec3, scale: f32, extra: f32) -> Result<Option<BlastDef>, String> {
        let k = self.explosions.iter().position(|(e, _, _)| e == id).ok_or_else(|| format!("unknown explosion '{id}'"))?;
        let b = bodies.get(body);
        let up = b.up(at);
        let ground = (at - b.center).length();
        let altitude = b.altitude(at);
        let def = &self.explosions[k].1;
        let (mut crater, mut flash, mut shake, mut damage) = (def.crater, def.flash, def.shake, def.damage);
        let mut bursts = std::mem::take(&mut self.scratch);
        bursts.clear();
        bursts.extend_from_slice(&self.explosions[k].2);
        if let Some(c) = def.charge {
            let d = self.rules.resolve(&c, extra, bodies.field(at).g() as f32);
            let grounded = altitude < f64::from(d.crater.radius);
            crater = crater.or(Some(d.crater));
            flash = flash.or(Some(d.flash));
            shake = shake.or(Some(d.shake));
            damage = damage.or(Some(d.damage));
            self.rules.bursts(&d, &c, grounded, &self.layer_styles, &mut bursts);
        } else if let Some(d) = &mut damage {
            d.energy += extra;
        }
        if let Some(c) = crater.filter(|c| altitude < f64::from(c.radius)) {
            let crater = Crater { dir: up, radius: f64::from(c.radius), depth: f64::from(c.depth), rim: f64::from(c.rim), seed: f64::from(self.rnd()), ground: b.height(up) };
            if c.delay <= 0.0 || self.pending.len() == MAX_PENDING {
                b.edit(|d| d.add(crater, b.radius));
            } else {
                self.pending.push(PendingCrater { body, crater, left: c.delay });
            }
        }
        if let Some(f) = flash {
            if self.flashes.len() == MAX_FLASHES {
                self.flashes.remove(0);
            }
            let pos = at + up * f64::from(f.lift);
            self.flashes.push(Flash { pos, color: Vec3::from_array(f.color), intensity: f.intensity, range: f.range, age: 0.0, duration: f.duration });
        }
        if let Some(s) = shake {
            if self.shakes.len() == MAX_FLASHES {
                self.shakes.remove(0);
            }
            self.shakes.push(Shake { pos: at, def: s, age: 0.0 });
        }
        // a frame round the vertical for the launch cones
        let east = up.cross(if up.y.abs() < 0.9 { DVec3::Y } else { DVec3::X }).normalize();
        let north = up.cross(east);
        let n_written = self.explosions[k].2.len();
        for (i, burst) in bursts.iter().enumerate() {
            let s = if i < n_written { scale } else { 1.0 };
            if !self.throw(burst, s, at, ground, (up, east, north), body, b) {
                break;
            }
        }
        self.scratch = bursts;
        Ok(damage)
    }

    /// The particles of one burst; false once the list is full.
    #[allow(clippy::too_many_arguments)]
    fn throw(&mut self, em: &Burst, scale: f32, at: DVec3, ground: f64, (up, east, north): (DVec3, DVec3, DVec3), body: BodyId, b: &crate::body::Body) -> bool {
        let spread = em.spread * scale;
        let count = (em.count as f32 * scale.clamp(1.0, 3.0)).round() as u32;
        let cos_max = em.cone.to_radians().cos();
        for _ in 0..count {
            let a = self.rnd() * std::f32::consts::TAU;
            let cos = match em.tilt {
                // around the tilt, a few degrees either way (an ejecta curtain)
                Some(t) => (t + (self.rnd() - 0.5) * 16.0).clamp(0.0, em.cone).to_radians().cos(),
                None => 1.0 - self.rnd() * (1.0 - cos_max),
            };
            let sin = (1.0 - cos * cos).max(0.0).sqrt();
            let side = east * f64::from(a.cos()) + north * f64::from(a.sin());
            let dir = up * f64::from(cos) + side * f64::from(sin);
            let speed = em.speed_at(self.rnd());
            let off = self.rnd().sqrt() * spread;
            let oa = self.rnd() * std::f32::consts::TAU;
            let pos = at + up * f64::from(em.lift) + (east * f64::from(oa.cos()) + north * f64::from(oa.sin())) * f64::from(off) + dir * f64::from(off * 0.3);
            let p = Particle { pos, vel: (dir * f64::from(speed)).as_vec3(), age: 0.0, life: self.range(em.life), size: self.range(em.size), seed: self.rnd(), ground, height: ((pos - b.center).length() - ground) as f32, body, style: em.style };
            if !self.particles.spawn(p) {
                return false;
            }
        }
        true
    }

    pub fn update(&mut self, dt: f32, bodies: &BodyRegistry) {
        self.particles.update(dt, bodies);
        for f in &mut self.flashes {
            f.age += dt;
        }
        self.flashes.retain(|f| f.age < f.duration);
        for s in &mut self.shakes {
            s.age += dt;
        }
        self.shakes.retain(|s| s.age < s.def.duration);
        for c in &mut self.pending {
            c.left -= dt;
            if c.left <= 0.0 {
                let b = bodies.get(c.body);
                b.edit(|d| d.add(c.crater, b.radius));
            }
        }
        self.pending.retain(|c| c.left > 0.0);
    }

    /// Craters still waiting to be dug.
    pub fn pending_craters(&self) -> usize {
        self.pending.len()
    }

    /// Lights of the moment (intensity already faded).
    pub fn flashes(&self) -> &[Flash] {
        &self.flashes
    }

    /// Camera jitter amplitude (rad) at `eye` now.
    pub fn shake(&self, eye: DVec3) -> f32 {
        self.shakes
            .iter()
            .map(|s| {
                let d = (s.pos.distance(eye) as f32 / s.def.range).min(1.0);
                let t = s.age / s.def.duration;
                s.def.amplitude * (1.0 - d) * (1.0 - d) * (1.0 - t) * (1.0 - t)
            })
            .sum()
    }
}

/// The written emitters of `e` as bursts.
fn written(e: &ExplosionDef, style: &dyn Fn(&str) -> u8) -> Vec<Burst> {
    e.emitters.iter().map(|em| Burst { style: style(&em.style), count: em.count, speed: em.speed, power: 0.0, cone: em.cone, tilt: em.tilt, size: em.size, life: em.life, spread: em.spread, lift: em.lift }).collect()
}
