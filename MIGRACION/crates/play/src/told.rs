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

/// Seen things in one message at most (a loose message fits a datagram).
pub const SEEN_EACH: usize = 20;

/// Strikes into `w`: the dice they are done with (`seed`, the first; one more each), each
/// against the structure named (the strike's own id is this game's and does not go), and how
/// the articulated ones among those were posed here (`poses`: bones, as `Structure::bones`
/// past the root): every game does them so.
pub fn write_strikes(w: &mut Writer, seed: u64, strikes: &[(Named, Strike)], poses: &[(Named, &[Affine3A])]) {
    w.u8(STRIKES);
    w.var(seed);
    w.var(strikes.len() as u64);
    for (named, s) in strikes {
        match *s {
            Strike::Hit { hit, .. } => {
                w.u8(0);
                w.var(named.code());
                w.vec3(hit.point);
                w.vec3(hit.dir);
                w.f32(hit.energy);
                w.f32(hit.radius);
                w.f32(hit.area);
            }
            Strike::Blow { part, push, energy, .. } => {
                w.u8(1);
                w.var(named.code());
                w.var(u64::from(part));
                w.vec3(push);
                w.f32(energy);
            }
        }
    }
    w.var(poses.len() as u64);
    for (named, bones) in poses {
        w.var(named.code());
        w.var(bones.len() as u64);
        for b in *bones {
            let (_, rot, at) = b.to_scale_rotation_translation();
            w.vec3(at);
            w.f32(rot.x);
            w.f32(rot.y);
            w.f32(rot.z);
            w.f32(rot.w);
        }
    }
}

