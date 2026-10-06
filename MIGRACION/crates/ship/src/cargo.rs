//! Cargo and what holds it.
//!
//! Cargo is placed like any component, but it is not of the ship: it says which clamp holds it
//! (`ComponentDef::anclaje`, a component of the ship) and it is joined to nothing else — not the
//! deck it stands on, not the crate beside it. Its joints to its clamp are lashings (the joint
//! kind `amarre`, weaker than a weld).
//!
//! While its clamp holds, it rides as part of the ship (its mass is the ship's, a hit hurts it
//! like any part). Let go — a hand opens the clamp, its order comes (`<clamp>.soltar`), the clamp
//! is destroyed, a blow tears the lashings — it comes off whole as a structure of its own
//! (`Structures::separate`, the breakup of any structure) with the physics of any loose body: it
//! falls, slides, is struck, breaks; and if what it holds bursts (`PieceDef::estalla`), it does.
//! Nothing of it is then missing from the ship (`PartKindDef::carried`).
//!
//! A clamp also takes what is brought to it: shut, whatever is loose in its zone (`Zone`) is held
//! by the ship (`lunar_core::structure::hold`: it follows the ship, it weighs on it) until the
//! clamp opens again. A deck clamp sets its load down as it lies; a cradle sets a small ship in
//! its middle, squared; a magnet takes it where it is. The ship only asks (`Ask`): whoever owns
//! the structures does it, since it takes two of them.
//!
//! A magnet (crane or tug: one kind of clamp, `Mode::Grip`) takes everything under its face:
//! every loose body whose middle is in its zone and whose top is against the face (`choose`),
//! the nearest to its centre first, as long as all of it stays within its rated load (kg, data
//! of its kind: `"carga"`). What would take it past its rating it leaves, and says so
//! (`<id>.sobrecarga`, a notice to whoever works it). It looks for what is under it when it is
//! switched on, and while it is on and holds nothing (a few times a second): never while it
//! holds, never off. `<id>.masa` says what a clamp holds weighs (kg).
use glam::Vec3;
use lunar_core::structure::{catalog::Catalog, contents::figure, state::Structure};
use std::collections::BTreeMap;

/// A clamp as built: its parts, the parts of the cargo it holds and what that cargo is.
#[derive(Clone, Debug, PartialEq)]
pub struct ClampPlan {
    /// The component it is.
    pub id: String,
    pub name: String,
    pub parts: Vec<u32>,
    pub cargo: Vec<u32>,
    /// What it holds, by name, in the order placed ("Palé de regolito").
    pub holds: Vec<String>,
    /// Where it takes what is loose, if it does; and the bone it moves with (a crane's hook).
    pub zone: Option<Zone>,
    pub bone: u16,
}

/// A clamp's zone at rest (ship frame): a box, what it takes and how.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Zone {
    pub centre: Vec3,
    pub half: Vec3,
    /// The heaviest thing it takes (kg), and how many.
    pub max: f32,
    pub count: u32,
    pub mode: Mode,
    /// All it holds at once (kg): its rated load (infinite: no limit).
    pub rated: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Set down on the floor of the zone as it lies.
    Seat,
    /// Set down in the middle of the zone, squared to it.
    Cradle,
    /// As it is, where it is.
    Grip,
}

impl Zone {
    /// From a clamp kind's definition, placed by `at` (component frame → ship frame).
    pub fn new(d: &crate::components::ClampDef, at: glam::Affine3A) -> Result<Zone, String> {
        let mode = match d.modo.as_deref() {
            None | Some("asentar") => Mode::Seat,
            Some("cuna") => Mode::Cradle,
            Some("iman") => Mode::Grip,
            Some(m) => return Err(format!("modo de anclaje desconocido '{m}'")),
        };
        // (clamps stand square to the ship: the box keeps to its axes)
        let half = at.transform_vector3(Vec3::from_array(d.zona)).abs();
        // (a magnet takes whatever is under it, however many they are)
        let count = d.cuantos.unwrap_or(if mode == Mode::Grip { u32::MAX } else { 6 });
        let rated = d.carga.unwrap_or(f32::INFINITY);
        if !(d.masa > 0.0 && rated > 0.0) {
            return Err("un anclaje sujeta más de 0 kg ('masa', 'carga')".into());
        }
        Ok(Zone { centre: at.transform_point3(Vec3::from_array(d.centro)), half, max: d.masa, count, mode, rated })
    }
}

