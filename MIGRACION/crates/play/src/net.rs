//! What a player's game and a server that has the game say to each other (protocol 2,
//! `docs/PLAN_AUTORITATIVO.md` §3.8). The network (`lunar_net`) carries it as it is, without
//! reading it: `Msg::Quick` (unreliable) and `Msg::Game` (reliable, in order).
//!
//! | Message | Way | How | What |
//! |---|---|---|---|
//! | `CMDS` | to the server | loose, each one repeating the last few | what the player asks of each step (`Cmd`), and what its own game made of the body at a step (its digest), to be compared |
//! | `ACT` | to the server | sure, in order, each with its step | what the player means to do that the server must check first (`Act`) |
//! | `SNAP` | to a player | loose | the step it is of, the newest step of the player's it took and how early it came, the others as they are, the things that move |
//! | `EVENTS` | to a player | sure, in order | what happened (`Event`): what came into being or is gone, what was struck, what was let fly and ended, what hands did to ships, what ships said, and the body put right when the player's game went astray |
//!
//! Every structure is named by the server's id for it (`NetId`): the ids of what the scenario sets
//! are the same in every game; what is made later the server names, and a game that made the
//! same thing itself (a piece off a strike) finds it by its lineage.
use crate::{
    blasts::{Launch, What},
    controls::Act as Hand,
    pilot::Input,
    told,
};
use glam::{DVec3, Quat, Vec3};
use lunar_net::{PlayerState, Reader, RigidState, WireError, Writer};

/// What travels: the kind of a message, its first byte.
pub const CMDS: u8 = 1;
pub const ACT: u8 = 2;
pub const SNAP: u8 = 3;
pub const EVENTS: u8 = 4;

/// What the server and the players' games say to each other, besides the network's own: said
/// in the hello after the build (`V41+p2`). Another version of this is not let in.
pub const PROTOCOL: &str = "p2";

/// The commands a `CMDS` message repeats (one lost datagram, or three, loses nothing).
pub const REPEAT: usize = 4;

/// Steps per radian of the look as it travels (what a step is given, here and in the server).
const LOOK_UNITS: f64 = 1_048_576.0;
/// Steps of each of the keys' axes (-1..1).
const AXIS_UNITS: f64 = 127.0;

type Wire<T> = Result<T, WireError>;

/// What a player asks of one step: the keys, the look, and what the others need to draw them that
/// no step makes (the head's own turn, what is in hand, a gesture).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Cmd {
    pub step: u64,
    pub input: Input,
    /// The body's look (`Pilot::yaw`, `pitch`, rad).
    pub yaw: f64,
    pub pitch: f64,
    /// The way the player acts along (what the hands reach for, a test key's aim), if it is not
    /// the body's own look (from outside, the middle of the picture).
    pub aim: Option<Vec3>,
    pub head: [f32; 2],
    pub tool: u8,
    pub trigger: bool,
    /// Seen from outside (the others draw the whole body).
    pub outside: bool,
    pub gesture: u8,
}

impl Cmd {
    /// The command as it arrives: its numbers as they travel (what both games step with, so that
    /// the server's step and the player's own are the same step).
    pub fn travelled(mut self) -> Cmd {
        let axis = |v: f64| (v * AXIS_UNITS).round().clamp(-AXIS_UNITS, AXIS_UNITS) / AXIS_UNITS;
        let look = |v: f64| (v * LOOK_UNITS).round() / LOOK_UNITS;
        let i = &mut self.input;
        (i.forward, i.side, i.vertical, i.roll) = (axis(i.forward), axis(i.side), axis(i.vertical), axis(i.roll));
        (self.yaw, self.pitch) = (look(self.yaw), look(self.pitch));
        self.head = self.head.map(|h| ((f64::from(h) * HEAD_UNITS).round().clamp(-32767.0, 32767.0) / HEAD_UNITS) as f32);
        self.aim = self.aim.map(|a| a.normalize_or(Vec3::Z));
        self
    }

    fn flags(&self) -> u8 {
        let i = &self.input;
        u8::from(i.run) | u8::from(i.boost) << 1 | u8::from(i.jump) << 2 | u8::from(i.crouch) << 3 | u8::from(self.trigger) << 4 | u8::from(self.outside) << 5
    }

