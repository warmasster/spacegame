//! Exhaust plumes: what is seen leaving a thruster that fires — a rocket engine, a reaction
//! control jet, the cold gas of a suit's pack. How each kind looks is data (`chorros.jsonc`: a
//! named style); what fires is whatever its owner says (a `Nozzle` each: where, which way, how
//! hard). From them a frame gets its plumes (cones of light the renderer draws from one small
//! instance buffer), a few lights on what is round the brightest ones, and the gas of the
//! transients as particles (`effects`).
//!
//! In a vacuum a plume is no flame: the gas leaves under-expanded, opens wide and thins out of
//! sight within a few nozzle diameters; what is seen is the hot gas at the nozzle's mouth and a
//! faint cone beyond it (docs/CHORROS.md). Its size goes with the root of the thrust, as the
//! reach of the dust it raises does (`app::dust`).
//!
//! Bounded: nothing is made for a thruster that is not firing; a plume too small to see from the
//! eye is not drawn; particles only near the eye, a few per thruster and frame, all of them
//! within a share of the particle budget.
use crate::{
    body::{BodyId, BodyRegistry},
    defs::{self, DefError},
    effects::Effects,
};
use glam::{DVec3, Vec3};
use serde::Deserialize;
use std::{collections::BTreeMap, path::Path};

/// Styles the renderer's table holds (the same number in `render/shaders/plumes.wgsl`).
pub const MAX_STYLES: usize = 16;
/// A plume shorter than this on screen (px) is not drawn, unless the glow at its mouth is this
/// big (px): a far engine is a point of light.
pub const MIN_PX: f32 = 2.0;
pub const CORE_MIN_PX: f32 = 0.08;
/// Particles only for plumes this long on screen (px, at full thrust); all of them from
/// `PARTICLES_FULL_PX`.
pub const PARTICLES_FROM_PX: f32 = 14.0;
pub const PARTICLES_FULL_PX: f32 = 70.0;
/// Particles a second for all the plumes together, as a share of the particle budget (those alive
/// at once), with this many seconds of it kept in hand for bursts; and none while the list is
/// this full (what is left is the explosions').
pub const BUDGET_SHARE: f32 = 0.025;
pub const BUDGET_HELD: f32 = 0.5;
pub const FULL_FROM: f32 = 0.9;
/// Particles of a stream per thruster and frame at most; particles of bursts (starts, stops) per
/// frame, all thrusters together.
pub const PER_FRAME: u32 = 3;
pub const BURSTS_PER_FRAME: u32 = 48;
/// Lights at most, and only of plumes this near the eye (m).
pub const MAX_GLOWS: usize = 4;
pub const GLOW_WITHIN: f32 = 300.0;
/// A thruster fires from this share of its rated thrust; it puffs again (start, stop) only after
/// this long (s).
pub const FIRING: f32 = 0.02;
pub const PUFF_GAP: f32 = 0.3;

/// Gas thrown as particles: a burst (`cuantas`) or a stream (`por_segundo` at full thrust).
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct GasDef {
    /// Particle style (`particles.jsonc`).
    pub estilo: String,
    #[serde(default)]
    pub cuantas: u32,
    #[serde(default)]
    pub por_segundo: f32,
    /// Speed out of the nozzle, min..max (plume lengths at full thrust per second), within a cone
    /// of this half-angle (deg).
    pub velocidad: [f32; 2],
    pub cono: f32,
    /// Radius at birth, min..max (plume lengths at full thrust), and life (s).
    pub tamano: [f32; 2],
    pub vida: [f32; 2],
}

/// The light a plume throws on what is round it: sRGB colour, intensity at full thrust, and its
/// range (m per root of kN of thrust).
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct GlowDef {
    pub color: [u8; 3],
    pub intensidad: f32,
    pub alcance: f32,
}

/// How the exhaust of a kind of thruster looks.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PlumeStyleDef {
    /// Light of the hot gas at the nozzle's mouth, and of the plume beyond it (linear, HDR).
    pub nucleo: [f32; 3],
    pub pluma: [f32; 3],
    /// Length of the plume (m) per root of kN of thrust, and the half-angle of its edge (deg).
    pub largo: f32,
    pub apertura: f32,
    /// The core: how far along the plume it reaches (share of its length) and how wide it is
    /// (nozzle radii).
    #[serde(default = "core_long")]
    pub nucleo_largo: f32,
    #[serde(default = "core_wide")]
    pub nucleo_ancho: f32,
    /// How fast the plume thins as it opens: its light goes with (nozzle radius / radius there)
    /// to this power.
    #[serde(default = "fall")]
    pub caida: f32,
    /// Flicker (0 steady .. 1) and how fast its streaks run (plume lengths a second).
    #[serde(default)]
    pub parpadeo: f32,
    #[serde(default = "run")]
    pub corriente: f32,
    /// Share of its light that is the sun's on it (frost, cold gas): none of that in shadow.
    #[serde(default)]
    pub sol: f32,
    /// Its light goes with the throttle to this power.
    #[serde(default = "gamma")]
    pub gamma: f32,
    /// Seconds it takes to go out once the thrust is gone.
    #[serde(default = "fade")]
    pub desvanecer: f32,
    #[serde(default)]
    pub luz: Option<GlowDef>,
    /// Gas thrown when it starts, when it stops, and all the while it fires.
    #[serde(default)]
    pub arranque: Option<GasDef>,
    #[serde(default)]
    pub apagado: Option<GasDef>,
    #[serde(default)]
    pub particulas: Option<GasDef>,
}

