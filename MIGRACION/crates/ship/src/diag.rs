//! Diagnostics of a ship as built, for tests, the editor and the MCP server alike:
//! - `leaks`: each compartment, flood-filled over a voxel grid of its pressure boundary with every
//!   closure as it stands, must not reach space nor another compartment (where it gets out);
//! - `cables`: conduits out along the outside of the ship, half buried in a wall, through a device
//!   not their own;
//! - `usability`: what every compartment needs is worked from inside it (pressure read, vented,
//!   made up; each door opened, its difference read and equalised from that side; its lights),
//!   and the airlock from outside;
//! - `single_failures`: every conduit part cut in turn, and what the ship says it must keep
//!   (`ShipDef::esenciales`) that it then loses: a ship is built so that no one cut loses any.
//!
//! - `panels_cut`: a panel's plate with something through it (the wall it is on, a part) or
//!   standing on nothing; `panel_faces`: something only seen (a drop, a pipe) across a panel;
//! - `seat_reach`: from each seat, every control and instrument of the panels it works
//!   (`asientos[].paneles`) in sight and within the hand's reach, and at arm's length from one of
//!   the seats that share it.
//!
//! Each returns plain findings (what and where), empty when all is well.
use crate::{
    atmos::room_of,
    geom::Role,
    kind::{SPACE, ShipKind},
    panels::Panels,
    ship::Ship,
};
use glam::{IVec3, Vec3};
use lunar_core::structure::{catalog::Catalog, state::Structure};
use std::collections::{HashSet, VecDeque};

// ---------------------------------------------------------------- leaks

/// Voxel size (m) and how far round a pressure plate a cell counts as solid (gaps under ~5 cm —
/// seals, a leaf's clearance — count as shut).
const CELL: f32 = 0.08;
const SKIN: f32 = 0.055;

/// A compartment that gets out: where (the last cell of its way out still in a compartment), and
/// the compartments its fill reached.
#[derive(Clone, Debug)]
pub struct Leak {
    pub room: String,
    /// Where it gets out (None: it is shut toward space).
    pub hole: Option<Vec3>,
    /// Other compartments it reaches through no declared opening.
    pub reaches: Vec<String>,
}

struct Grid {
    lo: Vec3,
    n: IVec3,
    solid: Vec<bool>,
}

impl Grid {
    fn index(&self, c: IVec3) -> Option<usize> {
        (c.cmpge(IVec3::ZERO).all() && c.cmplt(self.n).all()).then(|| (c.x + self.n.x * (c.y + self.n.y * c.z)) as usize)
    }
    fn cell(&self, p: Vec3) -> IVec3 {
        ((p - self.lo) / CELL).floor().as_ivec3()
    }
    fn center(&self, c: IVec3) -> Vec3 {
        self.lo + (c.as_vec3() + 0.5) * CELL
    }
    fn coords(&self, i: usize) -> IVec3 {
        let i = i as i32;
        IVec3::new(i % self.n.x, (i / self.n.x) % self.n.y, i / (self.n.x * self.n.y))
    }
}

/// The pressure boundary (hull, decks, bulkheads, glass, closure leaves, compartment walls) as
/// posed now.
fn boundary(s: &Structure, k: &ShipKind) -> Grid {
    let mut wall = vec![false; s.parts.len()];
    for (i, r) in k.roles.iter().enumerate() {
        wall[i] = matches!(r, Role::Hull | Role::Floor | Role::Bulkhead | Role::Glass);
    }
    for c in &k.closures {
        for &p in &c.parts {
            wall[p as usize] = true;
        }
    }
    for (i, id) in k.parts.iter().enumerate() {
        if k.closures.iter().any(|c| id.starts_with(&format!("{}.", c.id))) {
            wall[i] = true;
        }
    }
    for c in &k.compartments {
        for &p in &c.hull {
            wall[p as usize] = true;
        }
        for &(p, _) in &c.walls {
            wall[p as usize] = true;
        }
    }
    let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for c in &k.compartments {
        for [a, b] in &c.boxes {
            lo = lo.min(*a);
            hi = hi.max(*b);
        }
    }
    lo -= Vec3::splat(1.0);
    hi += Vec3::splat(1.0);
    let n = ((hi - lo) / CELL).ceil().as_ivec3();
    let mut g = Grid { lo, n, solid: vec![false; (n.x * n.y * n.z) as usize] };
    for (i, p) in s.parts.iter().enumerate() {
        if !wall[i] || !p.alive {
            continue;
        }
        let inv = p.local.inverse();
        let (mut a, mut b) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        for v in p.shape.verts() {
            let w = p.local.transform_point3(v);
            a = a.min(w);
            b = b.max(w);
        }
        let (c0, c1) = (g.cell(a - SKIN).max(IVec3::ZERO), g.cell(b + SKIN).min(n - 1));
        for z in c0.z..=c1.z {
            for y in c0.y..=c1.y {
                for x in c0.x..=c1.x {
                    let c = IVec3::new(x, y, z);
                    if p.shape.distance(inv.transform_point3(g.center(c))) <= SKIN
                        && let Some(j) = g.index(c)
                    {
                        g.solid[j] = true;
                    }
                }
            }
        }
    }
    g
}

