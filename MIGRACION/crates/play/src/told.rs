//! What the games tell each other besides the states the network carries by itself (each
//! player, each thing that moves): a kind byte and its fields. One place for every kind, how it
//! travels and what it carries:
//!
//! | Kind | How | What |
//! |---|---|---|
//! | `CONTROL` | sure, to the others | a control of a ship left at a value by a hand |
//! | `ACT` | sure, to the others | a door or a clamp of a ship worked by a hand (`controls::Act`) |
//! | `SPAWN` | sure, echoed | a ship made in play (its number: its place among those made) |
//! | `STRIKES` | sure, echoed | what was done to shared structures (hits, plates torn out), done by every game in the order the server passed it on |
//! | `SEEN` | sure, to the others (where guided missiles are: loose) | what was let fly or set off and where it ended (`blasts::Seen`), to be seen: it does nothing; its damage goes as `STRIKES` |
//!
//! Places that must be right however fast things go travel in the frame of a structure every
//! game has (`Named`): a hit in its own frame; a launch in the frame of what it left (the ship
//! whose gun fired it, the ship its shooter rides or floats by), so that it leaves the muzzle in
//! every game whatever each one's clock says; where it ended, on what it struck.
use crate::{
    blasts::{Launch, Seen, What},
    builds::Strike,
    controls::Act,
};
use glam::{Affine3A, DVec3, Quat, Vec3};
use lunar_core::structure::damage::Hit;
use lunar_net::{Reader, WireError, Writer};

pub const CONTROL: u8 = 1;
pub const SPAWN: u8 = 2;
pub const STRIKES: u8 = 3;
pub const SEEN: u8 = 4;
pub const ACT: u8 = 5;

/// A structure as every game names it: a ship by its number on the network, anything the
/// scenario set round the site by its own id (the same in every game that starts with it), a
/// piece come off either (or off a piece) by its lineage (`Structure::lineage`: the same in every
/// game that broke it off the same).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Named {
    Ship(u64),
    Built(u64),
    Piece(u64),
}

impl Named {
    pub fn code(self) -> u64 {
        match self {
            Named::Built(id) => id << 2,
            Named::Ship(k) => (k << 2) | 1,
            Named::Piece(l) => (l << 2) | 2,
        }
    }

    pub fn of(code: u64) -> Named {
        match code & 3 {
            1 => Named::Ship(code >> 2),
            2 => Named::Piece(code >> 2),
            _ => Named::Built(code >> 2),
        }
    }
}

/// The dice of the `n`-th strike the server did (`Game::struck`): what every game does it with.
pub fn dice(n: u64) -> u64 {
    n.wrapping_mul(0x9e37_79b9_7f4a_7c15)
}

/// How a struck structure's articulations were posed, told against what was told before to the
/// same game: its bones (past the root, `Structure::bones`), and which of them go (`mask`, bone
/// `k` at bit `k`; all of them when there are more than 64 or none was told before).
#[derive(Clone, Copy, Debug)]
pub struct Posed<'a> {
    pub named: Named,
    pub bones: &'a [Affine3A],
    pub mask: u64,
    pub all: bool,
}

/// The first byte of each strike told: a plate torn out (else a hit); its dice one past the
/// one before; on the structure the one before was on; a round (no reach: its way in 6 bytes,
/// `as_told`); the energy, and the cross-section, of the hit before.
const S_BLOW: u8 = 1;
const S_NEXT: u8 = 1 << 1;
const S_SAME_ON: u8 = 1 << 2;
const S_ROUND: u8 = 1 << 3;
const S_SAME_ENERGY: u8 = 1 << 4;
const S_SAME_AREA: u8 = 1 << 5;
const S_ALL: u8 = (1 << 6) - 1;

/// The most bones a pose is told with.
pub const MOST_BONES: u64 = 1024;
/// The most strikes in one message.
pub const MOST_STRIKES: u64 = 4096;

/// A bone as it is, to the bit (what a strike does hangs on where each part is: decomposed and
/// put together again, it would not be the same in the last bit).
fn write_bone(w: &mut Writer, b: &Affine3A) {
    b.to_cols_array().iter().for_each(|v| w.f32(*v));
}

fn read_bone(r: &mut Reader) -> Result<Affine3A, WireError> {
    let mut c = [0.0f32; 12];
    for v in &mut c {
        *v = r.f32()?;
        if !v.is_finite() {
            return Err(WireError::Value);
        }
    }
    Ok(Affine3A::from_cols_array(&c))
}

/// Bytes `write_strikes` takes at most for `strikes` strikes and poses of `bones` bones in all.
pub fn strikes_room(strikes: usize, poses: usize, bones: usize) -> usize {
    16 + strikes * 64 + poses * 32 + bones * 48
}

