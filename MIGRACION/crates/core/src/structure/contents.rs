//! What things hold, and what it weighs: water in a drum, propellant in a tank, regolith in
//! sacks, spares in a crate. Nothing of it is written in the container: a registry says what
//! substances there are (`SubstanceDef`, `assets/defs/sustancias.jsonc`: how dense, how it lies,
//! how it is counted, what it does when its container is destroyed), a part kind says which one
//! it holds and how much (`ContainerDef`), and a structure keeps how much is left in each of its
//! parts that hold something (`Stored`, `Structure::stored`).
//!
//! What is held weighs: a part's mass is its shell's, and what it holds is a block of its own
//! inside it — lying on the side of the container that is down as its structure stands, as deep
//! as its share of the room — whose mass, place and inertia go into the structure's
//! (`Structure::mass_props`). The amount changes through `Structure::take`, `put` and `fill`
//! and nowhere else; mass properties are worked out again only when it has changed by a
//! quantum (`QUANTUM`) since they last were, never while nothing changes. And then only its own
//! block is looked at: the structure keeps its mass, first moment and inertia as sums about its
//! origin (`Sums`), what the block counted for is taken out of them and what it counts for now
//! put in (`Structure::reweigh`): a tank that drains costs its ship a handful of sums a
//! quantum, however many parts the ship has.
//!
//! Approximations, said once: the block is a box (a lying cylinder fills like a box of its
//! section); it does not slosh, it turns with its container (a drum on its side keeps its
//! water where it was); a gas fills its container whole.
use super::{
    catalog::{BurstDef, Catalog},
    convex::Convex,
    state::{Part, Structure},
};
use glam::{Affine3A, DMat3, DVec3, Mat3, Vec3, Vec3A};
use serde::Deserialize;

/// Mass properties are worked out again when what a part holds has changed by this share of
/// its capacity since they last were (or it has just run empty, or been filled).
pub const QUANTUM: f32 = 0.005;

/// Under this share of its capacity a container is spent: what it held does nothing more when
/// the container is destroyed (an empty drum of propellant does not burst).
pub const SPENT: f32 = 0.02;

/// How a substance lies in what holds it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub enum State {
    #[serde(rename = "liquido")]
    Liquid,
    /// A loose solid: soil, grain, powder.
    #[serde(rename = "granel")]
    Bulk,
    /// Things packed in it: spares, rations.
    #[serde(rename = "piezas")]
    Items,
    #[serde(rename = "gas")]
    Gas,
}

/// How a substance is counted for whoever reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub enum Measure {
    #[serde(rename = "L")]
    Litres,
    #[serde(rename = "kg")]
    Kilograms,
}

/// What a container of a substance lets go of when it is destroyed, by what it held: `energia`
/// J per kg, reaching `radio` m times the cube root of the kg (a blast's reach goes with the
/// cube root of its energy), shown as explosion `efecto`.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HazardDef {
    pub energia: f32,
    pub radio: f32,
    pub efecto: String,
}

/// A substance of the registry (`assets/defs/sustancias.jsonc`, a map of ids).
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubstanceDef {
    /// As it is written in a sentence ("agua"); labels write it in capitals.
    pub nombre: String,
    /// kg/m³ as it is carried (a liquid's density; a loose solid's as it is packed).
    pub densidad: f32,
    pub estado: State,
    /// Counted by volume or by mass; by default a liquid by volume, anything else by mass.
    #[serde(default)]
    pub medida: Option<Measure>,
    #[serde(default)]
    pub estalla: Option<HazardDef>,
}

impl SubstanceDef {
    pub fn measure(&self) -> Measure {
        self.medida.unwrap_or(if self.estado == State::Liquid { Measure::Litres } else { Measure::Kilograms })
    }

    /// `kg` of it as it is counted: "170 L", "600 kg".
    pub fn amount(&self, kg: f32) -> String {
        match self.measure() {
            Measure::Litres => format!("{} L", figure(kg / self.densidad.max(1e-3) * 1000.0)),
            Measure::Kilograms => format!("{} kg", figure(kg)),
        }
    }

