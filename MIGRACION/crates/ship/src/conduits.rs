//! Conduits of a ship, simple on purpose: cheap to draw, to hit and to understand.
//!
//! - **Trunks**: one line per wall of every room (a steel tube along each side wall, one over the
//!   doorway of each end wall), joined at the corners and through the bulkheads. Everything
//!   electrical and every data line goes along them, and every room is a ring, so every run is
//!   laid twice: the short way and the other way round, through other pieces. Cut one wall's
//!   trunk and what went through it still gets there the other way (only what plugs into that
//!   very piece is lost). Trunks are parts (a few per wall): a hit cuts what goes through that
//!   piece, and that is all the damage there is to model.
//! - **Outlets**: boxes on a trunk every half metre at most, only where something plugs in. An
//!   outlet is a part too; what plugs into it goes dead with it (and it sparks, badly hurt).
//!   A junction of a network (a bus, a circuit's node) is not a device: it sits where two pieces
//!   of trunk meet, so losing either piece leaves it on the other.
//! - **Drops**: the cable from an outlet to its device. Only seen: it lies along the wall in the
//!   outlet's look, nothing strikes it, it does not break. A device too far from any wall, or
//!   outside the rooms, has none (it is connected all the same).
//! - **Pipes** between machines (hydraulics, propellant, coolant, gas): fixed, only seen, down to
//!   under the deck and along it. **Ducts**: straight pieces between their junctions, as parts.
//!
//! The network a run belongs to does not care about any of this: each run comes back as the
//! points it goes through and the part each stretch lives in (none: it cannot be cut).
use crate::{
    def::{BulkheadDef, ConduitsDef, HullDef},
    geom::{self, GHOST, GenPart, NO_COLLIDE, Role, SPARKS},
};
use glam::{Affine3A, Vec2, Vec3};
use lunar_core::mesh::{Material, Mesh};
use std::{
    cmp::Ordering,
    collections::{BTreeMap, BinaryHeap},
};

/// Classes of conduit: cables (and data), pipes, ducts.
pub const CLASSES: usize = 3;

/// Radius of a trunk (m), its gap to the wall, and the longest piece of it that is one part.
pub const TRUNK_R: f32 = 0.032;
const TRUNK_OFF: f32 = 0.02;
const PIECE: f32 = 2.2;
/// An outlet every so far along a trunk at most (m).
const EVERY: f32 = 0.5;
/// Heights tried for a side wall's trunk (share of the room's height), best first.
const HEIGHTS: [f32; 4] = [0.77, 0.66, 0.5, 0.08];
/// A wall whose best line still meets something this often along it gets no trunk.
const TOO_BUSY: f32 = 0.08;
/// An end wall's trunk goes this far over its doorway (m), and no nearer the ceiling than this.
const OVER_DOOR: f32 = 0.1;
const UNDER_CEILING: f32 = 0.12;
/// A drop keeps this far off its wall (m); side by side they are this far apart.
const CABLE_OFF: f32 = 0.014;
const LANE: f32 = 0.022;
/// The last stretch of a drop, off the wall to its device: no longer than this (m), or no drop.
const MAX_FREE: f32 = 1.3;
/// Trunks whose corners are this near are joined (m).
const LINK: f32 = 0.6;
/// Pipes run this far under the deck (m); a port higher than this over it goes down its wall.
const UNDER: f32 = 0.1;
const RISE: f32 = 1.2;
/// A room takes in what is this far outside its box (m).
const ROOM_PAD: f32 = 0.2;

/// A run to lay: between nodes `from` and `to` of network `net`, from `a` to `b` (ship frame).
#[derive(Clone, Debug)]
pub struct Run {
    pub net: u16,
    pub from: u32,
    pub to: u32,
    pub a: Vec3,
    pub b: Vec3,
    /// Conduit kind (its look) and colour.
    pub kind: String,
    pub color: [u8; 3],
    /// A device's own connection (from the device at `a` to its junction at `b`): it gets an
    /// outlet and a drop. The rest are runs between junctions: they only go along the trunks.
    pub stub: bool,
    /// Ids of its parts start with this.
    pub prefix: String,
    /// Points it must pass by on its way (the editor, `por` in the data).
    pub via: Vec<Vec3>,
}

/// One way a run goes: the points it goes through (from its `a` to its `b`) and the part (index
/// into `Laid::parts`) each stretch between two points lives in (none: it cannot be cut).
#[derive(Clone, Debug, Default)]
pub struct Chain {
    pub run: usize,
    pub pts: Vec<Vec3>,
    pub owners: Vec<Option<usize>>,
}

/// What laying the runs gives: new parts, and the ways each run goes (one at least; two for a
/// cable run that can go round the other way).
pub struct Laid {
    pub parts: Vec<GenPart>,
    pub chains: Vec<Chain>,
    /// The ways of what is only seen (drops, pipes), for whoever checks where they go.
    pub seen: Vec<Vec<Vec3>>,
}

/// A face nothing may run across: a panel's plate (ship frame → its plane: x across, y up, z out
/// of it, from its bottom-left corner), its size and how deep its box is behind it.
#[derive(Clone, Copy, Debug)]
pub struct Keep {
    pub inv: Affine3A,
    pub size: Vec2,
    pub depth: f32,
}

/// Round a panel's plate and out of its face, the room a hand needs (m).
pub const KEEP_SIDE: f32 = 0.035;
pub const KEEP_FRONT: f32 = 0.2;

/// An edge of a path already taken costs this many times its length to another (the other way
/// round keeps to other pieces where it can).
const SHARED: f32 = 60.0;

/// The class of a conduit kind: cables, pipes or ducts.
pub fn class_of(kind: &str) -> usize {
    if kind.starts_with("conducto") {
        2
    } else if kind.starts_with("tubo") {
        1
    } else {
        0
    }
}

/// Where the conduits go: the rooms, the hull round them, the bulkheads and what is already there.
pub struct Site<'a> {
    pub hull: Option<&'a HullDef>,
    pub bulkheads: &'a [BulkheadDef],
    /// Rooms: a name and a box each (ship frame).
    pub rooms: Vec<(String, [Vec3; 2])>,
    /// Everything solid so far (trunks keep clear of devices and of glass).
    pub parts: &'a [GenPart],
    pub cfg: &'a ConduitsDef,
    /// The panels' faces: nothing only seen (a drop, a pipe) runs across one.
    pub keep: &'a [Keep],
}