/// Strikes into `w`: each with the number of its dice (`dice`; one after another, told as what
/// each adds to the one before), against the structure named (the strike's own id is this game's
/// and does not go); then how the articulated ones among those were posed, what changed of each
/// since this game was told (`Posed`): every game does them so.
pub fn write_strikes(w: &mut Writer, strikes: &[(u64, Named, Strike)], poses: &[Posed]) {
    w.u8(STRIKES);
    w.var(strikes.len() as u64);
    let (mut last, mut on, mut energy, mut area) = (0u64, u64::MAX, None, None);
    for &(n, named, s) in strikes {
        let code = named.code();
        let mut head = match s {
            Strike::Hit { hit, .. } => {
                let mut h = 0;
                if hit.radius == 0.0 {
                    h |= S_ROUND;
                }
                if energy == Some(hit.energy.to_bits()) {
                    h |= S_SAME_ENERGY;
                }
                if area == Some(hit.area.to_bits()) {
                    h |= S_SAME_AREA;
                }
                h
            }
            Strike::Blow { .. } => S_BLOW,
        };
        if n == last.wrapping_add(1) {
            head |= S_NEXT;
        }
        if code == on {
            head |= S_SAME_ON;
        }
        w.u8(head);
        if head & S_NEXT == 0 {
            w.var(n.wrapping_sub(last));
        }
        if head & S_SAME_ON == 0 {
            w.var(code);
        }
        (last, on) = (n, code);
        match s {
            Strike::Hit { hit, .. } => {
                w.vec3(hit.point);
                if head & S_ROUND != 0 {
                    write_unit(w, hit.dir);
                } else {
                    w.vec3(hit.dir);
                    w.f32(hit.radius);
                }
                if head & S_SAME_ENERGY == 0 {
                    w.f32(hit.energy);
                    energy = Some(hit.energy.to_bits());
                }
                if head & S_SAME_AREA == 0 {
                    w.f32(hit.area);
                    area = Some(hit.area.to_bits());
                }
            }
            Strike::Blow { part, push, energy, .. } => {
                w.var(u64::from(part));
                w.vec3(push);
                w.f32(energy);
            }
        }
    }
    w.var(poses.len() as u64);
    for p in poses {
        w.var(p.named.code());
        w.var(p.bones.len() as u64);
        if p.all || p.bones.len() > 64 {
            w.u8(1);
            p.bones.iter().for_each(|b| write_bone(w, b));
        } else {
            w.u8(0);
            w.var(p.mask);
            for (k, b) in p.bones.iter().enumerate() {
                if p.mask & (1 << k) != 0 {
                    write_bone(w, b);
                }
            }
        }
    }
}

/// What `write_strikes` wrote, after its kind byte: each strike with the number of its dice and
/// the structure it names (its id left 0, for whoever finds it); and each pose told, as the
/// structure named, how many bones it has, which go (all: every bit) and those (in order).
#[allow(clippy::type_complexity)]
pub fn read_strikes(r: &mut Reader, out: &mut Vec<(u64, Named, Strike)>, poses: &mut Vec<(Named, usize, u64, Vec<Affine3A>)>) -> Result<(), WireError> {
    let n = r.var()?;
    if n > MOST_STRIKES {
        return Err(WireError::Long);
    }
    let (mut last, mut on, mut energy, mut area) = (0u64, None, None, None);
    for _ in 0..n {
        let head = r.u8()?;
        if head & !S_ALL != 0 {
            return Err(WireError::Value);
        }
        last = last.wrapping_add(if head & S_NEXT != 0 { 1 } else { r.var()? });
        if head & S_SAME_ON == 0 {
            on = Some(Named::of(r.var()?));
        }
        let named = on.ok_or(WireError::Value)?;
        let s = if head & S_BLOW != 0 {
            Strike::Blow { id: 0, part: r.var32()?, push: r.vec3()?, energy: r.f32()? }
        } else {
            let point = r.vec3()?;
            let (dir, radius) = if head & S_ROUND != 0 { (read_unit(r)?, 0.0) } else { (r.vec3()?, r.f32()?) };
            if head & S_SAME_ENERGY == 0 {
                energy = Some(r.f32()?);
            }
            if head & S_SAME_AREA == 0 {
                area = Some(r.f32()?);
            }
            Strike::Hit { id: 0, hit: Hit { point, dir, energy: energy.ok_or(WireError::Value)?, radius, area: area.ok_or(WireError::Value)? } }
        };
        out.push((last, named, s));
    }
    let n = r.var()?;
    if n > 256 {
        return Err(WireError::Long);
    }
    for _ in 0..n {
        let named = Named::of(r.var()?);
        let k = r.var()?;
        if k > MOST_BONES {
            return Err(WireError::Long);
        }
        let mask = match r.u8()? {
            1 => u64::MAX,
            0 if k <= 64 => r.var()?,
            _ => return Err(WireError::Value),
        };
        let mut bones = Vec::new();
        for b in 0..k {
            if mask == u64::MAX || mask & (1 << b) != 0 {
                bones.push(read_bone(r)?);
            }
        }
        poses.push((named, k as usize, mask, bones));
    }
    Ok(())
}

/// A structure's frame as a game has it at a moment: where it is, how it is turned, how it
/// moves and turns. What is told in it is told as offsets in it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    pub pos: DVec3,
    pub rot: Quat,
    pub vel: DVec3,
    pub spin: Vec3,
}

impl Frame {
    fn local(&self, p: DVec3) -> Vec3 {
        self.rot.inverse() * (p - self.pos).as_vec3()
    }

