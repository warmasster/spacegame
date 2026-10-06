//! Marks on loose ground: what touches it leaves its mark, and the mark stays. A boot leaves its
//! print (a left one or a right one, turned the way the boot faced, deeper coming down from a
//! jump or at a run), a ship's landing pads theirs where it set down, what is set down or dragged
//! its dent, a jet near the ground the patch it swept. On any body whose ground is loose — the
//! ground that raises dust (`dust::Dust::loose`) —, for whoever walks (every boot set down goes
//! through `Dust::step`), for every structure that stands on legs or lies loose, for every jet
//! the dust knows of.
//!
//! What a mark is, is data (`assets/defs/huellas.jsonc`): kinds of mark (their shape, how deep,
//! how dark, how long they last, how far they are seen) and who leaves which. Here:
//!
//! - `Marks`: the marks kept, a ring of a few thousand (the oldest goes when another needs its
//!   place), each with its place in f64, and the few of them near the eye that the GPU holds
//!   (`lunar_render::prints`). The GPU's are written when they change and not otherwise: one mark
//!   when one is left (64 bytes), all the near ones when the eye has gone far enough for others
//!   to be near. A mark fades by its age in the shader: nothing is written for that;
//! - `Footprints`: who leaves them. Nothing is allocated in a frame.
use crate::dust::{Dust, Grounds, Tread};
use glam::{DVec3, Quat, Vec3};
use lunar_core::{
    body::{Body, BodyRegistry},
    structure::{set::Structures, state::Structure},
};
use lunar_render::{
    Renderer,
    prints::{self, Kind, Look, Print, Shape},
};
use serde::Deserialize;
use std::{
    collections::{BTreeMap, HashMap},
    path::Path,
};