/// Lower than this under the highest of what is under a magnet's face (m), a thing is not
/// against the face: it is under something else, or too short to be reached.
pub const AGAINST: f32 = 0.2;

/// A loose body under a magnet's face: what it weighs (kg) and how high its top is (the y of
/// its highest point, in the magnet's ship's frame).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Found {
    pub mass: f32,
    pub top: f32,
}

/// What a magnet does with a body under its face.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Take,
    /// Not against the face: under another body, or shorter than what the face rests on.
    Low,
    /// With it, it would hold more than its rated load (or it is heavier than anything it takes).
    Heavy,
    /// It holds as many things as it can.
    Full,
}

/// What a magnet of zone `zone` that already holds `held` (how many things, their kg) does with
/// each of the bodies `found` under its face, given the nearest to its centre first: it takes
/// every one that is against its face while all of it stays within its rated load. The nearest
/// first because that is the one whoever works it brought the magnet over, and what hangs then
/// hangs under the middle of the face; one too heavy is passed over, and those after it are
/// still taken if they fit.
pub fn choose(zone: &Zone, held: (usize, f32), found: &[Found]) -> Vec<Verdict> {
    let high = found.iter().map(|f| f.top).fold(f32::MIN, f32::max);
    let (mut count, mut total) = held;
    found
        .iter()
        .map(|f| {
            if f.top < high - AGAINST {
                Verdict::Low
            } else if count >= zone.count as usize {
                Verdict::Full
            } else if f.mass > zone.max || total + f.mass > zone.rated {
                Verdict::Heavy
            } else {
                count += 1;
                total += f.mass;
                Verdict::Take
            }
        })
        .collect()
}

/// What a ship asks of whoever owns the structures.
#[derive(Clone, Debug, PartialEq)]
pub enum Ask {
    /// Clamp `c` shut: take what is loose in its zone.
    Take(usize),
    /// Let these structures go.
    Drop(Vec<u64>),
}

/// What may join two parts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Join {
    /// Whatever any two parts that touch are joined by.
    Any,
    /// Cargo to its clamp.
    Lashing,
    /// Cargo to anything else: nothing.
    Never,
}

/// Who holds what, part by part.
pub struct Holds {
    /// Per part: the piece of cargo it is of, the clamp it is of.
    item: Vec<Option<u32>>,
    clamp: Vec<Option<u32>>,
    /// Per piece of cargo: its component and its clamp; per clamp: its component.
    items: Vec<(String, u32)>,
    clamps: Vec<String>,
}

impl Holds {
    /// From the component each part is of (`comp`) and the clamp that holds each cargo component
    /// (`held`: cargo → clamp).
    /// `empty`: the clamps that hold nothing as built.
    pub fn new(comp: &[Option<&str>], held: &BTreeMap<String, String>, empty: &[String]) -> Result<Holds, String> {
        let mut clamps: Vec<String> = empty.to_vec();
        let mut items = Vec::new();
        for (cargo, clamp) in held {
            if held.contains_key(clamp) {
                return Err(format!("carga {cargo}: su anclaje '{clamp}' es carga también"));
            }
            if !comp.iter().any(|c| *c == Some(clamp.as_str())) {
                return Err(format!("carga {cargo}: no hay anclaje '{clamp}'"));
            }
            let k = clamps.iter().position(|c| c == clamp).unwrap_or_else(|| {
                clamps.push(clamp.clone());
                clamps.len() - 1
            });
            items.push((cargo.clone(), k as u32));
        }
        let item = comp.iter().map(|c| c.and_then(|c| items.iter().position(|(i, _)| i == c)).map(|k| k as u32)).collect();
        let clamp = comp.iter().map(|c| c.and_then(|c| clamps.iter().position(|i| i == c)).map(|k| k as u32)).collect();
        Ok(Holds { item, clamp, items, clamps })
    }