    /// What a container that held `kg` of it lets go of when it is destroyed.
    pub fn burst(&self, kg: f32) -> Option<BurstDef> {
        let h = self.estalla.as_ref().filter(|_| kg > 0.0)?;
        Some(BurstDef { energy: h.energia * kg, radius: h.radio * kg.cbrt(), effect: h.efecto.clone() })
    }
}

/// A figure as it is written: whole from ten up (in threes from ten thousand), one decimal
/// under ten ("2,5") unless it is whole.
pub fn figure(v: f32) -> String {
    let v = v.max(0.0);
    if v < 9.95 && (v - v.round()).abs() >= 0.05 {
        return format!("{v:.1}").replace('.', ",");
    }
    let n = v.round() as u64;
    if n < 10_000 {
        return n.to_string();
    }
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (k, c) in digits.chars().enumerate() {
        if k > 0 && (digits.len() - k) % 3 == 0 {
            out.push(' ');
        }
        out.push(c);
    }
    out
}

/// What holds `kg` of `sub` of `capacity` at most, told: `format` with `{cantidad}` (how much
/// now), `{capacidad}` (how much at most) and `{sustancia}` (what); "170 L de agua" without one.
pub fn text(format: Option<&str>, sub: &SubstanceDef, kg: f32, capacity: f32) -> String {
    format.unwrap_or("{cantidad} de {sustancia}").replace("{cantidad}", &sub.amount(kg)).replace("{capacidad}", &sub.amount(capacity)).replace("{sustancia}", &sub.nombre)
}

/// What a part kind holds (`PartKindDef::holds`).
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContainerDef {
    /// A substance of the registry, by id.
    pub substance: String,
    /// kg at most; without it, what fits in it.
    #[serde(default)]
    pub capacity: Option<f32>,
    /// The share of that it holds as built.
    #[serde(default = "full")]
    pub fill: f32,
    /// Its inside (m³), if whoever made it knows it better than its shape says (a round drum is
    /// a prism of a few sides here); without it, its shape's volume less its walls.
    #[serde(default)]
    pub room: Option<f32>,
    /// How what it holds is told (`text`).
    #[serde(default)]
    pub text: Option<String>,
}

fn full() -> f32 {
    1.0
}

/// A part kind's container, ready (`PartKind::container`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Container {
    /// Index into `Catalog::substances`.
    pub substance: u16,
    /// kg at most, and as built.
    pub capacity: f32,
    pub initial: f32,
    /// Its inside in the part's frame (a box), and the kg that fill it to the brim.
    pub lo: Vec3,
    pub hi: Vec3,
    pub brim: f32,
    /// What it holds lies on its low side (a liquid, a solid); a gas fills it whole.
    pub settles: bool,
}

impl Container {
    /// From a kind's definition: `shape` its part's, `walls` their thickness (m), `solid` the
    /// share of the shape's volume that is material.
    pub(crate) fn new(def: &ContainerDef, shape: &Convex, walls: f32, solid: f32, substances: &[(String, SubstanceDef)]) -> Result<Container, String> {
        let k = substances.iter().position(|(id, _)| *id == def.substance).ok_or_else(|| format!("unknown substance '{}' (sustancias.jsonc)", def.substance))?;
        let sub = &substances[k].1;
        if !(sub.densidad > 0.0) {
            return Err(format!("substance '{}': density must be > 0", def.substance));
        }
        let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        for v in shape.verts() {
            lo = lo.min(v);
            hi = hi.max(v);
        }
        let t = Vec3::splat(walls.max(0.0)).min((hi - lo) * 0.25);
        let room = def.room.unwrap_or_else(|| shape.volume() * (1.0 - solid));
        if !(room > 0.0) {
            return Err("a container with no room inside (give it walls: hollow)".into());
        }
        let brim = sub.densidad * room;
        let capacity = def.capacity.unwrap_or(brim);
        if !(capacity > 0.0) || capacity > brim * 1.001 {
            return Err(format!("it holds {capacity:.1} kg of {} but only {brim:.1} kg fit in its {:.1} L", def.substance, room * 1000.0));
        }
        Ok(Container { substance: k as u16, capacity, initial: capacity * def.fill.clamp(0.0, 1.0), lo: lo + t, hi: hi - t, brim, settles: sub.estado != State::Gas })
    }
}