fn core_long() -> f32 {
    0.12
}
fn core_wide() -> f32 {
    0.8
}
fn fall() -> f32 {
    1.3
}
fn run() -> f32 {
    3.0
}
fn gamma() -> f32 {
    0.6
}
fn fade() -> f32 {
    0.08
}

/// A suit's pack: the style of its jets and which of the suit's places are its nozzles.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PackDef {
    pub estilo: String,
    /// The suit's points (`rigs/*.jsonc`, `puntos`) whose name starts with this are its nozzles:
    /// where each is and the way its jet leaves (`palma`).
    pub toberas: String,
    /// Radius of a nozzle's mouth (m).
    pub radio: f32,
    /// A nozzle answers a push when its jet leaves within this angle of the way against it (deg).
    #[serde(default = "answer")]
    pub cono: f32,
}

fn answer() -> f32 {
    60.0
}

/// `chorros.jsonc`: the styles, which one a machine gets by its model when its own data names
/// none, and the suit's pack.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlumeDefs {
    pub estilos: BTreeMap<String, PlumeStyleDef>,
    #[serde(default)]
    pub por_modelo: BTreeMap<String, String>,
    #[serde(default)]
    pub mochila: Option<PackDef>,
}

impl PlumeDefs {
    /// `chorros.jsonc` under `dir`.
    pub fn load(dir: &Path) -> Result<PlumeDefs, DefError> {
        let path = defs::file(dir, "chorros");
        let d: PlumeDefs = defs::load(&path)?;
        d.validate().map_err(|e| DefError::new(path.display().to_string(), e))?;
        Ok(d)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.estilos.len() > MAX_STYLES {
            return Err(format!("{} estilos de chorro > {MAX_STYLES}", self.estilos.len()));
        }
        for (id, s) in &self.estilos {
            if s.largo <= 0.0 || !(0.0..80.0).contains(&s.apertura) {
                return Err(format!("chorro {id}: largo > 0 y apertura entre 0 y 80 grados"));
            }
            if s.desvanecer < 0.0 || !(0.0..=1.0).contains(&s.sol) || !(0.0..=1.0).contains(&s.parpadeo) || s.nucleo_largo <= 0.0 || s.nucleo_ancho <= 0.0 {
                return Err(format!("chorro {id}: desvanecer >= 0, sol y parpadeo entre 0 y 1, núcleo con tamaño"));
            }
            for (what, g, burst) in [("arranque", &s.arranque, true), ("apagado", &s.apagado, true), ("particulas", &s.particulas, false)] {
                let Some(g) = g else { continue };
                if burst && g.cuantas == 0 || !burst && g.por_segundo <= 0.0 {
                    return Err(format!("chorro {id}: '{what}' necesita {}", if burst { "'cuantas'" } else { "'por_segundo'" }));
                }
                if g.vida[0] <= 0.0 || g.tamano[0] <= 0.0 {
                    return Err(format!("chorro {id}: '{what}' con vida y tamaño > 0"));
                }
            }
        }
        for (model, style) in &self.por_modelo {
            if !self.estilos.contains_key(style) {
                return Err(format!("por_modelo: '{model}' pide el estilo '{style}', que no hay"));
            }
        }
        if let Some(p) = &self.mochila
            && !self.estilos.contains_key(&p.estilo)
        {
            return Err(format!("mochila: pide el estilo '{}', que no hay", p.estilo));
        }
        Ok(())
    }
}

/// Gas ready to throw: its particle style found, its cone as a cosine.
#[derive(Clone, Copy, Debug)]
struct Gas {
    style: u8,
    count: u32,
    rate: f32,
    speed: [f32; 2],
    cos: f32,
    size: [f32; 2],
    life: [f32; 2],
}

/// A style ready to use.
#[derive(Clone, Debug)]
pub struct Style {
    pub def: PlumeStyleDef,
    /// Tangent of its edge's half-angle, and its light's colour (linear) if it gives any.
    tan: f32,
    glow: Option<[f32; 3]>,
    start: Option<Gas>,
    stop: Option<Gas>,
    stream: Option<Gas>,
}

impl Style {
    /// How long its plume is (m) with `thrust` N.
    pub fn length(&self, thrust: f32) -> f32 {
        self.def.largo * (thrust.max(0.0) / 1000.0).sqrt()
    }

    /// How wide the plume is at its far end (m, its radius there).
    pub fn spread(&self, radius: f32, length: f32) -> f32 {
        radius + length * self.tan
    }