    /// What differs from `than` (all of it if none), as a mask (`CHANGED_*`) and those fields:
    /// the newest command goes whole, each older one as what it changes of the one after it
    /// (most of the time, the look only).
    fn write(&self, than: Option<&Cmd>, w: &mut Writer) {
        let axes = |c: &Cmd| [c.input.forward, c.input.side, c.input.vertical, c.input.roll].map(|v| (v * AXIS_UNITS).round().clamp(-AXIS_UNITS, AXIS_UNITS) as i8);
        let look = |v: f64| (v * LOOK_UNITS).round() as i64;
        let head = |c: &Cmd| c.head.map(|h| (f64::from(h) * HEAD_UNITS).round().clamp(-32767.0, 32767.0) as i16);
        let mut mask = 0u8;
        let differs = |a: bool| if than.is_none() { true } else { a };
        if differs(than.is_some_and(|t| t.flags() != self.flags())) {
            mask |= CHANGED_FLAGS;
        }
        if differs(than.is_some_and(|t| axes(t) != axes(self))) {
            mask |= CHANGED_AXES;
        }
        if differs(than.is_some_and(|t| look(t.yaw) != look(self.yaw) || look(t.pitch) != look(self.pitch))) {
            mask |= CHANGED_LOOK;
        }
        if self.aim.is_some() {
            mask |= CHANGED_AIM;
        }
        if differs(than.is_some_and(|t| head(t) != head(self))) {
            mask |= CHANGED_HEAD;
        }
        if differs(than.is_some_and(|t| (t.tool, t.gesture) != (self.tool, self.gesture))) {
            mask |= CHANGED_HAND;
        }
        w.u8(mask);
        if mask & CHANGED_FLAGS != 0 {
            w.u8(self.flags());
        }
        if mask & CHANGED_AXES != 0 {
            for a in axes(self) {
                w.u8(a as u8);
            }
        }
        if mask & CHANGED_LOOK != 0 {
            let (y0, p0) = than.map_or((0, 0), |t| (look(t.yaw), look(t.pitch)));
            w.zig(look(self.yaw) - y0);
            w.zig(look(self.pitch) - p0);
        }
        if let Some(a) = self.aim {
            w.vec3(a);
        }
        if mask & CHANGED_HEAD != 0 {
            for h in head(self) {
                w.u16(h as u16);
            }
        }
        if mask & CHANGED_HAND != 0 {
            w.u8(self.tool);
            w.u8(self.gesture);
        }
    }

    /// What `write` wrote, the one after it being `than` (none: it went whole).
    fn read(r: &mut Reader, step: u64, than: Option<&Cmd>) -> Wire<Cmd> {
        let mask = r.u8()?;
        if than.is_none() && mask & (CHANGED_FLAGS | CHANGED_AXES | CHANGED_LOOK | CHANGED_HEAD | CHANGED_HAND) != CHANGED_FLAGS | CHANGED_AXES | CHANGED_LOOK | CHANGED_HEAD | CHANGED_HAND {
            return Err(WireError::Value);
        }
        let mut c = than.copied().unwrap_or_default();
        c.step = step;
        if mask & CHANGED_FLAGS != 0 {
            let flags = r.u8()?;
            let bit = |k: u8| flags & (1 << k) != 0;
            (c.input.run, c.input.boost, c.input.jump, c.input.crouch, c.trigger, c.outside) = (bit(0), bit(1), bit(2), bit(3), bit(4), bit(5));
        }
        if mask & CHANGED_AXES != 0 {
            let mut a = [0.0; 4];
            for x in &mut a {
                *x = f64::from(r.u8()? as i8) / AXIS_UNITS;
            }
            (c.input.forward, c.input.side, c.input.vertical, c.input.roll) = (a[0], a[1], a[2], a[3]);
        }
        if mask & CHANGED_LOOK != 0 {
            let (y0, p0) = than.map_or((0, 0), |t| ((t.yaw * LOOK_UNITS).round() as i64, (t.pitch * LOOK_UNITS).round() as i64));
            let (y, p) = (y0.checked_add(r.zig()?).ok_or(WireError::Value)?, p0.checked_add(r.zig()?).ok_or(WireError::Value)?);
            (c.yaw, c.pitch) = (y as f64 / LOOK_UNITS, p as f64 / LOOK_UNITS);
        }
        c.aim = if mask & CHANGED_AIM != 0 { Some(r.vec3()?.normalize_or(Vec3::Z)) } else { None };
        if mask & CHANGED_HEAD != 0 {
            c.head = [(r.u16()? as i16), (r.u16()? as i16)].map(|h| (f64::from(h) / HEAD_UNITS) as f32);
        }
        if mask & CHANGED_HAND != 0 {
            (c.tool, c.gesture) = (r.u8()?, r.u8()?);
        }
        Ok(c)
    }
}