/// What `write_strikes` wrote, after its kind byte: the first seed, each strike with the
/// structure it names (its id left 0, for whoever finds it), and the poses.
pub fn read_strikes(r: &mut Reader, out: &mut Vec<(Named, Strike)>, poses: &mut Vec<(Named, Vec<Affine3A>)>) -> Result<u64, WireError> {
    let seed = r.var()?;
    let n = r.var()?;
    if n > 4096 {
        return Err(WireError::Long);
    }
    for _ in 0..n {
        let tag = r.u8()?;
        let named = Named::of(r.var()?);
        let s = match tag {
            0 => Strike::Hit { id: 0, hit: Hit { point: r.vec3()?, dir: r.vec3()?, energy: r.f32()?, radius: r.f32()?, area: r.f32()? } },
            1 => Strike::Blow { id: 0, part: r.var32()?, push: r.vec3()?, energy: r.f32()? },
            _ => return Err(WireError::Value),
        };
        out.push((named, s));
    }
    let n = r.var()?;
    if n > 256 {
        return Err(WireError::Long);
    }
    for _ in 0..n {
        let named = Named::of(r.var()?);
        let k = r.var()?;
        if k > 1024 {
            return Err(WireError::Long);
        }
        let mut bones = Vec::with_capacity(k as usize);
        for _ in 0..k {
            let at = r.vec3()?;
            let rot = Quat::from_xyzw(r.f32()?, r.f32()?, r.f32()?, r.f32()?).normalize();
            bones.push(Affine3A::from_rotation_translation(rot, at));
        }
        poses.push((named, bones));
    }
    Ok(seed)
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

/// Things seen (`Seen`) into `w`, each told from where it says (`From`), at `stamp` (the
/// server's clock). `name(id)`: how every game names one of our structures (a target, what
/// let it fly, what it struck), if they all have it. Places beside a structure go as offsets in
/// its frame, ways turned with it, speeds as what they add to its own there.
pub fn write_seen(w: &mut Writer, stamp: f64, seen: &[(From, Seen)], name: impl Fn(u64) -> Option<Named>) {
    w.u8(SEEN);
    w.f64(stamp);
    w.var(seen.len() as u64);
    let code = |n: Option<Named>| n.map_or(0, |n| n.code() + 1);
    for (from, s) in seen {
        let frame = match from {
            From::World => {
                w.var(0);
                None
            }
            From::Beside { named, frame } => {
                w.var(named.code() + 1);
                Some(*frame)
            }
        };
        let place = |w: &mut Writer, p: DVec3| match frame {
            Some(f) => w.vec3(f.local(p)),
            None => {
                w.f64(p.x);
                w.f64(p.y);
                w.f64(p.z);
            }
        };
        let way = |w: &mut Writer, d: DVec3| w.vec3(frame.map_or(d.as_vec3(), |f| f.rot.inverse() * d.as_vec3()));
        // (a way with a length: turned with the frame, kept as long)
        let way_long = |w: &mut Writer, d: DVec3| w.vec3(frame.map_or(d.as_vec3(), |f| f.rot.inverse() * d.as_vec3()));
        let speed = |w: &mut Writer, v: DVec3, at: DVec3| w.vec3(frame.map_or(v.as_vec3(), |f| f.rot.inverse() * (v - f.moving(at)).as_vec3()));
        let what = |w: &mut Writer, x: What| {
            let (c, i) = what_code(x);
            w.u8(c);
            w.u16(i);
        };
        match *s {
            Seen::Launch { tag, launch: l } => {
                w.u8(0);
                w.var(u64::from(tag));
                what(w, l.what);
                place(w, l.from);
                way(w, l.dir);
                w.f32(l.speed);
                speed(w, l.vel, l.from);
                w.var(code(l.target.and_then(&name)));
                w.var(code(l.by.and_then(&name)));
            }
            Seen::End { tag, what: x, at, dir, vel, on, extra } => {
                w.u8(1);
                w.var(u64::from(tag));
                what(w, x);
                place(w, at);
                way(w, dir);
                speed(w, vel, at);
                w.var(code(on.and_then(&name)));
                w.f32(extra);
            }
            Seen::Track { tag, pos, vel, push } => {
                w.u8(2);
                w.var(u64::from(tag));
                place(w, pos);
                speed(w, vel, pos);
                way_long(w, push);
            }
        }
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
    if n > SEEN_EACH as u64 * 4 {
        return Err(WireError::Long);
    }
    let id = |c: u64| if c == 0 { None } else { find(Named::of(c - 1)).map(|f| f.0) };
    for _ in 0..n {
        let code = r.var()?;
        // (beside a structure this game does not have: read, and left)
        let (frame, shown) = match code {
            0 => (None, true),
            c => match find(Named::of(c - 1)) {
                Some((_, f)) => (Some(f), true),
                None => (Some(Frame { pos: DVec3::ZERO, rot: Quat::IDENTITY, vel: DVec3::ZERO, spin: Vec3::ZERO }), false),
            },
        };
        let kind = r.u8()?;
        // (what was let fly, where it was then; where something ended, where that is now)
        let frame = frame.map(|f| if kind == 1 { f } else { f.before(age) });
        let place = |r: &mut Reader| -> Result<DVec3, WireError> {
            Ok(match frame {
                Some(f) => f.world(r.vec3()?),
                None => DVec3::new(r.f64()?, r.f64()?, r.f64()?),
            })
        };
        let way = |r: &mut Reader| -> Result<DVec3, WireError> {
            let d = r.vec3()?;
            Ok(frame.map_or(d, |f| f.rot * d).as_dvec3().normalize_or(DVec3::Z))
        };
        let speed = |r: &mut Reader, at: DVec3| -> Result<DVec3, WireError> {
            let v = r.vec3()?;
            Ok(frame.map_or(v.as_dvec3(), |f| f.moving(at) + (f.rot * v).as_dvec3()))
        };
        let what = |r: &mut Reader| -> Result<What, WireError> {
            let c = r.u8()?;
            what_of(c, r.u16()?)
        };
        let s = match kind {
            0 => {
                let tag = r.var()? as u32;
                let what = what(r)?;
                let from = place(r)?;
                let dir = way(r)?;
                let speed_own = r.f32()?;
                let vel = speed(r, from)?;
                let target = id(r.var()?);
                let by = id(r.var()?);
                Seen::Launch { tag, launch: Launch { what, from, dir, speed: speed_own, vel, target, by } }
            }
            1 => {
                let tag = r.var()? as u32;
                let what = what(r)?;
                let at = place(r)?;
                let dir = way(r)?;
                let vel = speed(r, at)?;
                let on = id(r.var()?);
                Seen::End { tag, what, at, dir, vel, on, extra: r.f32()? }
            }
            2 => {
                let tag = r.var()? as u32;
                let pos = place(r)?;
                let vel = speed(r, pos)?;
                let push = r.vec3()?;
                Seen::Track { tag, pos, vel, push: frame.map_or(push, |f| f.rot * push).as_dvec3() }
            }
            _ => return Err(WireError::Value),
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
        let strikes = vec![(Named::Ship(3), Strike::Hit { id: 99, hit }), (Named::Built(12), Strike::Blow { id: 5, part: 41, push: Vec3::new(0.0, 0.0, 1.0), energy: 2e4 })];
        // (the ship's dish turned and its ramp down, where the strikes were decided)
        let bones = [Affine3A::from_rotation_translation(Quat::from_rotation_y(0.7), Vec3::new(0.0, 2.0, -1.0)), Affine3A::from_rotation_translation(Quat::from_rotation_x(-1.2), Vec3::new(0.0, -0.5, -9.0))];
        let bytes = written(|w| write_strikes(w, 0xdead_beef, &strikes, &[(Named::Ship(3), &bones)]));
        let mut r = Reader::new(&bytes);
        assert_eq!(r.u8(), Ok(STRIKES));
        let (mut back, mut poses) = (Vec::new(), Vec::new());
        assert_eq!(read_strikes(&mut r, &mut back, &mut poses), Ok(0xdead_beef));
        assert!(r.is_empty());
        assert_eq!(back[0], (Named::Ship(3), Strike::Hit { id: 0, hit }));
        assert_eq!(back[1], (Named::Built(12), Strike::Blow { id: 0, part: 41, push: Vec3::Z, energy: 2e4 }));
        assert_eq!(poses.len(), 1);
        assert!(poses[0].0 == Named::Ship(3) && poses[0].1.iter().zip(&bones).all(|(a, b)| a.abs_diff_eq(*b, 1e-6)));
        // (one hit is 39 bytes: a gun hitting 80 times a second is 3 kB/s while it hits; a pose,
        // 28 bytes a bone, once a message)
        assert!(written(|w| write_strikes(w, 1, &strikes[..1], &[])).len() <= 42);
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
        // a full message of launches fits well inside what may be told at once
        let many: Vec<(From, Seen)> = (0..SEEN_EACH).map(|_| seen[0]).collect();
        let full = written(|w| write_seen(w, 1.0, &many, name));
        assert!(full.len() <= lunar_net::MAX_HINT, "{} bytes", full.len());
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
