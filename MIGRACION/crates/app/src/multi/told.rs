//! What the games tell each other besides the states the network carries by itself (each
//! player, each thing that moves): a kind byte and its fields. One place for every kind, how it
//! travels and what it carries:
//!
//! | Kind | How | What |
//! |---|---|---|
//! | `CONTROL` | sure, to the others | a control of a ship left at a value by a hand |
//! | `ACT` | sure, to the others | a door or a clamp of a ship worked by a hand (`aboard::Act`) |
//! | `SPAWN` | sure, echoed | a ship made in play (its number: its place among those made) |
//! | `STRIKES` | sure, echoed | what was done to shared structures (hits, plates torn out), done by every game in the order the server passed it on |
//! | `SEEN` | loose | what a player fired or set off, to be seen (it does nothing: its damage goes as `STRIKES`) |
//!
//! Places that must be right however fast things go travel in the frame of a structure every
//! game has (`Named`): a hit in its own frame; a shot beside what its shooter rides, so that it
//! leaves the muzzle in every game whatever each one's clock says.
use crate::{aboard::Act, blasts::Seen, builds::Strike};
use glam::DVec3;
use lunar_core::structure::damage::Hit;
use lunar_net::{Reader, WireError, Writer};

pub const CONTROL: u8 = 1;
pub const SPAWN: u8 = 2;
pub const STRIKES: u8 = 3;
pub const SEEN: u8 = 4;
pub const ACT: u8 = 5;

/// A structure as every game names it: a ship by its number on the network, anything the
/// scenario set round the site by its own id (the same in every game that starts with it).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Named {
    Ship(u64),
    Built(u64),
}

impl Named {
    pub fn code(self) -> u64 {
        match self {
            Named::Ship(k) => (k << 1) | 1,
            Named::Built(id) => id << 1,
        }
    }

    pub fn of(code: u64) -> Named {
        if code & 1 == 1 { Named::Ship(code >> 1) } else { Named::Built(code >> 1) }
    }
}

/// Seen things in one message at most (a loose message fits a datagram).
pub const SEEN_EACH: usize = 20;

/// Strikes into `w`: the dice they are done with (`seed`, the first; one more each), and each
/// against the structure named (the strike's own id is this game's and does not go).
pub fn write_strikes(w: &mut Writer, seed: u64, strikes: &[(Named, Strike)]) {
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
}

/// What `write_strikes` wrote, after its kind byte: the first seed, and each strike with the
/// structure it names (its id left 0, for whoever finds it).
pub fn read_strikes(r: &mut Reader, out: &mut Vec<(Named, Strike)>) -> Result<u64, WireError> {
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
    Ok(seed)
}

/// Where what was seen was told from: the world, or beside a structure every game has (where
/// it was and how fast it went when it was seen, as its sender had it).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum From {
    World,
    Beside { named: Named, pos: DVec3, vel: DVec3 },
}

/// Things seen (`Seen`) into `w`: when (the server's clock), told from where. Places beside a
/// structure go as offsets from it, speeds as what they add to its own.
pub fn write_seen(w: &mut Writer, stamp: f64, from: From, seen: &[Seen]) {
    w.u8(SEEN);
    w.f64(stamp);
    let (base, base_vel) = match from {
        From::World => {
            w.var(0);
            (None, DVec3::ZERO)
        }
        From::Beside { named, pos, vel } => {
            w.var(named.code() + 1);
            (Some(pos), vel)
        }
    };
    let place = |w: &mut Writer, p: DVec3| match base {
        Some(b) => w.vec3((p - b).as_vec3()),
        None => {
            w.f64(p.x);
            w.f64(p.y);
            w.f64(p.z);
        }
    };
    w.var(seen.len() as u64);
    for s in seen {
        match s {
            Seen::Round { shot, from, dir, speed, vel } => {
                w.u8(0);
                w.u16(*shot);
                place(w, *from);
                w.vec3(dir.as_vec3());
                w.f32(*speed);
                w.vec3((*vel - base_vel).as_vec3());
            }
            Seen::Missile { kind, from, to } => {
                w.u8(1);
                w.u16(*kind);
                place(w, *from);
                place(w, *to);
            }
            Seen::Boom { id, at, vel, scale, extra } => {
                w.u8(2);
                w.str(id);
                place(w, *at);
                w.vec3((*vel - base_vel).as_vec3());
                w.f32(*scale);
                w.f32(*extra);
            }
        }
    }
}