/// What a command repeated says has changed from the one after it.
const CHANGED_FLAGS: u8 = 1;
const CHANGED_AXES: u8 = 2;
const CHANGED_LOOK: u8 = 4;
const CHANGED_AIM: u8 = 8;
const CHANGED_HEAD: u8 = 16;
const CHANGED_HAND: u8 = 32;
/// Steps per radian of the head's own turn (it only shows).
const HEAD_UNITS: f64 = 10_000.0;

/// The commands of the last steps, newest first (`cmds[0]` is of step `cmds[0].step`, each one a
/// step before the one ahead of it), and what the player's game made of its body at a step (its
/// `Pilot::digest`), into `out`.
pub fn write_cmds(cmds: &[Cmd], checked: Option<(u64, u64)>, out: &mut Vec<u8>) {
    framed(out, 64 + cmds.len() * 40, |w| {
        w.u8(CMDS);
        w.var(cmds.first().map_or(0, |c| c.step));
        w.u8(cmds.len().min(REPEAT) as u8);
        let mut than: Option<&Cmd> = None;
        for c in cmds.iter().take(REPEAT) {
            c.write(than, w);
            than = Some(c);
        }
        match checked {
            Some((step, digest)) => {
                w.var(step + 1);
                w.u64(digest);
            }
            None => w.var(0),
        }
    });
}

/// What `write_cmds` wrote, after its kind byte: the commands (newest first) into `out`, and the
/// digest checked, if any.
pub fn read_cmds(r: &mut Reader, out: &mut Vec<Cmd>) -> Wire<Option<(u64, u64)>> {
    let newest = r.var()?;
    let n = r.u8()?;
    if usize::from(n) > REPEAT || u64::from(n) > newest + 1 {
        return Err(WireError::Value);
    }
    let from = out.len();
    for k in 0..u64::from(n) {
        let than = (k > 0).then(|| out[out.len() - 1]);
        out.push(Cmd::read(r, newest - k, than.as_ref())?);
    }
    debug_assert!(out.len() - from == usize::from(n));
    Ok(match r.var()? {
        0 => None,
        s => Some((s - 1, r.u64()?)),
    })
}

/// What a player means to do that the server must check before it is so (`PLAN_AUTORITATIVO.md`
/// §3.11): what the player's game decided from where the crosshair is, the mouse and the keys,
/// as it decided it.
#[derive(Clone, Debug, PartialEq)]
pub enum Act {
    /// A control of a ship left at a value.
    Control { ship: u64, control: u16, value: f64 },
    /// A door or a clamp of a ship worked by hand.
    Hand { ship: u64, act: Hand },
    /// Sit in seat `seat` of the ship on `ship`; get up from the seat.
    Sit { ship: u64, seat: u16 },
    Stand,
    /// Bare hands: take hold of what the look is on; let go; bring it nearer or farther.
    Grab,
    Release,
    Wheel(f32),
    /// Something let fly (a tool in hand, a test key): from where and how.
    Launch(Launch),
    /// The welder at work on part `part` of `structure`: mended by `hp`, or put back.
    Mend { structure: u64, part: u32, hp: f32 },
    Rebuild { structure: u64, part: u32 },
    /// The suit's switches.
    Pack(bool),
    Steady(bool),
    Lamps(bool),
    /// Free flight (a test).
    Fly(bool),
    /// A ship of kind `kind` put where it is told (a test).
    Spawn { kind: String, pos: DVec3, rot: Quat },
}