/// The free cell nearest the middle of compartment `c`.
fn seed(g: &Grid, k: &ShipKind, c: usize) -> Vec3 {
    let [a, b] = k.compartments[c].boxes[0];
    let mid = (a + b) * 0.5 + Vec3::new(0.0, -0.3, 0.0);
    for r in 0..20 {
        for dz in -r..=r {
            for dy in -r..=r {
                for dx in -r..=r {
                    let p = mid + Vec3::new(dx as f32, dy as f32, dz as f32) * CELL;
                    if g.index(g.cell(p)).is_some_and(|j| !g.solid[j]) {
                        return p;
                    }
                }
            }
        }
    }
    mid
}

/// Flood-fill each compartment (as the structure is posed now): what gets out and where.
pub fn leaks(s: &Structure, k: &ShipKind) -> Vec<Leak> {
    let g = boundary(s, k);
    let dirs = [IVec3::X, IVec3::NEG_X, IVec3::Y, IVec3::NEG_Y, IVec3::Z, IVec3::NEG_Z];
    let mut out = Vec::new();
    for (ci, plan) in k.compartments.iter().enumerate() {
        let Some(s0) = g.index(g.cell(seed(&g, k, ci))) else {
            continue;
        };
        let mut from = vec![u32::MAX; g.solid.len()];
        from[s0] = s0 as u32;
        let mut q = VecDeque::from([s0]);
        let mut reached = Vec::new();
        let mut hole = None;
        'fill: while let Some(i) = q.pop_front() {
            let c = g.coords(i);
            if let Some(r) = room_of(k, g.center(c))
                && !reached.contains(&r)
            {
                reached.push(r);
            }
            for d in dirs {
                match g.index(c + d) {
                    None => {
                        // out: back along the way to the last cell still in a compartment
                        let mut j = i;
                        let mut last_in = g.center(c);
                        while from[j] as usize != j {
                            let p = g.center(g.coords(j));
                            if room_of(k, p).is_some() {
                                last_in = p;
                                break;
                            }
                            j = from[j] as usize;
                        }
                        hole = Some(last_in);
                        break 'fill;
                    }
                    Some(j) if !g.solid[j] && from[j] == u32::MAX => {
                        from[j] = i as u32;
                        q.push_back(j);
                    }
                    _ => {}
                }
            }
        }
        let reaches: Vec<String> = reached.iter().filter(|&&r| r != ci).map(|&r| k.compartments[r].id.clone()).collect();
        if hole.is_some() || !reaches.is_empty() {
            out.push(Leak { room: plan.id.clone(), hole, reaches });
        }
    }
    out
}

// ---------------------------------------------------------------- cables

#[derive(Clone, Debug, Default)]
pub struct CableReport {
    pub conduits: usize,
    /// Out along the outside of the ship (not by a device out there, not inside a wall).
    pub exposed: Vec<String>,
    /// Half buried along a surface (not crossing it square).
    pub buried: Vec<String>,
    /// Through a device that is not its own.
    pub through: Vec<String>,
}

/// A conduit part's axis (ship frame): its ends and half its thickness.
fn axis(s: &Structure, i: usize) -> (Vec3, Vec3, f32) {
    let p = &s.parts[i];
    let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for v in p.shape.verts() {
        lo = lo.min(v);
        hi = hi.max(v);
    }
    let e = hi - lo;
    let c = (lo + hi) * 0.5;
    let k = if e.x >= e.y && e.x >= e.z {
        0
    } else if e.y >= e.z {
        1
    } else {
        2
    };
    let mut half = Vec3::ZERO;
    half[k] = e[k] * 0.5;
    let mut sorted = e.to_array();
    sorted.sort_by(f32::total_cmp);
    (p.local.transform_point3(c - half), p.local.transform_point3(c + half), sorted[0] * 0.5)
}