/// What one part of a structure holds now.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stored {
    pub part: u32,
    /// Index into `Catalog::substances`.
    pub substance: u16,
    /// kg now, and at most.
    pub mass: f32,
    pub capacity: f32,
    /// The block it made when it was last weighed (part frame): its middle, its size, and the
    /// kg it was then. This is what the structure's mass properties count.
    pub at: Vec3,
    pub size: Vec3,
    pub weighed: f32,
    /// The container's inside (`Container`).
    lo: Vec3,
    hi: Vec3,
    brim: f32,
    settles: bool,
}

impl Stored {
    fn new(part: u32, c: &Container, local: &Affine3A) -> Stored {
        let mut s = Stored { part, substance: c.substance, mass: c.initial, capacity: c.capacity, at: Vec3::ZERO, size: Vec3::ZERO, weighed: 0.0, lo: c.lo, hi: c.hi, brim: c.brim, settles: c.settles };
        s.block(local);
        s
    }

    /// How full it is (0..1 of its capacity).
    pub fn share(&self) -> f32 {
        (self.mass / self.capacity.max(1e-9)).clamp(0.0, 1.0)
    }

    /// The block what it holds now makes in its part, placed by `local` in its structure: on
    /// the side of the container that faces the structure's down, as deep as its share of the
    /// room.
    fn block(&mut self, local: &Affine3A) {
        let share = if self.settles { (self.mass / self.brim.max(1e-9)).clamp(0.0, 1.0) } else { 1.0 };
        let mut size = self.hi - self.lo;
        let mut at = (self.lo + self.hi) * 0.5;
        if share < 1.0 {
            let down = Vec3::from(local.matrix3.transpose() * Vec3A::NEG_Y);
            let k = (0..3).max_by(|&a, &b| down[a].abs().total_cmp(&down[b].abs())).unwrap_or(1);
            let deep = size[k] * share;
            at[k] = if down[k] > 0.0 { self.hi[k] - deep * 0.5 } else { self.lo[k] + deep * 0.5 };
            size[k] = deep;
        }
        (self.at, self.size, self.weighed) = (at, size, self.mass);
    }
}

/// What the parts of a structure hold as built (those whose kind is a container), by part.
pub(crate) fn as_built(parts: &[Part], cat: &Catalog) -> Vec<Stored> {
    parts.iter().enumerate().filter(|(_, p)| p.alive && !p.fragment).filter_map(|(i, p)| cat.parts[usize::from(p.kind)].container.as_ref().map(|c| Stored::new(i as u32, c, &p.local))).collect()
}

/// What goes with the parts `idx` of a structure that come off it as a structure of their own
/// (`idx[k]` becomes its part `k`): theirs, as it was.
pub(crate) fn moved(stored: &[Stored], idx: &[usize]) -> Vec<Stored> {
    let mut out: Vec<Stored> = stored.iter().filter_map(|s| idx.iter().position(|&i| i == s.part as usize).map(|k| Stored { part: k as u32, ..*s })).collect();
    out.sort_by_key(|s| s.part);
    out
}

/// Mass and first moment (structure frame) of what live parts hold (for `Structure::mass_props`).
pub(crate) fn weigh(stored: &[Stored], parts: &[Part]) -> (f32, Vec3) {
    stored.iter().filter(|s| s.weighed > 0.0 && parts[s.part as usize].alive).fold((0.0, Vec3::ZERO), |(m, mc), s| (m + s.weighed, mc + parts[s.part as usize].local.transform_point3(s.at) * s.weighed))
}

/// A structure's mass, first moment and inertia about its own origin (structure frame), in
/// doubles: what `Structure::mass_props` last worked out from all its parts, and what
/// `Structure::reweigh` moves by one block's difference. About the origin they are plain sums
/// (no centre of mass in them), so a term can be taken out and another put in exactly.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Sums {
    pub mass: f64,
    pub first: DVec3,
    pub second: DMat3,
}

/// A mass `m` at `r`, as inertia about the origin.
fn point(m: f64, r: DVec3) -> DMat3 {
    (DMat3::from_diagonal(DVec3::splat(r.length_squared())) - DMat3::from_cols(r * r.x, r * r.y, r * r.z)) * m
}