    /// How far (m) anything of the plume of a thruster rated `rated` N, its nozzle `radius` m
    /// wide, is drawn, for an eye that sees `px_per_rad` pixels per radian.
    pub fn reach(&self, rated: f32, radius: f32, px_per_rad: f32) -> f32 {
        (self.length(rated) / MIN_PX).max(radius * self.def.nucleo_ancho / CORE_MIN_PX) * px_per_rad
    }

    /// Particles a second its stream throws at full thrust seen from close by (0: none).
    pub fn stream_rate(&self) -> f32 {
        self.stream.map_or(0.0, |g| g.rate)
    }

    /// Particles of its start and of its stop.
    pub fn bursts(&self) -> (u32, u32) {
        (self.start.map_or(0, |g| g.count), self.stop.map_or(0, |g| g.count))
    }
}

/// Every style, by number (what the renderer's table holds, `Nozzle::style`).
#[derive(Clone, Debug, Default)]
pub struct Styles {
    pub names: Vec<String>,
    pub list: Vec<Style>,
    by_model: Vec<(String, u8)>,
    pub pack: Option<(u8, PackDef)>,
}

impl Styles {
    /// The styles of `defs`; `particle` gives the number of a particle style by its name.
    pub fn new(defs: &PlumeDefs, particle: impl Fn(&str) -> Option<u8>) -> Result<Styles, String> {
        defs.validate()?;
        let names: Vec<String> = defs.estilos.keys().cloned().collect();
        let find = |name: &str| names.iter().position(|n| n == name).map(|k| k as u8);
        let mut list = Vec::with_capacity(names.len());
        for (id, def) in &defs.estilos {
            let gas = |g: &Option<GasDef>| -> Result<Option<Gas>, String> {
                let Some(g) = g else { return Ok(None) };
                let style = particle(&g.estilo).ok_or_else(|| format!("chorro {id}: no hay estilo de partícula '{}'", g.estilo))?;
                Ok(Some(Gas { style, count: g.cuantas, rate: g.por_segundo, speed: g.velocidad, cos: g.cono.to_radians().cos(), size: g.tamano, life: g.vida }))
            };
            let linear = |c: u8| (f32::from(c) / 255.0).powf(2.2);
            list.push(Style {
                tan: def.apertura.to_radians().tan(),
                glow: def.luz.map(|l| [linear(l.color[0]), linear(l.color[1]), linear(l.color[2])]),
                start: gas(&def.arranque)?,
                stop: gas(&def.apagado)?,
                stream: gas(&def.particulas)?,
                def: def.clone(),
            });
        }
        let by_model = defs.por_modelo.iter().filter_map(|(m, s)| Some((m.clone(), find(s)?))).collect();
        let pack = defs.mochila.as_ref().and_then(|p| Some((find(&p.estilo)?, p.clone())));
        Ok(Styles { names, list, by_model, pack })
    }

    /// The number of the style called `name`.
    pub fn find(&self, name: &str) -> Option<u8> {
        self.names.iter().position(|n| n == name).map(|k| k as u8)
    }

    /// The style of a machine: the one its data names, else its model's.
    pub fn of(&self, named: Option<&str>, model: &str) -> Option<u8> {
        match named {
            Some(n) => self.find(n),
            None => self.by_model.iter().find(|(m, _)| m == model).map(|(_, s)| *s),
        }
    }
}

/// A thruster as it is this frame.
#[derive(Clone, Copy, Debug)]
pub struct Nozzle {
    /// The middle of its mouth (world) and the way the gas leaves it (unit).
    pub at: DVec3,
    pub dir: Vec3,
    /// Radius of its mouth (m).
    pub radius: f32,
    /// Its rated thrust (N) and how much of it it gives now (0..1 and a little over).
    pub rated: f32,
    pub level: f32,
    pub style: u8,
    /// How fast it goes (world, m/s): the gas leaves with that too.
    pub vel: Vec3,
    pub body: BodyId,
    /// 0..1, its own: no two plumes flicker alike.
    pub seed: f32,
}

/// What a thruster's plume remembers from one frame to the next.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Memory {
    /// The level shown (it follows the thrust up at once, down in a moment) and the level it had.
    pub shown: f32,
    pub level: f32,
    /// Particles owed to its stream, and seconds until it may puff again.
    pub owed: f32,
    pub cool: f32,
}

impl Memory {
    /// Nothing to show and nothing going on: its thruster need not be looked at while it gives
    /// no thrust.
    pub fn idle(&self) -> bool {
        self.shown <= 0.0 && self.level < FIRING
    }
}

/// A plume to draw: a cone of light from `pos` along `dir`, `radius` wide at the nozzle and
/// `spread` at its end `length` away.
#[derive(Clone, Copy, Debug)]
pub struct Plume {
    pub pos: DVec3,
    pub dir: Vec3,
    pub radius: f32,
    pub length: f32,
    pub spread: f32,
    /// Its light, 0..1 and a little over.
    pub gain: f32,
    pub style: u8,
    pub seed: f32,
}