/// The cables, pipes and ducts as laid (see the module).
pub fn cables(s: &Structure, k: &ShipKind, cat: &Catalog) -> CableReport {
    let is_wall = |j: usize| matches!(k.roles[j], Role::Hull | Role::Glass | Role::Floor | Role::Bulkhead) || matches!(cat.parts[usize::from(s.parts[j].kind)].def.material.as_str(), "panel_casco" | "panel_mamparo" | "panel_suelo");
    let in_hull = |p: Vec3| k.def.casco.as_ref().is_none_or(|h| crate::far::in_hull(h, p));
    let solid: Vec<usize> = (0..s.parts.len()).filter(|&i| k.roles[i] != Role::Conduit).collect();
    let outside_dev: Vec<usize> = solid.iter().copied().filter(|&i| !matches!(k.roles[i], Role::Hull | Role::Glass | Role::Floor | Role::Bulkhead) && !in_hull(k.centers[i])).collect();
    let near_outside_dev = |p: Vec3| {
        outside_dev.iter().any(|&d| {
            let q = &s.parts[d];
            q.shape.closest(q.local.inverse().transform_point3(p)).0 < 1.0
        })
    };
    let inside_solid = |p: Vec3| {
        solid.iter().any(|&j| {
            let q = &s.parts[j];
            p.distance(q.center) < q.radius && q.shape.distance(q.local.inverse().transform_point3(p)) <= 0.0
        })
    };
    let mut r = CableReport::default();
    for i in 0..s.parts.len() {
        if k.roles[i] != Role::Conduit {
            continue;
        }
        r.conduits += 1;
        let id = &k.parts[i];
        let (a, b, rad) = axis(s, i);
        let len = a.distance(b);
        let n = ((len / 0.02).ceil() as usize).clamp(2, 60);
        let pts: Vec<Vec3> = (0..=n).map(|j| a.lerp(b, j as f32 / n as f32)).collect();
        let c = (a + b) * 0.5;
        if !in_hull(c) && !inside_solid(c) && !near_outside_dev(c) {
            r.exposed.push(format!("{id} ({:.2}, {:.2}, {:.2})", c.x, c.y, c.z));
        }
        let own = id.split_once(".acometida").map(|(o, _)| o);
        for &j in &solid {
            let q = &s.parts[j];
            if c.distance(q.center) > q.radius + len * 0.5 + rad + 0.05 {
                continue;
            }
            let inv = q.local.inverse();
            let d: Vec<f32> = pts.iter().map(|p| q.shape.closest(inv.transform_point3(*p)).0).collect();
            let straddle = d.iter().filter(|x| x.abs() < rad * 0.6).count() as f32 / pts.len() as f32 * len;
            let mine = own.is_some_and(|o| k.parts[j] == o || k.parts[j].starts_with(&format!("{o}.")));
            if straddle > (4.0 * rad).max(0.05) && !mine {
                r.buried.push(format!("{id} a lo largo de {} ({:.0} cm)", k.parts[j], straddle * 100.0));
            }
            if !is_wall(j) && !mine && d.iter().filter(|x| **x < -rad).count() * 3 > pts.len() {
                r.through.push(format!("{id} atraviesa {}", k.parts[j]));
            }
        }
    }
    r
}

// ---------------------------------------------------------------- usability

/// The signal names an expression reads.
pub fn reads(expr: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for ch in expr.chars().chain(std::iter::once(' ')) {
        if ch.is_alphanumeric() || ch == '_' || ch == '.' {
            cur.push(ch);
        } else {
            if cur.contains('.') && cur.chars().next().is_some_and(|c| c.is_alphabetic()) {
                out.push(cur.clone());
            }
            cur.clear();
        }
    }
    out
}

/// The room each panel of `ship` is in (None: outside).
pub fn panel_rooms(ship: &Ship, s: &Structure) -> Vec<Option<usize>> {
    ship.kind
        .panels
        .iter()
        .map(|plan| {
            let f = Panels::frame(&ship.kind, s, plan);
            let c = f.transform_point3(Vec3::new(plan.layout.size[0] * 0.0005, plan.layout.size[1] * 0.0005, 0.02));
            room_of(&ship.kind, c)
        })
        .collect()
}

/// Every signal a hand in `room` can turn (directly, or what follows from those).
pub fn writable(ship: &Ship, rooms: &[Option<usize>], room: Option<usize>) -> HashSet<String> {
    let mut set: HashSet<String> = HashSet::new();
    for c in &ship.panels.controls {
        if rooms[c.panel] == room {
            set.insert(ship.store.name(c.sig).to_string());
            if let Some(s2) = c.sig2 {
                set.insert(ship.store.name(s2).to_string());
            }
        }
    }
    loop {
        let before = set.len();
        for (name, expr) in &ship.kind.def.derivadas {
            if !set.contains(name) && reads(expr).iter().any(|r| set.contains(r)) {
                set.insert(name.clone());
            }
        }
        if set.len() == before {
            return set;
        }
    }
}

fn shown(ship: &Ship, rooms: &[Option<usize>], room: Option<usize>, sig: &str) -> bool {
    ship.kind.panels.iter().enumerate().any(|(pi, plan)| rooms[pi] == room && plan.def.mandos.iter().any(|m| m.senal.as_deref() == Some(sig)))
}