impl Sums {
    /// From a mass, its first moment and its inertia about its centre of mass `com`.
    pub(crate) fn of(mass: f32, first: Vec3, inertia: Mat3, com: Vec3) -> Sums {
        Sums { mass: f64::from(mass), first: first.as_dvec3(), second: inertia.as_dmat3() + point(f64::from(mass), com.as_dvec3()) }
    }

    /// What block `st` of a part placed by `local` counts for: nothing if it holds nothing.
    fn block(st: &Stored, local: &Affine3A) -> Sums {
        let w = f64::from(st.weighed);
        if !(w > 0.0) {
            return Sums::default();
        }
        let (e, r) = (st.size.as_dvec3(), local.transform_point3(st.at).as_dvec3());
        let own = DMat3::from_diagonal(DVec3::new(e.y * e.y + e.z * e.z, e.x * e.x + e.z * e.z, e.x * e.x + e.y * e.y) * (w / 12.0));
        let turn = DMat3::from_quat(local.to_scale_rotation_translation().1.as_dquat());
        Sums { mass: w, first: r * w, second: turn * own * turn.transpose() + point(w, r) }
    }
}

/// Its inertia about `com` (structure frame): each block a box of its size, turned with its
/// part and moved to where it lies.
pub(crate) fn inertia(stored: &[Stored], parts: &[Part], com: Vec3) -> Mat3 {
    let mut sum = Mat3::ZERO;
    for s in stored.iter().filter(|s| s.weighed > 0.0 && parts[s.part as usize].alive) {
        let p = &parts[s.part as usize];
        let e = s.size;
        let own = Mat3::from_diagonal(Vec3::new(e.y * e.y + e.z * e.z, e.x * e.x + e.z * e.z, e.x * e.x + e.y * e.y) * (s.weighed / 12.0));
        let turn = Mat3::from_quat(p.local.to_scale_rotation_translation().1);
        sum += turn * own * turn.transpose() + super::hold::point_inertia(s.weighed, p.local.transform_point3(s.at) - com);
    }
    sum
}

impl Structure {
    /// What part `part` holds now, if it is a container (and is there).
    pub fn contents(&self, part: u32) -> Option<&Stored> {
        let k = self.stored.binary_search_by_key(&part, |s| s.part).ok()?;
        self.parts.get(part as usize).filter(|p| p.alive).map(|_| &self.stored[k])
    }

    /// Up to `kg` taken out of part `part`: what came out (kg).
    pub fn take(&mut self, part: u32, kg: f32) -> f32 {
        let Some(now) = self.contents(part).map(|s| s.mass) else { return 0.0 };
        now - self.fill(part, now - kg.max(0.0))
    }

    /// Up to `kg` put into part `part`: what went in (kg; the rest does not fit).
    pub fn put(&mut self, part: u32, kg: f32) -> f32 {
        let Some(now) = self.contents(part).map(|s| s.mass) else { return 0.0 };
        self.fill(part, now + kg.max(0.0)) - now
    }

    /// Part `part` left holding `kg` (as near as it can: none, or its capacity): what it holds
    /// then. Its structure is weighed again only when that has changed by a quantum since it
    /// last was; asked for what it already holds, nothing is done.
    pub fn fill(&mut self, part: u32, kg: f32) -> f32 {
        let Ok(k) = self.stored.binary_search_by_key(&part, |s| s.part) else { return 0.0 };
        if !self.parts.get(part as usize).is_some_and(|p| p.alive) {
            return 0.0;
        }
        let st = &mut self.stored[k];
        let kg = if kg.is_finite() { kg.clamp(0.0, st.capacity) } else { st.mass };
        if kg == st.mass {
            return kg;
        }
        st.mass = kg;
        let ends = kg <= 0.0 || kg >= st.capacity;
        if kg != st.weighed && (ends || (kg - st.weighed).abs() >= st.capacity * QUANTUM) {
            self.reweigh(k);
            // (what it stands on carries another weight now)
            self.resting = false;
            self.still = 0.0;
        }
        kg
    }