fn write_launch(w: &mut Writer, l: &Launch) {
    let (c, i) = match l.what {
        What::Shot(i) => (0, i),
        What::Missile(i) => (1, i),
        What::Guided(i) => (2, i),
        What::Decoy(i) => (3, i),
        What::Boom(i) => (4, i),
    };
    w.u8(c);
    w.u16(i);
    for v in [l.from, l.dir, l.vel] {
        w.f64(v.x);
        w.f64(v.y);
        w.f64(v.z);
    }
    w.f32(l.speed);
    w.var(l.target.map_or(0, |t| t + 1));
    w.var(l.by.map_or(0, |b| b + 1));
}

fn read_launch(r: &mut Reader) -> Wire<Launch> {
    let (c, i) = (r.u8()?, r.u16()?);
    let what = match c {
        0 => What::Shot(i),
        1 => What::Missile(i),
        2 => What::Guided(i),
        3 => What::Decoy(i),
        4 => What::Boom(i),
        _ => return Err(WireError::Value),
    };
    let mut v = [DVec3::ZERO; 3];
    for x in &mut v {
        *x = DVec3::new(r.f64()?, r.f64()?, r.f64()?);
        if !x.is_finite() {
            return Err(WireError::Value);
        }
    }
    let speed = r.f32()?;
    let opt = |n: u64| if n == 0 { None } else { Some(n - 1) };
    let (target, by) = (opt(r.var()?), opt(r.var()?));
    Ok(Launch { what, from: v[0], dir: v[1].normalize_or(DVec3::Z), vel: v[2], speed: if speed.is_finite() { speed } else { 0.0 }, target, by })
}

/// An act of step `step` into `out`.
pub fn write_act(step: u64, act: &Act, out: &mut Vec<u8>) {
    framed(out, 256, |w| {
        w.u8(ACT);
        w.var(step);
        match act {
            Act::Control { ship, control, value } => told::write_control(w, *ship, *control, *value),
            Act::Hand { ship, act } => told::write_act(w, *ship, *act),
            Act::Sit { ship, seat } => {
                w.u8(10);
                w.var(*ship);
                w.u16(*seat);
            }
            Act::Stand => w.u8(11),
            Act::Grab => w.u8(12),
            Act::Release => w.u8(13),
            Act::Wheel(n) => {
                w.u8(14);
                w.f32(*n);
            }
            Act::Launch(l) => {
                w.u8(15);
                write_launch(w, l);
            }
            Act::Mend { structure, part, hp } => {
                w.u8(16);
                w.var(*structure);
                w.var(u64::from(*part));
                w.f32(*hp);
            }
            Act::Rebuild { structure, part } => {
                w.u8(17);
                w.var(*structure);
                w.var(u64::from(*part));
            }
            Act::Pack(on) => {
                w.u8(18);
                w.u8(u8::from(*on));
            }
            Act::Steady(on) => {
                w.u8(19);
                w.u8(u8::from(*on));
            }
            Act::Lamps(on) => {
                w.u8(20);
                w.u8(u8::from(*on));
            }
            Act::Fly(on) => {
                w.u8(21);
                w.u8(u8::from(*on));
            }
            Act::Spawn { kind, pos, rot } => {
                w.u8(22);
                w.str(kind);
                w.f64(pos.x);
                w.f64(pos.y);
                w.f64(pos.z);
                for v in rot.to_array() {
                    w.f32(v);
                }
            }
        }
    });
}