    /// Whether part `k` is cargo.
    pub fn cargo(&self, k: usize) -> bool {
        self.item[k].is_some()
    }

    /// What may join parts `a` and `z`.
    pub fn join(&self, a: usize, z: usize) -> Join {
        let to_clamp = |item: u32, other: usize| {
            if self.clamp[other] == Some(self.items[item as usize].1) { Join::Lashing } else { Join::Never }
        };
        match (self.item[a], self.item[z]) {
            (None, None) => Join::Any,
            (Some(x), Some(y)) if x == y => Join::Any,
            (Some(_), Some(_)) => Join::Never,
            (Some(x), None) => to_clamp(x, z),
            (None, Some(y)) => to_clamp(y, a),
        }
    }

    /// The pieces of cargo no joint ties to their clamp (they touch it nowhere: a crate on other
    /// crates), each with the pair of parts to lash: its own nearest to the clamp's.
    pub fn unlashed(&self, joined: &[(u32, u32)], spheres: &[(Vec3, f32)]) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        for (k, (_, clamp)) in self.items.iter().enumerate() {
            let mine = |p: u32| self.item[p as usize] == Some(k as u32);
            let its = |p: u32| self.clamp[p as usize] == Some(*clamp);
            if joined.iter().any(|&(a, z)| (mine(a) && its(z)) || (mine(z) && its(a))) {
                continue;
            }
            let mut best: Option<(usize, usize, f32)> = None;
            for a in (0..self.item.len()).filter(|&a| mine(a as u32)) {
                for z in (0..self.item.len()).filter(|&z| its(z as u32)) {
                    let d = spheres[a].0.distance(spheres[z].0) - spheres[a].1 - spheres[z].1;
                    if best.is_none_or(|b| d < b.2) {
                        best = Some((a, z, d));
                    }
                }
            }
            out.extend(best.map(|b| (b.0, b.1)));
        }
        out
    }

    /// The clamps as plans; `name` says what a component is called, `zone` where a clamp takes
    /// what is loose, `bones` the bone of each part.
    pub fn plans(&self, name: &dyn Fn(&str) -> String, zone: &dyn Fn(&str) -> Option<Zone>, bones: &[u16]) -> Vec<ClampPlan> {
        let of = |v: &[Option<u32>], k: usize| -> Vec<u32> { v.iter().enumerate().filter(|(_, c)| **c == Some(k as u32)).map(|(p, _)| p as u32).collect() };
        self.clamps
            .iter()
            .enumerate()
            .map(|(c, id)| {
                let held: Vec<usize> = self.items.iter().enumerate().filter(|(_, (_, k))| *k as usize == c).map(|(i, _)| i).collect();
                let parts = of(&self.clamp, c);
                let bone = parts.first().map_or(0, |&p| bones[p as usize]);
                ClampPlan { id: id.clone(), name: name(id), parts, cargo: held.iter().flat_map(|&i| of(&self.item, i)).collect(), holds: held.iter().map(|&i| name(&self.items[i].0)).collect(), zone: zone(id), bone }
            })
            .collect()
    }
}

/// The lashings of clamp `plan` on structure `s`: its joints between a part of the clamp and a
/// part of its cargo.
pub fn lashings(plan: &ClampPlan, s: &Structure) -> Vec<u32> {
    let pair = |a: u32, b: u32| plan.parts.contains(&a) && plan.cargo.contains(&b);
    s.joints.iter().enumerate().filter(|(_, j)| pair(j.a, j.b) || pair(j.b, j.a)).map(|(k, _)| k as u32).collect()
}