impl Site<'_> {
    fn room_of(&self, p: Vec3) -> Option<usize> {
        self.rooms.iter().position(|(_, [lo, hi])| p.cmpge(*lo - ROOM_PAD).all() && p.cmple(*hi + ROOM_PAD).all())
    }

    /// Where something kept `off` m clear of the wall on side `s` (±1 along x) is at height `y`
    /// and `z`, in room `r`.
    fn wall_x(&self, r: usize, y: f32, z: f32, s: f32, off: f32) -> f32 {
        let [lo, hi] = self.rooms[r].1;
        self.wall(r, y, z, s, off).unwrap_or(if s > 0.0 { hi.x - off } else { lo.x + off })
    }

    /// The same, or None where the room's cut does not reach that height (over its ceiling, under
    /// its keel).
    fn wall(&self, r: usize, y: f32, z: f32, s: f32, off: f32) -> Option<f32> {
        let [lo, hi] = self.rooms[r].1;
        let side = if s > 0.0 { hi.x - off } else { lo.x + off };
        let ring = self.outline(r, z, off);
        let mut xs = (f32::MAX, f32::MIN);
        for i in 0..ring.len() {
            let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
            if (a.y - y) * (b.y - y) <= 0.0 && (a.y - b.y).abs() > 1e-6 {
                let x = a.x + (b.x - a.x) * (y - a.y) / (b.y - a.y);
                xs = (xs.0.min(x), xs.1.max(x));
            }
        }
        if xs.0 > xs.1 {
            return None;
        }
        Some(if s > 0.0 { xs.1.min(side) } else { xs.0.max(side) })
    }

    /// The inside of room `r` cut at `z`, `off` m clear of its walls: a closed outline (x, y),
    /// counter-clockwise.
    fn outline(&self, r: usize, z: f32, off: f32) -> Vec<Vec2> {
        let [lo, hi] = self.rooms[r].1;
        let ring = self.hull.and_then(|h| geom::outline_at(h, z).ok()).unwrap_or_else(|| vec![Vec2::new(lo.x, lo.y), Vec2::new(hi.x, lo.y), Vec2::new(hi.x, hi.y), Vec2::new(lo.x, hi.y)]);
        if off > 0.0 { geom::offset(&ring, off) } else { ring }
    }

    /// Whether `p` is on a panel's face or in the room in front of it.
    fn on_panel(&self, p: Vec3) -> bool {
        self.keep.iter().any(|k| {
            let q = k.inv.transform_point3(p);
            q.x > -KEEP_SIDE && q.x < k.size.x + KEEP_SIDE && q.y > -KEEP_SIDE && q.y < k.size.y + KEEP_SIDE && q.z > -k.depth - 0.01 && q.z < KEEP_FRONT
        })
    }

    /// Whether the straight way from `a` to `b` goes across a panel.
    fn crosses_panel(&self, a: Vec3, b: Vec3) -> bool {
        let steps = (a.distance(b) / 0.03).ceil().max(1.0) as usize;
        !self.keep.is_empty() && (0..=steps).any(|k| self.on_panel(a.lerp(b, k as f32 / steps as f32)))
    }

    /// The stations to try for a run down a side wall meant for `z`: `z` itself, then just fore
    /// and aft of every panel (the nearest first), within the room (`lo` to `hi`).
    fn stations(&self, z: f32, lo: f32, hi: f32) -> Vec<f32> {
        let mut out = vec![z];
        for k in self.keep {
            let f = k.inv.inverse();
            let zs = [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)].map(|(a, b)| f.transform_point3(Vec3::new(a * k.size.x, b * k.size.y, 0.0)).z);
            let (z0, z1) = (zs.iter().copied().fold(f32::MAX, f32::min), zs.iter().copied().fold(f32::MIN, f32::max));
            out.extend([z0 - KEEP_SIDE - 0.03, z1 + KEEP_SIDE + 0.03].into_iter().filter(|c| *c > lo && *c < hi));
        }
        out[1..].sort_by(|a, b| (a - z).abs().total_cmp(&(b - z).abs()));
        out
    }

    /// Whether `p` is in a device (`.0`) or on glass (`.1`).
    fn meets(&self, p: Vec3) -> (bool, bool) {
        let (mut device, mut glass) = (false, false);
        for g in self.parts.iter().filter(|g| matches!(g.role, Role::Component | Role::Glass)) {
            let q = g.at.inverse().transform_point3(p);
            let (c, r) = g.shape.sphere();
            if q.distance(c) > r + 0.06 {
                continue;
            }
            let d = g.shape.distance(q);
            if g.role == Role::Glass {
                glass |= d < 0.03;
            } else {
                device |= d < 0.045;
            }
        }
        (device, glass)
    }

    /// Whether glass is right behind `p`, looking along `dir` (a trunk before a window).
    fn before_glass(&self, p: Vec3, dir: Vec3) -> bool {
        self.parts.iter().filter(|g| g.role == Role::Glass).any(|g| {
            let inv = g.at.inverse();
            g.shape.raycast(inv.transform_point3(p - dir * 0.05), inv.transform_vector3(dir), 0.3).is_some()
        })
    }

    /// The bulkhead at `z` (its face toward `toward`, its doorway), if there is one.
    fn bulkhead(&self, z: f32, toward: f32) -> (f32, Option<[f32; 3]>) {
        match self.bulkheads.iter().find(|b| (b.z - z).abs() < 0.2) {
            Some(b) => (b.z + toward * b.espesor * 0.5, b.puerta),
            None => (z, None),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Wall {
    /// A side wall: which (±1 along x).
    Side(f32),
    /// An end wall: the way to it along z (±1) and its doorway.
    End(f32, Option<[f32; 3]>),
}

#[derive(Clone, Debug)]
struct Trunk {
    room: usize,
    name: String,
    pts: Vec<Vec3>,
    wall: Wall,
}

impl Trunk {
    fn length(&self) -> f32 {
        self.pts.windows(2).map(|w| w[0].distance(w[1])).sum()
    }

    /// The point `arc` m along it, and the direction there.
    fn at(&self, arc: f32) -> (Vec3, Vec3) {
        let mut left = arc.max(0.0);
        let last = self.pts.len().saturating_sub(2);
        for (i, w) in self.pts.windows(2).enumerate() {
            let l = w[0].distance(w[1]);
            if left <= l || i == last {
                let d = (w[1] - w[0]) / l.max(1e-6);
                return (w[0] + d * left.min(l), d);
            }
            left -= l;
        }
        (self.pts[0], Vec3::Z)
    }

    /// The nearest point of it to `p`: how far along, and how far away.
    fn nearest(&self, p: Vec3) -> (f32, f32) {
        let (mut best, mut arc0) = ((0.0, f32::MAX), 0.0);
        for w in self.pts.windows(2) {
            let d = w[1] - w[0];
            let l = d.length();
            let t = if l > 1e-6 { ((p - w[0]).dot(d) / (l * l)).clamp(0.0, 1.0) } else { 0.0 };
            let dist = p.distance(w[0] + d * t);
            if dist < best.1 {
                best = (arc0 + l * t, dist);
            }
            arc0 += l;
        }
        best
    }

    /// Toward its wall (unit).
    fn to_wall(&self) -> Vec3 {
        match self.wall {
            Wall::Side(s) => Vec3::X * s,
            Wall::End(s, _) => Vec3::Z * s,
        }
    }
}

fn dedup(pts: &mut Vec<Vec3>) {
    pts.dedup_by(|a, b| a.distance(*b) < 2e-3);
}

/// The trunks of every room: side walls first, then the end walls (which join them).
/// The points between `a` and `b` a line on a wall needs to stay on it: its middle wherever that
/// is more than 2 cm off the straight way, and so on down (three times at most).
fn follow(at: &dyn Fn(f32) -> Option<Vec3>, a: Vec3, b: Vec3, depth: u32, out: &mut Vec<Vec3>) -> Option<()> {
    let m = at((a.z + b.z) * 0.5)?;
    if depth < 3 && (m.x - (a.x + b.x) * 0.5).abs() > 0.02 {
        follow(at, a, m, depth + 1, out)?;
        out.push(m);
        follow(at, m, b, depth + 1, out)?;
    }
    Some(())
}

fn trunks(site: &Site) -> Vec<Trunk> {
    let off = TRUNK_R + TRUNK_OFF;
    let mut out: Vec<Trunk> = Vec::new();
    // where each room's end trunks stand (z), and its side trunks' height
    let ends: Vec<[f32; 2]> = site.rooms.iter().map(|(_, [lo, hi])| [site.bulkhead(lo.z, 1.0).0 + off, site.bulkhead(hi.z, -1.0).0 - off]).collect();
    let mut side_y: Vec<Option<f32>> = vec![None; site.rooms.len()];
    for (r, (name, [lo, hi])) in site.rooms.iter().enumerate() {
        let h = hi.y - lo.y;
        let [z0, z1] = ends[r];
        if z1 - z0 < 0.4 {
            continue;
        }
        let mut zs = vec![z0, z1];
        if let Some(hull) = site.hull {
            zs.extend(hull.estaciones.iter().map(|s| s.z).filter(|z| *z > z0 + 0.15 && *z < z1 - 0.15));
        }
        zs.sort_by(f32::total_cmp);
        // along the wall at a height: by the stations, and between two of them wherever the wall
        // is not straight (where the section changes shape, a level line on it bends)
        let line = |y: f32, s: f32| -> Option<Vec<Vec3>> {
            let at = |z: f32| Some(Vec3::new(site.wall(r, y, z, s, off)?, y, z));
            let mut pts = vec![at(zs[0])?];
            for w in zs.windows(2) {
                let (a, b) = (at(w[0])?, at(w[1])?);
                follow(&at, a, b, 0, &mut pts)?;
                pts.push(b);
            }
            Some(pts)
        };
        // how often a line meets a device, or glass (which counts four times)
        let busy = |pts: &[Vec3], s: f32| -> f32 {
            let (mut n, mut bad) = (0.0, 0.0);
            for w in pts.windows(2) {
                let steps = (w[0].distance(w[1]) / 0.25).ceil().max(1.0) as usize;
                for k in 0..=steps {
                    let p = w[0].lerp(w[1], k as f32 / steps as f32);
                    let (device, glass) = (site.meets(p).0, site.before_glass(p, Vec3::X * s));
                    n += 1.0;
                    bad += if glass {
                        4.0
                    } else if device {
                        1.0
                    } else {
                        0.0
                    };
                }
            }
            bad / n
        };
        // one height for both sides: the first that is clear on both, else the least busy
        let heights: Vec<f32> = match site.cfg.altura {
            Some(a) => vec![a],
            None => HEIGHTS.to_vec(),
        };
        let mut best: Option<(f32, f32)> = None;
        for f in heights {
            let y = lo.y + h * f;
            let (Some(port), Some(starboard)) = (line(y, 1.0), line(y, -1.0)) else {
                continue;
            };
            let b = busy(&port, 1.0).max(busy(&starboard, -1.0));
            if best.is_none_or(|(_, bb)| b < bb - 1e-4) {
                best = Some((y, b));
            }
            if b == 0.0 {
                break;
            }
        }
        if let Some((y, b)) = best.filter(|(_, b)| *b <= TOO_BUSY || site.cfg.altura.is_some()) {
            let _ = b;
            side_y[r] = Some(y);
            for (s, tag) in [(1.0, "babor"), (-1.0, "estribor")] {
                let Some(mut pts) = line(y, s) else { continue };
                dedup(&mut pts);
                if pts.len() >= 2 {
                    out.push(Trunk { room: r, name: format!("canal.{name}.{tag}"), pts, wall: Wall::Side(s) });
                }
            }
        }
    }
    // the end walls: over the doorway, at the height of the side trunks on either side of the wall
    for (r, (name, [lo, hi])) in site.rooms.iter().enumerate() {
        let h = hi.y - lo.y;
        for (e, zt, sign, tag) in [(lo.z, ends[r][0], -1.0f32, "popa"), (hi.z, ends[r][1], 1.0f32, "proa")] {
            let door = site.bulkhead(e, -sign).1;
            // the rooms this wall stands between
            let across = site.rooms.iter().enumerate().filter(|(_, (_, [l, h2]))| (l.z - e).abs() < 0.3 || (h2.z - e).abs() < 0.3).filter_map(|(k, _)| side_y[k]).fold(f32::MIN, f32::max);
            let mut y = if across > f32::MIN { across } else { lo.y + h * HEIGHTS[0] };
            if let Some([_, _, top]) = door {
                y = y.max(top + OVER_DOOR);
            }
            let y = y.min(hi.y - UNDER_CEILING);
            let (Some(xp), Some(xs)) = (site.wall(r, y, zt, 1.0, off), site.wall(r, y, zt, -1.0, off)) else {
                continue;
            };
            if xp - xs < 0.3 {
                continue;
            }
            let mut pts = Vec::new();
            // from the port side trunk's end, up (or down) the corner to this height, and across
            let corner = |s: f32| -> Option<Vec3> {
                let t = out.iter().find(|t| t.room == r && t.wall == Wall::Side(s))?;
                Some(if sign < 0.0 { t.pts[0] } else { *t.pts.last()? })
            };
            let riser = |c: Vec3, s: f32| -> Vec<Vec3> {
                if (c.y - y).abs() < 0.05 {
                    return vec![Vec3::new(c.x, y, zt)];
                }
                let x = |h: f32| site.wall_x(r, h, zt, s, off);
                let (a, b) = (c.y.min(y), c.y.max(y));
                if b - a > 0.5 {
                    // a long climb: straight up the end wall, the nearest the corner it is clear
                    // (of glass, of what hangs on the wall), not past the doorway
                    let reach = x(a).abs().min(x(b).abs()) - 0.1;
                    let stop = door.map_or(0.2, |[x0, x1, _]| x0.abs().max(x1.abs()) + 0.12);
                    let clear = |at: f32| (0..=((b - a) / 0.1) as usize).all(|k| site.meets(Vec3::new(s * at, a + 0.1 * k as f32, zt)) == (false, false));
                    let mut at = reach;
                    while at > stop && !clear(at) {
                        at -= 0.1;
                    }
                    return vec![c, Vec3::new(s * at, c.y, zt), Vec3::new(s * at, y, zt)];
                }
                // a short one: up (or down) the corner, by the bends of the wall on the way
                let mut ys: Vec<f32> = site.outline(r, zt, off).iter().filter(|q| q.x * s > 0.0 && q.y > a + 0.05 && q.y < b - 0.05).map(|q| q.y).collect();
                ys.sort_by(f32::total_cmp);
                if c.y > y {
                    ys.reverse();
                }
                let mut v = vec![c];
                v.extend(ys.into_iter().map(|h| Vec3::new(x(h), h, zt)));
                v.push(Vec3::new(x(y), y, zt));
                v
            };
            match corner(1.0) {
                Some(c) => pts.extend(riser(c, 1.0)),
                None => pts.push(Vec3::new(xp, y, zt)),
            }
            match corner(-1.0) {
                Some(c) => pts.extend(riser(c, -1.0).into_iter().rev()),
                None => pts.push(Vec3::new(xs, y, zt)),
            }
            dedup(&mut pts);
            if pts.len() >= 2 {
                out.push(Trunk { room: r, name: format!("canal.{name}.{tag}"), pts, wall: Wall::End(sign, door) });
            }
        }
    }
    out
}

/// The graph of the trunks: points along them joined by the pieces they are made of.
#[derive(Default)]
struct Graph {
    nodes: Vec<Vec3>,
    /// (to, length, part) per node.
    edges: Vec<Vec<(u32, f32, Option<usize>)>>,
    /// Node of a place on a trunk: (trunk, mm along it).
    at: BTreeMap<(usize, i32), u32>,
}

impl Graph {
    fn node(&mut self, p: Vec3) -> u32 {
        self.nodes.push(p);
        self.edges.push(Vec::new());
        (self.nodes.len() - 1) as u32
    }

    fn join(&mut self, a: u32, b: u32, part: Option<usize>) {
        let l = self.nodes[a as usize].distance(self.nodes[b as usize]);
        self.edges[a as usize].push((b, l, part));
        self.edges[b as usize].push((a, l, part));
    }

    /// The shortest way from `a` to `b`: the nodes after `a` and the part of each step. Steps in
    /// `taken` (pairs of nodes, lower first) are not walked again, and the pieces in `parts` cost
    /// `SHARED` times their length.
    fn path(&self, a: u32, b: u32, taken: &[(u32, u32)], parts: &[usize]) -> Option<Vec<(u32, Option<usize>)>> {
        #[derive(PartialEq)]
        struct Open(f32, u32);
        impl Eq for Open {}
        impl Ord for Open {
            fn cmp(&self, o: &Self) -> Ordering {
                o.0.total_cmp(&self.0).then(o.1.cmp(&self.1))
            }
        }
        impl PartialOrd for Open {
            fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
                Some(self.cmp(o))
            }
        }
        let n = self.nodes.len();
        let mut dist = vec![f32::MAX; n];
        let mut prev: Vec<Option<(u32, Option<usize>)>> = vec![None; n];
        let mut heap = BinaryHeap::new();
        dist[a as usize] = 0.0;
        heap.push(Open(0.0, a));
        while let Some(Open(d, v)) = heap.pop() {
            if v == b {
                break;
            }
            if d > dist[v as usize] {
                continue;
            }
            for &(to, l, part) in &self.edges[v as usize] {
                if taken.contains(&(v.min(to), v.max(to))) {
                    continue;
                }
                let nd = d + l * if part.is_some_and(|p| parts.contains(&p)) { SHARED } else { 1.0 };
                if nd < dist[to as usize] {
                    dist[to as usize] = nd;
                    prev[to as usize] = Some((v, part));
                    heap.push(Open(nd, to));
                }
            }
        }
        if a != b && prev[b as usize].is_none() {
            return None;
        }
        let mut out = Vec::new();
        let mut v = b;
        while v != a {
            let (p, part) = prev[v as usize]?;
            out.push((v, part));
            v = p;
        }
        out.reverse();
        Some(out)
    }
}

fn steel() -> Material {
    Material::new([92, 95, 100], 110, 190).with_finish(geom::fin("cepillado"))
}

/// A tube from `a` to `b` (no caps: its ends are in a wall, a collar or another tube).
fn tube(m: &mut Mesh, a: Vec3, b: Vec3, r: f32, sides: u32, mat: Material) {
    let d = (b - a).normalize_or(Vec3::Y);
    let u = d.any_orthonormal_vector();
    let v = d.cross(u);
    let first = m.pos.len() as u32;
    for i in 0..sides {
        let ang = std::f32::consts::TAU * i as f32 / sides as f32;
        let n = u * ang.cos() + v * ang.sin();
        m.vertex(a + n * r, n, mat);
        m.vertex(b + n * r, n, mat);
    }
    for i in 0..sides {
        let j = (i + 1) % sides;
        let (a0, b0, a1, b1) = (first + 2 * i, first + 2 * i + 1, first + 2 * j, first + 2 * j + 1);
        m.tri(a0, a1, b1);
        m.tri(a0, b1, b0);
    }
}

/// A line of tubes through `pts`, each a little long so they meet at the bends.
fn sweep(m: &mut Mesh, pts: &[Vec3], r: f32, sides: u32, mat: Material) {
    for (k, w) in pts.windows(2).enumerate() {
        let d = (w[1] - w[0]).normalize_or_zero();
        let a = if k == 0 { w[0] } else { w[0] - d * r * 0.6 };
        let b = if k + 2 == pts.len() { w[1] } else { w[1] + d * r * 0.6 };
        if a.distance(b) > 1e-3 {
            tube(m, a, b, r, sides, mat);
        }
    }
}

/// A part round the points `pts` (ship frame) whose look is `look` (ship frame too).
fn part(id: String, material: &str, pts: &[Vec3], mut look: Mesh, color: [u8; 3], flags: u8) -> Option<GenPart> {
    let mut p = GenPart::from_points(id, material, Role::Conduit, pts)?;
    let c = Vec3::from(p.at.translation);
    for q in &mut look.pos {
        *q = (Vec3::from_array(*q) - c).to_array();
    }
    p.look = Some(look);
    p.color = Some(color);
    p.flags = flags;
    Some(p)
}

/// The eight corners of a box along `a`→`b`: `half` wide across, from `back` behind the line
/// (toward `w`) to `front` before it.
fn slab(a: Vec3, b: Vec3, w: Vec3, half: f32, back: f32, front: f32) -> Vec<Vec3> {
    let d = (b - a).normalize_or(Vec3::Z);
    let w = (w - d * w.dot(d)).normalize_or(d.any_orthonormal_vector());
    let u = d.cross(w);
    let mut out = Vec::with_capacity(8);
    for p in [a, b] {
        for s in [-half, half] {
            out.push(p + u * s + w * back);
            out.push(p + u * s - w * front);
        }
    }
    out
}

/// The way a drop takes from the outlet at `o` on trunk `t` to its device at `d`: along the wall,
/// then off it. None when the device is too far from the wall.
fn drop_path(site: &Site, t: &Trunk, o: Vec3, d: Vec3) -> Option<Vec<Vec3>> {
    let [lo, hi] = site.rooms[t.room].1;
    let mut pts = vec![o];
    match t.wall {
        Wall::Side(_) => {
            // down the wall at the device's own station; where a panel is in the way there, down
            // beside the panel and along the wall to the device
            let zd = d.z.clamp(lo.z + 0.05, hi.z - 0.05);
            let way = site.stations(zd, lo.z + 0.05, hi.z - 0.05).into_iter().find_map(|z| side_way(site, t.room, o, d, z, zd))?;
            pts.extend(way);
        }
        Wall::End(sign, door) => {
            // down (or up) the end wall right over the device, unless that is the doorway
            let zf = o.z + sign * (TRUNK_R + TRUNK_OFF - CABLE_OFF);
            let in_door = door.is_some_and(|[x0, x1, top]| d.x > x0 - 0.05 && d.x < x1 + 0.05 && d.y < top + 0.05);
            if !in_door {
                let (a, b) = (Vec3::new(o.x, o.y, zf), Vec3::new(d.x, d.y.clamp(lo.y + 0.02, hi.y - 0.02), zf));
                if site.crosses_panel(a, b) {
                    return None;
                }
                pts.push(a);
                pts.push(b);
            }
        }
    }
    dedup(&mut pts);
    if pts.last()?.distance(d) > MAX_FREE || site.crosses_panel(*pts.last()?, d) {
        return None;
    }
    pts.push(d);
    dedup(&mut pts);
    Some(pts)
}

/// A drop's way round a room's side wall at station `z`, from the outlet at `o` toward the device
/// at `d` (whose own station is `zd`): the short way round the outline, never across glass nor a
/// panel; from another station than the device's, then along the wall to it.
fn side_way(site: &Site, room: usize, o: Vec3, d: Vec3, z: f32, zd: f32) -> Option<Vec<Vec3>> {
    let ring: Vec<Vec2> = site.outline(room, z, CABLE_OFF);
    let n = ring.len();
    // (edge, share along it, the point) nearest a point of the cut
    let nearest = |ring: &[Vec2], q: Vec2| -> (usize, f32, Vec2) {
        let n = ring.len();
        let mut best = (0, 0.0, ring[0], f32::MAX);
        for i in 0..n {
            let (a, b) = (ring[i], ring[(i + 1) % n]);
            let e = b - a;
            let s = if e.length_squared() > 1e-9 { ((q - a).dot(e) / e.length_squared()).clamp(0.0, 1.0) } else { 0.0 };
            let p = a + e * s;
            if p.distance(q) < best.3 {
                best = (i, s, p, p.distance(q));
            }
        }
        (best.0, best.1, best.2)
    };
    let (ea, sa, pa) = nearest(&ring, Vec2::new(o.x, o.y));
    let (eb, sb, pb) = nearest(&ring, Vec2::new(d.x, d.y));
    // forward (with the outline) and backward, the shorter
    let walk = |forward: bool| -> Vec<Vec2> {
        let mut v = vec![pa];
        let direct = ea == eb && if forward { sb >= sa } else { sb <= sa };
        if !direct {
            let mut i = if forward { (ea + 1) % n } else { ea };
            let stop = if forward { eb } else { (eb + 1) % n };
            loop {
                v.push(ring[i]);
                if i == stop {
                    break;
                }
                i = if forward { (i + 1) % n } else { (i + n - 1) % n };
            }
        }
        v.push(pb);
        v
    };
    let len = |v: &[Vec2]| v.windows(2).map(|w| w[0].distance(w[1])).sum::<f32>();
    // never across glass or a panel: the other way round if the short one is, else no drop here
    let blocked = |v: &[Vec2]| {
        v.windows(2).any(|w| {
            let steps = (w[0].distance(w[1]) / 0.04).ceil().max(1.0) as usize;
            (0..=steps).any(|k| {
                let q = w[0].lerp(w[1], k as f32 / steps as f32);
                let p = Vec3::new(q.x, q.y, z);
                site.meets(p).1 || site.on_panel(p)
            })
        })
    };
    let (f, b) = (walk(true), walk(false));
    let (short, long) = if len(&f) <= len(&b) { (f, b) } else { (b, f) };
    let way = if !blocked(&short) {
        short
    } else if !blocked(&long) && len(&long) < 3.0 * len(&short) + 1.0 {
        long
    } else {
        return None;
    };
    let mut out: Vec<Vec3> = way.into_iter().map(|p| Vec3::new(p.x, p.y, z)).collect();
    if (z - zd).abs() > 1e-4 {
        // along the wall, at the device's height, to its own station
        let at = nearest(&site.outline(room, zd, CABLE_OFF), Vec2::new(d.x, d.y)).2;
        let end = Vec3::new(at.x, at.y, zd);
        let last = *out.last()?;
        let steps = (last.distance(end) / 0.04).ceil().max(1.0) as usize;
        if (0..=steps).any(|k| {
            let p = last.lerp(end, k as f32 / steps as f32);
            site.meets(p).1 || site.on_panel(p)
        }) {
            return None;
        }
        out.push(end);
    }
    Some(out)
}

/// The way of a fixed pipe from `a` to `b`, both in rooms: down to under the deck, along it, up.
fn pipe_path(site: &Site, a: Vec3, b: Vec3) -> Option<Vec<Vec3>> {
    let down = |p: Vec3| -> Option<Vec<Vec3>> {
        let r = site.room_of(p)?;
        let [lo, hi] = site.rooms[r].1;
        let under = lo.y - UNDER;
        if p.y - lo.y <= RISE {
            return Some(vec![p, Vec3::new(p.x, under, p.z)]);
        }
        // high up: to the nearest side wall and down it, at its own station or, where a panel
        // is in the way there, beside the panel
        let s = if p.x >= (lo.x + hi.x) * 0.5 { 1.0 } else { -1.0 };
        let run = |z: f32| -> Option<Vec<Vec3>> {
            let x = |y: f32| site.wall_x(r, y, z, s, CABLE_OFF + 0.02);
            let mut v = vec![p, Vec3::new(x(p.y), p.y, z)];
            // by the corners of the wall on the way down
            let ring = site.outline(r, z, CABLE_OFF + 0.02);
            let mut ys: Vec<f32> = ring.iter().filter(|q| q.x * s > 0.0 && q.y < p.y - 0.05 && q.y > lo.y + 0.05).map(|q| q.y).collect();
            ys.sort_by(|a, b| b.total_cmp(a));
            v.extend(ys.into_iter().map(|y| Vec3::new(x(y), y, z)));
            let foot = x(lo.y + 0.05);
            v.push(Vec3::new(foot, lo.y + 0.05, z));
            v.push(Vec3::new(foot, under, z));
            (!v.windows(2).any(|w| site.crosses_panel(w[0], w[1]))).then_some(v)
        };
        site.stations(p.z.clamp(lo.z + 0.05, hi.z - 0.05), lo.z + 0.05, hi.z - 0.05).into_iter().find_map(run)
    };
    let (mut pa, pb) = (down(a)?, down(b)?);
    pa.extend(pb.into_iter().rev());
    dedup(&mut pa);
    (pa.len() >= 2).then_some(pa)
}

/// Every run laid: the trunks of the rooms, an outlet and a drop for every device, the pipes and
/// the ducts.
pub fn lay(site: &Site, runs: &[Run]) -> Laid {
    let mut parts: Vec<GenPart> = Vec::new();
    let mut chains: Vec<Chain> = (0..runs.len()).map(|run| Chain { run, ..Chain::default() }).collect();
    let trunks = trunks(site);
    let off = TRUNK_R + TRUNK_OFF;
    if std::env::var_os("LUNAR_PERFIL").is_some() {
        for t in &trunks {
            eprintln!("  {}: {:.1} m, de ({:.2}, {:.2}, {:.2}) a ({:.2}, {:.2}, {:.2})", t.name, t.length(), t.pts[0].x, t.pts[0].y, t.pts[0].z, t.pts[t.pts.len() - 1].x, t.pts[t.pts.len() - 1].y, t.pts[t.pts.len() - 1].z);
        }
    }

    // the pieces of each trunk: between its corners, no longer than a piece (where it is cut)
    let cuts: Vec<Vec<f32>> = trunks
        .iter()
        .map(|t| {
            let mut cuts = vec![0.0];
            let mut arc = 0.0;
            for w in t.pts.windows(2) {
                let l = w[0].distance(w[1]);
                let n = (l / PIECE).ceil().max(1.0) as usize;
                for i in 1..=n {
                    cuts.push(arc + l * i as f32 / n as f32);
                }
                arc += l;
            }
            cuts
        })
        .collect();
    // ---- where every cable run's ends (and the points it must pass by) meet the trunks ----
    // a junction's place: where two pieces of a trunk meet, the nearest (so either piece gone
    // leaves it on the other)
    let joint = |p: Vec3, room: Option<usize>| -> Option<(usize, f32, f32)> {
        let mut best: Option<(usize, f32, f32)> = None;
        for (k, t) in trunks.iter().enumerate().filter(|(_, t)| room.is_none_or(|r| t.room == r)) {
            let n = cuts[k].len();
            // between two pieces where it has more than one; else its ends (where it meets others)
            let inner = if n > 2 { 1..n - 1 } else { 0..n };
            for &arc in &cuts[k][inner] {
                let d = t.at(arc).0.distance(p);
                if best.is_none_or(|b| d < b.2) {
                    best = Some((k, arc, d));
                }
            }
        }
        best
    };
    // a device's place on a trunk: one of its outlet positions
    let place = |p: Vec3, room: Option<usize>| -> Option<(usize, f32, f32)> {
        let mut best: Option<(usize, f32, f32)> = None;
        for (k, t) in trunks.iter().enumerate().filter(|(_, t)| room.is_none_or(|r| t.room == r)) {
            let (arc, dist) = t.nearest(p);
            if best.is_none_or(|b| dist < b.2) {
                best = Some((k, arc, dist));
            }
        }
        let (k, arc, dist) = best?;
        let l = trunks[k].length();
        let edge = (EVERY * 0.5).min(l * 0.5);
        let slots = ((l - 2.0 * edge) / EVERY).floor().max(0.0) as i32;
        let want = (((arc - edge) / EVERY).round() as i32).clamp(0, slots);
        // the nearest position that is not in a device
        let free = |i: i32| !site.meets(trunks[k].at(edge + i as f32 * EVERY).0 - trunks[k].to_wall() * (TRUNK_R + 0.02)).0;
        let slot = (0..=slots).filter(|i| free(*i)).min_by_key(|i| (i - want).abs()).unwrap_or(want);
        Some((k, edge + slot as f32 * EVERY, dist))
    };
    // (trunk, mm along it) of every place asked for
    let mut places: Vec<(usize, i32)> = Vec::new();
    let mut ends: Vec<Option<[(usize, i32, bool); 2]>> = vec![None; runs.len()];
    let mut vias: Vec<Vec<(usize, i32)>> = vec![Vec::new(); runs.len()];
    let key = |arc: f32| (arc * 1000.0).round() as i32;
    for (ri, r) in runs.iter().enumerate().filter(|(_, r)| class_of(&r.kind) == 0) {
        let end = |p: Vec3, device: bool| -> Option<(usize, i32, bool)> {
            let room = site.room_of(p);
            let (k, arc, _) = if device { place(p, room).or_else(|| place(p, None))? } else { joint(p, room).or_else(|| joint(p, None))? };
            Some((k, key(arc), room.is_some_and(|r| trunks[k].room == r)))
        };
        // a device's own run starts at the device; its other end, and both of any other, are junctions
        let (Some(a), Some(b)) = (end(r.a, r.stub), end(r.b, false)) else {
            continue;
        };
        places.push((a.0, a.1));
        places.push((b.0, b.1));
        ends[ri] = Some([a, b]);
        for v in &r.via {
            if let Some((k, arc, _)) = joint(*v, None) {
                places.push((k, key(arc)));
                vias[ri].push((k, key(arc)));
            }
        }
    }
    places.sort_unstable();
    places.dedup();

    // ---- the trunks as parts and as a graph ----
    let mut g = Graph::default();
    let mut corners: Vec<(usize, u32, Vec3)> = Vec::new();
    for (k, t) in trunks.iter().enumerate() {
        let cuts = &cuts[k];
        let mut made: Vec<Option<usize>> = Vec::new();
        for (i, w) in cuts.windows(2).enumerate() {
            let ((a, _), (b, _)) = (t.at(w[0] + 1e-4), t.at(w[1] - 1e-4));
            let mut m = Mesh::default();
            tube(&mut m, a, b, TRUNK_R, 8, steel());
            // a collar where it starts
            let d = (b - a).normalize_or(Vec3::Z);
            tube(&mut m, a, a + d * 0.04, TRUNK_R + 0.007, 8, Material::new([60, 62, 66], 140, 160));
            made.push(part(format!("{}.{i}", t.name), "acero", &slab(a, b, t.to_wall(), TRUNK_R, off, TRUNK_R), m, [92, 95, 100], NO_COLLIDE).map(|p| {
                parts.push(p);
                parts.len() - 1
            }));
        }
        let piece = |arc: f32| -> Option<usize> { made[cuts.windows(2).position(|w| arc >= w[0] - 1e-4 && arc <= w[1] + 1e-4)?] };
        // its nodes: the cuts and the places asked for, in order
        let mut arcs: Vec<i32> = cuts.iter().map(|c| key(*c)).chain(places.iter().filter(|p| p.0 == k).map(|p| p.1)).collect();
        arcs.sort_unstable();
        arcs.dedup();
        let mut last: Option<(u32, i32)> = None;
        for a in arcs {
            let p = t.at(a as f32 / 1000.0).0;
            let n = g.node(p);
            g.at.insert((k, a), n);
            if let Some((ln, la)) = last {
                g.join(ln, n, piece((a + la) as f32 / 2000.0));
            }
            last = Some((n, a));
        }
        // its corners (where trunks meet)
        let mut arc = 0.0;
        for (i, p) in t.pts.iter().enumerate() {
            if i > 0 {
                arc += t.pts[i - 1].distance(*p);
            }
            if let Some(&n) = g.at.get(&(k, key(arc))) {
                corners.push((k, n, *p));
            }
        }
    }
    // trunks joined where their corners meet: at once in a room, by a short piece through a wall
    let mut passes = 0;
    let mut linked: Vec<(Vec3, Vec3)> = Vec::new();
    for ta in 0..trunks.len() {
        for tb in ta + 1..trunks.len() {
            let same = trunks[ta].room == trunks[tb].room;
            let reach = if same { 0.03 } else { LINK };
            let mut best: Option<(u32, u32, Vec3, Vec3, f32)> = None;
            for &(_, na, pa) in corners.iter().filter(|c| c.0 == ta) {
                for &(_, nb, pb) in corners.iter().filter(|c| c.0 == tb) {
                    let d = pa.distance(pb);
                    if d <= reach && best.is_none_or(|b| d < b.4) {
                        best = Some((na, nb, pa, pb, d));
                    }
                }
            }
            let Some((na, nb, pa, pb, d)) = best else {
                continue;
            };
            let mut owner = None;
            if d > 0.03 && !linked.iter().any(|(a, b)| (a.distance(pa) < 0.05 && b.distance(pb) < 0.05) || (a.distance(pb) < 0.05 && b.distance(pa) < 0.05)) {
                let mut m = Mesh::default();
                tube(&mut m, pa, pb, TRUNK_R, 8, steel());
                if let Some(p) = part(format!("canal.paso.{passes}"), "acero", &slab(pa, pb, Vec3::Y, TRUNK_R, TRUNK_R, TRUNK_R), m, [92, 95, 100], NO_COLLIDE) {
                    owner = Some(parts.len());
                    parts.push(p);
                    passes += 1;
                    linked.push((pa, pb));
                }
            }
            g.join(na, nb, owner);
        }
    }

    // ---- cable runs: along the trunks; a device's own gets an outlet and a drop ----
    let mut seen: Vec<Vec<Vec3>> = Vec::new();
    // per outlet (trunk, mm): its drops (the way, radius, colour)
    let mut outlets: BTreeMap<(usize, i32), Vec<(Vec<Vec3>, f32, [u8; 3])>> = BTreeMap::new();
    // runs waiting for their outlet's part: (run, the stretch of its chain)
    let mut waiting: Vec<(usize, usize, (usize, i32))> = Vec::new();
    for (ri, r) in runs.iter().enumerate() {
        let Some([a, b]) = ends[ri] else { continue };
        let (Some(&na), Some(&nb)) = (g.at.get(&(a.0, a.1)), g.at.get(&(b.0, b.1))) else {
            continue;
        };
        let look = geom::conduit_look(&r.kind, Some(r.color));
        let (mut pts, mut owners): (Vec<Vec3>, Vec<Option<usize>>) = (vec![r.a], Vec::new());
        // the device's end: its outlet and its drop, where it can have one
        let outlet = g.nodes[na as usize];
        let drop = if r.stub && a.2 { drop_path(site, &trunks[a.0], outlet, r.a) } else { None };
        pts.push(outlet);
        owners.push(None);
        if let Some(way) = drop {
            waiting.push((ri, 0, (a.0, a.1)));
            let list = outlets.entry((a.0, a.1)).or_default();
            if !list.iter().any(|(w, _, c)| w.last().is_some_and(|e| e.distance(r.a) < 0.02) && *c == r.color) {
                seen.push(way.clone());
                list.push((way, look.radius.clamp(0.006, 0.014), r.color));
            }
        }
        // along the trunks, by the points it must pass: the short way, and the other way round
        // (other steps, other pieces where there are any)
        let stops: Vec<u32> = vias[ri].iter().filter_map(|v| g.at.get(v).copied()).chain(std::iter::once(nb)).collect();
        let walk = |taken: &[(u32, u32)], parts: &[usize]| -> Option<Vec<(u32, Option<usize>)>> {
            let (mut at, mut steps) = (na, Vec::new());
            for &stop in &stops {
                steps.extend(g.path(at, stop, taken, parts)?);
                at = stop;
            }
            Some(steps)
        };
        let first = walk(&[], &[]);
        let second = first.as_ref().filter(|f| !f.is_empty()).and_then(|f| {
            let mut at = na;
            let taken: Vec<(u32, u32)> = f
                .iter()
                .map(|&(n, _)| {
                    let e = (at.min(n), at.max(n));
                    at = n;
                    e
                })
                .collect();
            let parts: Vec<usize> = f.iter().filter_map(|s| s.1).collect();
            walk(&taken, &parts)
        });
        let whole = |steps: &[(u32, Option<usize>)]| -> (Vec<Vec3>, Vec<Option<usize>>) {
            let (mut p, mut o) = (pts.clone(), owners.clone());
            let own = o.len();
            for &(n, part) in steps {
                // steps along one piece are one stretch (fewer edges for the network to solve)
                if o.len() > own
                    && o.last() == Some(&part)
                    && let Some(last) = p.last_mut()
                {
                    *last = g.nodes[n as usize];
                } else {
                    p.push(g.nodes[n as usize]);
                    o.push(part);
                }
            }
            p.push(r.b);
            o.push(None);
            (p, o)
        };
        match first {
            Some(f) => {
                let (p, o) = whole(&f);
                chains[ri] = Chain { run: ri, pts: p, owners: o };
                if let Some(sec) = second {
                    let (p, o) = whole(&sec);
                    // its first stretch is the device's own (its outlet), like the first way's
                    if chains[ri].owners.first().is_some() && waiting.last().is_some_and(|w| w.0 == ri) {
                        waiting.push((chains.len(), 0, (a.0, a.1)));
                    }
                    chains.push(Chain { run: ri, pts: p, owners: o });
                }
            }
            // trunks that do not meet (a room apart from the rest): connected all the same
            None => chains[ri] = Chain { run: ri, pts: vec![r.a, r.b], owners: vec![None] },
        }
    }
    // the outlets: a box on the trunk, its drops side by side in its look
    let box_mat = Material::new([34, 36, 40], 150, 40).with_finish(geom::fin("plastico"));
    let mut outlet_part: BTreeMap<(usize, i32), usize> = BTreeMap::new();
    for (n, ((k, arc), drops)) in outlets.iter().enumerate() {
        let t = &trunks[*k];
        let (o, d) = t.at(*arc as f32 / 1000.0);
        let w = t.to_wall();
        let corners = slab(o - d * 0.055, o + d * 0.055, w, 0.06, TRUNK_R * 0.5, TRUNK_R + 0.03);
        let mut m = Mesh::default();
        if let Some(shape) = lunar_core::structure::convex::Convex::hull(&corners) {
            shape.mesh_into(&mut m, glam::Affine3A::IDENTITY, box_mat);
        }
        let lanes = drops.len() as f32;
        for (i, (way, radius, color)) in drops.iter().enumerate() {
            let shift = d * ((i as f32 - (lanes - 1.0) * 0.5) * LANE);
            let n = way.len();
            let pts: Vec<Vec3> = way.iter().enumerate().map(|(j, p)| if j + 1 == n { *p } else { *p + shift }).collect();
            sweep(&mut m, &pts, *radius, 5, Material::new(*color, 170, 0).with_finish(geom::fin("goma")));
        }
        if let Some(p) = part(format!("toma.{n}"), "electronica", &corners, m, [34, 36, 40], NO_COLLIDE | SPARKS) {
            outlet_part.insert((*k, *arc), parts.len());
            parts.push(p);
        }
    }
    for (ci, stretch, at) in waiting {
        if let Some(&p) = outlet_part.get(&at)
            && let Some(o) = chains[ci].owners.get_mut(stretch)
        {
            *o = Some(p);
        }
    }

    // ---- pipes: fixed and only seen; ducts: straight pieces ----
    let mut pipes: BTreeMap<u16, (Mesh, Vec3, [u8; 3])> = BTreeMap::new();
    for (ri, r) in runs.iter().enumerate() {
        match class_of(&r.kind) {
            1 => {
                chains[ri] = Chain { run: ri, pts: vec![r.a, r.b], owners: vec![None] };
                let Some(way) = pipe_path(site, r.a, r.b) else {
                    continue;
                };
                seen.push(way.clone());
                let look = geom::conduit_look(&r.kind, Some(r.color));
                let e = pipes.entry(r.net).or_insert_with(|| (Mesh::default(), way[0], r.color));
                sweep(&mut e.0, &way, look.radius, 6, Material::new(look.color, 90, look.metal).with_finish(look.finish));
            }
            2 if r.stub => chains[ri] = Chain { run: ri, pts: vec![r.a, r.b], owners: vec![None] },
            2 => {
                let look = geom::conduit_look(&r.kind, Some(r.color));
                let [w, h] = look.duct.unwrap_or([0.22, 0.14]);
                // level first, then up or down to its end
                let mut way = vec![r.a];
                for v in &r.via {
                    way.push(*v);
                }
                if (r.a.y - r.b.y).abs() > 0.25 && Vec2::new(r.a.x - r.b.x, r.a.z - r.b.z).length() > 0.25 {
                    way.push(Vec3::new(r.b.x, r.a.y, r.b.z));
                }
                way.push(r.b);
                dedup(&mut way);
                let (mut pts, mut owners) = (vec![r.a], Vec::new());
                let seen = site.room_of(r.a).is_some() && site.room_of(r.b).is_some();
                let mat = Material::new(look.color, 120, look.metal).with_finish(look.finish);
                let mut k = 0;
                for seg in way.windows(2) {
                    let l = seg[0].distance(seg[1]);
                    let n = if seen { (l / PIECE).ceil().max(1.0) as usize } else { 1 };
                    for i in 0..n {
                        let (a, b) = (seg[0].lerp(seg[1], i as f32 / n as f32), seg[0].lerp(seg[1], (i + 1) as f32 / n as f32));
                        let mut owner = None;
                        if seen && a.distance(b) > 0.05 {
                            let d = (b - a).normalize_or(Vec3::Z);
                            let up = if d.y.abs() > 0.9 { Vec3::X } else { Vec3::Y };
                            let corners = slab(a - d * (h * 0.25), b + d * (h * 0.25), up, w * 0.5, h * 0.5, h * 0.5);
                            let mut m = Mesh::default();
                            if let Some(shape) = lunar_core::structure::convex::Convex::hull(&corners) {
                                shape.mesh_into(&mut m, glam::Affine3A::IDENTITY, mat);
                            }
                            if let Some(p) = part(format!("{}.{k}", r.prefix), look.material, &corners, m, look.color, NO_COLLIDE) {
                                owner = Some(parts.len());
                                parts.push(p);
                                k += 1;
                            }
                        }
                        pts.push(b);
                        owners.push(owner);
                    }
                }
                if owners.is_empty() {
                    pts.push(r.b);
                    owners.push(None);
                }
                chains[ri] = Chain { run: ri, pts, owners };
            }
            _ => {
                if chains[ri].pts.is_empty() {
                    chains[ri] = Chain { run: ri, pts: vec![r.a, r.b], owners: vec![None] };
                }
            }
        }
    }
    for (net, (m, at, color)) in pipes {
        let c = Vec3::splat(0.02);
        let pts: Vec<Vec3> = (0..8).map(|i| at + Vec3::new(if i & 1 == 0 { -c.x } else { c.x }, if i & 2 == 0 { -c.y } else { c.y }, if i & 4 == 0 { -c.z } else { c.z })).collect();
        if let Some(p) = part(format!("tuberia.{net}"), "acero", &pts, m, color, GHOST | NO_COLLIDE) {
            parts.push(p);
        }
    }
    Laid { parts, chains, seen }
}