/// What `write_act` wrote, after its kind byte: its step and the act.
pub fn read_act(r: &mut Reader) -> Wire<(u64, Act)> {
    let step = r.var()?;
    let on = |r: &mut Reader| -> Wire<bool> {
        match r.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(WireError::Value),
        }
    };
    let act = match r.u8()? {
        told::CONTROL => {
            let (ship, control, value) = told::read_control(r)?;
            if !value.is_finite() {
                return Err(WireError::Value);
            }
            Act::Control { ship, control, value }
        }
        told::ACT => {
            let (ship, act) = told::read_act(r)?;
            Act::Hand { ship, act }
        }
        10 => Act::Sit { ship: r.var()?, seat: r.u16()? },
        11 => Act::Stand,
        12 => Act::Grab,
        13 => Act::Release,
        14 => {
            let n = r.f32()?;
            if !n.is_finite() {
                return Err(WireError::Value);
            }
            Act::Wheel(n)
        }
        15 => Act::Launch(read_launch(r)?),
        16 => {
            let (structure, part, hp) = (r.var()?, r.var32()?, r.f32()?);
            if !hp.is_finite() {
                return Err(WireError::Value);
            }
            Act::Mend { structure, part, hp }
        }
        17 => Act::Rebuild { structure: r.var()?, part: r.var32()? },
        18 => Act::Pack(on(r)?),
        19 => Act::Steady(on(r)?),
        20 => Act::Lamps(on(r)?),
        21 => Act::Fly(on(r)?),
        22 => {
            let kind = r.str(64)?.to_string();
            let pos = DVec3::new(r.f64()?, r.f64()?, r.f64()?);
            let rot = Quat::from_xyzw(r.f32()?, r.f32()?, r.f32()?, r.f32()?);
            if !pos.is_finite() || !rot.is_finite() {
                return Err(WireError::Value);
            }
            Act::Spawn { kind, pos, rot: rot.normalize() }
        }
        _ => return Err(WireError::Value),
    };
    Ok((step, act))
}

/// A snapshot: the step it is of, what of the player's was taken, and what moves.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Snap {
    pub step: u64,
    /// The newest step of this player's commands the server has (0: none yet), and by how many
    /// steps its command for `step` came ahead of when it was needed (less than 0: too late, it
    /// was guessed).
    pub took: u64,
    pub ahead: i32,
    /// The other players, as they are (`PlayerState`, by their id on the network).
    pub players: Vec<(u32, PlayerState)>,
    /// The structures that move, as they are (each `RigidState`'s id is the server's).
    pub things: Vec<RigidState>,
}

/// A snapshot into `out` (as much of it as fits in `room` bytes: what is left out goes next time).
pub fn write_snap(s: &Snap, room: usize, out: &mut Vec<u8>) -> usize {
    let mut things = 0;
    framed(out, room, |w| {
        w.u8(SNAP);
        w.var(s.step);
        w.var(s.took);
        w.zig(i64::from(s.ahead));
        w.var(s.players.len() as u64);
        for (id, p) in &s.players {
            w.var(u64::from(*id));
            p.encode(w);
        }
        // (the things to the end of the message: as many as fit)
        for t in &s.things {
            let before = w.mark();
            t.encode(w);
            if !w.ok() {
                w.rewind(before);
                break;
            }
            things += 1;
        }
    });
    things
}

/// What `write_snap` wrote, after its kind byte.
pub fn read_snap(r: &mut Reader, out: &mut Snap) -> Wire<()> {
    out.players.clear();
    out.things.clear();
    out.step = r.var()?;
    out.took = r.var()?;
    out.ahead = i32::try_from(r.zig()?).map_err(|_| WireError::Value)?;
    let n = r.var()?;
    if n > 1024 {
        return Err(WireError::Long);
    }
    for _ in 0..n {
        let id = r.var32()?;
        out.players.push((id, PlayerState::decode(r)?));
    }
    while !r.is_empty() {
        out.things.push(RigidState::decode(r)?);
    }
    Ok(())
}

/// What happened, as a player's game is told of it.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// You are in: the step the game is at, your id on the network, toward the sun.
    Hello { step: u64, you: u32, sun: DVec3 },
    /// Your body as the server has it at the end of step `step` (`Pilot::write_state`): your game
    /// went astray there; put it so and step on from there with what you asked since.
    Correct { step: u64, state: Vec<u8> },
    /// Structure `id` came into being (or into your knowing): of what it is made (`sync::write_make`),
    /// what is left of it (`sync::write_state`), how it moves, and its systems if it is a ship of
    /// kind `ship` (`sync::write_ship`).
    Made { id: u64, lineage: u64, ship: String, rigid: RigidState, make: Vec<u8>, state: Vec<u8>, systems: Vec<u8> },
    /// Structure `id` is gone (or out of your knowing).
    Gone { id: u64 },
    /// What was done to structures, as `told::STRIKES` says it (each named by its id).
    Strikes(Vec<u8>),
    /// What was let fly and where it ended, as `told::SEEN` says it.
    Seen(Vec<u8>),
    /// What a hand did to a ship (a control, a door, a clamp).
    Control { ship: u64, control: u16, value: f64 },
    Hand { ship: u64, act: Hand },
    /// What a ship told whoever works it.
    Said { ship: u64, about: String, text: String, level: u8 },
    /// A ship's systems as they are (`sync::write_ship`): yours were found to differ.
    Systems { ship: u64, data: Vec<u8> },
    /// What you asked could not be: why.
    Denied(String),
}