/// What `write_seen` wrote, after its kind byte, as this game must show it now: `now` is the
/// server's clock here; `find` where a named structure is here and how fast it goes (none: not
/// here, and what was told beside it is not shown). Each thing comes with how long ago it was.
/// Told beside a structure, it is placed beside this game's copy of it as it is now.
pub fn read_seen(r: &mut Reader, now: f64, find: impl Fn(Named) -> Option<(DVec3, DVec3)>, out: &mut Vec<(Seen, f32)>) -> Result<(), WireError> {
    let stamp = r.f64()?;
    let age = (now - stamp).clamp(0.0, 1.0);
    let code = r.var()?;
    // (beside something: where it was then, as this game has it now and goes back by its speed)
    let (base, base_vel) = match code {
        0 => (None, DVec3::ZERO),
        c => match find(Named::of(c - 1)) {
            Some((pos, vel)) => (Some(pos - vel * age), vel),
            None => return Ok(()),
        },
    };
    let place = |r: &mut Reader| -> Result<DVec3, WireError> {
        Ok(match base {
            Some(b) => b + r.vec3()?.as_dvec3(),
            None => DVec3::new(r.f64()?, r.f64()?, r.f64()?),
        })
    };
    let n = r.var()?;
    if n > SEEN_EACH as u64 * 4 {
        return Err(WireError::Long);
    }
    for _ in 0..n {
        let s = match r.u8()? {
            0 => {
                let shot = r.u16()?;
                let from = place(r)?;
                let dir = r.vec3()?.as_dvec3().normalize_or(DVec3::Z);
                let speed = r.f32()?;
                let vel = r.vec3()?.as_dvec3() + base_vel;
                Seen::Round { shot, from, dir, speed, vel }
            }
            1 => {
                let kind = r.u16()?;
                let from = place(r)?;
                let to = place(r)?;
                Seen::Missile { kind, from, to }
            }
            2 => {
                let id = r.str(64)?.to_string();
                let at = place(r)?;
                let vel = r.vec3()?.as_dvec3() + base_vel;
                Seen::Boom { id, at, vel, scale: r.f32()?, extra: r.f32()? }
            }
            _ => return Err(WireError::Value),
        };
        out.push((s, age as f32));
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

/// What a hand did to ship `ship` that is not a control (a door, a clamp: `aboard::Act`).
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
        for n in [Named::Ship(0), Named::Ship(77), Named::Built(1), Named::Built(1 << 40)] {
            assert_eq!(Named::of(n.code()), n);
        }
    }

    #[test]
    fn strikes_go_and_come_back_as_they_were() {
        let hit = Hit { point: Vec3::new(1.5, -2.0, 7.25), dir: Vec3::new(0.0, 0.6, -0.8), energy: 4.2e5, radius: 3.0, area: 0.002 };
        let strikes = vec![(Named::Ship(3), Strike::Hit { id: 99, hit }), (Named::Built(12), Strike::Blow { id: 5, part: 41, push: Vec3::new(0.0, 0.0, 1.0), energy: 2e4 })];
        let bytes = written(|w| write_strikes(w, 0xdead_beef, &strikes));
        let mut r = Reader::new(&bytes);
        assert_eq!(r.u8(), Ok(STRIKES));
        let mut back = Vec::new();
        assert_eq!(read_strikes(&mut r, &mut back), Ok(0xdead_beef));
        assert!(r.is_empty());
        assert_eq!(back[0], (Named::Ship(3), Strike::Hit { id: 0, hit }));
        assert_eq!(back[1], (Named::Built(12), Strike::Blow { id: 0, part: 41, push: Vec3::Z, energy: 2e4 }));
        // (one hit is 38 bytes: a gun hitting 80 times a second is 3 kB/s while it hits)
        assert!(written(|w| write_strikes(w, 1, &strikes[..1])).len() <= 42);
        // cut short or with nonsense in it, it says so and never panics
        for cut in 1..bytes.len() {
            let mut r = Reader::new(&bytes[1..cut]);
            let _ = read_strikes(&mut r, &mut Vec::new());
        }
    }

    #[test]
    fn what_is_seen_beside_a_ship_at_orbital_speed_leaves_its_muzzle_in_every_game() {
        // the shooter's ship at 1.7 km/s; the round leaves 4 m ahead of it at 1 km/s more
        let ship = (DVec3::new(1.738e6, 2.0e4, -3.0e5), DVec3::new(1700.0, 30.0, -12.0));
        let muzzle = ship.0 + DVec3::new(0.0, 0.0, 4.0);
        let dir = DVec3::new(0.0, 0.0, 1.0);
        let seen = vec![
            Seen::Round { shot: 2, from: muzzle, dir, speed: 1000.0, vel: ship.1 },
            Seen::Boom { id: "impacto".into(), at: muzzle + DVec3::X * 30.0, vel: ship.1, scale: 1.0, extra: 0.0 },
            Seen::Missile { kind: 1, from: muzzle, to: muzzle + DVec3::Z * 900.0 },
        ];
        let bytes = written(|w| write_seen(w, 100.0, From::Beside { named: Named::Ship(4), pos: ship.0, vel: ship.1 }, &seen));
        // another game, whose clock is 40 ms later and whose copy of the ship is 6 m off where
        // the shooter has it (a copy carried on by its speed): the round leaves its muzzle there
        let later = 0.04;
        let theirs = (ship.0 + ship.1 * later + DVec3::new(6.0, 0.0, 0.0), ship.1);
        let mut r = Reader::new(&bytes);
        assert_eq!(r.u8(), Ok(SEEN));
        let mut out = Vec::new();
        read_seen(&mut r, 100.0 + later, |n| (n == Named::Ship(4)).then_some(theirs), &mut out).unwrap();
        assert!(r.is_empty() && out.len() == 3);
        let (Seen::Round { from, vel, .. }, age) = &out[0] else { panic!() };
        assert!((f64::from(*age) - later).abs() < 1e-6);
        // where it is now, by what `Blasts::show` makes of it: beside their copy, as far ahead as it flew
        let now = *from + (*vel + dir * 1000.0) * f64::from(*age);
        let want = theirs.0 + DVec3::new(0.0, 0.0, 4.0) + dir * 1000.0 * later;
        assert!(now.distance(want) < 0.01, "{:.3} m from where it must be", now.distance(want));
        assert!(vel.distance(ship.1) < 1e-3);
        // a ship this game does not have: nothing is shown
        let mut out = Vec::new();
        read_seen(&mut Reader::new(&bytes[1..]), 100.0, |_| None, &mut out).unwrap();
        assert!(out.is_empty());
        // told from the world: as it was (the far side of the Moon is no problem for an f64)
        let bytes = written(|w| write_seen(w, 5.0, From::World, &seen[..1]));
        let mut out = Vec::new();
        read_seen(&mut Reader::new(&bytes[1..]), 5.0, |_| None, &mut out).unwrap();
        assert!(matches!(out[0].0, Seen::Round { from, .. } if from == muzzle));
        // a full message fits a datagram
        let many: Vec<Seen> = (0..SEEN_EACH).map(|_| seen[0].clone()).collect();
        assert!(written(|w| write_seen(w, 1.0, From::Beside { named: Named::Ship(4), pos: ship.0, vel: ship.1 }, &many)).len() <= lunar_net::MAX_HINT);
        for cut in 1..bytes.len() {
            let _ = read_seen(&mut Reader::new(&bytes[1..cut]), 5.0, |_| None, &mut Vec::new());
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