/// The light of a plume on what is round it.
#[derive(Clone, Copy, Debug)]
pub struct Glow {
    pub pos: DVec3,
    /// Linear colour times intensity.
    pub color: [f32; 3],
    pub range: f32,
    /// Smaller: nearer or brighter.
    score: f32,
}

/// Who looks: where from, how many pixels a radian is on its screen, and the way to the sun.
#[derive(Clone, Copy, Debug)]
pub struct Eye {
    pub at: DVec3,
    pub px_per_rad: f32,
    pub sun: DVec3,
}

/// A frame's exhaust: `begin`, an `add` per thruster that has something to show, and what is to
/// be drawn is in `plumes` and `glows` (the particles went to the effects as they were thrown).
pub struct Exhaust {
    pub styles: Styles,
    pub plumes: Vec<Plume>,
    pub glows: Vec<Glow>,
    rng: u64,
    /// Particles a second the streams asked for this frame, and the share of what each asks
    /// that the budget gives (from what they asked the frame before).
    asked: f32,
    share: f32,
    /// Particles it may still throw (filled at the budget's rate, `BUDGET_HELD` s of it at most).
    tokens: f32,
    bursts: u32,
    room: bool,
    /// Thrusters looked at this frame and particles thrown since the start (for tools and tests).
    pub walked: u32,
    pub thrown: u64,
}

impl Exhaust {
    pub fn new(styles: Styles) -> Exhaust {
        Exhaust { styles, plumes: Vec::with_capacity(64), glows: Vec::with_capacity(MAX_GLOWS + 1), rng: 0x2545_f491_4f6c_dd1d, asked: 0.0, share: 1.0, tokens: -1.0, bursts: BURSTS_PER_FRAME, room: true, walked: 0, thrown: 0 }
    }

    /// A new frame, `dt` s after the last: nothing to draw yet; the budget is a share of what
    /// `fx` may hold.
    pub fn begin(&mut self, fx: &Effects, dt: f32) {
        self.plumes.clear();
        self.glows.clear();
        let capacity = fx.particles.capacity() as f32;
        let budget = capacity * BUDGET_SHARE;
        let held = budget * BUDGET_HELD;
        self.tokens = if self.tokens < 0.0 { held } else { (self.tokens + budget * dt).min(held) };
        self.share = if self.asked > budget { budget / self.asked } else { 1.0 };
        self.asked = 0.0;
        self.bursts = BURSTS_PER_FRAME;
        self.room = (fx.particles.len() as f32) < capacity * FULL_FROM;
        self.walked = 0;
    }