    /// The block of `stored[k]` made again for what it holds now, and the structure's mass,
    /// centre of mass and inertia moved by the difference: what the block counted for when it
    /// was last weighed taken out of the sums, what it counts for now put in. No part but its
    /// own is looked at, and nothing of the structure's shape changes (its bounds, its index of
    /// parts): the same figures `mass_props` would give going over everything.
    fn reweigh(&mut self, k: usize) {
        let local = self.parts[self.stored[k].part as usize].local;
        let st = &mut self.stored[k];
        let was = Sums::block(st, &local);
        st.block(&local);
        let now = Sums::block(st, &local);
        let s = &mut self.sums;
        s.mass += now.mass - was.mass;
        s.first += now.first - was.first;
        s.second += now.second - was.second;
        let com = if s.mass > 0.0 { s.first / s.mass } else { DVec3::ZERO };
        let inertia = (s.second - point(s.mass, com)).as_mat3();
        self.mass = s.mass as f32;
        self.com = com.as_vec3();
        self.inertia = inertia + Mat3::IDENTITY * (self.mass.max(0.01) * 1e-4);
        self.inv_inertia = self.inertia.inverse();
        self.weighings += 1;
    }

    /// What part `part` held is gone (it was destroyed: put back, it is empty).
    pub(crate) fn spill(&mut self, part: u32) {
        if let Ok(k) = self.stored.binary_search_by_key(&part, |s| s.part) {
            let local = self.parts[part as usize].local;
            let st = &mut self.stored[k];
            st.mass = 0.0;
            st.block(&local);
        }
    }

    /// Whether part `part` is a container with next to nothing left in it (`SPENT`).
    pub fn spent(&self, part: u32) -> bool {
        self.stored.binary_search_by_key(&part, |s| s.part).is_ok_and(|k| self.stored[k].mass <= self.stored[k].capacity * SPENT)
    }

    /// What parts `parts` of it hold now, all together, told as the first of them that holds
    /// something tells it, with how full: "85 L de agua · 50 %". None if none of them holds
    /// anything (they are no containers).
    pub fn told(&self, cat: &Catalog, parts: impl IntoIterator<Item = u32>) -> Option<String> {
        let mut first: Option<&Stored> = None;
        let (mut mass, mut capacity) = (0.0, 0.0);
        for p in parts {
            let Some(st) = self.contents(p) else { continue };
            if first.is_some_and(|f| f.substance != st.substance) {
                continue;
            }
            first.get_or_insert(st);
            mass += st.mass;
            capacity += st.capacity;
        }
        let st = first?;
        let sub = &cat.substances.get(usize::from(st.substance))?.1;
        let format = cat.parts[usize::from(self.parts[st.part as usize].kind)].def.holds.as_ref().and_then(|h| h.text.as_deref());
        Some(format!("{} · {:.0} %", text(format, sub, mass, capacity), (mass / capacity.max(1e-9) * 100.0).clamp(0.0, 100.0)))
    }