    fn world(&self, l: Vec3) -> DVec3 {
        self.pos + (self.rot * l).as_dvec3()
    }

    /// How fast the frame itself goes at `p`.
    fn moving(&self, p: DVec3) -> DVec3 {
        self.vel + self.spin.as_dvec3().cross(p - self.pos)
    }

    /// The frame `age` s before (turning that little is left out).
    fn before(self, age: f64) -> Frame {
        Frame { pos: self.pos - self.vel * age, ..self }
    }
}

/// Where each thing seen is told: beside a structure every game has (`named`, and how the
/// sender had it then), or in the world.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum From {
    World,
    Beside { named: Named, frame: Frame },
}

fn what_code(w: What) -> (u8, u16) {
    match w {
        What::Shot(i) => (0, i),
        What::Missile(i) => (1, i),
        What::Guided(i) => (2, i),
        What::Decoy(i) => (3, i),
        What::Boom(i) => (4, i),
    }
}

fn what_of(c: u8, i: u16) -> Result<What, WireError> {
    Ok(match c {
        0 => What::Shot(i),
        1 => What::Missile(i),
        2 => What::Guided(i),
        3 => What::Decoy(i),
        4 => What::Boom(i),
        _ => return Err(WireError::Value),
    })
}

/// The first byte of each thing seen: its kind (0 launch, 1 end, 2 where a guided one is) in its
/// two low bits, and these, each saying what is left out as the same as in the thing before it
/// in the message, or as nothing.
const SAME_FRAME: u8 = 1 << 2;
/// Its number, one past the one before (else the difference).
const NEXT_TAG: u8 = 1 << 3;
const SAME_WHAT: u8 = 1 << 4;
/// Going with its frame there (slower than `STILL_UNDER` beside it).
const STILL: u8 = 1 << 5;
/// What let it fly (a launch) or what it struck (an end) is the structure it is told beside.
const BY_FRAME: u8 = 1 << 6;
/// A launch with no target; an end with nothing more in its blast.
const PLAIN: u8 = 1 << 7;
/// Slower than this (m/s) beside its frame, a thing seen goes with it (it is only seen).
const STILL_UNDER: f32 = 0.01;
/// The most one thing seen takes (bytes: everything told, in the world's own places).
const SEEN_ONE_MOST: usize = 112;
/// Things seen in one message at most (told whole, the smallest take 19 bytes).
pub const SEEN_MOST: usize = 128;

/// What each thing seen in a message is told against: the one before it.
#[derive(Clone, Copy)]
struct Prev {
    code: u64,
    tag: u32,
    what: Option<What>,
}

impl Prev {
    const NONE: Prev = Prev { code: u64::MAX, tag: 0, what: None };
}

/// The steps of each side of a unit way told in 24 bits.
const UNIT_SIDE: f64 = ((1u32 << 24) - 1) as f64;

/// A unit way as two 24-bit numbers (octahedral: what they give back is off by less than
/// 3e-7 rad).
fn unit_code(d: Vec3) -> [u32; 2] {
    let d = d.as_dvec3();
    let l1 = d.x.abs() + d.y.abs() + d.z.abs();
    let (mut x, mut y) = if l1 > 0.0 { (d.x / l1, d.y / l1) } else { (0.0, 0.0) };
    if d.z < 0.0 {
        let sx = if x >= 0.0 { 1.0 } else { -1.0 };
        let sy = if y >= 0.0 { 1.0 } else { -1.0 };
        (x, y) = ((1.0 - y.abs()) * sx, (1.0 - x.abs()) * sy);
    }
    [x, y].map(|v| ((v.clamp(-1.0, 1.0) * 0.5 + 0.5) * UNIT_SIDE).round() as u32)
}

/// The unit way two 24-bit numbers give (`unit_code`): the same, to the bit, in every game.
fn unit_of(code: [u32; 2]) -> Vec3 {
    let [mut x, mut y] = code.map(|q| f64::from(q.min((1 << 24) - 1)) / UNIT_SIDE * 2.0 - 1.0);
    let z = 1.0 - x.abs() - y.abs();
    if z < 0.0 {
        let sx = if x >= 0.0 { 1.0 } else { -1.0 };
        let sy = if y >= 0.0 { 1.0 } else { -1.0 };
        (x, y) = ((1.0 - y.abs()) * sx, (1.0 - x.abs()) * sy);
    }
    DVec3::new(x, y, z).normalize_or(DVec3::Z).as_vec3()
}

/// A unit way in 6 bytes (`unit_code`).
fn write_unit(w: &mut Writer, d: Vec3) {
    for q in unit_code(d) {
        w.bytes(&q.to_le_bytes()[..3]);
    }
}

fn read_unit(r: &mut Reader) -> Result<Vec3, WireError> {
    let mut code = [0u32; 2];
    for q in &mut code {
        let b = r.bytes(3)?;
        *q = u32::from_le_bytes([b[0], b[1], b[2], 0]);
    }
    Ok(unit_of(code))
}