/// Whether any of those lashings still holds (the joint whole, both its parts there).
pub fn holding(lashings: &[u32], s: &Structure) -> bool {
    lashings.iter().any(|&k| {
        let j = &s.joints[k as usize];
        j.alive && s.parts[j.a as usize].alive && s.parts[j.b as usize].alive
    })
}

/// The lashings let go: what they held comes off at the structures' next `separate`. How many
/// were holding.
pub fn release(lashings: &[u32], s: &mut Structure) -> usize {
    let mut n = 0;
    for &k in lashings {
        let j = &mut s.joints[k as usize];
        if j.alive {
            j.alive = false;
            n += 1;
        }
    }
    if n > 0 {
        s.parted = true;
        s.resting = false;
    }
    n
}

/// What ship `sh`'s clamps ask, done on the structures: what is loose in the zone of one that
/// shut is held by the ship (set down, cradled or gripped, as the clamp takes), what one that
/// opened held is let go; and what they held that is no longer theirs is forgotten.
pub fn serve(sh: &mut crate::Ship, set: &mut lunar_core::structure::set::Structures) {
    if sh.clamp_asks.is_empty() && sh.clamp_held.iter().all(Vec::is_empty) {
        return;
    }
    let mut ids = Vec::new();
    for ask in std::mem::take(&mut sh.clamp_asks) {
        match ask {
            Ask::Take(c) => {
                let Some(zone) = set.get(sh.structure).and_then(|s| sh.clamp_zone(s, c)) else {
                    continue;
                };
                let bone = sh.kind.clamps[c].bone;
                let grip = zone.mode == Mode::Grip;
                // (a magnet is offered whatever is under it, however heavy: what it leaves, it says)
                set.loose_in(sh.structure, zone.centre, zone.half, if grip { f32::INFINITY } else { zone.max }, &mut ids);
                let took: Vec<u64> = if grip {
                    grip_under(sh, set, c, &zone, bone, &ids)
                } else {
                    let room = (zone.count as usize).saturating_sub(sh.clamp_held[c].len());
                    let floor = zone.centre.y - zone.half.y;
                    ids.iter().copied().take(room).filter(|&id| set.hold_seated(id, sh.structure, bone, floor, (zone.mode == Mode::Cradle).then_some(zone.centre))).collect()
                };
                sh.clamp_took(c, &took);
            }
            Ask::Drop(held) => {
                for id in held {
                    set.let_go(id);
                }
            }
        }
    }
    let me = sh.structure;
    sh.clamp_check(&|id| set.get(id).is_some_and(|s| s.held.is_some_and(|h| h.by == me)));
}

/// Magnet `c` of ship `sh` takes what is under its face, of the loose bodies `ids` in its zone
/// (the nearest first): those `choose` says, hung as they stand (the highest against the face,
/// the others beside it as they were), and what it leaves for its rating told to the ship.
/// What it took.
fn grip_under(sh: &mut crate::Ship, set: &mut lunar_core::structure::set::Structures, c: usize, zone: &Zone, bone: u16, ids: &[u64]) -> Vec<u64> {
    let me = sh.structure;
    let found: Vec<(u64, Found)> = ids.iter().filter_map(|&id| Some((id, Found { mass: set.get(id)?.mass, top: set.top_in(id, me)? }))).collect();
    // (what it holds already counts against its rating)
    let carried: f32 = set.get(me).map_or(0.0, |s| s.loads.iter().filter(|l| sh.clamp_held[c].contains(&l.id)).map(|l| l.mass).sum());
    let bodies: Vec<Found> = found.iter().map(|f| f.1).collect();
    let verdicts = choose(zone, (sh.clamp_held[c].len(), carried), &bodies);
    let face = zone.centre.y + zone.half.y;
    let high = bodies.iter().zip(&verdicts).filter(|(_, v)| **v == Verdict::Take).map(|(f, _)| f.top).fold(f32::MIN, f32::max);
    let (mut took, mut left) = (Vec::new(), Vec::new());
    for ((id, f), v) in found.iter().zip(&verdicts) {
        match v {
            Verdict::Take => {
                if set.hold_under(*id, me, bone, face - (high - f.top)) {
                    took.push(*id);
                }
            }
            Verdict::Heavy => left.push((set.get(*id).map_or_else(String::new, |s| s.label(&set.lib.catalog)), f.mass)),
            Verdict::Low | Verdict::Full => {}
        }
    }
    sh.clamp_left(c, &left);
    took
}