fn blob(w: &mut Writer, b: &[u8]) {
    w.var(b.len() as u64);
    w.bytes(b);
}

fn read_blob(r: &mut Reader) -> Wire<Vec<u8>> {
    let n = r.var()?;
    if n > lunar_net::MAX_TELL as u64 {
        return Err(WireError::Long);
    }
    Ok(r.bytes(n as usize)?.to_vec())
}

/// Events into `out`, as one message (the caller keeps it under `lunar_net::MAX_TELL`).
pub fn write_events(events: &[Event], out: &mut Vec<u8>) {
    let room = 64 + events.iter().map(|e| match e {
        Event::Correct { state, .. } => state.len() + 16,
        Event::Made { make, state, systems, .. } => make.len() + state.len() + systems.len() + 256,
        Event::Strikes(b) | Event::Seen(b) => b.len() + 8,
        Event::Systems { data, .. } => data.len() + 16,
        Event::Said { about, text, .. } => about.len() + text.len() + 32,
        Event::Denied(t) => t.len() + 8,
        _ => 64,
    }).sum::<usize>();
    framed(out, room, |w| {
        w.u8(EVENTS);
        w.var(events.len() as u64);
        for e in events {
            match e {
                Event::Hello { step, you, sun } => {
                    w.u8(1);
                    w.var(*step);
                    w.var(u64::from(*you));
                    w.f64(sun.x);
                    w.f64(sun.y);
                    w.f64(sun.z);
                }
                Event::Correct { step, state } => {
                    w.u8(2);
                    w.var(*step);
                    blob(w, state);
                }
                Event::Made { id, lineage, ship, rigid, make, state, systems } => {
                    w.u8(3);
                    w.var(*id);
                    w.var(*lineage);
                    w.str(ship);
                    rigid.encode(w);
                    blob(w, make);
                    blob(w, state);
                    blob(w, systems);
                }
                Event::Gone { id } => {
                    w.u8(4);
                    w.var(*id);
                }
                Event::Strikes(b) => {
                    w.u8(5);
                    blob(w, b);
                }
                Event::Seen(b) => {
                    w.u8(6);
                    blob(w, b);
                }
                Event::Control { ship, control, value } => {
                    w.u8(7);
                    told::write_control(w, *ship, *control, *value);
                }
                Event::Hand { ship, act } => {
                    w.u8(8);
                    told::write_act(w, *ship, *act);
                }
                Event::Said { ship, about, text, level } => {
                    w.u8(9);
                    w.var(*ship);
                    w.str(about);
                    w.str(text);
                    w.u8(*level);
                }
                Event::Systems { ship, data } => {
                    w.u8(10);
                    w.var(*ship);
                    blob(w, data);
                }
                Event::Denied(why) => {
                    w.u8(11);
                    w.str(why);
                }
            }
        }
    });
}