/// A strike as every game does it: a round's way as it is told (6 bytes, `unit_code`); the
/// server does it so as well (`Game::strike`), so all do the same to the bit.
pub fn as_told(s: Strike) -> Strike {
    match s {
        Strike::Hit { id, mut hit } if hit.radius == 0.0 => {
            hit.dir = unit_of(unit_code(hit.dir));
            Strike::Hit { id, hit }
        }
        s => s,
    }
}

/// One thing seen into `w`, told against the one before it (`prev`, then this one).
fn write_seen_one(w: &mut Writer, prev: &mut Prev, from: &From, s: &Seen, name: &impl Fn(u64) -> Option<Named>) {
    let code = |n: Option<Named>| n.map_or(0, |n| n.code() + 1);
    let (here, frame) = match *from {
        From::World => (0, None),
        From::Beside { named, frame } => (named.code() + 1, Some(frame)),
    };
    let (kind, tag, what, at, vel, by, plain) = match *s {
        Seen::Launch { tag, launch: l } => (0u8, tag, Some(l.what), l.from, l.vel, code(l.by.and_then(name)), code(l.target.and_then(name)) == 0),
        Seen::End { tag, what, at, vel, on, extra, .. } => (1, tag, Some(what), at, vel, code(on.and_then(name)), extra == 0.0),
        Seen::Track { tag, pos, vel, .. } => (2, tag, None, pos, vel, 0, false),
    };
    // (ways turned with the frame; speeds as what they add to its own there)
    let turned = |d: DVec3| frame.map_or(d.as_vec3(), |f| f.rot.inverse() * d.as_vec3());
    let rel = frame.map_or(vel.as_vec3(), |f| f.rot.inverse() * (vel - f.moving(at)).as_vec3());
    let mut head = kind;
    if here == prev.code {
        head |= SAME_FRAME;
    }
    if tag == prev.tag.wrapping_add(1) {
        head |= NEXT_TAG;
    }
    if what.is_some() && what == prev.what {
        head |= SAME_WHAT;
    }
    if rel.length_squared() < STILL_UNDER * STILL_UNDER {
        head |= STILL;
    }
    if kind != 2 && by == here {
        head |= BY_FRAME;
    }
    if kind != 2 && plain {
        head |= PLAIN;
    }
    w.u8(head);
    if head & SAME_FRAME == 0 {
        w.var(here);
    }
    if head & NEXT_TAG == 0 {
        w.zig(i64::from(tag) - i64::from(prev.tag));
    }
    if let Some(x) = what
        && head & SAME_WHAT == 0
    {
        let (c, i) = what_code(x);
        w.u8(c);
        w.u16(i);
    }
    match frame {
        Some(f) => w.vec3(f.local(at)),
        None => {
            w.f64(at.x);
            w.f64(at.y);
            w.f64(at.z);
        }
    }
    match *s {
        Seen::Launch { launch: l, .. } => {
            write_unit(w, turned(l.dir));
            w.f32(l.speed);
        }
        Seen::End { dir, .. } => write_unit(w, turned(dir)),
        Seen::Track { .. } => {}
    }
    if head & STILL == 0 {
        w.vec3(rel);
    }
    match *s {
        Seen::Launch { launch: l, .. } => {
            if head & PLAIN == 0 {
                w.var(code(l.target.and_then(name)));
            }
            if head & BY_FRAME == 0 {
                w.var(by);
            }
        }
        Seen::End { extra, .. } => {
            if head & BY_FRAME == 0 {
                w.var(by);
            }
            if head & PLAIN == 0 {
                w.f32(extra);
            }
        }
        Seen::Track { push, .. } => w.vec3(turned(push)),
    }
    prev.code = here;
    prev.tag = tag;
    if what.is_some() {
        prev.what = what;
    }
}

/// Things seen (`Seen`) into `w`, each told from where it says (`From`), at `stamp` (the
/// server's clock). `name(id)`: how every game names one of our structures (a target, what
/// let it fly, what it struck), if they all have it. Places beside a structure go as offsets in
/// its frame, ways turned with it, speeds as what they add to its own there; each one against
/// the one before it (what is the same is left out: sorted by frame, most of it is).
pub fn write_seen(w: &mut Writer, stamp: f64, seen: &[(From, Seen)], name: impl Fn(u64) -> Option<Named>) {
    w.u8(SEEN);
    w.f64(stamp);
    w.var(seen.len() as u64);
    let mut prev = Prev::NONE;
    for (from, s) in seen {
        write_seen_one(w, &mut prev, from, s, &name);
    }
}