    /// 0..1.
    fn rnd(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng >> 40) as f32 / (1u64 << 24) as f32
    }

    fn between(&mut self, r: [f32; 2]) -> f32 {
        r[0] + (r[1] - r[0]) * self.rnd()
    }

    /// One particle of `g` out of nozzle `n`, whose plume is `full` m long at full thrust.
    fn throw(&mut self, g: &Gas, n: &Nozzle, full: f32, fx: &mut Effects, bodies: &BodyRegistry) {
        let (u, v) = n.dir.any_orthonormal_pair();
        let a = self.rnd() * std::f32::consts::TAU;
        let cos = 1.0 - self.rnd() * (1.0 - g.cos);
        let sin = (1.0 - cos * cos).max(0.0).sqrt();
        let dir = n.dir * cos + (u * a.cos() + v * a.sin()) * sin;
        let speed = self.between(g.speed) * full;
        // born across the mouth, and along what it travels in a frame (a stream shows no steps)
        let off = self.rnd().sqrt() * n.radius;
        let oa = self.rnd() * std::f32::consts::TAU;
        let at = (u * oa.cos() + v * oa.sin()) * off + n.dir * (self.rnd() * speed / 60.0);
        let (size, life) = (self.between(g.size) * full, self.between(g.life));
        fx.puff(g.style, bodies, n.body, n.at + at.as_dvec3(), n.vel + dir * speed, size, life);
        self.thrown += 1;
    }

    /// Thruster `n` this frame, `m` its memory: its plume if it is seen from `eye`, its light if
    /// it is among the nearest, its gas as particles if it is near.
    pub fn add(&mut self, n: &Nozzle, m: &mut Memory, dt: f32, eye: &Eye, fx: &mut Effects, bodies: &BodyRegistry) {
        self.walked += 1;
        let Some(style) = self.styles.list.get(usize::from(n.style)) else { return };
        let (def_fade, gamma, sol, core_wide, luz) = (style.def.desvanecer, style.def.gamma, style.def.sol, style.def.nucleo_ancho, style.def.luz);
        let (start, stop, stream, glow) = (style.start, style.stop, style.stream, style.glow);
        let (full, tan) = (style.length(n.rated), style.tan);
        let level = n.level.clamp(0.0, 1.5);
        let (started, stopped) = (m.level < FIRING && level >= FIRING, m.level >= FIRING && level < FIRING);
        m.level = level;
        m.cool = (m.cool - dt).max(0.0);
        // what is shown follows the thrust up at once and down in a moment: a jet does not blink
        m.shown = if level >= m.shown { level } else { level.max(m.shown - dt / def_fade.max(1e-3)) };
        if m.shown < 1e-3 {
            m.shown = 0.0;
        }
        let d = n.at.distance(eye.at) as f32;
        let px = eye.px_per_rad / d.max(0.05);
        // ---- its gas: near the eye only, within the budget ----
        let near = ((full * px - PARTICLES_FROM_PX) / (PARTICLES_FULL_PX - PARTICLES_FROM_PX)).clamp(0.0, 1.0);
        if near > 0.0 && self.room {
            let puff = if started {
                start
            } else if stopped {
                stop
            } else {
                None
            };
            if let Some(g) = puff
                && m.cool <= 0.0
            {
                m.cool = PUFF_GAP;
                let k = ((g.count as f32 * near).round() as u32).min(self.bursts).min(self.tokens.max(0.0) as u32);
                self.bursts -= k;
                self.tokens -= k as f32;
                for _ in 0..k {
                    self.throw(&g, n, full, fx, bodies);
                }
            }
            if let Some(g) = stream
                && level >= FIRING
            {
                let want = g.rate * level.min(1.0) * near;
                self.asked += want;
                m.owed = (m.owed + want * self.share * dt).min(PER_FRAME as f32);
                while m.owed >= 1.0 && self.tokens >= 1.0 {
                    m.owed -= 1.0;
                    self.tokens -= 1.0;
                    self.throw(&g, n, full, fx, bodies);
                }
            }
        }
        if m.shown <= 0.0 {
            m.owed = 0.0;
            return;
        }
        // ---- its plume, if it can be seen from here ----
        let Some(style) = self.styles.list.get(usize::from(n.style)) else { return };
        let length = style.length(n.rated * m.shown);
        if length * px < MIN_PX && n.radius * core_wide * px < CORE_MIN_PX {
            return;
        }
        let mut gain = m.shown.min(1.2).powf(gamma);
        if sol > 0.0 && !sunlit(bodies, n.body, n.at, eye.sun) {
            gain *= 1.0 - sol;
        }
        if gain < 0.004 {
            return;
        }
        self.plumes.push(Plume { pos: n.at, dir: n.dir, radius: n.radius, length, spread: n.radius + length * tan, gain, style: n.style, seed: n.seed });
        // ---- its light on what is round it ----
        if let (Some(l), Some(color)) = (luz, glow)
            && d < GLOW_WITHIN
        {
            let power = l.intensidad * gain;
            let range = l.alcance * (n.rated / 1000.0).max(0.0).sqrt() * (0.5 + 0.5 * m.shown.min(1.0));
            let g = Glow { pos: n.at + (n.dir * (length * 0.2)).as_dvec3(), color: [color[0] * power, color[1] * power, color[2] * power], range, score: d / (power * range).max(1e-3).sqrt() };
            let at = self.glows.iter().position(|o| o.score > g.score).unwrap_or(self.glows.len());
            if at < MAX_GLOWS {
                self.glows.truncate(MAX_GLOWS - 1);
                self.glows.insert(at, g);
            }
        }
    }
}