/// What a crew cannot do where it must (see the module).
pub fn usability(ship: &Ship, s: &Structure) -> Vec<String> {
    let kind = ship.kind.clone();
    let rooms = panel_rooms(ship, s);
    let out_writes = writable(ship, &rooms, None);
    let mut missing = Vec::new();
    let index = |n: &str| kind.compartments.iter().position(|x| x.id == n).unwrap_or(SPACE);
    for (ci, c) in kind.compartments.iter().enumerate() {
        let here = writable(ship, &rooms, Some(ci));
        let name = &c.id;
        if !shown(ship, &rooms, Some(ci), &format!("{name}.p")) {
            missing.push(format!("{name}: no se lee su presión dentro"));
        }
        if !c.vents.iter().any(|v| v.to == SPACE && here.contains(&v.signal)) {
            missing.push(format!("{name}: no se ventea desde dentro"));
        }
        let feeds = kind.machines.iter().any(|m| matches!(m.def.modelo.as_str(), "inyector" | "rejilla") && m.part.is_some_and(|p| room_of(&kind, kind.centers[p as usize]) == Some(ci)));
        if !feeds {
            missing.push(format!("{name}: nada le repone el aire"));
        }
        for cl in &kind.def.cierres {
            let Some([a, b]) = &cl.entre else { continue };
            let (a, b) = (index(a), index(b));
            if a != ci && b != ci {
                continue;
            }
            let other = if a == ci { b } else { a };
            let order = cl.orden.clone().unwrap_or_else(|| format!("{}.orden", cl.id));
            if !here.contains(&order) {
                missing.push(format!("{name}: no abre {} desde dentro", cl.id));
            }
            if !shown(ship, &rooms, Some(ci), &format!("{}.dp", cl.id)) {
                missing.push(format!("{name}: no se lee la diferencia de presión de {}", cl.id));
            }
            if other != SPACE && !kind.compartments.iter().enumerate().any(|(k, x)| (k == ci || k == other) && x.vents.iter().any(|v| (v.to == other || v.to == ci) && here.contains(&v.signal))) {
                missing.push(format!("{name}: no iguala con el otro lado de {}", cl.id));
            }
            if other == SPACE {
                if !out_writes.contains(&order) {
                    missing.push(format!("{}: no se abre desde fuera", cl.id));
                }
                if !c.vents.iter().any(|v| v.to == SPACE && out_writes.contains(&v.signal)) {
                    missing.push(format!("{name}: no se ventea desde fuera (para abrir {})", cl.id));
                }
            }
        }
        for m in &kind.machines {
            let Some(p) = m.part else { continue };
            if m.light.is_none() || room_of(&kind, kind.centers[p as usize]) != Some(ci) {
                continue;
            }
            if let Some(serde_json::Value::String(order)) = m.def.ordenes.get("encender")
                && !here.contains(order)
                && !kind.def.derivadas.get(order).is_some_and(|e| reads(e).iter().all(|r| !here.contains(r) && !r.starts_with("luces.")))
                && !order.starts_with("luces.emergencia")
                && !order.starts_with("luces.focos")
            {
                missing.push(format!("{name}: la luz {} no se manda desde dentro", m.id));
            }
        }
    }
    missing
}

// ---------------------------------------------------------------- single failures

/// What losing one part costs: the essentials (`ShipDef::esenciales`) that no longer hold.
#[derive(Clone, Debug)]
pub struct Failure {
    pub part: String,
    pub lost: Vec<String>,
}

/// Seconds the ship runs before anything is cut (stores full, buses up) and after each cut.
const WARM: f64 = 4.0;
const AFTER: f64 = 2.0;