/// What `write_events` wrote, after its kind byte, into `out`.
pub fn read_events(r: &mut Reader, out: &mut Vec<Event>) -> Wire<()> {
    let n = r.var()?;
    if n > 4096 {
        return Err(WireError::Long);
    }
    for _ in 0..n {
        let e = match r.u8()? {
            1 => {
                let (step, you) = (r.var()?, r.var32()?);
                let sun = DVec3::new(r.f64()?, r.f64()?, r.f64()?);
                Event::Hello { step, you, sun }
            }
            2 => Event::Correct { step: r.var()?, state: read_blob(r)? },
            3 => {
                let (id, lineage) = (r.var()?, r.var()?);
                let ship = r.str(64)?.to_string();
                let rigid = RigidState::decode(r)?;
                Event::Made { id, lineage, ship, rigid, make: read_blob(r)?, state: read_blob(r)?, systems: read_blob(r)? }
            }
            4 => Event::Gone { id: r.var()? },
            5 => Event::Strikes(read_blob(r)?),
            6 => Event::Seen(read_blob(r)?),
            7 => {
                if r.u8()? != told::CONTROL {
                    return Err(WireError::Value);
                }
                let (ship, control, value) = told::read_control(r)?;
                Event::Control { ship, control, value }
            }
            8 => {
                if r.u8()? != told::ACT {
                    return Err(WireError::Value);
                }
                let (ship, act) = told::read_act(r)?;
                Event::Hand { ship, act }
            }
            9 => Event::Said { ship: r.var()?, about: r.str(256)?.to_string(), text: r.str(1024)?.to_string(), level: r.u8()? },
            10 => Event::Systems { ship: r.var()?, data: read_blob(r)? },
            11 => Event::Denied(r.str(1024)?.to_string()),
            _ => return Err(WireError::Value),
        };
        out.push(e);
    }
    Ok(())
}