/// Things seen told as `write_seen` does, in as many messages as it takes, none longer than
/// `budget` bytes (nor with more than `SEEN_MOST` things): each one whole into `send`. `body`
/// and `msg` are reused.
pub fn write_seen_split<'a>(stamp: f64, seen: impl IntoIterator<Item = &'a (From, Seen)>, name: impl Fn(u64) -> Option<Named>, budget: usize, body: &mut Vec<u8>, msg: &mut Vec<u8>, mut send: impl FnMut(&[u8])) {
    let mut seal = |n: usize, body: &mut Vec<u8>, msg: &mut Vec<u8>| {
        msg.clear();
        msg.resize(body.len() + 16, 0);
        let mut w = Writer::new(msg);
        w.u8(SEEN);
        w.f64(stamp);
        w.var(n as u64);
        w.bytes(body);
        let k = w.finish().unwrap_or(0);
        msg.truncate(k);
        send(msg);
        body.clear();
    };
    body.clear();
    let (mut prev, mut n) = (Prev::NONE, 0usize);
    let mut one = [0u8; SEEN_ONE_MOST];
    for (from, s) in seen {
        let mut p = prev;
        let mut w = Writer::new(&mut one);
        write_seen_one(&mut w, &mut p, from, s, &name);
        let mut len = w.finish().unwrap_or(0);
        if len == 0 {
            continue;
        }
        if n > 0 && (11 + body.len() + len > budget || n == SEEN_MOST) {
            seal(n, body, msg);
            n = 0;
            // (the first of a message is told whole)
            p = Prev::NONE;
            let mut w = Writer::new(&mut one);
            write_seen_one(&mut w, &mut p, from, s, &name);
            len = w.finish().unwrap_or(0);
        }
        body.extend_from_slice(&one[..len]);
        prev = p;
        n += 1;
    }
    if n > 0 {
        seal(n, body, msg);
    }
}

/// What `write_seen` wrote, after its kind byte, as this game must show it: `now` is the
/// server's clock here; `find(named)` this game's number for a structure every game has, and
/// its frame now (none: not here; what was told beside it is not shown, and what named it names
/// nothing). Each thing comes with how long ago it was. What was let fly is placed beside this
/// game's copy of its frame as it was then; where something ended, on the copy as it is now.
pub fn read_seen(r: &mut Reader, now: f64, find: impl Fn(Named) -> Option<(u64, Frame)>, out: &mut Vec<(Seen, f32)>) -> Result<(), WireError> {
    let stamp = r.f64()?;
    let age = (now - stamp).clamp(0.0, 10.0);
    let n = r.var()?;
    if n > SEEN_MOST as u64 {
        return Err(WireError::Long);
    }
    let id = |c: u64| if c == 0 { None } else { find(Named::of(c - 1)).map(|f| f.0) };
    let mut prev = Prev::NONE;
    // (the frame told, as found here: its number and how it is now; and whether it is here)
    let (mut here, mut found, mut shown): (Option<u64>, Option<Frame>, bool) = (None, None, true);
    for _ in 0..n {
        let head = r.u8()?;
        let kind = head & 3;
        if kind > 2 {
            return Err(WireError::Value);
        }
        if head & SAME_FRAME == 0 {
            prev.code = r.var()?;
            // (beside a structure this game does not have: read, and left)
            (here, found, shown) = match prev.code {
                0 => (None, None, true),
                c => match find(Named::of(c - 1)) {
                    Some((k, f)) => (Some(k), Some(f), true),
                    None => (None, Some(Frame { pos: DVec3::ZERO, rot: Quat::IDENTITY, vel: DVec3::ZERO, spin: Vec3::ZERO }), false),
                },
            };
        } else if prev.code == u64::MAX {
            return Err(WireError::Value);
        }
        prev.tag = if head & NEXT_TAG != 0 { prev.tag.wrapping_add(1) } else { i64::from(prev.tag).wrapping_add(r.zig()?) as u32 };
        if kind != 2 {
            if head & SAME_WHAT == 0 {
                let c = r.u8()?;
                prev.what = Some(what_of(c, r.u16()?)?);
            } else if prev.what.is_none() {
                return Err(WireError::Value);
            }
        }
        let (tag, what) = (prev.tag, prev.what.unwrap_or(What::Shot(0)));
        // (what was let fly, where it was then; where something ended, where that is now)
        let frame = found.map(|f| if kind == 1 { f } else { f.before(age) });
        let at = match frame {
            Some(f) => f.world(r.vec3()?),
            None => DVec3::new(r.f64()?, r.f64()?, r.f64()?),
        };
        let turned = |d: Vec3| frame.map_or(d, |f| f.rot * d).as_dvec3();
        let way = |r: &mut Reader| -> Result<DVec3, WireError> { Ok(turned(read_unit(r)?).normalize_or(DVec3::Z)) };
        let speed = |r: &mut Reader| -> Result<DVec3, WireError> {
            let v = if head & STILL != 0 { Vec3::ZERO } else { r.vec3()? };
            Ok(frame.map_or(v.as_dvec3(), |f| f.moving(at) + (f.rot * v).as_dvec3()))
        };
        let s = match kind {
            0 => {
                let dir = way(r)?;
                let speed_own = r.f32()?;
                let vel = speed(r)?;
                let target = if head & PLAIN != 0 { None } else { id(r.var()?) };
                let by = if head & BY_FRAME != 0 { here } else { id(r.var()?) };
                Seen::Launch { tag, launch: Launch { what, from: at, dir, speed: speed_own, vel, target, by } }
            }
            1 => {
                let dir = way(r)?;
                let vel = speed(r)?;
                let on = if head & BY_FRAME != 0 { here } else { id(r.var()?) };
                let extra = if head & PLAIN != 0 { 0.0 } else { r.f32()? };
                Seen::End { tag, what, at, dir, vel, on, extra }
            }
            _ => {
                let vel = speed(r)?;
                let push = turned(r.vec3()?);
                Seen::Track { tag, pos: at, vel, push }
            }
        };
        if shown {
            out.push((s, age as f32));
        }
    }
    Ok(())
}