/// The sun is seen from `p` (it is not behind the body `p` is over).
pub fn sunlit(bodies: &BodyRegistry, body: BodyId, p: DVec3, sun: DVec3) -> bool {
    let b = bodies.get(body);
    let d = p - b.center;
    let t = d.dot(sun);
    t >= 0.0 || (d - sun * t).length() > b.radius
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        body::{Body, BodyDef},
        effects::EffectDefs,
    };

    fn dir() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/defs")
    }

    fn moon() -> BodyRegistry {
        let def: BodyDef = crate::defs::parse("b", r#"{ "name": "b", "center": [0, 0, 0], "radius": 1000, "gravity": 1.62, "reach": { "to": 500, "band": 200 }, "north": [0, 0, -1], "horizon_depth": 10 }"#).unwrap();
        BodyRegistry::new(vec![Body::from_def("b", &def).unwrap()])
    }

    fn effects(capacity: usize) -> Effects {
        Effects::new(&EffectDefs::load(&dir()).unwrap_or_else(|e| panic!("{e}")), capacity)
    }

    fn exhaust(fx: &Effects) -> Exhaust {
        let defs = PlumeDefs::load(&dir()).unwrap_or_else(|e| panic!("{e}"));
        Exhaust::new(Styles::new(&defs, |n| fx.style(n)).unwrap_or_else(|e| panic!("{e}")))
    }

    /// A screen 1080 px high seen with 60° of it.
    const PX: f32 = 935.0;

    fn eye(d: f64) -> Eye {
        Eye { at: DVec3::new(d, 1010.0, 0.0), px_per_rad: PX, sun: DVec3::Y }
    }

    fn nozzle(style: u8, rated: f32, level: f32) -> Nozzle {
        Nozzle { at: DVec3::new(0.0, 1010.0, 0.0), dir: Vec3::NEG_Y, radius: 0.5, rated, level, style, vel: Vec3::ZERO, body: 0, seed: 0.3 }
    }

    #[test]
    fn the_registry_loads_and_every_style_is_whole() {
        let fx = effects(1000);
        let ex = exhaust(&fx);
        assert!(ex.styles.list.len() >= 3 && ex.styles.list.len() <= MAX_STYLES);
        // the models of the machines that push have a style, and the pack has one
        for model in ["motor_cohete", "rcs"] {
            assert!(ex.styles.of(None, model).is_some(), "sin estilo para el modelo '{model}'");
        }
        assert!(ex.styles.of(None, "bateria").is_none());
        assert!(ex.styles.of(Some("no_hay"), "rcs").is_none(), "un estilo que no hay no es el del modelo");
        let (pack, def) = ex.styles.pack.clone().expect("la mochila tiene estilo");
        assert!(usize::from(pack) < ex.styles.list.len() && def.radio > 0.0 && !def.toberas.is_empty());
        for (name, s) in ex.styles.names.iter().zip(&ex.styles.list) {
            eprintln!("{name}: {:.1} m con 48 kN, {:.2} m con 2,2 kN; {:.0} partículas/s; ráfagas {:?}", s.length(48_000.0), s.length(2200.0), s.stream_rate(), s.bursts());
        }
        // a style that asks for a particle style there is not is refused, and so is a bad shape
        let mut defs = PlumeDefs::load(&dir()).unwrap();
        let first = defs.estilos.keys().next().unwrap().clone();
        defs.estilos.get_mut(&first).unwrap().arranque = Some(GasDef { estilo: "no_hay".into(), cuantas: 3, por_segundo: 0.0, velocidad: [1.0, 2.0], cono: 20.0, tamano: [0.1, 0.2], vida: [0.2, 0.4] });
        assert!(Styles::new(&defs, |n| fx.style(n)).is_err());
        defs.estilos.get_mut(&first).unwrap().arranque = None;
        defs.estilos.get_mut(&first).unwrap().largo = 0.0;
        assert!(defs.validate().is_err());
    }

    #[test]
    fn nothing_fires_nothing_is_made() {
        let (bodies, mut fx) = (moon(), effects(4000));
        let mut ex = exhaust(&fx);
        let style = ex.styles.of(None, "motor_cohete").unwrap();
        let mut m = Memory::default();
        for _ in 0..120 {
            ex.begin(&fx, 1.0 / 60.0);
            ex.add(&nozzle(style, 48_000.0, 0.0), &mut m, 1.0 / 60.0, &eye(20.0), &mut fx, &bodies);
            assert!(ex.plumes.is_empty() && ex.glows.is_empty());
        }
        assert!(fx.particles.is_empty() && ex.thrown == 0);
        assert!(m.idle() && m.owed == 0.0, "{m:?}");
        // (a whisper under what counts as firing shows its faint plume, and throws nothing)
        ex.begin(&fx, 1.0 / 60.0);
        ex.add(&nozzle(style, 48_000.0, 0.01), &mut m, 1.0 / 60.0, &eye(20.0), &mut fx, &bodies);
        assert!(ex.plumes.len() == 1 && fx.particles.is_empty());
    }

    #[test]
    fn a_plume_goes_with_the_root_of_the_thrust_and_its_light_with_the_throttle() {
        let (bodies, mut fx) = (moon(), effects(4000));
        let mut ex = exhaust(&fx);
        let style = ex.styles.of(None, "motor_cohete").unwrap();
        let mut shape = |rated: f32, level: f32| {
            let mut m = Memory::default();
            ex.begin(&fx, 1.0 / 60.0);
            ex.add(&nozzle(style, rated, level), &mut m, 1.0 / 60.0, &eye(30.0), &mut fx, &bodies);
            let p = ex.plumes[0];
            (p.length, p.spread, p.gain, ex.glows.first().map(|g| (g.color[0], g.range)))
        };
        let (full, quarter) = (shape(48_000.0, 1.0), shape(48_000.0, 0.25));
        // a quarter of the thrust: half as long, narrower at its end, dimmer
        assert!((quarter.0 / full.0 - 0.5).abs() < 0.01, "{} m y {} m", full.0, quarter.0);
        assert!(quarter.1 < full.1 && quarter.2 < full.2 * 0.6 && quarter.2 > 0.2);
        assert!(full.0 > 4.0 && full.0 < 20.0 && full.1 > 2.0 * 0.5, "un motor de 48 kN: {} m de largo, {} m de ancho al final", full.0, full.1);
        // four times the engine: twice the plume
        assert!((shape(192_000.0, 1.0).0 / full.0 - 2.0).abs() < 0.01);
        // it lights what is round it, the more the harder it pushes
        let (lf, lq) = (full.3.expect("un motor da luz"), quarter.3.unwrap());
        assert!(lf.0 > lq.0 && lf.1 > lq.1 && lf.1 > 3.0, "{lf:?} {lq:?}");
    }

    #[test]
    fn a_stream_goes_with_the_thrust_and_never_over_its_budget() {
        let bodies = moon();
        let fx0 = effects(16_000);
        let stream = {
            let ex = exhaust(&fx0);
            (0..ex.styles.list.len() as u8).find(|&k| ex.styles.list[usize::from(k)].stream_rate() > 0.0).expect("algún estilo tiene partículas")
        };
        // `n` thrusters from close by for four seconds: what they throw a second once it is
        // steady (the last three), in all, and the most in one frame
        let thrown = |level: f32, n: usize, capacity: usize| {
            let mut fx = effects(capacity);
            let mut ex = exhaust(&fx);
            let mut ms = vec![Memory { level, shown: level, ..Memory::default() }; n];
            let (mut worst, mut first) = (0, 0);
            for f in 0..240 {
                ex.begin(&fx, 1.0 / 60.0);
                let before = ex.thrown;
                for m in &mut ms {
                    ex.add(&nozzle(stream, 600.0, level), m, 1.0 / 60.0, &eye(1.5), &mut fx, &bodies);
                }
                worst = worst.max(ex.thrown - before);
                if f == 59 {
                    first = ex.thrown;
                }
                fx.particles.clear();
            }
            ((ex.thrown - first) as f32 / 3.0, ex.thrown as f32, worst)
        };
        // one: its rate at full thrust, half of it at half
        let rate = exhaust(&fx0).styles.list[usize::from(stream)].stream_rate();
        let (full, half) = (thrown(1.0, 1, 16_000).0, thrown(0.5, 1, 16_000).0);
        assert!((full - rate).abs() <= rate * 0.05 + 1.0, "{full} partículas por segundo, de {rate}");
        assert!((half - rate * 0.5).abs() <= rate * 0.05 + 1.0, "{half} a medio empuje");
        // five hundred of them at once: what the budget gives, whatever they ask, and never
        // more in all than that and what is kept in hand
        let (many, all, worst) = thrown(1.0, 500, 16_000);
        let budget = 16_000.0 * BUDGET_SHARE;
        assert!(rate * 500.0 > budget * 10.0, "la prueba no pide de más");
        // (over any three seconds: what the budget gives in them and what was in hand)
        let most = (3.0 + BUDGET_HELD) / 3.0;
        assert!(many <= budget * most + 0.5 && many > budget * 0.8, "{many} partículas por segundo con un tope de {budget}");
        assert!(all <= budget * (4.0 + BUDGET_HELD) + 1.0 && worst <= (budget * BUDGET_HELD) as u64 + 8, "{all} en cuatro segundos, {worst} en un fotograma");
        // the lowest quality: a smaller budget
        let (low, ..) = thrown(1.0, 20, 2000);
        assert!(low <= 2000.0 * BUDGET_SHARE * most + 0.5 && low > 2000.0 * BUDGET_SHARE * 0.8, "{low} con 2000 de tope");
    }

    #[test]
    fn far_plumes_are_not_drawn_and_farther_ones_throw_no_gas() {
        let bodies = moon();
        let mut fx = effects(8000);
        let mut ex = exhaust(&fx);
        let (engine, pack) = (ex.styles.of(None, "motor_cohete").unwrap(), ex.styles.pack.clone().unwrap().0);
        let (engine_reach, pack_reach) = (ex.styles.list[usize::from(engine)].reach(48_000.0, 0.6, PX), ex.styles.list[usize::from(pack)].reach(315.0, 0.014, PX));
        let mut seen = |style: u8, rated: f32, radius: f32, d: f64| {
            let mut m = Memory::default();
            let mut n = nozzle(style, rated, 1.0);
            n.radius = radius;
            fx.particles.clear();
            for _ in 0..30 {
                ex.begin(&fx, 1.0 / 60.0);
                ex.add(&n, &mut m, 1.0 / 60.0, &eye(d), &mut fx, &bodies);
            }
            (ex.plumes.len(), fx.particles.len(), ex.glows.len())
        };
        // an engine: its start puff from close by, its plume from far, its light only near,
        // and nothing at all from farther than its reach
        let near = seen(engine, 48_000.0, 0.6, 30.0);
        assert!(near.0 == 1 && near.1 > 0 && near.2 == 1, "{near:?}");
        let far = seen(engine, 48_000.0, 0.6, 2000.0);
        assert!(far == (1, 0, 0), "{far:?}");
        let reach = engine_reach;
        assert!(reach > 2000.0 && reach < 20_000.0, "un motor se ve hasta {reach:.0} m");
        assert_eq!(seen(engine, 48_000.0, 0.6, f64::from(reach) * 1.05).0, 0);
        assert_eq!(seen(engine, 48_000.0, 0.6, f64::from(reach) * 0.9).0, 1);
        // the pack's little jets: gone within a few hundred metres
        let reach = pack_reach;
        assert!(reach > 60.0 && reach < 600.0, "un chorro de mochila se ve hasta {reach:.0} m");
        assert_eq!(seen(pack, 315.0, 0.014, f64::from(reach) * 1.05), (0, 0, 0));
        let close = seen(pack, 315.0, 0.014, 3.0);
        assert!(close.0 == 1 && close.1 > 0, "{close:?}");
    }

    #[test]
    fn a_jet_fades_out_and_puffs_once_as_it_starts_and_stops() {
        let bodies = moon();
        let mut fx = effects(8000);
        let mut ex = exhaust(&fx);
        let engine = ex.styles.of(None, "motor_cohete").unwrap();
        let (start, _) = ex.styles.list[usize::from(engine)].bursts();
        assert!(start > 0, "un motor echa una bocanada al encender");
        let fade = ex.styles.list[usize::from(engine)].def.desvanecer;
        let mut m = Memory::default();
        let frame = |ex: &mut Exhaust, fx: &mut Effects, m: &mut Memory, level: f32| {
            ex.begin(fx, 1.0 / 60.0);
            ex.add(&nozzle(engine, 48_000.0, level), m, 1.0 / 60.0, &eye(12.0), fx, &bodies);
            ex.plumes.len()
        };
        // it lights: one puff, however long it then runs
        frame(&mut ex, &mut fx, &mut m, 0.6);
        let puff = ex.thrown;
        assert!(puff >= u64::from(start) && puff <= u64::from(start + PER_FRAME), "{puff} partículas al encender");
        let rate = ex.styles.list[usize::from(engine)].stream_rate();
        for _ in 0..90 {
            frame(&mut ex, &mut fx, &mut m, 0.6);
        }
        assert!((ex.thrown - puff) as f32 <= rate * 1.5 * 0.6 + 1.0, "{} partículas más en marcha", ex.thrown - puff);
        // cut: it is still seen for a moment, dimmer each frame, then gone
        let mut frames = 0;
        let mut last = m.shown;
        while frame(&mut ex, &mut fx, &mut m, 0.0) > 0 {
            assert!(m.shown < last);
            last = m.shown;
            frames += 1;
            assert!(frames < 600);
        }
        let took = frames as f32 / 60.0;
        assert!(took > 0.0 && took <= fade * 0.6 + 2.0 / 60.0 + fade, "tarda {took} s en apagarse (desvanecer {fade})");
        assert!(m.idle());
        // a full list of particles: no more are thrown (they are the explosions')
        let mut fx = effects(40);
        while fx.particles.len() < 38 {
            fx.puff(0, &bodies, 0, DVec3::Y * 1010.0, Vec3::ZERO, 0.1, 5.0);
        }
        let mut ex = exhaust(&fx);
        let mut m = Memory::default();
        frame(&mut ex, &mut fx, &mut m, 1.0);
        assert_eq!(ex.thrown, 0);
    }

    #[test]
    fn cold_gas_is_the_sun_on_it_and_lights_come_nearest_first() {
        let bodies = moon();
        let mut fx = effects(8000);
        let mut ex = exhaust(&fx);
        let (engine, pack) = (ex.styles.of(None, "motor_cohete").unwrap(), ex.styles.pack.clone().unwrap().0);
        assert!(ex.styles.list[usize::from(pack)].def.sol > 0.5, "el gas frío no da luz propia");
        let mut gain = |sun: DVec3| {
            let mut m = Memory::default();
            ex.begin(&fx, 1.0 / 60.0);
            ex.add(&nozzle(pack, 315.0, 1.0), &mut m, 1.0 / 60.0, &Eye { sun, ..eye(2.0) }, &mut fx, &bodies);
            ex.plumes.first().map_or(0.0, |p| p.gain)
        };
        let (day, night) = (gain(DVec3::Y), gain(DVec3::NEG_Y));
        assert!(day > 0.5 && night < day * 0.5, "al sol {day}, a la sombra {night}");
        assert!(sunlit(&bodies, 0, DVec3::Y * 1010.0, DVec3::X) && !sunlit(&bodies, 0, DVec3::Y * 1010.0, DVec3::NEG_Y));
        // ten engines in a row: the four nearest light the scene
        ex.begin(&fx, 1.0 / 60.0);
        for k in 0..10 {
            let mut m = Memory::default();
            let mut n = nozzle(engine, 48_000.0, 1.0);
            n.at += DVec3::Z * f64::from(k) * 9.0;
            ex.add(&n, &mut m, 1.0 / 60.0, &eye(15.0), &mut fx, &bodies);
        }
        assert_eq!(ex.plumes.len(), 10);
        assert_eq!(ex.glows.len(), MAX_GLOWS);
        assert!(ex.glows.windows(2).all(|w| w[0].score <= w[1].score) && ex.glows.iter().all(|g| g.pos.z < 9.0 * 3.5));
    }
}