/// Writes into `out` (made as long as `room` first, then cut to what was written; left empty if
/// it did not fit) with a `Writer`.
pub fn framed(out: &mut Vec<u8>, room: usize, f: impl FnOnce(&mut Writer)) {
    out.clear();
    out.resize(room, 0);
    let mut w = Writer::new(out);
    f(&mut w);
    let n = w.finish().unwrap_or(0);
    out.truncate(n);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_go_and_come_back_as_they_travel() {
        let cmds: Vec<Cmd> = (0..4)
            .map(|k| Cmd {
                step: 1000 - k,
                input: Input { forward: 0.7, side: -1.0, vertical: 0.0, run: k % 2 == 0, boost: false, jump: k == 1, crouch: true, roll: 0.25 },
                yaw: 1.234_567 + k as f64,
                pitch: -0.3,
                aim: (k == 2).then_some(Vec3::new(0.0, 0.6, 0.8)),
                head: [0.1, -0.2],
                tool: 2,
                trigger: k == 3,
                outside: false,
                gesture: 4,
            })
            .collect();
        let mut out = Vec::new();
        write_cmds(&cmds, Some((996, 0xdead_beef)), &mut out);
        let mut r = Reader::new(&out);
        assert_eq!(r.u8().unwrap(), CMDS);
        let mut back = Vec::new();
        let checked = read_cmds(&mut r, &mut back).unwrap();
        assert!(r.is_empty());
        assert_eq!(checked, Some((996, 0xdead_beef)));
        for (a, b) in cmds.iter().zip(&back) {
            assert_eq!(a.travelled(), *b);
        }
        // (what both step with is the same: travelled twice is travelled once)
        assert_eq!(back[0].travelled(), back[0]);
        // as they come when playing: walking and looking round, a jump in the middle. The newest
        // whole, the other three each what it changes of the one after it
        let walking: Vec<Cmd> = (0..4)
            .map(|k| Cmd { step: 2000 - k, input: Input { forward: 1.0, run: true, jump: k == 1, ..Input::default() }, yaw: 0.5 + k as f64 * 0.003, pitch: -0.1, ..Cmd::default() })
            .collect();
        write_cmds(&walking, Some((1996, 1)), &mut out);
        assert!(out.len() <= 48, "{} bytes for four commands", out.len());
        let mut r = Reader::new(&out);
        r.u8().unwrap();
        let mut back = Vec::new();
        read_cmds(&mut r, &mut back).unwrap();
        assert_eq!(back, walking.iter().map(|c| c.travelled()).collect::<Vec<_>>());
    }

    #[test]
    fn acts_and_events_go_and_come_back() {
        let launch = Launch { what: What::Shot(3), from: DVec3::new(4e6, 3e6, -2e6), dir: DVec3::Z, speed: 150.0, vel: DVec3::new(7800.0, 0.0, 1.0), target: None, by: Some(42) };
        let acts = [
            Act::Control { ship: 7, control: 12, value: 0.75 },
            Act::Hand { ship: 7, act: Hand::Closure(2, true) },
            Act::Sit { ship: 7, seat: 1 },
            Act::Stand,
            Act::Grab,
            Act::Release,
            Act::Wheel(-2.0),
            Act::Launch(launch),
            Act::Mend { structure: 9, part: 31, hp: 12.5 },
            Act::Rebuild { structure: 9, part: 32 },
            Act::Pack(true),
            Act::Steady(false),
            Act::Lamps(true),
            Act::Fly(false),
            Act::Spawn { kind: "alcotan".into(), pos: DVec3::new(1.0, 2.0, 3.0), rot: Quat::IDENTITY },
        ];
        for (k, a) in acts.iter().enumerate() {
            let mut out = Vec::new();
            write_act(500 + k as u64, a, &mut out);
            let mut r = Reader::new(&out);
            assert_eq!(r.u8().unwrap(), ACT);
            assert_eq!(read_act(&mut r).unwrap(), (500 + k as u64, a.clone()));
            assert!(r.is_empty());
        }
        let events = vec![
            Event::Hello { step: 99, you: 3, sun: DVec3::Y },
            Event::Correct { step: 98, state: vec![1, 2, 3] },
            Event::Made { id: 77, lineage: 5, ship: "abejorro".into(), rigid: RigidState { id: 77, pos: DVec3::new(1.0, 2.0, 3.0), ..RigidState::default() }, make: vec![9; 40], state: vec![], systems: vec![1] },
            Event::Gone { id: 77 },
            Event::Strikes(vec![3, 1, 2]),
            Event::Seen(vec![4]),
            Event::Control { ship: 7, control: 3, value: 1.0 },
            Event::Hand { ship: 7, act: Hand::Clamp(0, false) },
            Event::Said { ship: 7, about: "reactor".into(), text: "en línea".into(), level: 1 },
            Event::Systems { ship: 7, data: vec![0; 300] },
            Event::Denied("no llegas".into()),
        ];
        let mut out = Vec::new();
        write_events(&events, &mut out);
        let mut r = Reader::new(&out);
        assert_eq!(r.u8().unwrap(), EVENTS);
        let mut back = Vec::new();
        read_events(&mut r, &mut back).unwrap();
        assert_eq!(back, events);
    }

    #[test]
    fn a_snapshot_that_does_not_fit_leaves_what_does_not_for_next_time() {
        let things: Vec<RigidState> = (0..400).map(|k| RigidState { id: k, pos: DVec3::new(4e6 + k as f64, 3e6, 2e6), vel: Vec3::X, ..RigidState::default() }).collect();
        let snap = Snap { step: 5, took: 4, ahead: 2, players: vec![(1, PlayerState::default())], things };
        let mut out = Vec::new();
        let sent = write_snap(&snap, 1100, &mut out);
        assert!(sent > 10 && sent < 400, "{sent} sent");
        assert!(out.len() <= 1100);
        let mut r = Reader::new(&out);
        assert_eq!(r.u8().unwrap(), SNAP);
        let mut back = Snap::default();
        read_snap(&mut r, &mut back).unwrap();
        assert_eq!(back.things.len(), sent);
        assert_eq!((back.step, back.took, back.ahead), (5, 4, 2));
        assert_eq!(back.things[..], snap.things[..sent]);
    }

    #[test]
    fn garbage_is_refused_without_a_panic() {
        let mut seed = 7u64;
        for n in 0..20_000 {
            let len = n % 97;
            let bytes: Vec<u8> = (0..len)
                .map(|_| {
                    seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                    (seed >> 56) as u8
                })
                .collect();
            let mut r = Reader::new(&bytes);
            let _ = read_cmds(&mut r, &mut Vec::new());
            let mut r = Reader::new(&bytes);
            let _ = read_act(&mut r);
            let mut r = Reader::new(&bytes);
            let _ = read_snap(&mut r, &mut Snap::default());
            let mut r = Reader::new(&bytes);
            let _ = read_events(&mut r, &mut Vec::new());
        }
    }
}