/// A ship made in play into `w`: its kind and its state.
pub fn write_spawn(w: &mut Writer, kind: &str, state: &lunar_net::RigidState) {
    w.u8(SPAWN);
    w.str(kind);
    state.encode(w);
}

/// A control of ship `ship` (its number) left at `value`.
pub fn write_control(w: &mut Writer, ship: u64, control: u16, value: f64) {
    w.u8(CONTROL);
    w.var(ship);
    w.var(u64::from(control));
    w.f64(value);
}

/// What `write_control` wrote, after its kind byte.
pub fn read_control(r: &mut Reader) -> Result<(u64, u16, f64), WireError> {
    Ok((r.var()?, r.var16()?, r.f64()?))
}

/// What a hand did to ship `ship` that is not a control (a door, a clamp: `controls::Act`).
pub fn write_act(w: &mut Writer, ship: u64, act: Act) {
    w.u8(ACT);
    w.var(ship);
    let (kind, which, on) = match act {
        Act::Closure(c, open) => (0, c, open),
        Act::Clamp(c, holding) => (1, c, holding),
    };
    w.u8(kind);
    w.var(u64::from(which));
    w.u8(u8::from(on));
}

/// What `write_act` wrote, after its kind byte.
pub fn read_act(r: &mut Reader) -> Result<(u64, Act), WireError> {
    let ship = r.var()?;
    let (kind, which, on) = (r.u8()?, r.var16()?, r.u8()? != 0);
    Ok((ship, if kind == 0 { Act::Closure(which, on) } else { Act::Clamp(which, on) }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;

    fn written(f: impl FnOnce(&mut Writer)) -> Vec<u8> {
        let mut buf = vec![0u8; 70_000];
        let mut w = Writer::new(&mut buf);
        f(&mut w);
        let n = w.finish().expect("it fits");
        buf.truncate(n);
        buf
    }

    #[test]
    fn names_go_and_come_back() {
        for n in [Named::Ship(0), Named::Ship(77), Named::Built(1), Named::Built(1 << 40), Named::Piece(1), Named::Piece((1 << 61) - 1)] {
            assert_eq!(Named::of(n.code()), n);
        }
    }

    #[test]
    fn strikes_go_and_come_back_as_they_were() {
        let hit = Hit { point: Vec3::new(1.5, -2.0, 7.25), dir: Vec3::new(0.0, 0.6, -0.8), energy: 4.2e5, radius: 3.0, area: 0.002 };
        // (a round: no reach, its way told short, and done so in the server too)
        let round = Hit { point: Vec3::new(-0.3, 1.1, 2.0), dir: Vec3::new(0.48, -0.6, 0.64), radius: 0.0, energy: 5.5e4, area: 3e-4 };
        let strikes = vec![
            (41, Named::Ship(3), Strike::Hit { id: 99, hit }),
            (42, Named::Ship(3), Strike::Hit { id: 99, hit }),
            (45, Named::Built(12), Strike::Blow { id: 5, part: 41, push: Vec3::new(0.0, 0.0, 1.0), energy: 2e4 }),
            (46, Named::Built(12), Strike::Hit { id: 5, hit: round }),
            (47, Named::Built(12), Strike::Hit { id: 5, hit: Hit { point: Vec3::new(0.2, 1.0, 2.1), ..round } }),
        ];
        // (the ship's dish turned and its ramp down, where the strikes were decided: told whole
        // the first time, and then only the bone that moved)
        let bones = [Affine3A::from_rotation_translation(Quat::from_rotation_y(0.7), Vec3::new(0.0, 2.0, -1.0)), Affine3A::from_rotation_translation(Quat::from_rotation_x(-1.2), Vec3::new(0.0, -0.5, -9.0))];
        let first = Posed { named: Named::Ship(3), bones: &bones, mask: u64::MAX, all: true };
        let bytes = written(|w| write_strikes(w, &strikes, &[first]));
        let mut r = Reader::new(&bytes);
        assert_eq!(r.u8(), Ok(STRIKES));
        let (mut back, mut poses) = (Vec::new(), Vec::new());
        assert_eq!(read_strikes(&mut r, &mut back, &mut poses), Ok(()));
        assert!(r.is_empty());
        assert_eq!(back[0], (41, Named::Ship(3), Strike::Hit { id: 0, hit }));
        assert_eq!(back[1].0, 42);
        assert_eq!(back[2], (45, Named::Built(12), Strike::Blow { id: 0, part: 41, push: Vec3::Z, energy: 2e4 }));
        for k in 3..5 {
            let Strike::Hit { hit, .. } = as_told(strikes[k].2) else { unreachable!() };
            assert_eq!(back[k], (strikes[k].0, Named::Built(12), Strike::Hit { id: 0, hit }));
            assert!(hit.dir.angle_between(round.dir) < 1e-6 && hit.dir != round.dir);
        }
        assert_eq!(poses.len(), 1);
        // (to the bit: what a strike does hangs on it)
        assert!(poses[0].0 == Named::Ship(3) && poses[0].1 == 2 && poses[0].2 == u64::MAX && poses[0].3 == bones);
        let moved = Posed { named: Named::Ship(3), bones: &bones, mask: 0b10, all: false };
        let bytes = written(|w| write_strikes(w, &strikes[..1], &[moved]));
        let (mut back, mut poses) = (Vec::new(), Vec::new());
        assert_eq!(read_strikes(&mut Reader::new(&bytes[1..]), &mut back, &mut poses), Ok(()));
        assert!(poses[0].1 == 2 && poses[0].2 == 0b10 && poses[0].3 == bones[1..]);
        // (a blast's hit is some 40 bytes; another like it on the same, 29; a round after another
        // on the same, 19; a bone, 48 bytes, only when it moved)
        let size = |k: usize| written(|w| write_strikes(w, &strikes[..k], &[])).len();
        assert!(size(1) <= 44 && size(2) - size(1) <= 29 && size(5) - size(4) <= 19, "{} {} {}", size(1), size(2) - size(1), size(5) - size(4));
        // cut short or with nonsense in it, it says so and never panics
        for cut in 1..bytes.len() {
            let mut r = Reader::new(&bytes[1..cut]);
            let _ = read_strikes(&mut r, &mut Vec::new(), &mut Vec::new());
        }
    }

    #[test]
    fn what_is_seen_beside_a_ship_at_orbital_speed_leaves_its_muzzle_in_every_game() {
        // the shooter's ship at 1.7 km/s, turned and turning; a round leaves 4 m ahead of it,
        // out of its nose, at 1 km/s more
        let ship = Frame { pos: DVec3::new(1.738e6, 2.0e4, -3.0e5), rot: Quat::from_rotation_y(0.7) * Quat::from_rotation_x(0.2), vel: DVec3::new(1700.0, 30.0, -12.0), spin: Vec3::new(0.0, 0.3, 0.0) };
        let nose = (ship.rot * Vec3::Z).as_dvec3();
        let muzzle = ship.pos + nose * 4.0;
        let launch = Launch { what: What::Shot(2), from: muzzle, dir: nose, speed: 1000.0, vel: ship.moving(muzzle), target: Some(70), by: Some(70) };
        // where it struck the other ship, 900 m on, which goes at its own speed
        let other = Frame { pos: muzzle + nose * 900.0, rot: Quat::from_rotation_z(1.1), vel: DVec3::new(1650.0, 0.0, 40.0), spin: Vec3::ZERO };
        let hit = other.pos + DVec3::new(1.0, 2.0, -0.5);
        let seen = vec![
            (From::Beside { named: Named::Ship(4), frame: ship }, Seen::Launch { tag: 7, launch }),
            (From::Beside { named: Named::Ship(5), frame: other }, Seen::End { tag: 7, what: What::Shot(2), at: hit, dir: nose, vel: other.vel, on: Some(71), extra: 0.0 }),
            (From::World, Seen::Track { tag: 9, pos: muzzle, vel: ship.vel, push: DVec3::new(0.0, 30.0, -200.0) }),
        ];
        let name = |id: u64| match id {
            70 => Some(Named::Ship(4)),
            71 => Some(Named::Ship(5)),
            _ => None,
        };
        let bytes = written(|w| write_seen(w, 100.0, &seen, name));
        // another game, whose clock is 40 ms later, whose copy of the shooter is 6 m off where
        // the shooter has it and turned a little more, and whose copy of the other ship is 3 m
        // off: the round leaves its muzzle there, out of its nose, and ends on that copy's hull
        let later = 0.04;
        let turned = Quat::from_rotation_y(0.05) * ship.rot;
        let theirs = Frame { pos: ship.pos + ship.vel * later + DVec3::new(6.0, 0.0, 0.0), rot: turned, ..ship };
        let theirs_other = Frame { pos: other.pos + DVec3::new(0.0, 3.0, 0.0), ..other };
        let find = |n: Named| match n {
            Named::Ship(4) => Some((170, theirs)),
            Named::Ship(5) => Some((171, theirs_other)),
            _ => None,
        };
        let mut r = Reader::new(&bytes);
        assert_eq!(r.u8(), Ok(SEEN));
        let mut out = Vec::new();
        read_seen(&mut r, 100.0 + later, find, &mut out).unwrap();
        assert!(r.is_empty() && out.len() == 3);
        let (Seen::Launch { tag: 7, launch: l }, age) = out[0] else { panic!("{:?}", out[0]) };
        assert!((f64::from(age) - later).abs() < 1e-6);
        assert_eq!((l.target, l.by, l.what, l.speed), (Some(170), Some(170), What::Shot(2), 1000.0));
        // where it is now, by what `Blasts::show` makes of it: at their copy's muzzle, out of its
        // nose, as far ahead as it flew
        let their_nose = (turned * Vec3::Z).as_dvec3();
        let now = l.from + (l.vel + l.dir * 1000.0) * f64::from(age);
        // (the muzzle then, carried on with the ship: where the copy's muzzle is now, plus what the
        // ship's turning added and the round's own way)
        let spun = theirs.moving(theirs.pos + their_nose * 4.0) - theirs.vel;
        let want = theirs.pos + their_nose * 4.0 + (spun + their_nose * 1000.0) * later;
        assert!(now.distance(want) < 0.05, "{:.3} m from where it must be", now.distance(want));
        assert!(l.dir.distance(their_nose) < 1e-5);
        let (Seen::End { on, at, .. }, _) = out[1] else { panic!() };
        assert_eq!(on, Some(171));
        assert!(at.distance(hit + DVec3::new(0.0, 3.0, 0.0)) < 0.001, "on their copy's hull: {:.4}", at.distance(hit + DVec3::new(0.0, 3.0, 0.0)));
        assert!(matches!(out[2].0, Seen::Track { tag: 9, pos, push, .. } if pos == muzzle && push == DVec3::new(0.0, 30.0, -200.0)));
        // ships this game does not have: what was told beside them is not shown, and what named
        // them names nothing
        let mut out = Vec::new();
        read_seen(&mut Reader::new(&bytes[1..]), 100.0, |n| (n == Named::Ship(5)).then_some((171, other)), &mut out).unwrap();
        assert_eq!(out.len(), 2);
        assert!(matches!(out[0].0, Seen::End { .. }));
        // a round of a ship's gun beside it, and where one ended on the ship it struck, take
        // little: what is the same as in the one before is left out
        let one = |s: &[(From, Seen)]| written(|w| write_seen(w, 1.0, s, name)).len() - 10;
        assert!(one(&seen[..1]) <= 34, "a launch: {} bytes", one(&seen[..1]));
        let ends = [seen[1], (seen[1].0, Seen::End { tag: 8, what: What::Shot(2), at: hit + DVec3::X, dir: nose, vel: other.vel, on: Some(71), extra: 0.0 })];
        assert!(one(&ends) - one(&ends[..1]) <= 20, "an end after another: {} bytes", one(&ends) - one(&ends[..1]));
        // many, told in as many messages as it takes, none longer than may go in a datagram, and
        // every one comes back as it was told
        let many: Vec<(From, Seen)> = (0..300u32)
            .map(|k| {
                let mut s = seen[(k % 3) as usize];
                match &mut s.1 {
                    Seen::Launch { tag, launch } => {
                        *tag = 1000 + k;
                        launch.from += nose * f64::from(k);
                    }
                    Seen::End { tag, .. } | Seen::Track { tag, .. } => *tag = 1000 + k,
                }
                s
            })
            .collect();
        let (mut body, mut msg, mut told) = (Vec::new(), Vec::new(), Vec::new());
        write_seen_split(100.0, &many, name, lunar_net::MAX_HINT, &mut body, &mut msg, |m| told.push(m.to_vec()));
        assert!(told.len() > 1 && told.iter().all(|m| m.len() <= lunar_net::MAX_HINT), "{:?}", told.iter().map(Vec::len).collect::<Vec<_>>());
        let mut back = Vec::new();
        let same = |n: Named| match n {
            Named::Ship(4) => Some((170, ship)),
            Named::Ship(5) => Some((171, other)),
            _ => None,
        };
        for m in &told {
            let mut r = Reader::new(m);
            assert_eq!(r.u8(), Ok(SEEN));
            read_seen(&mut r, 100.0, same, &mut back).unwrap();
            assert!(r.is_empty());
        }
        assert_eq!(back.len(), many.len());
        for ((_, a), (b, _)) in many.iter().zip(&back) {
            match (a, b) {
                (Seen::Launch { tag: t, launch: x }, Seen::Launch { tag: u, launch: y }) => assert!(t == u && x.from.distance(y.from) < 1e-3 && x.dir.distance(y.dir) < 1e-6 && x.vel.distance(y.vel) < 0.02, "{x:?} {y:?}"),
                (Seen::End { tag: t, at: x, .. }, Seen::End { tag: u, at: y, .. }) => assert!(t == u && x.distance(*y) < 1e-3),
                (Seen::Track { tag: t, .. }, Seen::Track { tag: u, .. }) => assert_eq!(t, u),
                _ => panic!("{a:?} came back as {b:?}"),
            }
        }
        for cut in 1..bytes.len() {
            let _ = read_seen(&mut Reader::new(&bytes[1..cut]), 5.0, find, &mut Vec::new());
        }
    }

    #[test]
    fn controls_go_and_come_back() {
        let bytes = written(|w| write_control(w, 7, 300, -0.25));
        let mut r = Reader::new(&bytes);
        assert_eq!(r.u8(), Ok(CONTROL));
        assert_eq!(read_control(&mut r), Ok((7, 300, -0.25)));
    }

    #[test]
    fn what_a_hand_does_to_doors_and_clamps_goes_and_comes_back() {
        for act in [Act::Closure(3, true), Act::Closure(400, false), Act::Clamp(1, true), Act::Clamp(0, false)] {
            let bytes = written(|w| write_act(w, 9, act));
            let mut r = Reader::new(&bytes);
            assert_eq!(r.u8(), Ok(ACT));
            assert_eq!(read_act(&mut r), Ok((9, act)));
            assert!(bytes.len() <= 6);
        }
    }
}