    /// The same of the whole of it, with what it all weighs (a loose drum, a pallet come off a
    /// ship): "85 L de agua · 50 % · 97 kg".
    pub fn cargo_card(&self, cat: &Catalog) -> Option<String> {
        self.told(cat, self.stored.iter().map(|s| s.part)).map(|t| format!("{t} · {} kg", figure(self.mass)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::structure::{
        Library,
        blueprint::{Blueprint, Joined, Placed},
        catalog::{JointKindDef, MaterialDef, PartKindDef},
        set::Structures,
    };
    use glam::{DVec3, Quat};
    use std::sync::Arc;

    fn water() -> Vec<(String, SubstanceDef)> {
        vec![
            ("agua".into(), SubstanceDef { nombre: "agua".into(), densidad: 1000.0, estado: State::Liquid, medida: None, estalla: None }),
            ("aire".into(), SubstanceDef { nombre: "aire".into(), densidad: 20.0, estado: State::Gas, medida: Some(Measure::Kilograms), estalla: None }),
            ("propelente".into(), SubstanceDef { nombre: "propelente".into(), densidad: 875.0, estado: State::Liquid, medida: None, estalla: Some(HazardDef { energia: 5000.0, radio: 1.1, efecto: "granada".into() }) }),
        ]
    }

    #[test]
    fn figures_are_written_as_a_label_writes_them() {
        assert_eq!(figure(170.2), "170");
        assert_eq!(figure(2.5), "2,5");
        assert_eq!(figure(3.0), "3");
        assert_eq!(figure(9.96), "10");
        assert_eq!(figure(2500.0), "2500");
        assert_eq!(figure(12500.0), "12 500");
        let subs = water();
        assert_eq!(subs[0].1.amount(85.0), "85 L");
        assert_eq!(subs[2].1.amount(87.5), "100 L");
        assert_eq!(subs[1].1.amount(3.0), "3 kg");
        assert_eq!(text(None, &subs[0].1, 85.0, 170.0), "85 L de agua");
        assert_eq!(text(Some("{sustancia}: {cantidad} de {capacidad}"), &subs[0].1, 85.0, 170.0), "agua: 85 L de 170 L");
    }

    #[test]
    fn a_container_holds_what_fits_in_it_and_no_more() {
        let subs = water();
        // a box a metre on a side with walls of a centimetre: its inside is what its walls leave
        let shape = Convex::cuboid(Vec3::splat(0.5));
        let solid = shape.area() * 0.01 / shape.volume();
        let def = |capacity: Option<f32>| ContainerDef { substance: "agua".into(), capacity, fill: 0.5, room: None, text: None };
        let c = Container::new(&def(None), &shape, 0.01, solid, &subs).unwrap();
        assert!((c.brim - 940.0).abs() < 1.0 && c.capacity == c.brim, "it holds what fits: {} kg", c.brim);
        assert!((c.initial - 470.0).abs() < 0.5);
        assert!((c.lo - Vec3::splat(-0.49)).length() < 1e-6 && (c.hi - Vec3::splat(0.49)).length() < 1e-6);
        assert!(Container::new(&def(Some(900.0)), &shape, 0.01, solid, &subs).is_ok());
        let e = Container::new(&def(Some(1000.0)), &shape, 0.01, solid, &subs).unwrap_err();
        assert!(e.contains("only 940"), "{e}");
        assert!(Container::new(&ContainerDef { substance: "vino".into(), ..def(None) }, &shape, 0.01, solid, &subs).is_err());
        assert!(Container::new(&def(None), &shape, 0.0, 1.0, &subs).is_err(), "a solid block holds nothing");
        // what it bursts with goes with what it holds
        let b = subs[2].1.burst(125.0).unwrap();
        assert!((b.energy - 625_000.0).abs() < 1.0 && (b.radius - 5.5).abs() < 0.01 && b.effect == "granada");
        assert!(subs[2].1.burst(0.0).is_none() && subs[0].1.burst(100.0).is_none());
    }

    #[test]
    fn what_is_held_lies_on_the_low_side_of_its_container() {
        let subs = water();
        let shape = Convex::cuboid(Vec3::new(0.5, 1.0, 0.25));
        let c = Container::new(&ContainerDef { substance: "agua".into(), capacity: None, fill: 0.25, room: Some(1.0), text: None }, &shape, 0.0, 0.0, &subs).unwrap();
        // standing: a quarter full, the bottom quarter
        let mut st = Stored::new(0, &c, &Affine3A::IDENTITY);
        assert!((st.mass - 250.0).abs() < 1e-3 && st.weighed == st.mass);
        assert!((st.size - Vec3::new(1.0, 0.5, 0.5)).length() < 1e-5 && (st.at - Vec3::new(0.0, -0.75, 0.0)).length() < 1e-5, "{:?} {:?}", st.at, st.size);
        // lying on its side (its +x down): the quarter by that side
        st.block(&Affine3A::from_rotation_z(-std::f32::consts::FRAC_PI_2));
        assert!((st.size - Vec3::new(0.25, 2.0, 0.5)).length() < 1e-5 && (st.at - Vec3::new(0.375, 0.0, 0.0)).length() < 1e-5, "{:?} {:?}", st.at, st.size);
        // a gas fills it whole however little there is
        let g = Container::new(&ContainerDef { substance: "aire".into(), capacity: None, fill: 0.1, room: Some(1.0), text: None }, &shape, 0.0, 0.0, &subs).unwrap();
        let st = Stored::new(0, &g, &Affine3A::IDENTITY);
        assert!((st.size - Vec3::new(1.0, 2.0, 0.5)).length() < 1e-5 && st.at.length() < 1e-5);
    }

    /// A beam with a tank of water half full standing on each end: any structure, no ship.
    fn cistern() -> (Structures, u64) {
        let material: MaterialDef = serde_json::from_str(r#"{ "name": "Acero", "density": 8000, "toughness": 4.0e6, "brittleness": 0.2, "color": [150, 150, 150] }"#).unwrap();
        let tank: PartKindDef = serde_json::from_str(r#"{ "name": "Tanque", "shape": { "kind": "box", "size": [1, 1, 1] }, "material": "acero", "hollow": 0.01, "holds": { "substance": "agua", "fill": 0.5 } }"#).unwrap();
        let beam: PartKindDef = serde_json::from_str(r#"{ "name": "Viga", "shape": { "kind": "box", "size": [4, 0.5, 0.5] }, "material": "acero" }"#).unwrap();
        let joint: JointKindDef = serde_json::from_str(r#"{ "name": "Soldadura", "strength": 1.0e5 }"#).unwrap();
        let cat = Catalog::with(vec![("acero".into(), material)], vec![("tanque".into(), tank), ("viga".into(), beam)], vec![("soldadura".into(), joint)], Default::default(), water()).unwrap();
        let at = |x: f32, y: f32| Affine3A::from_translation(Vec3::new(x, y, 0.0));
        let bp = Blueprint {
            name: "aljibe".into(),
            anchored: false,
            ids: vec!["viga".into(), "a".into(), "b".into()],
            parts: vec![Placed { kind: 1, local: at(0.0, 0.0), bone: 0 }, Placed { kind: 0, local: at(-1.5, 0.75), bone: 0 }, Placed { kind: 0, local: at(1.5, 0.75), bone: 0 }],
            joints: vec![Joined { a: 0, b: 1, kind: 0, at: Vec3::new(-1.5, 0.25, 0.0), networks: 0 }, Joined { a: 0, b: 2, kind: 0, at: Vec3::new(1.5, 0.25, 0.0), networks: 0 }],
        };
        let mut set = Structures::new(Arc::new(Library { catalog: cat, blueprints: vec![("aljibe".into(), bp)] }));
        let id = set.spawn("aljibe", DVec3::ZERO, Quat::IDENTITY).unwrap();
        (set, id)
    }

    #[test]
    fn what_the_parts_of_any_structure_hold_weighs_where_it_lies() {
        let (mut set, _) = cistern();
        let s = &mut set.list[0];
        // 8 t of beam, two tanks of 480 kg, 470 kg of water in each, lying in their lower half
        assert_eq!(s.stored.iter().map(|st| st.part).collect::<Vec<_>>(), vec![1, 2]);
        assert!((s.mass - 9900.0).abs() < 0.5, "{} kg", s.mass);
        assert!(s.com.x.abs() < 1e-4 && (s.com.y - (960.0 * 0.75 + 940.0 * 0.505) / 9900.0).abs() < 1e-3, "{:?}", s.com);
        let (w0, i0) = (s.weighings, s.inertia);
        assert!(s.contents(0).is_none() && s.contents(7).is_none() && s.contents(1).is_some_and(|c| (c.share() - 0.5).abs() < 1e-6));
        // one emptied: lighter, and its centre of mass goes toward the other; weighed once
        assert!((s.take(2, 470.0) - 470.0).abs() < 1e-3 && s.spent(2) && !s.spent(1) && !s.spent(0));
        assert!((s.mass - 9430.0).abs() < 0.5 && (s.com.x + 705.0 / 9430.0).abs() < 1e-3, "{} kg, {:?}", s.mass, s.com);
        assert!(s.weighings == w0 + 1 && s.inertia != i0 && !s.resting);
        // under a quantum (4.7 kg here) nothing is weighed; the amount is kept all the same
        assert!((s.put(2, 3.0) - 3.0).abs() < 1e-4 && s.weighings == w0 + 1 && (s.mass - 9430.0).abs() < 0.5);
        assert!((s.put(2, 3.0) - 3.0).abs() < 1e-4 && s.weighings == w0 + 2 && (s.mass - 9436.0).abs() < 0.5);
        assert!((s.fill(2, 6.0) - 6.0).abs() < 1e-4 && s.weighings == w0 + 2, "asked for what it holds, nothing is done");
        // filled past its brim: only what fits goes in
        assert!((s.put(2, 5000.0) - 934.0).abs() < 1e-2 && (s.mass - 10370.0).abs() < 0.5);
        assert_eq!(s.told(&set.lib.catalog, [1, 2]).as_deref(), Some("1410 L de agua · 75 %"));
        assert_eq!(set.list[0].cargo_card(&set.lib.catalog).as_deref(), Some("1410 L de agua · 75 % · 10 370 kg"));
    }

    #[test]
    fn weighed_again_block_by_block_it_is_what_weighing_all_of_it_gives() {
        let (mut set, _) = cistern();
        let s = &mut set.list[0];
        // a thousand fillings and drainings of both tanks, each weighed by its own block alone
        let (mut seed, w0) = (7u32, s.weighings);
        for _ in 0..1000 {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            s.fill(1 + (seed >> 16) % 2, ((seed >> 8) % 9400) as f32 * 0.1);
        }
        assert!(s.weighings > w0 + 900, "weighed {} times", s.weighings - w0);
        let (mass, com, inertia, inv) = (s.mass, s.com, s.inertia, s.inv_inertia);
        let held: f32 = s.stored.iter().map(|st| st.weighed).sum();
        assert!((mass - 8960.0 - held).abs() < 0.01, "{mass} kg with {held} kg held");
        // ... and all of it gone over again: the same mass, centre of mass and inertia
        s.refresh();
        assert!((s.mass - mass).abs() < 0.01 && (s.com - com).length() < 1e-5, "{} kg at {:?}, kept as {mass} kg at {com:?}", s.mass, s.com);
        for k in 0..3 {
            assert!((s.inertia.col(k) - inertia.col(k)).length() < s.inertia.col(k).length() * 1e-5, "inertia {:?}, kept as {inertia:?}", s.inertia);
            assert!((s.inv_inertia.col(k) - inv.col(k)).length() < s.inv_inertia.col(k).length() * 1e-4);
        }
        // nothing of its shape is touched by it: where it is in space and how big it is stay
        let (centre, radius) = (s.center, s.radius);
        s.fill(1, 12.0);
        assert!(s.center == centre && s.radius == radius);
    }

    #[test]
    fn what_a_part_holds_goes_with_it_when_it_comes_off_and_is_lost_with_it() {
        let (mut set, id) = cistern();
        set.list[0].take(2, 370.0);
        // the joint of one tank let go: it comes off with its 100 kg of water, the other stays
        set.list[0].joints[1].alive = false;
        set.list[0].parted = true;
        assert_eq!(set.separate(), 1);
        let (s, n) = (&set.list[0], &set.list[1]);
        assert_eq!((s.id, s.stored.len(), n.stored.len(), n.stored[0].part), (id, 1, 1, 0));
        assert!((n.stored[0].mass - 100.0).abs() < 1e-3 && (n.mass - 580.0).abs() < 0.1 && (s.mass - 8950.0).abs() < 0.5, "{} kg and {} kg", n.mass, s.mass);
        assert!(s.contents(2).is_none() && s.contents(1).is_some());
        // (where it lies in what came off: in its lower part, where it was)
        assert!((n.com - (Vec3::new(1.5, 0.75, 0.0) * 480.0 + Vec3::new(1.5, 0.75 - 0.49 + 0.98 * (100.0 / 940.0) / 2.0, 0.0) * 100.0) / 580.0).length() < 2e-3, "{:?}", n.com);
        // the tank that stayed destroyed: its water is gone; put back, it is there empty
        let cat = set.lib.clone();
        let s = &mut set.list[0];
        s.parts[1].alive = false;
        s.refresh();
        assert!((s.mass - 8000.0).abs() < 0.5 && s.contents(1).is_none() && s.take(1, 10.0) == 0.0);
        assert!(s.rebuild(&cat.catalog, 1, 1.0));
        assert!((s.mass - 8480.0).abs() < 0.5 && s.contents(1).is_some_and(|c| c.mass == 0.0) && s.spent(1));
        assert!((s.put(1, 200.0) - 200.0).abs() < 1e-3 && (s.mass - 8680.0).abs() < 0.5);
    }
}