/// What part `part` of a ship is, for whoever looks at it (`ShipKind::describe`), and, if it is
/// of something that holds, what that holds *now* and what it weighs with it: ("Bidón de
/// agua", "85 L de agua · 50 % · 97 kg").
pub fn scan(kind: &crate::ShipKind, s: &Structure, cat: &Catalog, part: usize) -> (String, String) {
    let (title, detail) = kind.describe(part);
    let id = kind.parts.get(part).map_or("", String::as_str);
    let head = id.split('.').next().unwrap_or(id);
    let Some(holder) = kind.holders.iter().find(|h| h.id == head) else {
        return (title, detail);
    };
    match s.told(cat, holder.holds.iter().copied()) {
        Some(told) => {
            let shell: f32 = holder.parts.iter().map(|&p| &s.parts[p as usize]).filter(|p| p.alive).map(|p| p.mass).sum();
            let held: f32 = holder.holds.iter().filter_map(|&p| s.contents(p)).map(|c| c.mass).sum();
            (title, format!("{told} · {} kg", figure(shell + held)))
        }
        None => (title, detail),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cargo_joins_its_own_and_its_clamp_and_nothing_else() {
        // parts: 0 deck, 1 clamp, 2-3 a crate, 4 another crate on the same clamp, 5 a drum on
        // another clamp (6), 7 a wall
        let comp = [Some("suelo"), Some("anclaje_a"), Some("caja_1"), Some("caja_1"), Some("caja_2"), Some("bidon"), Some("anclaje_b"), None];
        let held: BTreeMap<String, String> = [("caja_1", "anclaje_a"), ("caja_2", "anclaje_a"), ("bidon", "anclaje_b")].into_iter().map(|(a, b)| (a.to_string(), b.to_string())).collect();
        let h = Holds::new(&comp, &held, &[]).unwrap();
        assert!(h.cargo(2) && h.cargo(4) && h.cargo(5) && !h.cargo(1) && !h.cargo(0));
        assert_eq!(h.join(2, 3), Join::Any, "a crate's own pieces");
        assert_eq!(h.join(1, 2), Join::Lashing);
        assert_eq!(h.join(4, 1), Join::Lashing);
        assert_eq!(h.join(0, 2), Join::Never, "the deck it stands on");
        assert_eq!(h.join(2, 4), Join::Never, "the crate beside it");
        assert_eq!(h.join(5, 1), Join::Never, "another's clamp");
        assert_eq!(h.join(0, 1), Join::Any, "a clamp is of the ship");
        assert_eq!(h.join(0, 7), Join::Any);
        // crate 2 (part 4) touches its clamp nowhere: it is lashed to it by its nearest parts
        let spheres: Vec<(Vec3, f32)> = (0..8).map(|k| (Vec3::new(k as f32, 0.0, 0.0), 0.3)).collect();
        assert_eq!(h.unlashed(&[(1, 2), (5, 6)], &spheres), vec![(4, 1)]);
        let plans = h.plans(&|id| id.to_uppercase(), &|_| None, &[0; 8]);
        assert_eq!(plans.len(), 2);
        let a = plans.iter().find(|p| p.id == "anclaje_a").unwrap();
        assert_eq!(*a, ClampPlan { id: "anclaje_a".into(), name: "ANCLAJE_A".into(), parts: vec![1], cargo: vec![2, 3, 4], holds: vec!["CAJA_1".into(), "CAJA_2".into()], zone: None, bone: 0 });
        // a clamp with nothing on it as built is a clamp all the same
        let h = Holds::new(&comp, &held, &["suelo".to_string()]).unwrap();
        assert!(h.plans(&|id| id.to_string(), &|_| None, &[0; 8]).iter().any(|p| p.id == "suelo" && p.cargo.is_empty()));
        // what holds must be there, and be of the ship
        assert!(Holds::new(&comp, &[("caja_1".to_string(), "nada".to_string())].into_iter().collect(), &[]).is_err());
        assert!(Holds::new(&comp, &[("caja_1".to_string(), "caja_2".to_string()), ("caja_2".to_string(), "anclaje_a".to_string())].into_iter().collect(), &[]).is_err());
    }

    fn magnet(json: &str) -> Zone {
        let def: crate::components::ClampDef = lunar_core::defs::parse("anclaje", json).unwrap();
        Zone::new(&def, glam::Affine3A::IDENTITY).unwrap()
    }

    #[test]
    fn a_magnet_takes_everything_against_its_face_up_to_its_rated_load() {
        use Verdict::*;
        let z = magnet(r#"{ "centro": [0, -0.78, 0], "zona": [0.5, 0.5, 0.5], "masa": 2500, "carga": 2500, "modo": "iman" }"#);
        assert_eq!((z.mode, z.rated, z.count), (Mode::Grip, 2500.0, u32::MAX), "everything under it, by its rating");
        let b = |mass: f32, top: f32| Found { mass, top };
        // two drums side by side: both
        assert_eq!(choose(&z, (0, 0.0), &[b(182.0, 0.9), b(182.0, 0.9)]), vec![Take, Take]);
        // a short crate beside a drum, and one under another: the face does not reach them
        assert_eq!(choose(&z, (0, 0.0), &[b(182.0, 0.9), b(50.0, 0.51), b(50.0, 1.0), b(50.0, 0.85)]), vec![Take, Low, Take, Take]);
        // the nearest first, as long as all of it is within the rating; one too heavy is passed
        // over and what comes after it is taken if it fits
        assert_eq!(choose(&z, (0, 0.0), &[b(1200.0, 1.0), b(1200.0, 1.0), b(1200.0, 1.0), b(90.0, 1.0)]), vec![Take, Take, Heavy, Take]);
        assert_eq!(choose(&z, (0, 0.0), &[b(2600.0, 1.0), b(100.0, 1.0)]), vec![Heavy, Take], "heavier than anything it takes");
        // what it holds already counts
        assert_eq!(choose(&z, (1, 2000.0), &[b(400.0, 1.0), b(600.0, 1.0)]), vec![Take, Heavy]);
        assert_eq!(choose(&z, (0, 0.0), &[]), vec![]);
        // a clamp that says how many takes that many; a deck clamp, six of any weight it takes
        let one = magnet(r#"{ "centro": [0, 0, 0], "zona": [1, 1, 1], "masa": 500, "cuantos": 1, "modo": "iman" }"#);
        assert_eq!(choose(&one, (0, 0.0), &[b(100.0, 1.0), b(100.0, 1.0)]), vec![Take, Full]);
        let deck = magnet(r#"{ "centro": [0, 0.5, 0], "zona": [0.62, 0.5, 0.62], "masa": 2500 }"#);
        assert_eq!((deck.mode, deck.count, deck.rated), (Mode::Seat, 6, f32::INFINITY));
        let wrong: crate::components::ClampDef = lunar_core::defs::parse("anclaje", r#"{ "centro": [0, 0, 0], "zona": [1, 1, 1], "masa": 500, "modo": "gancho" }"#).unwrap();
        assert!(Zone::new(&wrong, glam::Affine3A::IDENTITY).is_err());
    }
}