// ---- the data

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Defs {
    /// Marks kept at once.
    pub capacidad: usize,
    /// How loose the ground of each body is, by its id ("*": every other).
    pub suelos: BTreeMap<String, GroundDef>,
    pub marcas: BTreeMap<String, KindDef>,
    pub deja: LeavesDef,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GroundDef {
    pub suelto: f32,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Forma {
    Bota,
    Disco,
    Caja,
    Barrido,
}

/// A kind of mark (`huellas.jsonc` says what each number is).
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KindDef {
    pub forma: Forma,
    #[serde(default = "boot_size")]
    pub tamano: [f32; 2],
    pub hondo: f32,
    pub suelo: f32,
    #[serde(default = "one")]
    pub borde: f32,
    #[serde(default)]
    pub borde_alto: f32,
    #[serde(default = "tenth")]
    pub borde_ancho: f32,
    #[serde(default)]
    pub barras: f32,
    #[serde(default)]
    pub barras_hondo: f32,
    pub dura: f32,
    pub ver: [f32; 2],
    #[serde(default = "one")]
    pub relieve: f32,
    #[serde(default)]
    pub grano: f32,
}

fn boot_size() -> [f32; 2] {
    [0.14, 0.33]
}

fn one() -> f32 {
    1.0
}

fn tenth() -> f32 {
    0.1
}

/// The share of a mark's life it fades over.
const FADE: f32 = 0.2;

impl KindDef {
    fn look(&self) -> Look {
        let shape = match self.forma {
            Forma::Bota => Shape::Boot,
            Forma::Disco => Shape::Disc,
            Forma::Caja => Shape::Box,
            Forma::Barrido => Shape::Swept,
        };
        Look { shape, floor: self.suelo, rim: self.borde, depth: self.hondo, rim_height: self.borde_alto, rim_width: self.borde_ancho, bars: self.barras, bars_depth: self.barras_hondo, life: self.dura, fade: FADE, seen: self.ver, relief: self.relieve, grain: self.grano }
    }
}

/// Who leaves which mark, and how.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LeavesDef {
    pub pisada: StepDef,
    pub pata: PadDef,
    pub bulto: CargoDef,
    pub chorro: JetDef,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StepDef {
    pub marca: String,
    pub adelante: f64,
    pub andar: f32,
    pub al_correr: f32,
    pub al_caer: f32,
    pub junta: f64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PadDef {
    pub marca: String,
    pub carga: f32,
    pub paso: f32,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CargoDef {
    pub marca: String,
    pub redondo: String,
    pub radio_max: f32,
    pub masa_min: f32,
    pub velocidad_max: f64,
    pub presion: f32,
    pub paso: f32,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JetDef {
    pub marca: String,
    pub radio: f32,
    pub desde: f32,
    pub tiempo: f32,
    pub junta: f32,
}

impl Defs {
    pub fn load(path: &Path) -> Result<Defs, String> {
        let d: Defs = lunar_core::defs::load(path).map_err(|e| format!("{}: {}", e.file, e.message))?;
        d.check().map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(d)
    }

    /// The number of the kind of mark called `name` (they are numbered in the order of their names).
    pub fn kind(&self, name: &str) -> Option<u8> {
        self.marcas.keys().position(|k| k == name).map(|k| k as u8)
    }

    pub fn check(&self) -> Result<(), String> {
        if !(16..=1 << 20).contains(&self.capacidad) {
            return Err(format!("capacidad {}: entre 16 y 1048576", self.capacidad));
        }
        if self.marcas.is_empty() || self.marcas.len() > prints::MAX_KINDS {
            return Err(format!("{} marcas: de 1 a {}", self.marcas.len(), prints::MAX_KINDS));
        }
        for (name, k) in &self.marcas {
            let ok = k.tamano.iter().all(|x| *x > 0.0) && k.hondo >= 0.0 && k.suelo > 0.0 && k.borde > 0.0 && k.borde_ancho > 0.0 && k.dura >= 0.0 && k.ver[0] > 0.0 && k.ver[1] > k.ver[0] && k.barras >= 0.0 && (0.0..=1.0).contains(&k.barras_hondo);
            if !ok {
                return Err(format!("marca '{name}': números fuera de lo que pueden ser"));
            }
        }
        let l = &self.deja;
        for name in [&l.pisada.marca, &l.pata.marca, &l.bulto.marca, &l.bulto.redondo, &l.chorro.marca] {
            if self.kind(name).is_none() {
                return Err(format!("deja: no hay marca '{name}'"));
            }
        }
        if l.pata.paso <= 0.0 || l.bulto.paso <= 0.0 || l.pata.carga <= 0.0 || l.bulto.presion <= 0.0 || l.chorro.tiempo <= 0.0 || l.chorro.radio <= 0.0 || l.chorro.junta <= 0.0 {
            return Err("deja: pasos, cargas y tiempos mayores que cero".into());
        }
        Ok(())
    }

    /// The grounds, as the dust reads them.
    pub fn grounds(&self) -> Grounds {
        Grounds { named: self.suelos.iter().filter(|(id, _)| id.as_str() != "*").map(|(id, g)| (id.clone(), g.suelto)).collect(), other: self.suelos.get("*").map_or(1.0, |g| g.suelto) }
    }
}

// ---- the marks kept

/// A mark left on the ground.
#[derive(Clone, Copy, Debug)]
pub struct Mark {
    /// Its middle, on the ground (world), the ground's normal there and the way it points along it.
    pub at: DVec3,
    pub normal: Vec3,
    pub ahead: Vec3,
    /// Half its width and its length (m).
    pub half: [f32; 2],
    /// Its kind (`Defs::kind`); `GONE`: nothing (its place is free).
    pub kind: u8,
    /// 1 a left one, -1 a right one (swept ground: any number, its streaks turn with it).
    pub side: f32,
    /// How hard it was pressed (1: as its kind says).
    pub hard: f32,
    /// When it was left (s, the world's clock).
    pub born: f64,
    pub seed: f32,
    /// Its place among the ones the GPU holds (`NONE`: it is not one of them).
    shown: u32,
}

impl Mark {
    #[allow(clippy::too_many_arguments)]
    pub fn new(kind: u8, at: DVec3, normal: DVec3, ahead: DVec3, half: [f32; 2], side: f32, hard: f32, born: f64, seed: f32) -> Mark {
        // (the way it points, on the ground: at right angles to the normal)
        let n = normal.normalize_or(DVec3::Y);
        let f = (ahead - n * ahead.dot(n)).normalize_or(n.any_orthonormal_vector());
        Mark { at, normal: n.as_vec3(), ahead: f.as_vec3(), half, kind, side, hard, born, seed, shown: NONE }
    }
}

const NONE: u32 = u32::MAX;
const GONE: u8 = u8::MAX;
/// A mark is one of the GPU's from this far (m) before it could be seen; the near ones are looked
/// for again `MARGIN` m before one that is not could be, and, with some shown, once the eye has
/// gone `SHED` m (the ones left behind go).
const AHEAD: f64 = 20.0;
const MARGIN: f64 = 5.0;
const SHED: f64 = 40.0;

/// Where the marks near the eye are written: the renderer (and, in tests, a list).
pub trait Sink {
    fn set(&mut self, anchor: DVec3, prints: &[Print]);
    fn put(&mut self, index: u32, print: &Print);
}

impl Sink for Renderer {
    fn set(&mut self, anchor: DVec3, prints: &[Print]) {
        self.set_prints(anchor, prints);
    }

    fn put(&mut self, index: u32, print: &Print) {
        self.put_print(index, print);
    }
}

/// The marks kept (a ring: the oldest goes when another needs its place) and the ones of them
/// near the eye, as the GPU holds them.
pub struct Marks {
    kinds: Vec<KindDef>,
    ring: Vec<Mark>,
    capacity: usize,
    /// Where the next one goes once the ring is full: the oldest's place.
    next: usize,
    alive: usize,
    /// The ring place of each of the GPU's, and they as it reads them (their places from `anchor`).
    shown: Vec<u32>,
    staged: Vec<Print>,
    /// The most the GPU holds.
    room: usize,
    anchor: DVec3,
    /// The near ones are to be looked for again; else, the ones of them to write.
    all: bool,
    puts: Vec<u32>,
    /// How far the eye can go from the anchor before a mark that is not shown could be seen (m),
    /// and when the first of the shown ones will be gone (s).
    free: f64,
    expires: f64,
    /// Times the near ones were looked for, and marks written one by one (what it all costs).
    pub culls: u64,
    pub writes: u64,
}

impl Marks {
    pub fn new(kinds: Vec<KindDef>, capacity: usize) -> Marks {
        let room = prints::CAPACITY.min(capacity);
        Marks { kinds, ring: Vec::with_capacity(capacity), capacity, next: 0, alive: 0, shown: Vec::with_capacity(room), staged: Vec::with_capacity(room), room, anchor: DVec3::ZERO, all: false, puts: Vec::with_capacity(64), free: f64::INFINITY, expires: f64::INFINITY, culls: 0, writes: 0 }
    }

    /// Marks kept, and how many of them the GPU holds.
    pub fn len(&self) -> usize {
        self.alive
    }

    pub fn shown(&self) -> usize {
        self.shown.len()
    }

    /// The marks kept, the newest first.
    pub fn iter(&self) -> impl Iterator<Item = &Mark> {
        let n = self.ring.len();
        (0..n).map(move |k| &self.ring[self.newest(k)]).filter(|m| m.kind != GONE)
    }

    /// The ring place of the `k`-th newest.
    fn newest(&self, k: usize) -> usize {
        let n = self.ring.len();
        (if n < self.capacity { 0 } else { self.next } + n - 1 - k) % n
    }

    fn print(&self, m: &Mark) -> Print {
        Print::new((m.at - self.anchor).as_vec3(), m.normal, m.ahead, m.half, m.kind, m.hard, prints::clock(m.born), m.seed, m.side)
    }

    /// How far a mark of kind `kind` is seen (m).
    fn reach(&self, kind: u8) -> f64 {
        f64::from(self.kinds[usize::from(kind)].ver[1])
    }

    /// A mark left: its place in the ring.
    pub fn add(&mut self, mut m: Mark) -> usize {
        m.shown = NONE;
        let mut place = NONE;
        let slot = if self.ring.len() < self.capacity {
            self.ring.push(m);
            self.alive += 1;
            self.ring.len() - 1
        } else {
            let slot = self.next;
            self.next = (self.next + 1) % self.capacity;
            let old = std::mem::replace(&mut self.ring[slot], m);
            self.alive += usize::from(old.kind == GONE);
            place = old.shown;
            slot
        };
        if self.all {
            return slot;
        }
        // near where the GPU's are kept from: it is one of them now, in the place of the one it
        // took the place of if that was one, else one more (no room: the near ones looked for again)
        let far = m.at.distance(self.anchor) - self.reach(m.kind);
        if far <= AHEAD {
            if place == NONE && self.shown.len() < self.room {
                place = self.shown.len() as u32;
                self.shown.push(slot as u32);
                self.staged.push(Print::default());
            }
            if place == NONE {
                self.all = true;
                return slot;
            }
            self.shown[place as usize] = slot as u32;
            self.ring[slot].shown = place;
            self.staged[place as usize] = self.print(&m);
            self.puts.push(place);
            let life = f64::from(self.kinds[usize::from(m.kind)].dura);
            if life > 0.0 {
                self.expires = self.expires.min(m.born + life);
            }
        } else {
            self.free = self.free.min((far - MARGIN).max(MARGIN));
            // (the one it took the place of is not to be drawn any more)
            if place != NONE {
                self.shown[place as usize] = NONE;
                self.staged[place as usize] = Print::default();
                self.puts.push(place);
            }
        }
        slot
    }

    /// The mark at ring place `slot`, to change it (`touch` after).
    pub fn get(&mut self, slot: usize) -> &mut Mark {
        &mut self.ring[slot]
    }

    /// The mark at `slot` changed: the GPU's copy of it, if it has one, is written again.
    pub fn touch(&mut self, slot: usize) {
        let m = self.ring[slot];
        if m.shown != NONE && !self.all {
            self.staged[m.shown as usize] = self.print(&m);
            self.puts.push(m.shown);
        }
    }

    /// The newest mark of kind `kind` within `within` m of `at`, among the last `back` left.
    pub fn near(&self, kind: u8, at: DVec3, within: f64, back: usize) -> Option<usize> {
        self.near_that(kind, at, within, back, |_| true)
    }

    /// ... and that is such (`that`): the newest of those.
    pub fn near_that(&self, kind: u8, at: DVec3, within: f64, back: usize, that: impl Fn(&Mark) -> bool) -> Option<usize> {
        let n = self.ring.len();
        (0..n.min(back)).map(|k| self.newest(k)).find(|&slot| {
            let m = &self.ring[slot];
            m.kind == kind && m.at.distance_squared(at) < within * within && that(m)
        })
    }

    /// The marks near `eye` looked for again: the ones whose time is up go, the near ones are
    /// the GPU's (the newest first, if there are more than it holds).
    fn cull(&mut self, eye: DVec3, now: f64) {
        self.culls += 1;
        self.all = false;
        self.anchor = eye;
        self.shown.clear();
        self.staged.clear();
        self.puts.clear();
        (self.free, self.expires) = (f64::INFINITY, f64::INFINITY);
        for k in 0..self.ring.len() {
            let slot = self.newest(k);
            let m = self.ring[slot];
            if m.kind == GONE {
                continue;
            }
            let life = f64::from(self.kinds[usize::from(m.kind)].dura);
            if life > 0.0 && now - m.born >= life {
                self.ring[slot].kind = GONE;
                self.ring[slot].shown = NONE;
                self.alive -= 1;
                continue;
            }
            let far = m.at.distance(eye) - self.reach(m.kind);
            if far <= AHEAD && self.shown.len() < self.room {
                self.ring[slot].shown = self.shown.len() as u32;
                self.shown.push(slot as u32);
                self.staged.push(self.print(&m));
                if life > 0.0 {
                    self.expires = self.expires.min(m.born + life);
                }
            } else {
                self.ring[slot].shown = NONE;
                self.free = self.free.min((far - MARGIN).max(MARGIN));
            }
        }
    }

    /// What changed since the last time goes to the GPU. With nothing left and the eye where no
    /// other mark comes near, nothing is written.
    pub fn sync(&mut self, eye: DVec3, now: f64, sink: &mut impl Sink) {
        let moved = eye.distance(self.anchor);
        // (with some shown, also when the eye has gone a way: the ones left behind go)
        if self.all || moved >= self.free.min(if self.shown.is_empty() { f64::INFINITY } else { SHED }) || now >= self.expires {
            self.cull(eye, now);
            sink.set(self.anchor, &self.staged);
            return;
        }
        for &i in &self.puts {
            sink.put(i, &self.staged[i as usize]);
        }
        self.writes += self.puts.len() as u64;
        self.puts.clear();
    }
}

// ---- who leaves them

/// The ground under `at`: the point of it there and its normal (its slope over `span` m).
pub fn ground_at(b: &Body, at: DVec3, span: f64) -> (DVec3, DVec3) {
    let up = b.up(at);
    let p = b.above_ground(up, 0.0);
    let x = up.any_orthonormal_vector();
    let z = up.cross(x);
    let (px, pz) = (b.above_ground(b.up(p + x * span), 0.0), b.above_ground(b.up(p + z * span), 0.0));
    (p, (px - p).cross(pz - p).normalize_or(up))
}

/// What of a structure presses the ground: its middle (world), the way its length runs (world),
/// half its width and length (m), whether it is round, and its area (m²).
#[derive(Clone, Copy, Debug)]
pub struct Sole {
    pub at: DVec3,
    pub along: DVec3,
    pub half: [f32; 2],
    pub round: bool,
    pub area: f32,
}

/// Corners within this of a structure's lowest (m) are on the ground with it.
const BAND: f32 = 0.05;
/// A sole is no smaller than this (m, half).
const LEAST: f32 = 0.04;

/// What of `s` presses the ground, `up` being up (world): of its parts on `bone` (none: all of
/// them), the corners within `BAND` of the lowest. `pts`: a list to work in.
pub fn sole(s: &Structure, bone: Option<u16>, up: DVec3, pts: &mut Vec<Vec3>) -> Option<Sole> {
    let n = (s.rot.inverse() * up.as_vec3()).normalize_or(Vec3::Y);
    pts.clear();
    for p in s.parts.iter().filter(|p| p.alive && !p.ghost && bone.map_or(p.collide, |b| p.bone == b)) {
        pts.extend(p.shape.verts().map(|v| p.local.transform_point3(v)));
    }
    let bottom = pts.iter().map(|v| v.dot(n)).fold(f32::MAX, f32::min);
    pts.retain(|v| v.dot(n) < bottom + BAND);
    if pts.is_empty() {
        return None;
    }
    // along the one of its own axes that lies flattest: a crate's dent is square with the crate
    // (of two that lie as flat, the first: the same one whichever way the rounding falls)
    let axis = [Vec3::Z, Vec3::Y].into_iter().fold(Vec3::X, |best, a| if a.dot(n).abs() < best.dot(n).abs() - 1e-3 { a } else { best });
    let f = (axis - n * axis.dot(n)).normalize_or(n.any_orthonormal_vector());
    let x = n.cross(f);
    let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
    for v in pts.iter() {
        for (k, a) in [x, f].into_iter().enumerate() {
            lo[k] = lo[k].min(v.dot(a));
            hi[k] = hi[k].max(v.dot(a));
        }
    }
    let mid = x * ((lo[0] + hi[0]) * 0.5) + f * ((lo[1] + hi[1]) * 0.5) + n * bottom;
    let far = |v: &Vec3| {
        let d = *v - mid;
        (d - n * d.dot(n)).length()
    };
    let (near, most) = pts.iter().fold((f32::MAX, 0.0f32), |(a, b), v| (a.min(far(v)), b.max(far(v))));
    // round: many corners, all as far from its middle (a pad, a drum on end); a leg's is its pad.
    // (Corners, not points: a shape may give a corner once for each face that meets at it, and
    // the four of a rectangle are all as far from its middle too.)
    let corners = (0..pts.len()).filter(|&i| pts[..i].iter().all(|o| o.distance_squared(pts[i]) > 1e-6)).count();
    let round = bone.is_some() || (corners >= 6 && most - near < 0.25 * most);
    let half = if round { [most.max(LEAST); 2] } else { [((hi[0] - lo[0]) * 0.5).max(LEAST), ((hi[1] - lo[1]) * 0.5).max(LEAST)] };
    let area = if round { std::f32::consts::PI * half[0] * half[0] } else { 4.0 * half[0] * half[1] };
    Some(Sole { at: s.to_world(mid), along: (s.rot * f).as_dvec3(), half, round, area })
}

/// The kinds each thing leaves (their numbers) and how.
struct Leaves {
    step: (u8, StepDef),
    pad: (u8, PadDef),
    cargo: (u8, u8, CargoDef),
    jet: (u8, JetDef),
}

/// A tread just left, for telling the next one's side: where, the way it faced, its side, when,
/// its place in the ring and whether its side was a guess.
#[derive(Clone, Copy)]
struct Trod {
    at: DVec3,
    facing: DVec3,
    side: f32,
    when: f64,
    slot: usize,
    guess: bool,
}

/// Treads remembered (one or two walkers' worth each of the last few who walked).
const TRODS: usize = 12;
/// A tread is the same walker's as one before it within this far and this long ago.
const SAME_WALKER: (f64, f64) = (1.7, 2.5);
/// Marks structures may leave each time they are looked at (a fleet come upon is marked over a
/// few moments, not in one frame), and how often they are looked at (s).
const STRUCTURE_MARKS: usize = 8;
const LOOK_EVERY: f64 = 0.1;
/// A sole farther than this from the ground (m) is not on it (it stands on something else).
const ON_GROUND: f64 = 0.3;
/// Loose things of more parts than this are not measured for their dent.
const MOST_PARTS: usize = 96;
/// Patches of swept ground looked at for the one a jet is sweeping.
const SWEPT_BACK: usize = 48;
/// A scrape is no longer than this (m: a longer drag is one after another), goes on while the
/// way keeps within this of where it began (the cosine of the turn), and is pressed this much of
/// what a thing set down is.
const SCRAPE: f64 = 4.0;
const SCRAPE_TURN: f64 = 0.94;
const SCRAPE_HARD: f32 = 0.85;

/// Where a leg or a loose thing last left its mark: where it was then (a leg's foot, a thing's
/// middle) and how big the mark is (m); and of a loose thing, the mark itself (its place in the
/// ring and when it was left: `usize::MAX`, none), where its sole was when it began it and the
/// way it has been dragged since (zero: not yet).
#[derive(Clone, Copy, Debug)]
struct Left {
    at: DVec3,
    size: f32,
    slot: usize,
    born: f64,
    from: DVec3,
    way: DVec3,
}

pub struct Footprints {
    pub marks: Marks,
    leaves: Leaves,
    trods: [Option<Trod>; TRODS],
    /// Where each leg (structure, leg) and each loose thing (structure, `u16::MAX`) last left its
    /// mark; the structures that have not moved since theirs were left.
    soles: HashMap<(u64, u16), Left>,
    settled: HashMap<u64, (DVec3, Quat)>,
    looked: f64,
    pts: Vec<Vec3>,
    rng: u64,
}

impl Footprints {
    /// The marks of `huellas.jsonc`: the dust is told what ground is loose, the renderer how the
    /// kinds of mark look.
    pub fn load(path: &Path, dust: &mut Dust, r: &mut Renderer) -> Result<Footprints, String> {
        let defs = Defs::load(path)?;
        let kinds: Vec<Kind> = defs.marcas.values().map(|k| Kind::new(&k.look())).collect();
        r.set_print_kinds(&kinds);
        Ok(Footprints::new(&defs, dust))
    }

    pub fn new(defs: &Defs, dust: &mut Dust) -> Footprints {
        dust.grounds = defs.grounds();
        let kind = |name: &str| defs.kind(name).unwrap_or(0);
        let l = &defs.deja;
        let leaves = Leaves { step: (kind(&l.pisada.marca), l.pisada.clone()), pad: (kind(&l.pata.marca), l.pata.clone()), cargo: (kind(&l.bulto.marca), kind(&l.bulto.redondo), l.bulto.clone()), jet: (kind(&l.chorro.marca), l.chorro.clone()) };
        Footprints { marks: Marks::new(defs.marcas.values().copied().collect(), defs.capacidad), leaves, trods: [None; TRODS], soles: HashMap::new(), settled: HashMap::new(), looked: f64::NEG_INFINITY, pts: Vec::new(), rng: 0x2545_F491_4F6C_DD1D }
    }

    /// 0..1.
    fn chance(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng >> 40) as f32 / (1u64 << 24) as f32
    }

    /// This frame's marks: of the boots set down, of the jets on the ground, of what stands or
    /// lies on it near `eye`; and what changed goes to the GPU. `walker`: the player's own body
    /// (its feet say which is which and the way each points).
    #[allow(clippy::too_many_arguments)]
    pub fn frame(&mut self, now: f64, dt: f64, dust: &mut Dust, bodies: &BodyRegistry, walker: Option<&crate::body::Body>, set: &Structures, eye: DVec3, r: &mut Renderer) {
        self.leave(now, dt, dust, bodies, walker, set, eye);
        self.marks.sync(eye, now, r);
    }

    /// The same without the GPU's part (`Marks::sync` is the caller's).
    #[allow(clippy::too_many_arguments)]
    pub fn leave(&mut self, now: f64, dt: f64, dust: &mut Dust, bodies: &BodyRegistry, walker: Option<&crate::body::Body>, set: &Structures, eye: DVec3) {
        for k in 0..dust.treads.len() {
            let t = dust.treads[k];
            // the player's own foot: the gait says which and the way it points
            let own = walker.and_then(|w| (0..2).find(|&i| w.gait.feet[i].at.distance_squared(t.at) < 1e-8).map(|i| (if i == 0 { 1.0 } else { -1.0 }, w.gait.feet[i].dir)));
            self.tread(now, bodies, &t, own);
        }
        dust.treads.clear();
        self.jets(now, dt, dust, bodies);
        if now - self.looked >= LOOK_EVERY || now < self.looked {
            self.looked = now;
            self.structures(now, dust, bodies, set, eye);
        }
    }

    /// A boot set down: its print. `own`: its side (1 left, -1 right) and the way it points, if
    /// known; else they are told from the tread before it.
    fn tread(&mut self, now: f64, bodies: &BodyRegistry, t: &Tread, own: Option<(f32, DVec3)>) {
        let (kind, d) = (self.leaves.step.0, self.leaves.step.1.clone());
        let b = bodies.get(t.body);
        let (p, n) = ground_at(b, t.at, 0.15);
        let level = |v: DVec3| v - n * v.dot(n);
        let going = level(t.heading);
        // the treads before it: the last of the same walker
        let before = self.trods.iter().flatten().filter(|o| o.at.distance(t.at) < SAME_WALKER.0 && now - o.when < SAME_WALKER.1 && now >= o.when).max_by(|a, b| a.when.total_cmp(&b.when)).copied();
        let (side, facing, guess) = match (own, before) {
            (Some((side, dir)), _) => (side, level(dir).normalize_or(going.normalize_or(n.any_orthonormal_vector())), false),
            (None, Some(o)) => {
                let facing = if going.length() > 0.3 { going.normalize() } else { level(o.facing).normalize_or(n.any_orthonormal_vector()) };
                // to the left of the one before, a left one; right where it was, the other foot
                let beside = (t.at - o.at).dot(n.cross(facing));
                (if beside.abs() > 0.03 { beside.signum() as f32 } else { -o.side }, facing, false)
            }
            (None, None) => (1.0, going.normalize_or(n.any_orthonormal_vector()), true),
        };
        // (a first one guessed to be a left one, and this one to its left: it was the right one)
        if let Some(o) = before.filter(|o| o.guess && o.side == side && own.is_none()) {
            self.marks.get(o.slot).side = -side;
            self.marks.touch(o.slot);
        }
        let speed = going.length() as f32;
        let hard = (1.0 + d.al_correr * (speed - d.andar).max(0.0) + d.al_caer * (t.hard - 1.0)).clamp(0.6, 1.8);
        let at = p + facing * d.adelante;
        let size = self.marks.kinds[usize::from(kind)].tamano;
        let seed = self.chance();
        let mark = Mark::new(kind, at, n, facing, [size[0] * 0.5, size[1] * 0.5], side, hard, now, seed);
        // set down where its last print is (standing, turning on the spot): that one, renewed
        // (the last print of the same foot: the other foot's may lie between)
        let slot = match self.marks.near_that(kind, at, d.junta, 8, |m| m.side == side) {
            Some(slot) => {
                let m = self.marks.get(slot);
                *m = Mark { shown: m.shown, hard: m.hard.max(hard), ..mark };
                self.marks.touch(slot);
                slot
            }
            None => self.marks.add(mark),
        };
        // remembered in the place of the oldest
        let oldest = (0..TRODS).min_by(|&a, &b| self.trods[a].map_or(f64::NEG_INFINITY, |o| o.when).total_cmp(&self.trods[b].map_or(f64::NEG_INFINITY, |o| o.when))).unwrap_or(0);
        self.trods[oldest] = Some(Trod { at: t.at, facing, side, when: now, slot, guess });
    }

    /// The ground swept by the jets that strike it this frame (`Dust::jets`).
    fn jets(&mut self, now: f64, dt: f64, dust: &Dust, bodies: &BodyRegistry) {
        if dust.jets().is_empty() {
            return;
        }
        let (kind, d) = (self.leaves.jet.0, self.leaves.jet.1.clone());
        let life = f64::from(self.marks.kinds[usize::from(kind)].dura);
        for &(at, body, hard, thrust) in dust.jets() {
            if hard < d.desde {
                continue;
            }
            let r = (d.radio * (thrust / 1000.0).sqrt()).clamp(0.25, 6.0);
            let more = hard * dt as f32 / d.tiempo;
            match self.marks.near(kind, at, f64::from(r * d.junta), SWEPT_BACK) {
                // swept more: written again only as it gets an eighth lighter, and when it has
                // begun to fade
                Some(slot) => {
                    let m = self.marks.get(slot);
                    let was = (m.hard * 8.0) as u32;
                    m.hard = (m.hard + more).min(1.0);
                    let stale = life > 0.0 && now - m.born > life * 0.25;
                    if (m.hard * 8.0) as u32 != was || stale {
                        m.born = now;
                        self.marks.touch(slot);
                    }
                }
                None => {
                    let (p, n) = ground_at(bodies.get(body), at, f64::from(r) * 0.7);
                    let turn = self.chance() * 2.0 - 1.0;
                    let seed = self.chance();
                    self.marks.add(Mark::new(kind, p, n, n.any_orthonormal_vector(), [r, r], turn, more.max(0.02), now, seed));
                }
            }
        }
    }

    /// The marks of what stands on legs or lies loose on the ground near `eye`: one under each
    /// leg that carries, one under what lies; another once it has moved a share of its size. What
    /// is dragged draws its mark out behind it.
    fn structures(&mut self, now: f64, dust: &Dust, bodies: &BodyRegistry, set: &Structures, eye: DVec3) {
        let (pad, pd) = (self.leaves.pad.0, self.leaves.pad.1.clone());
        let (flat, round, cd) = (self.leaves.cargo.0, self.leaves.cargo.1, self.leaves.cargo.2.clone());
        let within = self.marks.reach(pad).max(self.marks.reach(flat)) + AHEAD;
        let mut left = STRUCTURE_MARKS;
        // (what is gone is forgotten once the gone outnumber the rest)
        if self.settled.len() + self.soles.len() > 4 * set.list.len() + 256 {
            self.settled.retain(|id, _| set.get(*id).is_some());
            self.soles.retain(|(id, _), _| set.get(*id).is_some());
        }
        for s in &set.list {
            if left == 0 {
                break;
            }
            if s.anchored || s.held.is_some() || !(s.grounded || s.resting) {
                continue;
            }
            let centre = s.to_world(s.center);
            let reach = within + f64::from(s.radius);
            if centre.distance_squared(eye) > reach * reach || self.settled.get(&s.id).is_some_and(|at| at.0 == s.pos && at.1 == s.rot) {
                continue;
            }
            // (the ground it lies on: the one under it now)
            let Some(ground) = bodies.field(centre).ground else { continue };
            let loose = dust.loose(bodies, ground);
            let legs = s.springs.iter().any(|sp| sp.active);
            let small = legs || (s.radius <= cd.radio_max && s.mass >= cd.masa_min && s.parts.len() <= MOST_PARTS);
            let slow = s.vel.length() <= cd.velocidad_max;
            if loose > 0.0 && small && slow {
                let b = bodies.get(ground);
                let up = b.up(centre);
                // (come to rest: its mark is brought to where it stopped)
                let moved = |last: Option<&Left>, at: DVec3, step: f32| last.is_none_or(|l| l.at.distance(at) >= if s.resting { 0.02 } else { f64::from(l.size * step) });
                if legs {
                    for (i, sp) in s.springs.iter().enumerate().filter(|(_, sp)| sp.active && sp.load > 0.0) {
                        let foot = s.to_world(sp.foot + sp.axis * sp.x);
                        let key = (s.id, i as u16);
                        if !moved(self.soles.get(&key), foot, pd.paso) {
                            continue;
                        }
                        let Some(so) = sole(s, Some(sp.bone), up, &mut self.pts) else { continue };
                        self.soles.insert(key, Left { at: foot, size: so.half[0], slot: usize::MAX, born: now, from: so.at, way: DVec3::ZERO });
                        if b.altitude(so.at).abs() < ON_GROUND {
                            let hard = (sp.load / pd.carga).sqrt().clamp(0.5, 1.5) * loose;
                            self.press(pad, b, &so, hard, now);
                            left = left.saturating_sub(1);
                        }
                    }
                } else {
                    let key = (s.id, u16::MAX);
                    let last = self.soles.get(&key).copied();
                    if moved(last.as_ref(), centre, cd.paso)
                        && let Some(so) = sole(s, None, up, &mut self.pts)
                    {
                        let mut here = Left { at: centre, size: so.half[0].min(so.half[1]), slot: usize::MAX, born: now, from: so.at, way: DVec3::ZERO };
                        if b.altitude(so.at).abs() < ON_GROUND {
                            let pressure = s.mass * bodies.field(centre).g() as f32 / so.area.max(1e-3);
                            let hard = (pressure / cd.presion).sqrt().clamp(0.4, 1.4) * loose;
                            // dragged from where it left its last mark: that mark drawn out to here
                            match last.and_then(|l| self.drag(&l, &so, centre - l.at, b, flat, hard, now)) {
                                Some(l) => here = Left { at: centre, ..l },
                                None => {
                                    here.slot = self.press(if so.round { round } else { flat }, b, &so, hard, now);
                                    here.born = now;
                                    left = left.saturating_sub(1);
                                }
                            }
                        }
                        self.soles.insert(key, here);
                    }
                }
            }
            // lying still: not looked at again until it moves (a fast one is, when it slows)
            if s.resting && (slow || !small) {
                self.settled.insert(s.id, (s.pos, s.rot));
            }
        }
    }

    /// The mark of a sole pressed into the ground: its place in the ring.
    fn press(&mut self, kind: u8, b: &Body, so: &Sole, hard: f32, now: f64) -> usize {
        let (p, n) = ground_at(b, so.at, f64::from(so.half[0].max(so.half[1])).max(0.15) * 0.7);
        let seed = self.chance();
        self.marks.add(Mark::new(kind, p, n, so.along, so.half, 1.0, hard, now, seed))
    }

    /// A thing dragged from where it left the mark of `l` to where its sole `so` is now (`step`:
    /// how it has moved since it last touched that mark): the mark drawn out from the one place
    /// to the other (a scrape as wide as the sole is across the way), while it is still there,
    /// the thing keeps going the way the scrape runs and it is not too long. What is to be
    /// remembered of it; None: it is not to be drawn out (a new mark where it is).
    #[allow(clippy::too_many_arguments)]
    fn drag(&mut self, l: &Left, so: &Sole, step: DVec3, b: &Body, kind: u8, hard: f32, now: f64) -> Option<Left> {
        let m = *self.marks.ring.get(l.slot)?;
        let way = so.at - l.from;
        let len = way.length();
        if m.kind == GONE || m.born != l.born || !(0.02..=SCRAPE).contains(&len) {
            return None;
        }
        let (p, n) = ground_at(b, l.from + way * 0.5, (len * 0.35).max(0.15));
        let level = |v: DVec3| (v - n * v.dot(n)).normalize_or_zero();
        let d = level(way);
        if d == DVec3::ZERO || (l.way != DVec3::ZERO && level(step).dot(l.way) < SCRAPE_TURN) {
            return None;
        }
        // the sole as it lies across the way and along it
        let (f, x) = (so.along, n.cross(so.along));
        let (hx, hf) = (f64::from(so.half[0]), f64::from(so.half[1]));
        let across = n.cross(d);
        let wide = if so.round { hx } else { across.dot(x).abs() * hx + across.dot(f).abs() * hf };
        let long = if so.round { hx } else { d.dot(x).abs() * hx + d.dot(f).abs() * hf };
        let drawn = Mark::new(kind, p, n, d, [wide as f32, (len * 0.5 + long) as f32], 1.0, m.hard.max(hard * SCRAPE_HARD), now, m.seed);
        *self.marks.get(l.slot) = Mark { shown: m.shown, ..drawn };
        self.marks.touch(l.slot);
        Some(Left { born: now, way: d, ..*l })
    }

    /// What there is, in a line (tools and scripts).
    pub fn report(&self, r: &Renderer) -> String {
        let (drawn, bytes) = r.prints();
        format!("huellas: {} guardadas, {} cerca del ojo ({drawn} en la GPU, 1 dibujo), {} repasos de las cercanas, {} escritas una a una, {bytes} bytes escritos en total", self.marks.len(), self.marks.shown(), self.marks.culls, self.marks.writes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lunar_core::{
        body::BodyDef,
        defs,
        effects::{EffectDefs, Effects},
        structure::{
            convex::Convex,
            state::{Part, Spring},
        },
    };
    use std::sync::Arc;

    fn moon() -> BodyRegistry {
        let def: BodyDef = defs::parse("luna", include_str!("../../../assets/defs/bodies/luna.jsonc")).unwrap();
        BodyRegistry::new(vec![Body::from_def("luna", &def).unwrap()])
    }

    fn the_defs() -> Defs {
        Defs::load(&crate::root().join("assets/defs/huellas.jsonc")).unwrap()
    }

    fn effects() -> Effects {
        Effects::new(&EffectDefs::load(&crate::root().join("assets/defs")).unwrap(), 20_000)
    }

    /// What a renderer would be told.
    #[derive(Default)]
    struct Told {
        anchor: DVec3,
        prints: Vec<Print>,
        sets: usize,
        puts: usize,
    }

    impl Sink for Told {
        fn set(&mut self, anchor: DVec3, prints: &[Print]) {
            self.anchor = anchor;
            self.prints = prints.to_vec();
            self.sets += 1;
        }

        fn put(&mut self, index: u32, print: &Print) {
            let i = index as usize;
            assert!(i <= self.prints.len(), "escrita fuera de las que hay");
            if i == self.prints.len() {
                self.prints.push(*print);
            } else {
                self.prints[i] = *print;
            }
            self.puts += 1;
        }
    }

    impl Told {
        /// The ones that are something (a place given up is all zeros).
        fn drawn(&self) -> usize {
            self.prints.iter().filter(|p| **p != Print::default()).count()
        }
    }

    fn a_kind(dura: f32, ver: [f32; 2]) -> KindDef {
        KindDef { forma: Forma::Bota, tamano: [0.14, 0.33], hondo: 0.02, suelo: 0.87, borde: 1.06, borde_alto: 0.005, borde_ancho: 0.16, barras: 9.0, barras_hondo: 0.5, dura, ver, relieve: 1.0, grano: 0.5 }
    }

    fn a_mark(at: DVec3, born: f64) -> Mark {
        Mark::new(0, at, DVec3::Y, DVec3::Z, [0.07, 0.165], 1.0, 1.0, born, 0.5)
    }

    #[test]
    fn the_data_is_sound_and_every_ground_named_is_a_body() {
        let d = the_defs();
        d.check().unwrap();
        // the kinds are numbered in the order of their names, and each thing leaves one there is
        for (k, name) in d.marcas.keys().enumerate() {
            assert_eq!(d.kind(name), Some(k as u8));
        }
        assert!(d.kind("bota").is_some() && d.kind("pata").is_some() && d.kind("barrido").is_some() && d.kind("nada").is_none());
        // a boot's print is a boot's size; pressed regolith is darker, swept regolith lighter
        let boot = &d.marcas[&d.deja.pisada.marca];
        assert!(boot.forma == Forma::Bota && (0.1..0.2).contains(&boot.tamano[0]) && (0.25..0.4).contains(&boot.tamano[1]) && (0.01..0.04).contains(&boot.hondo) && boot.barras >= 6.0);
        assert!(boot.suelo < 1.0 && d.marcas[&d.deja.pata.marca].suelo < 1.0 && d.marcas[&d.deja.chorro.marca].suelo > 1.0);
        // the grounds named are bodies of the game; the ones there are, are loose (regolith)
        let bodies: Vec<String> = defs::load_dir::<BodyDef>(&crate::root().join("assets/defs/bodies")).unwrap().into_iter().map(|b| b.0).collect();
        let g = d.grounds();
        assert!(g.named.iter().all(|(id, _)| bodies.contains(id)), "suelos de cuerpos que no hay: {:?}", g.named);
        assert!(bodies.iter().all(|id| g.loose(id) > 0.0));
        // what is wrong is said
        let mut bad = d.clone();
        bad.deja.pata.marca = "zapato".into();
        assert!(bad.check().unwrap_err().contains("zapato"));
        let mut bad = d.clone();
        bad.marcas.get_mut("bota").unwrap().ver = [50.0, 30.0];
        assert!(bad.check().is_err());
    }

    #[test]
    fn the_ring_keeps_the_newest_and_the_oldest_goes() {
        let mut m = Marks::new(vec![a_kind(0.0, [30.0, 50.0])], 16);
        for k in 0..40 {
            m.add(a_mark(DVec3::new(f64::from(k), 0.0, 0.0), f64::from(k)));
        }
        assert_eq!(m.len(), 16);
        let kept: Vec<f64> = m.iter().map(|x| x.born).collect();
        assert_eq!(kept, (24..40).rev().map(f64::from).collect::<Vec<_>>());
        // the newest near a point, among the last few left
        assert_eq!(m.near(0, DVec3::new(38.2, 0.0, 0.0), 0.5, 4).map(|s| m.get(s).born), Some(38.0));
        assert!(m.near(0, DVec3::new(30.0, 0.0, 0.0), 0.5, 4).is_none() && m.near(0, DVec3::new(30.0, 0.0, 0.0), 0.5, 16).is_some());
        assert!(m.near(1, DVec3::new(38.0, 0.0, 0.0), 0.5, 16).is_none());
    }

    #[test]
    fn only_the_marks_near_the_eye_are_the_gpus_and_they_are_there_on_coming_back() {
        let mut m = Marks::new(vec![a_kind(0.0, [30.0, 50.0])], 4096);
        let mut gpu = Told::default();
        let here = DVec3::new(1_737_400.0, 20.0, -300.0);
        // with no mark, nothing is ever written
        for k in 0..100 {
            m.sync(here + DVec3::X * f64::from(k) * 7.0, 0.0, &mut gpu);
        }
        assert_eq!((gpu.sets, gpu.puts, m.culls), (0, 0, 0));
        // a walk of 200 prints, 0.7 m apart
        for k in 0..200 {
            let at = here + DVec3::X * 0.7 * f64::from(k);
            m.add(a_mark(at, f64::from(k)));
            m.sync(at + DVec3::Y * 1.7, f64::from(k), &mut gpu);
        }
        assert_eq!(m.len(), 200);
        // the ones behind, past where they are seen, are not the GPU's any more
        assert!(gpu.drawn() < 120 && gpu.drawn() > 70, "{} en la GPU", gpu.drawn());
        // each print was written once (64 bytes), and the near ones looked for a few times in 140 m
        assert!(m.writes <= 200 && m.culls <= 16, "{} escritas, {} repasos", m.writes, m.culls);
        // their places are kept from a point near the eye: metres, not the 1700 km of the world
        assert!(gpu.prints.iter().all(|p| p.floats()[..3].iter().all(|x| x.abs() < 80.0)));
        // standing still, nothing more is written
        let eye = here + DVec3::X * 139.3 + DVec3::Y * 1.7;
        m.sync(eye, 200.0, &mut gpu);
        let (sets, puts) = (gpu.sets, gpu.puts);
        for _ in 0..600 {
            m.sync(eye, 200.0, &mut gpu);
        }
        assert_eq!((gpu.sets, gpu.puts), (sets, puts));
        // far away: none of them is drawn, and once there nothing is looked for again
        let away = here + DVec3::Z * 3000.0;
        m.sync(away, 201.0, &mut gpu);
        assert_eq!(gpu.drawn(), 0);
        let culls = m.culls;
        for k in 0..300 {
            m.sync(away + DVec3::X * f64::from(k), 202.0, &mut gpu);
        }
        assert_eq!((m.culls, gpu.drawn()), (culls, 0), "lejos de toda huella no hay nada que repasar");
        // back at the start of the walk: its prints are there again, each where it was left
        m.sync(here + DVec3::Y * 1.7, 300.0, &mut gpu);
        assert!(gpu.drawn() > 70, "{} al volver", gpu.drawn());
        let first = gpu.prints.last().unwrap().floats();
        assert!((DVec3::new(f64::from(first[0]), f64::from(first[1]), f64::from(first[2])) + gpu.anchor).distance(here) < 1e-3, "la primera huella no está donde se dejó");
    }

    #[test]
    fn the_gpu_never_holds_more_than_its_room_and_keeps_the_newest() {
        let mut m = Marks::new(vec![a_kind(0.0, [30.0, 50.0])], 8192);
        let mut gpu = Told::default();
        let eye = DVec3::new(0.0, 1_737_400.0, 0.0);
        // six thousand prints within ten metres of the eye
        for k in 0..6000 {
            let a = f64::from(k) * 0.37;
            m.add(a_mark(eye + DVec3::new(a.cos(), 0.0, a.sin()) * (f64::from(k % 100) * 0.1), f64::from(k)));
            if k % 50 == 0 {
                m.sync(eye, f64::from(k), &mut gpu);
            }
        }
        m.sync(eye, 6000.0, &mut gpu);
        assert_eq!(m.len(), 6000);
        assert_eq!(gpu.prints.len(), prints::CAPACITY);
        assert_eq!(m.shown(), prints::CAPACITY);
        // the ones kept for it are the newest: the oldest shown is no older than what fits
        let oldest = gpu.prints.iter().map(|p| p.floats()[11]).fold(f32::MAX, f32::min);
        assert!(oldest >= (6000 - prints::CAPACITY) as f32 - 60.0, "la más vieja en la GPU es la {oldest}");
        // the ring full: a new one takes the oldest's place, in the ring and (if shown) on the GPU
        let mut m = Marks::new(vec![a_kind(0.0, [30.0, 50.0])], 64);
        let mut gpu = Told::default();
        for k in 0..64 {
            m.add(a_mark(eye + DVec3::X * f64::from(k) * 0.1, f64::from(k)));
        }
        m.sync(eye, 64.0, &mut gpu);
        let (sets, puts) = (gpu.sets, gpu.puts);
        for k in 64..200 {
            m.add(a_mark(eye + DVec3::X * f64::from(k % 64) * 0.1, f64::from(k)));
            m.sync(eye, f64::from(k), &mut gpu);
        }
        assert_eq!((m.len(), gpu.prints.len(), gpu.sets, gpu.puts), (64, 64, sets, puts + 136));
    }

    #[test]
    fn marks_go_when_their_time_is_up_and_the_shader_is_told_when_each_was_left() {
        let mut m = Marks::new(vec![a_kind(60.0, [30.0, 50.0]), a_kind(0.0, [30.0, 50.0])], 256);
        let mut gpu = Told::default();
        let eye = DVec3::new(0.0, 1_737_400.0, 0.0);
        m.add(a_mark(eye + DVec3::X, 10.0));
        m.add(Mark { kind: 1, ..a_mark(eye + DVec3::Z, 10.0) });
        m.add(a_mark(eye - DVec3::X, 40.0));
        m.sync(eye, 40.0, &mut gpu);
        assert_eq!((m.len(), gpu.drawn()), (3, 3));
        // dated with the frame's clock (the shader fades it by its age: nothing is written for that)
        assert!(gpu.prints.iter().any(|p| p.floats()[11] == 40.0));
        let sets = gpu.sets;
        m.sync(eye, 69.0, &mut gpu);
        assert_eq!((gpu.sets, m.len()), (sets, 3));
        // the first one's minute is up: it goes, without the eye moving; then the other
        m.sync(eye, 70.5, &mut gpu);
        assert_eq!((m.len(), gpu.drawn(), gpu.sets), (2, 2, sets + 1));
        m.sync(eye, 101.0, &mut gpu);
        assert_eq!((m.len(), gpu.drawn()), (1, 1));
        // what lasts for ever is still there a year later, and nothing is looked for again
        let culls = m.culls;
        m.sync(eye, 3.0e7, &mut gpu);
        assert_eq!((m.len(), gpu.drawn(), m.culls), (1, 1, culls));
    }

    /// A walk along `ahead` from `from`: boots set down left and right of the line, `stride` m
    /// apart, each through the dust as the game does.
    #[allow(clippy::too_many_arguments)]
    fn walk(f: &mut Footprints, dust: &mut Dust, fx: &mut Effects, bodies: &BodyRegistry, set: &Structures, from: DVec3, ahead: DVec3, steps: usize, stride: f64, speed: f64, t0: f64) {
        let b = bodies.get(0);
        for k in 0..steps {
            let up = b.up(from);
            let left = up.cross(ahead);
            let side = if k % 2 == 0 { 1.0 } else { -1.0 };
            let at = b.above_ground(b.up(from + ahead * stride * k as f64 + left * 0.11 * side), 0.0);
            dust.step(fx, bodies, at, ahead * speed, 1.0);
            f.leave(t0 + k as f64 * 0.5, 1.0 / 60.0, dust, bodies, None, set, at);
        }
    }

    fn world() -> (Structures, BodyRegistry) {
        let lib = lunar_core::structure::Library::load(&crate::root().join("assets/defs/structures")).unwrap();
        (Structures::new(Arc::new(lib)), moon())
    }

    #[test]
    fn boots_leave_left_and_right_prints_turned_the_way_they_go_and_deeper_at_a_run_or_a_landing() {
        let ((set, bodies), mut fx) = (world(), effects());
        let b = bodies.get(0);
        let mut dust = Dust::new(&fx);
        let mut f = Footprints::new(&the_defs(), &mut dust);
        let up = DVec3::new(0.2, 0.9, -0.1).normalize();
        let from = b.above_ground(up, 0.0);
        let ahead = up.any_orthonormal_vector();
        let left = up.cross(ahead);
        walk(&mut f, &mut dust, &mut fx, &bodies, &set, from, ahead, 10, 0.7, 1.6, 0.0);
        assert_eq!(f.marks.len(), 10);
        assert!(dust.treads.is_empty(), "las pisadas se recogen");
        let marks: Vec<Mark> = f.marks.iter().copied().collect();
        for (k, m) in marks.iter().rev().enumerate() {
            // on the ground, flat on it, pointing the way it went; left and right in turn, each
            // on its side of the line
            assert!(b.altitude(m.at).abs() < 0.02, "huella a {:.3} m del suelo", b.altitude(m.at));
            assert!(m.normal.as_dvec3().dot(b.up(m.at)) > 0.9 && m.normal.dot(m.ahead).abs() < 1e-4);
            // (it lies on the slope: it is its heading over level ground that is the walker's)
            let level = (m.ahead.as_dvec3() - b.up(m.at) * m.ahead.as_dvec3().dot(b.up(m.at))).normalize();
            assert!(level.dot(ahead) > 0.97, "huella {k} girada: {:.3} con la marcha", level.dot(ahead));
            let beside = (m.at - from).dot(left);
            assert_eq!(m.side, if k % 2 == 0 { 1.0 } else { -1.0 }, "huella {k}");
            assert!(beside * f64::from(m.side) > 0.05, "la huella {k} no está en su lado");
            assert!((m.half[0] - 0.07).abs() < 0.02 && (m.half[1] - 0.165).abs() < 0.03);
            assert_eq!(m.hard, 1.0);
        }
        // at a run they are deeper, and deeper still coming down from a jump
        walk(&mut f, &mut dust, &mut fx, &bodies, &set, from + left * 5.0, ahead, 2, 1.4, 4.0, 20.0);
        let run = f.marks.iter().next().unwrap().hard;
        let at = b.above_ground(b.up(from + left * 9.0), 0.0);
        dust.step(&mut fx, &bodies, at, DVec3::ZERO, 3.0);
        f.leave(30.0, 1.0 / 60.0, &mut dust, &bodies, None, &set, at);
        let landing = f.marks.iter().next().unwrap().hard;
        assert!(run > 1.15 && landing > run && landing <= 1.8, "corriendo {run}, cayendo {landing}");
        // standing on the spot (the same foot set down where its print is): no more prints
        let n = f.marks.len();
        for k in 0..20 {
            dust.step(&mut fx, &bodies, at + ahead * 0.01 * f64::from(k % 3), DVec3::ZERO, 1.0);
            f.leave(31.0 + f64::from(k) * 0.1, 1.0 / 60.0, &mut dust, &bodies, None, &set, at);
        }
        assert!(f.marks.len() <= n + 1, "{} huellas más sin moverse del sitio", f.marks.len() - n);
    }

    #[test]
    fn the_first_foot_guessed_wrong_is_put_right_by_the_second() {
        let ((set, bodies), mut fx) = (world(), effects());
        let b = bodies.get(0);
        let mut dust = Dust::new(&fx);
        let mut f = Footprints::new(&the_defs(), &mut dust);
        let up = DVec3::Y;
        let ahead = up.any_orthonormal_vector();
        let left = up.cross(ahead);
        // both feet down at once from a jump, the right one told first
        let (r, l) = (b.above_ground(b.up(b.above_ground(up, 0.0) - left * 0.12), 0.0), b.above_ground(b.up(b.above_ground(up, 0.0) + left * 0.12), 0.0));
        dust.step(&mut fx, &bodies, r, ahead * 0.5, 2.0);
        dust.step(&mut fx, &bodies, l, ahead * 0.5, 2.0);
        f.leave(1.0, 1.0 / 60.0, &mut dust, &bodies, None, &set, r);
        let sides: Vec<(f32, f64)> = f.marks.iter().map(|m| (m.side, (m.at - r).dot(left))).collect();
        assert_eq!(sides.len(), 2);
        assert!(sides.iter().all(|&(side, beside)| (side > 0.0) == (beside > 0.12)), "{sides:?}");
    }

    #[test]
    fn hard_ground_raises_no_dust_and_takes_no_marks() {
        let ((set, bodies), mut fx) = (world(), effects());
        let b = bodies.get(0);
        let mut dust = Dust::new(&fx);
        let mut d = the_defs();
        d.suelos.insert("luna".into(), GroundDef { suelto: 0.0 });
        let mut f = Footprints::new(&d, &mut dust);
        assert_eq!(dust.loose(&bodies, 0), 0.0);
        let at = b.above_ground(DVec3::Y, 0.0);
        dust.step(&mut fx, &bodies, at, DVec3::X, 2.0);
        dust.strike(&bodies, b.above_ground(DVec3::Y, 1.0), -DVec3::Y, 45_000.0);
        f.leave(0.0, 1.0 / 60.0, &mut dust, &bodies, None, &set, at);
        assert_eq!((fx.particles.len(), f.marks.len(), dust.jets().len()), (0, 0, 0));
        // and a boot set down on a deck high over loose ground leaves none on the ground
        let mut dust = Dust::new(&fx);
        let mut f = Footprints::new(&the_defs(), &mut dust);
        dust.step(&mut fx, &bodies, b.above_ground(DVec3::Y, 2.5), DVec3::X, 1.0);
        f.leave(0.0, 1.0 / 60.0, &mut dust, &bodies, None, &set, at);
        assert_eq!(f.marks.len(), 0);
    }

    #[test]
    fn a_jet_sweeps_a_patch_that_grows_lighter_and_is_written_a_few_times_only() {
        let ((set, bodies), fx) = (world(), effects());
        let b = bodies.get(0);
        let mut dust = Dust::new(&fx);
        let mut f = Footprints::new(&the_defs(), &mut dust);
        let mut gpu = Told::default();
        let eye = b.above_ground(DVec3::Y, 1.7);
        // the pack a metre over the ground for three seconds
        for k in 0..180 {
            dust.strike(&bodies, b.above_ground(DVec3::Y, 1.0), -DVec3::Y, 630.0);
            f.leave(f64::from(k) / 60.0, 1.0 / 60.0, &mut dust, &bodies, None, &set, eye);
            f.marks.sync(eye, f64::from(k) / 60.0, &mut gpu);
        }
        assert_eq!(f.marks.len(), 1, "un solo barrido bajo un chorro quieto");
        let m = *f.marks.iter().next().unwrap();
        assert!(m.hard == 1.0 && (0.3..0.8).contains(&m.half[0]) && b.altitude(m.at).abs() < 0.05, "{m:?}");
        assert!(gpu.sets + gpu.puts <= 12, "{} escrituras para un barrido", gpu.sets + gpu.puts);
        // a lander's engine: a patch of metres; too high, none
        dust.strike(&bodies, b.above_ground(DVec3::X, 3.0), -DVec3::X, 45_000.0);
        f.leave(4.0, 1.0 / 60.0, &mut dust, &bodies, None, &set, eye);
        assert!(f.marks.iter().next().unwrap().half[0] > 3.0);
        let n = f.marks.len();
        dust.strike(&bodies, b.above_ground(DVec3::Z, 39.0), -DVec3::Z, 45_000.0);
        f.leave(5.0, 1.0 / 60.0, &mut dust, &bodies, None, &set, eye);
        assert_eq!(f.marks.len(), n);
        // a pack that moves along sweeps a streak: patch after patch
        for k in 0..240 {
            let dir = (DVec3::Y + DVec3::X * 2e-8 * f64::from(k) + DVec3::Z * 0.3).normalize();
            dust.strike(&bodies, b.above_ground(dir, 0.8), -dir, 630.0);
            f.leave(6.0 + f64::from(k) / 60.0, 1.0 / 60.0, &mut dust, &bodies, None, &set, eye);
        }
        assert!(f.marks.len() > n + 3, "{} barridos en la pasada", f.marks.len() - n);
    }

    /// A box of `half` lying on the ground at `dir`, loose, of 300 kg.
    fn a_box(set: &mut Structures, bodies: &BodyRegistry, dir: DVec3, half: Vec3) -> u64 {
        let b = bodies.get(0);
        let lib = set.lib.clone();
        let id = set.next_id();
        let part = Part::new(&lib.catalog, 0, glam::Affine3A::IDENTITY, Some(Convex::cuboid(half)), false);
        let rot = lunar_core::scene::basis(dir, dir.any_orthonormal_vector());
        let mut s = Structure::assemble(id, "caja".into(), b.above_ground(dir, f64::from(half.y)), rot, false, vec![part], Vec::new());
        (s.grounded, s.resting, s.mass) = (true, true, 300.0);
        set.list.push(s);
        id
    }

    #[test]
    fn what_is_set_down_leaves_its_dent_and_what_is_dragged_draws_it_out() {
        let ((mut set, bodies), fx) = (world(), effects());
        let b = bodies.get(0);
        let mut dust = Dust::new(&fx);
        let mut f = Footprints::new(&the_defs(), &mut dust);
        let mut gpu = Told::default();
        let dir = DVec3::new(0.05, 1.0, 0.02).normalize();
        let id = a_box(&mut set, &bodies, dir, Vec3::new(0.6, 0.4, 0.35));
        let eye = b.above_ground(dir, 1.7);
        for k in 0..30 {
            f.leave(f64::from(k) * 0.1, 0.1, &mut dust, &bodies, None, &set, eye);
            f.marks.sync(eye, f64::from(k) * 0.1, &mut gpu);
        }
        // one dent, as big as its base, lying as it lies
        assert_eq!(f.marks.len(), 1);
        let m = *f.marks.iter().next().unwrap();
        let s = set.get(id).unwrap();
        let (along, start) = ((s.rot * Vec3::X).as_dvec3(), b.above_ground(dir, 0.0));
        assert!(m.kind == f.leaves.cargo.0 && (m.half[0] - 0.35).abs() < 0.01 && (m.half[1] - 0.6).abs() < 0.01, "{m:?}");
        assert!(m.ahead.as_dvec3().dot(along).abs() > 0.99 && m.at.distance(start) < 0.05 && (0.4..=1.4).contains(&m.hard));
        // dragged three metres along the ground: the same mark drawn out, from where it was to
        // where it is, written a few times (not every frame)
        let written = gpu.sets + gpu.puts;
        let drag = |set: &mut Structures, f: &mut Footprints, dust: &mut Dust, gpu: &mut Told, way: DVec3, t0: f64| {
            for k in 0..300 {
                let s = set.list.iter_mut().find(|s| s.id == id).unwrap();
                (s.resting, s.vel) = (false, way);
                s.pos = b.above_ground(b.up(s.pos + way * 0.01), 0.4);
                f.leave(t0 + f64::from(k) * 0.1, 0.1, dust, &bodies, None, set, eye);
                f.marks.sync(eye, t0 + f64::from(k) * 0.1, gpu);
            }
            set.list.iter_mut().for_each(|s| (s.resting, s.vel) = (true, DVec3::ZERO));
            f.leave(t0 + 31.0, 0.1, dust, &bodies, None, set, eye);
            f.marks.sync(eye, t0 + 31.0, gpu);
        };
        drag(&mut set, &mut f, &mut dust, &mut gpu, along, 3.0);
        assert_eq!(f.marks.len(), 1, "arrastrada, su marca se alarga: no deja otras");
        let m = *f.marks.iter().next().unwrap();
        assert!((m.half[1] - 2.1).abs() < 0.05 && (m.half[0] - 0.35).abs() < 0.02, "arrastre de {:.2} x {:.2} m", m.half[0] * 2.0, m.half[1] * 2.0);
        // (over level ground: the ground it lies on rises and falls under it)
        let up = b.up(start);
        let flat = |v: DVec3| v - up * v.dot(up);
        assert!(flat(m.ahead.as_dvec3()).normalize().dot(along) > 0.99 && flat(m.at - (start + along * 1.5)).length() < 0.1, "{m:?}");
        assert!((10..=24).contains(&(gpu.sets + gpu.puts - written)), "{} escrituras en tres metros", gpu.sets + gpu.puts - written);
        // left there, nothing more is done
        let (n, written) = (f.marks.len(), gpu.sets + gpu.puts);
        for k in 0..50 {
            f.leave(40.0 + f64::from(k) * 0.1, 0.1, &mut dust, &bodies, None, &set, eye);
            f.marks.sync(eye, 40.0 + f64::from(k) * 0.1, &mut gpu);
        }
        assert_eq!((f.marks.len(), gpu.sets + gpu.puts), (n, written));
        // dragged off across the way it came: another mark, and a scrape is only so long
        let across = dir.cross(along);
        drag(&mut set, &mut f, &mut dust, &mut gpu, across, 50.0);
        drag(&mut set, &mut f, &mut dust, &mut gpu, across, 90.0);
        let lens: Vec<f32> = f.marks.iter().map(|m| m.half[1] * 2.0).collect();
        assert!(f.marks.len() >= 3 && lens.iter().all(|l| f64::from(*l) <= SCRAPE + 1.5), "{lens:?}");
        let n = f.marks.len();
        // far from the eye nothing is marked; in the air, neither
        let far = a_box(&mut set, &bodies, DVec3::new(0.0, 1.0, 0.01).normalize(), Vec3::splat(0.3));
        let aloft = a_box(&mut set, &bodies, DVec3::new(0.051, 1.0, 0.02).normalize(), Vec3::splat(0.3));
        set.list.iter_mut().filter(|s| s.id == aloft).for_each(|s| (s.grounded, s.resting, s.pos) = (false, false, s.pos + dir * 3.0));
        for k in 0..20 {
            f.leave(130.0 + f64::from(k) * 0.1, 0.1, &mut dust, &bodies, None, &set, eye);
        }
        assert_eq!(f.marks.len(), n, "marcas de lo que está lejos ({far}) o en el aire ({aloft})");
        // a drum stood on end: a round one
        let drum = set.next_id();
        // (a few metres from the box: within sight of the eye that is over where that one began)
        let dir2 = DVec3::new(0.05005, 1.0, 0.02005).normalize();
        let lib = set.lib.clone();
        let part = Part::new(&lib.catalog, 0, glam::Affine3A::IDENTITY, Some(Convex::prism(0.3, 0.45, 12, 1.0)), false);
        let mut s = Structure::assemble(drum, "bidon".into(), b.above_ground(dir2, 0.45), lunar_core::scene::basis(dir2, dir2.any_orthonormal_vector()), false, vec![part], Vec::new());
        (s.grounded, s.resting, s.mass) = (true, true, 200.0);
        set.list.push(s);
        for k in 0..20 {
            f.leave(140.0 + f64::from(k) * 0.1, 0.1, &mut dust, &bodies, None, &set, eye);
        }
        let m = *f.marks.iter().next().unwrap();
        assert!(f.marks.len() == n + 1 && m.kind == f.leaves.cargo.1 && (m.half[0] - 0.3).abs() < 0.02 && m.half[0] == m.half[1], "{m:?}");
    }

    #[test]
    fn landing_pads_leave_their_prints_where_a_ship_sets_down() {
        let ((mut set, bodies), fx) = (world(), effects());
        let b = bodies.get(0);
        let mut dust = Dust::new(&fx);
        let mut f = Footprints::new(&the_defs(), &mut dust);
        let dir = DVec3::new(-0.03, 1.0, 0.04).normalize();
        // a hull on four legs: a pad under each corner, 1.2 m under its floor
        let lib = set.lib.clone();
        let cat = &lib.catalog;
        let id = set.next_id();
        let mut parts = vec![Part::new(cat, 0, glam::Affine3A::from_translation(Vec3::new(0.0, 1.6, 0.0)), Some(Convex::cuboid(Vec3::new(1.0, 0.4, 1.2))), false)];
        let feet = [Vec3::new(0.8, 0.0, 1.0), Vec3::new(-0.8, 0.0, 1.0), Vec3::new(0.8, 0.0, -1.0), Vec3::new(-0.8, 0.0, -1.0)];
        for (k, at) in feet.iter().enumerate() {
            let mut pad = Part::new(cat, 0, glam::Affine3A::from_translation(*at + Vec3::Y * 0.05), Some(Convex::prism(0.25, 0.05, 16, 1.0)), false);
            pad.bone = k as u16 + 1;
            parts.push(pad);
        }
        let mut s = Structure::assemble(id, "nave".into(), b.above_ground(dir, 0.0), lunar_core::scene::basis(dir, dir.any_orthonormal_vector()), false, parts, Vec::new());
        for (k, at) in feet.iter().enumerate() {
            s.springs.push(Spring { bone: k as u16 + 1, foot: *at, axis: Vec3::Y, stroke: 0.4, preload: 2000.0, end: 60000.0, gas: 1.3, damp: [4000.0, 9000.0], grip: 0.8, active: true, x: 0.0, load: 0.0 });
        }
        s.pos += dir * 2.0;
        set.list.push(s);
        let eye = b.above_ground(dir, 1.7) + dir.any_orthonormal_vector() * 30.0;
        // coming down, its legs carrying nothing: no marks
        for k in 0..10 {
            f.leave(f64::from(k) * 0.1, 0.1, &mut dust, &bodies, None, &set, eye);
        }
        assert_eq!(f.marks.len(), 0);
        // down: its weight on its four legs
        {
            let s = &mut set.list[0];
            s.pos -= dir * 2.0;
            (s.grounded, s.resting) = (true, true);
            s.springs.iter_mut().for_each(|sp| sp.load = 30_000.0);
        }
        for k in 0..10 {
            f.leave(2.0 + f64::from(k) * 0.1, 0.1, &mut dust, &bodies, None, &set, eye);
        }
        assert_eq!(f.marks.len(), 4);
        let s = set.get(id).unwrap();
        for m in f.marks.iter() {
            // round, as big as the pad, under one of them, on the ground; pressed by its load
            assert!(m.kind == f.leaves.pad.0 && (m.half[0] - 0.25).abs() < 0.02 && m.half[0] == m.half[1], "{m:?}");
            assert!(feet.iter().any(|at| s.to_world(*at).distance(m.at) < 0.3), "una huella que no está bajo ninguna pata");
            assert!(b.altitude(m.at).abs() < 0.02 && m.hard > 1.0);
        }
        // it stays: nothing more; it slides a metre: four more; it lifts off: they stay
        for k in 0..50 {
            f.leave(4.0 + f64::from(k) * 0.1, 0.1, &mut dust, &bodies, None, &set, eye);
        }
        assert_eq!(f.marks.len(), 4);
        set.list[0].pos += dir.any_orthonormal_vector() * 1.0;
        f.leave(10.0, 0.1, &mut dust, &bodies, None, &set, eye);
        assert_eq!(f.marks.len(), 8);
        set.list[0].pos += dir * 50.0;
        (set.list[0].grounded, set.list[0].resting) = (false, false);
        set.list[0].springs.iter_mut().for_each(|sp| sp.load = 0.0);
        f.leave(11.0, 0.1, &mut dust, &bodies, None, &set, eye);
        assert_eq!(f.marks.len(), 8);
        // a fleet come upon: marked a few at a time, all of them in the end
        for k in 0..40 {
            let d = (dir + dir.any_orthonormal_vector() * 2e-6 * f64::from(k + 2)).normalize();
            let mut s = Structure::assemble(set.next_id(), "nave".into(), b.above_ground(d, 0.0), Quat::IDENTITY, false, vec![Part::new(cat, 0, glam::Affine3A::IDENTITY, Some(Convex::prism(0.4, 0.05, 12, 1.0)), false)], Vec::new());
            s.parts[0].bone = 1;
            s.springs.push(Spring { bone: 1, foot: Vec3::ZERO, axis: Vec3::Y, stroke: 0.4, preload: 2000.0, end: 60000.0, gas: 1.3, damp: [4000.0, 9000.0], grip: 0.8, active: true, x: 0.0, load: 9000.0 });
            (s.grounded, s.resting) = (true, true);
            set.list.push(s);
        }
        f.leave(12.0, 0.1, &mut dust, &bodies, None, &set, eye);
        assert_eq!(f.marks.len(), 8 + STRUCTURE_MARKS);
        for k in 0..10 {
            f.leave(12.2 + f64::from(k) * 0.1, 0.1, &mut dust, &bodies, None, &set, eye);
        }
        assert_eq!(f.marks.len(), 48);
    }
}