/// Each of the parts `which` picks (by index and role) destroyed in turn on a ship of `kind`
/// freshly built by `make`, run a moment, and the essentials read: the cuts that lose any, with
/// what they lose. Also the essentials that do not hold before anything is cut (part "").
pub fn single_failures(make: &dyn Fn() -> (Structure, Ship), w: &crate::World, which: &dyn Fn(usize, Role) -> bool) -> Result<Vec<Failure>, String> {
    let run = |ship: &mut Ship, s: &mut Structure, secs: f64| {
        for _ in 0..(secs / crate::ship::TICK).round() as usize {
            ship.update(s, w, crate::ship::TICK);
        }
    };
    let (mut s, mut ship) = make();
    let kind = ship.kind.clone();
    let checks: Vec<(String, lunar_signals::Program)> = kind.def.esenciales.iter().map(|(what, cond)| lunar_signals::compile_in(cond, &ship.store).map(|p| (what.clone(), p)).map_err(|e| format!("esencial '{what}': {}", e.0))).collect::<Result<_, _>>()?;
    let lost = |ship: &Ship| -> Vec<String> {
        let mut eval = lunar_signals::Eval::default();
        checks
            .iter()
            .filter(|(_, p)| {
                let mut slots = vec![0.0; p.slots];
                eval.run(p, &ship.store, &mut slots, crate::ship::TICK, ship.t) < 0.5
            })
            .map(|(what, _)| what.clone())
            .collect()
    };
    run(&mut ship, &mut s, WARM);
    let mut out = Vec::new();
    let before = lost(&ship);
    if !before.is_empty() {
        out.push(Failure { part: String::new(), lost: before.clone() });
    }
    for i in (0..kind.parts.len()).filter(|&i| which(i, kind.roles[i])) {
        let (mut s, mut ship) = make();
        run(&mut ship, &mut s, WARM);
        s.parts[i].alive = false;
        s.parts[i].working = false;
        s.refresh();
        run(&mut ship, &mut s, AFTER);
        let now: Vec<String> = lost(&ship).into_iter().filter(|l| !before.contains(l)).collect();
        if !now.is_empty() {
            out.push(Failure { part: kind.parts[i].clone(), lost: now });
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------- panels and seats

/// Panels with something through their plate, or standing on nothing: looked at straight on,
/// every few centimetres of the plate, nothing is in front of it that is not it, and it is on
/// something (its box, the part it is mounted on).
pub fn panels_cut(ship: &Ship, s: &Structure) -> Vec<String> {
    /// From this far in front of the plate (m); what stands less than this over it only touches
    /// it; and what it is on is no farther behind it than this.
    const OUT: f32 = 0.12;
    const TOUCH: f32 = 0.0015;
    const ON: f32 = 0.02;
    const STEP: f32 = 0.03;
    let mut out = Vec::new();
    for p in &ship.kind.panels {
        let f = Panels::frame(&ship.kind, s, p);
        let n = f.transform_vector3(Vec3::Z).normalize();
        let (w, h) = (p.layout.size[0] / 1000.0, p.layout.size[1] / 1000.0);
        let (nx, ny) = ((w / STEP).ceil().max(1.0) as usize, (h / STEP).ceil().max(1.0) as usize);
        let (mut through, mut floating): (Vec<(u32, f32)>, usize) = (Vec::new(), 0);
        for i in 0..=nx {
            for j in 0..=ny {
                let at = f.transform_point3(Vec3::new(w * i as f32 / nx as f32, h * j as f32 / ny as f32, 0.0));
                match s.raycast(at + n * OUT, -n, OUT + ON) {
                    Some(hit) if hit.part != p.part && hit.t < OUT - TOUCH => match through.iter_mut().find(|t| t.0 == hit.part) {
                        Some(t) => t.1 = t.1.max(OUT - hit.t),
                        None => through.push((hit.part, OUT - hit.t)),
                    },
                    Some(_) => {}
                    None => floating += 1,
                }
            }
        }
        for (part, by) in through {
            out.push(format!("panel {}: {} lo atraviesa ({:.0} mm por delante de su cara)", p.id, ship.kind.parts[part as usize], by * 1000.0));
        }
        if floating * 10 > (nx + 1) * (ny + 1) {
            out.push(format!("panel {}: {} de {} puntos de su placa no están sobre nada", p.id, floating, (nx + 1) * (ny + 1)));
        }
    }
    out
}

/// What is only seen (drops, pipes) and runs across a panel's face or right in front of it.
pub fn panel_faces(kind: &ShipKind) -> Vec<String> {
    let mut out = Vec::new();
    for p in &kind.panels {
        let inv = p.frame.inverse();
        let (w, h) = (p.layout.size[0] / 1000.0, p.layout.size[1] / 1000.0);
        let over = |q: Vec3| {
            let l = inv.transform_point3(q);
            l.x > 0.0 && l.x < w && l.y > 0.0 && l.y < h && l.z > -0.01 && l.z < 0.15
        };
        let n = kind.seen.iter().filter(|way| way.windows(2).any(|s| (0..=(s[0].distance(s[1]) / 0.02).ceil() as usize).any(|k| over(s[0].lerp(s[1], (k as f32 * 0.02 / s[0].distance(s[1]).max(1e-6)).min(1.0)))))).count();
        if n > 0 {
            out.push(format!("panel {}: {n} cables o tubos le cruzan la cara", p.id));
        }
    }
    out
}

/// Seated, what a hand reaches without getting up (m, from the eyes); how far the head turns to
/// either side of where the seat faces and up and down (degrees); and the least angle a plate is
/// looked at from to be read and worked (degrees over its plane).
pub const ARM: f32 = 1.3;
pub const HEAD_TURN: f32 = 125.0;
pub const HEAD_UP: f32 = 72.0;
pub const HEAD_DOWN: f32 = 80.0;
pub const FLATTEST: f32 = 12.0;

/// A crew member sat in a suit with its pack on, as the game draws them (`app/body`; a test
/// there holds these to its rig): how far over what they sit on their eyes are, how far ahead of
/// their hips, how long their thighs are, and how far off that a seat's eyes may be (m).
pub const SEATED_EYE: f32 = 0.89;
pub const EYE_AHEAD: f32 = 0.17;
pub const THIGH: f32 = 0.42;
pub const SEAT_FIT: f32 = 0.03;

/// The seats whose eyes are not where a body sat on them has its own: too low over the cushion
/// (the body would be sunk in it) or too high (it would float), off it to a side, or so far
/// forward or back that the thighs are not on it. The cushion is the seat's part (`pieza`),
/// whatever its shape.
pub fn seat_fit(kind: &ShipKind, s: &Structure) -> Vec<String> {
    let mut out = Vec::new();
    for seat in &kind.seats {
        let d = &seat.def;
        let Some(p) = s.parts.get(seat.part as usize) else { continue };
        let yaw = d.rumbo.to_radians();
        let (ahead, side) = (Vec3::new(yaw.sin(), 0.0, yaw.cos()), Vec3::new(yaw.cos(), 0.0, -yaw.sin()));
        let (mut top, mut front, mut lo, mut hi) = (f32::MIN, f32::MIN, f32::MAX, f32::MIN);
        for v in p.shape.verts() {
            let v = p.local.transform_point3(v);
            top = top.max(v.y);
            front = front.max(v.dot(ahead));
            lo = lo.min(v.dot(side));
            hi = hi.max(v.dot(side));
        }
        let eye = Vec3::from_array(d.ojos);
        let over = eye.y - top;
        if (over - SEATED_EYE).abs() > SEAT_FIT {
            out.push(format!("asiento {}: los ojos a {over:.2} m sobre el cojín; sentado quedan a {SEATED_EYE:.2} m ({})", d.id, if over < SEATED_EYE { "el cuerpo se hunde en él" } else { "el cuerpo flota sobre él" }));
        }
        // the knees a little past its front edge, no more: the thighs lie on it
        let knee = eye.dot(ahead) - EYE_AHEAD + THIGH - front;
        if !(0.0..=0.2).contains(&knee) {
            out.push(format!("asiento {}: las rodillas a {knee:+.2} m del borde del cojín (de 0 a 0,20 m por delante)", d.id));
        }
        let across = eye.dot(side);
        if across < lo + 0.15 || across > hi - 0.15 {
            out.push(format!("asiento {}: los ojos no están sobre el cojín (a {:.2} m de su centro)", d.id, across - (lo + hi) * 0.5));
        }
    }
    out
}

/// What cannot be worked or read from a seat that should be: every control and instrument of the
/// panels a seat works (`asientos[].paneles`) is in sight from it (the head turns that far, the
/// plate is not edge-on, nothing is in the way, the hand finds it) and within reach; and it is at
/// arm's length from one at least of the seats that share its panel.
pub fn seat_reach(ship: &Ship, s: &Structure) -> Vec<String> {
    let kind = &ship.kind;
    let mut out = Vec::new();
    // per (panel, is control, index): the nearest seat's distance
    let mut nearest: std::collections::BTreeMap<(usize, bool, usize), f32> = std::collections::BTreeMap::new();
    for seat in &kind.seats {
        let d = &seat.def;
        let eye = Vec3::from_array(d.ojos);
        let yaw = d.rumbo.to_radians();
        // where the seat faces (heading 0: the nose, +z; positive to port, +x)
        let (ahead, side) = (Vec3::new(yaw.sin(), 0.0, yaw.cos()), Vec3::new(yaw.cos(), 0.0, -yaw.sin()));
        for id in &d.paneles {
            let Some(pi) = kind.panels.iter().position(|p| &p.id == id) else {
                out.push(format!("asiento {}: no hay panel '{id}'", d.id));
                continue;
            };
            let plan = &kind.panels[pi];
            let mut check = |control: bool, k: usize, name: &str| {
                let rect = if control { &ship.panels.controls[k].rect } else { &ship.panels.indicators[k].rect };
                let at = Panels::frame(kind, s, plan).transform_point3(Vec3::new((rect.x + rect.w * 0.5) / 1000.0, (rect.y + rect.h * 0.5) / 1000.0, 0.0));
                let e = nearest.entry((pi, control, k)).or_insert(f32::MAX);
                *e = e.min(at.distance(eye));
                if let Some(why) = in_sight(ship, s, eye, ahead, side, control, k) {
                    out.push(format!("asiento {}: {} {name} {why}", d.id, if control { "el mando" } else { "el instrumento" }));
                }
            };
            for (k, c) in ship.panels.controls.iter().enumerate().filter(|(_, c)| c.panel == pi) {
                check(true, k, &c.id);
            }
            for (k, i) in ship.panels.indicators.iter().enumerate().filter(|(_, i)| i.panel == pi) {
                let name = format!("{}/{}", plan.id, plan.def.mandos[i.index].id);
                check(false, k, &name);
            }
        }
    }
    for ((pi, control, k), dist) in nearest {
        if control && dist > ARM {
            out.push(format!("{}: a {dist:.2} m del asiento más cercano (más que un brazo, {ARM} m)", ship.panels.controls[k].id));
        }
        let _ = pi;
    }
    out
}

/// Whether control `k` (`control`; else instrument `k`) is worked or read from eyes at `eye`
/// (ship frame, as the structure is posed now) facing `ahead` (level, unit; `side` to port):
/// within reach, the head turns that far, the plate is not edge-on, nothing is in the way and
/// the hand finds it. None if so; else why not, in a few words.
pub fn in_sight(ship: &Ship, s: &Structure, eye: Vec3, ahead: Vec3, side: Vec3, control: bool, k: usize) -> Option<String> {
    use crate::panels::{REACH, READ};
    let kind = &ship.kind;
    let (pi, rect) = if control { (ship.panels.controls[k].panel, &ship.panels.controls[k].rect) } else { (ship.panels.indicators[k].panel, &ship.panels.indicators[k].rect) };
    let f = Panels::frame(kind, s, &kind.panels[pi]);
    let n = f.transform_vector3(Vec3::Z).normalize();
    let at = f.transform_point3(Vec3::new((rect.x + rect.w * 0.5) / 1000.0, (rect.y + rect.h * 0.5) / 1000.0, 0.0));
    let to = at - eye;
    let dist = to.length();
    let dir = to / dist;
    let turn = dir.dot(side).atan2(dir.dot(ahead)).to_degrees().abs();
    let pitch = dir.y.asin().to_degrees();
    let over = (-dir).dot(n).asin().to_degrees();
    if dist > if control { REACH } else { READ } {
        return Some(format!("está a {dist:.2} m"));
    }
    if turn > HEAD_TURN || pitch > HEAD_UP || pitch < -HEAD_DOWN {
        return Some(format!("pide girar la cabeza {turn:.0}° y {pitch:+.0}°"));
    }
    if over < FLATTEST {
        return Some(format!("se ve de canto ({over:.0}° sobre su placa)"));
    }
    // aimed at its middle, on the plate or at the height its body stands off it
    let mut found = None;
    let mut other = String::new();
    for lift in [0.0, 0.03, 0.015] {
        let dir = (at + n * lift - eye).normalize();
        let wall = s.raycast(eye, dir, REACH);
        let hit = if control {
            let p = ship.panels.pick(kind, s, eye, dir, REACH);
            if let Some((j, _, _)) = p.filter(|(j, _, _)| *j != k) {
                other = ship.panels.controls[j].id.clone();
            }
            p.filter(|(j, _, _)| *j == k || ship.panels.controls[k].cover == Some(*j)).map(|x| x.2)
        } else {
            ship.panels.pick_indicator(kind, s, eye, dir, READ).filter(|(j, _)| *j == k).map(|x| x.1)
        };
        match hit {
            Some(t) if wall.is_none_or(|h| t <= h.t + 0.08) => {
                found = Some(Ok(()));
                break;
            }
            Some(_) => found = found.or(Some(Err(kind.parts[wall.map_or(0, |h| h.part) as usize].clone()))),
            None => {}
        }
    }
    match found {
        Some(Ok(())) => None,
        Some(Err(part)) => Some(format!("queda detrás de {part}")),
        None if other.is_empty() => Some("no lo encuentra la mano".to_string()),
        None => Some(format!("no lo encuentra la mano: encuentra {other} en su lugar")),
    }
}

// ---------------------------------------------------------------- what it holds and what it weighs

/// What is not said about a ship's stores (`contents`): a machine that counts what it holds
/// (`<id>.masa`) in a component that holds nothing or does not follow it (what it keeps would
/// weigh nothing, or always the same), and a container that follows a machine without being
/// tied to how much that takes (`cabida`, `volumen`: the two could say different things). What
/// does not fit is refused when the ship is put together (`contents::fit`).
pub fn stores(ship: &Ship) -> Vec<String> {
    let kind = &ship.kind;
    let mut out = Vec::new();
    for m in &kind.machines {
        let counts = ship.store.find(&format!("{}.masa", m.id)).is_some_and(|sig| &*ship.store.meta(sig).show == "kg");
        if !counts || kind.clamps.iter().any(|c| c.id == m.id) {
            continue;
        }
        match kind.holders.iter().find(|h| h.id == m.id) {
            None => out.push(format!("{}: su máquina cuenta lo que guarda ({}.masa) y sus piezas no llevan nada: no pesa (dale 'contenido' con 'nivel')", m.id, m.id)),
            Some(h) if h.level.is_none() => out.push(format!("{}: su máquina cuenta lo que guarda ({}.masa) y lo que llevan sus piezas no la sigue: pesa siempre lo mismo (falta 'nivel' en su 'contenido')", m.id, m.id)),
            Some(h) if !h.tied => out.push(format!("{}: lo que cabe en su máquina no sale de lo que cabe en sus piezas: pueden no decir lo mismo (falta 'cabida' o 'volumen' en su 'contenido')", m.id)),
            Some(h) if h.capacity > h.brim * 1.001 => out.push(format!("{}: lleva hasta {:.0} kg y dentro caben {:.0} kg", m.id, h.capacity, h.brim)),
            Some(_) => {}
        }
    }
    out
}

/// A ship takes off with this much more push than it weighs, at least; and brings what pushes
/// it up under this share of its weight to come down.
pub const LIFT_MARGIN: f64 = 1.2;
pub const LAND_MARGIN: f64 = 0.95;

/// What pushes a ship against what it weighs, full and empty (SI): the same figures under any
/// gravity (`Lift::faults`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Lift {
    /// kg with its tanks full and with no propellant left (what else it carries is in both).
    pub full: f64,
    pub dry: f64,
    /// N its engines push with together at full and at their least throttle.
    pub thrust: f64,
    pub least: f64,
    /// N its thrusters push it up with, and down, all those that point that way together.
    pub jets_up: f64,
    pub jets_down: f64,
    /// m/s its propellant is worth from full, and the engines' exhaust velocity (m/s).
    pub dv: f64,
    pub ve: f64,
}

/// A ship's `Lift`, as it stands now (its systems run a tick at least: `mass`).
pub fn lift(ship: &Ship, s: &Structure) -> Lift {
    let f = ship.mass.figures();
    let kind = &ship.kind;
    let ids: Vec<String> = kind.machines.iter().map(|m| m.id.clone()).collect();
    let (mut up, mut down) = (0.0, 0.0);
    for pat in kind.def.vuelo.iter().flat_map(|v| &v.rcs) {
        for m in crate::kind::resolve(&ids, pat) {
            let rt = &ship.machines[m as usize];
            let Some(part) = rt.part.map(|p| &s.parts[p as usize]).filter(|p| p.alive) else { continue };
            let y = f64::from(part.local.transform_vector3(rt.thrust_axis).normalize_or_zero().y) * f64::from(crate::exhaust::rated(&kind.machines[m as usize].def));
            if y > 0.0 { up += y } else { down -= y }
        }
    }
    let full = f.dry + f.capacity;
    Lift { full, dry: f.dry, thrust: f.thrust, least: f.least, jets_up: up, jets_down: down, dv: if f.dry > 0.0 { f.ve * (full / f.dry).ln() } else { 0.0 }, ve: f.ve }
}

impl Lift {
    /// Thrust over weight under gravity `g` (m/s²): full, and with no propellant left.
    pub fn ratios(&self, g: f64) -> (f64, f64) {
        (self.thrust / (self.full * g).max(1e-9), self.thrust / (self.dry * g).max(1e-9))
    }

    /// What it cannot do under gravity `g` (m/s²): take off with its tanks full (its engines,
    /// or its thrusters, push `LIFT_MARGIN` times what it weighs), and come down with them empty
    /// (what pushes it up can be brought under its weight: its engines at their least throttle,
    /// less what its thrusters push down with; or it hangs on its thrusters alone, engines off).
    pub fn faults(&self, g: f64) -> Vec<String> {
        let mut out = Vec::new();
        let kn = |n: f64| lunar_core::structure::contents::figure((n / 1e3) as f32);
        let kg = |m: f64| lunar_core::structure::contents::figure(m as f32);
        let (heavy, light) = (self.full * g, self.dry * g);
        let up = self.thrust.max(self.jets_up);
        if up < heavy * LIFT_MARGIN {
            out.push(format!("llena ({} kg) no despega con g = {g:.2} m/s²: empuja {} kN y pesa {} kN (hace falta {LIFT_MARGIN} veces su peso)", kg(self.full), kn(up), kn(heavy)));
        }
        if self.least - self.jets_down >= light * LAND_MARGIN && self.jets_up < light / LAND_MARGIN {
            out.push(format!("vacía ({} kg) no puede posarse con g = {g:.2} m/s²: pesa {} kN, sus motores al mínimo empujan {} kN (sus toberas la bajan con {} kN) y sus toberas solas la sostienen con {} kN", kg(self.dry), kn(light), kn(self.least), kn(self.jets_down), kn(self.jets_up)));
        }
        out
    }
}
