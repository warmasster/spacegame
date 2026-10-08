//! What a player's game and a server that has the game say to each other (protocol 2,
//! `docs/PLAN_AUTORITATIVO.md` §3.8). The network (`lunar_net`) carries it as it is, without
//! reading it: `Msg::Quick` (unreliable) and `Msg::Game` (reliable, in order).
//!
//! | Message | Way | How | What |
//! |---|---|---|---|
//! | `CMDS` | to the server | loose, each one repeating the last few | what the player asks of each step (`Cmd`), and what its own game made of the body at a step (`Pilot::summary`), to be compared |
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
    pilot::{Input, Summary},
    told,
};
use glam::{DVec3, Quat, Vec3};
use lunar_core::structure::hold::Held;
use lunar_net::{PlayerState, Reader, RigidState, WireError, Writer};
use lunar_ship::sync::Digest;

/// What travels: the kind of a message, its first byte.
pub const CMDS: u8 = 1;
pub const ACT: u8 = 2;
pub const SNAP: u8 = 3;
pub const EVENTS: u8 = 4;
pub const TRACKS: u8 = 5;

/// What a player's game says it is in its hello to a server that has the game (and the
/// fingerprint of its data, `defs::fingerprint`, as the scenario): the server lets in only its own.
/// After the `+`, the version of what they say to each other here: a new event, act or field is
/// a new one.
pub const BUILD: &str = "V41+p8";

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
    /// The suit's switches as they are left (`Pilot::pack_on`, `steady`, `lamps`; free flight,
    /// a test, only where the server lets it).
    pub pack: bool,
    pub steady: bool,
    pub lamps: bool,
    pub fly: bool,
    /// Seated: which of the seat's keys are held (bit `k`: its `k`-th key, as
    /// `lunar_ship::seat_keys::keys` lists them).
    pub keys: u32,
    /// Floating where nothing weighs, the mouse turns the whole body: its way up and the way its
    /// turn is counted from (`Pilot::body_frame`), as the player left them.
    pub frame: Option<[Vec3; 2]>,
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
        // (made a frame first, then rounded: what travels is what it is, and travels so again)
        self.frame = self.frame.map(|[up, fore]| {
            let q = |v: Vec3| Vec3::from_array(v.to_array().map(|x| (x * FRAME_UNITS).round().clamp(-FRAME_UNITS, FRAME_UNITS) / FRAME_UNITS));
            let up = up.normalize_or(Vec3::Y);
            let fore = (fore - up * fore.dot(up)).normalize_or(up.any_orthonormal_vector());
            [q(up), q(fore)]
        });
        self
    }

    fn flags(&self) -> u64 {
        let i = &self.input;
        let bits = [i.run, i.boost, i.jump, i.crouch, self.trigger, self.outside, self.pack, self.steady, self.lamps, self.fly];
        bits.iter().enumerate().fold(0, |f, (k, b)| f | u64::from(*b) << k)
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
        if self.frame.is_some() {
            mask |= CHANGED_FRAME;
        }
        if differs(than.is_some_and(|t| head(t) != head(self))) {
            mask |= CHANGED_HEAD;
        }
        if differs(than.is_some_and(|t| (t.tool, t.gesture) != (self.tool, self.gesture))) {
            mask |= CHANGED_HAND;
        }
        if differs(than.is_some_and(|t| t.keys != self.keys)) {
            mask |= CHANGED_KEYS;
        }
        w.u8(mask);
        if mask & CHANGED_FLAGS != 0 {
            w.var(self.flags());
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
        if mask & CHANGED_KEYS != 0 {
            w.var(u64::from(self.keys));
        }
        if let Some(f) = self.frame {
            for v in f {
                for x in v.to_array() {
                    w.u16((x * FRAME_UNITS).round().clamp(-FRAME_UNITS, FRAME_UNITS) as i16 as u16);
                }
            }
        }
    }

    /// What `write` wrote, the one after it being `than` (none: it went whole).
    fn read(r: &mut Reader, step: u64, than: Option<&Cmd>) -> Wire<Cmd> {
        let mask = r.u8()?;
        if (than.is_none() && mask & WHOLE != WHOLE) || mask & !(WHOLE | CHANGED_AIM | CHANGED_FRAME) != 0 {
            return Err(WireError::Value);
        }
        let mut c = than.copied().unwrap_or_default();
        c.step = step;
        if mask & CHANGED_FLAGS != 0 {
            let flags = r.var()?;
            if flags >> 10 != 0 {
                return Err(WireError::Value);
            }
            let bit = |k: u8| flags & (1 << k) != 0;
            (c.input.run, c.input.boost, c.input.jump, c.input.crouch, c.trigger, c.outside) = (bit(0), bit(1), bit(2), bit(3), bit(4), bit(5));
            (c.pack, c.steady, c.lamps, c.fly) = (bit(6), bit(7), bit(8), bit(9));
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
        if mask & CHANGED_KEYS != 0 {
            c.keys = r.var32()?;
        }
        c.frame = if mask & CHANGED_FRAME != 0 {
            let mut f = [Vec3::ZERO; 2];
            for v in &mut f {
                *v = Vec3::new(f32::from(r.u16()? as i16), f32::from(r.u16()? as i16), f32::from(r.u16()? as i16)) / FRAME_UNITS;
            }
            Some(f)
        } else {
            None
        };
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
const CHANGED_KEYS: u8 = 64;
const CHANGED_FRAME: u8 = 128;
/// Steps of each axis of the body's frame as it travels.
const FRAME_UNITS: f32 = 32767.0;
/// What a command that goes whole has.
const WHOLE: u8 = CHANGED_FLAGS | CHANGED_AXES | CHANGED_LOOK | CHANGED_HEAD | CHANGED_HAND | CHANGED_KEYS;
/// Steps per radian of the head's own turn (it only shows).
const HEAD_UNITS: f64 = 10_000.0;

/// What a player's game made of its body at a step, for the server to compare with its own:
/// the step, the body at its end (`Pilot::summary`), and how many times the server had put the
/// body right (`Event::Correct`) when it was stepped (a word from before the last correction is
/// not compared: it was said of a body the server has since put right).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Check {
    pub step: u64,
    pub body: Summary,
    pub fixes: u8,
}

/// The commands of the last steps, newest first (`cmds[0]` is of step `cmds[0].step`, each one a
/// step before the one ahead of it), and what the player's game made of its body at a step, into
/// `out`.
pub fn write_cmds(cmds: &[Cmd], checked: Option<Check>, out: &mut Vec<u8>) {
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
            Some(c) => {
                w.var(c.step + 1);
                c.body.write(w);
                w.u8(c.fixes);
            }
            None => w.var(0),
        }
    });
}

/// What `write_cmds` wrote, after its kind byte: the commands (newest first) into `out`, and the
/// digest checked, if any.
pub fn read_cmds(r: &mut Reader, out: &mut Vec<Cmd>) -> Wire<Option<Check>> {
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
        s => Some(Check { step: s - 1, body: Summary::read(r)?, fixes: r.u8()? }),
    })
}

/// What a player means to do that the server must check before it is so (`PLAN_AUTORITATIVO.md`
/// §3.11): what the player's game decided from where the crosshair is, the mouse and the keys,
/// as it decided it.
#[derive(Clone, Debug, PartialEq)]
pub enum Act {
    /// A control of a ship left at a value.
    Control {
        ship: u64,
        control: u16,
        value: f64,
    },
    /// A door or a clamp of a ship worked by hand.
    Hand {
        ship: u64,
        act: Hand,
    },
    /// Sit in seat `seat` of the ship on `ship`; get up from the seat.
    Sit {
        ship: u64,
        seat: u16,
    },
    Stand,
    /// Bare hands: take hold of what the look is on; let go; bring it nearer or farther.
    Grab,
    Release,
    Wheel(f32),
    /// What the hand lets fly (`Blasts::launch`), with the number this game gave it: where it
    /// ends is told back to this game by that number (`blasts::OWN`), so it is not seen twice.
    Launch(Launch, u32),
    /// Part `part` of `structure` mended by `hp`, or put back: what a script or a tool asks (held
    /// to the welder in the hands and its rate). The window's welder says nothing of this: the
    /// server works it from the command, its trigger and where it is aimed (`weld`).
    Mend {
        structure: u64,
        part: u32,
        hp: f32,
    },
    Rebuild {
        structure: u64,
        part: u32,
    },
    /// A ship of kind `kind` put where it is told (a test).
    Spawn {
        kind: String,
        pos: DVec3,
        rot: Quat,
    },
    /// Structure `id` here does not agree with what the server says of it (`Snap::checks`): all
    /// of it again, please (`Event::Made`).
    Resync {
        id: u64,
    },
    /// I was here before, as the one `Event::Hello` gave this key to: my body, please (it waited
    /// for me, `HostConfig::keep`). Said first, before any command.
    Back {
        key: u64,
    },
    /// My body was put so by whoever runs my game (a menu's «start here», a script):
    /// `Pilot::write_state`. Only where tests are let be (`HostConfig::cheats`: a game of one's
    /// own, a test server).
    Body(Vec<u8>),
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
    let room = 256
        + match act {
            Act::Body(b) => b.len(),
            Act::Spawn { kind, .. } => kind.len(),
            _ => 0,
        };
    framed(out, room, |w| {
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
            Act::Launch(l, tag) => {
                w.u8(15);
                write_launch(w, l);
                w.var(u64::from(*tag));
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
            Act::Resync { id } => {
                w.u8(23);
                w.var(*id);
            }
            Act::Back { key } => {
                w.u8(24);
                w.u64(*key);
            }
            Act::Body(state) => {
                w.u8(25);
                blob(w, state);
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
        15 => Act::Launch(read_launch(r)?, r.var32()?),
        16 => {
            let (structure, part, hp) = (r.var()?, r.var32()?, r.f32()?);
            if !hp.is_finite() {
                return Err(WireError::Value);
            }
            Act::Mend { structure, part, hp }
        }
        17 => Act::Rebuild { structure: r.var()?, part: r.var32()? },
        22 => {
            let kind = r.str(64)?.to_string();
            let pos = DVec3::new(r.f64()?, r.f64()?, r.f64()?);
            let rot = Quat::from_xyzw(r.f32()?, r.f32()?, r.f32()?, r.f32()?);
            if !pos.is_finite() || !rot.is_finite() {
                return Err(WireError::Value);
            }
            Act::Spawn { kind, pos, rot: rot.normalize() }
        }
        23 => Act::Resync { id: r.var()? },
        24 => Act::Back { key: r.u64()? },
        25 => {
            let n = r.var()?;
            if n > 4096 {
                return Err(WireError::Long);
            }
            Act::Body(r.bytes(n as usize)?.to_vec())
        }
        _ => return Err(WireError::Value),
    };
    Ok((step, act))
}

/// A snapshot: the step it is of, what of the player's was taken, and what moves.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Snap {
    pub step: u64,
    /// The newest step of this player's commands the server has heard of, plus one (0: none yet),
    /// and how many steps ahead of the server's the newest command of each message came, at
    /// least, since the last snapshot (less than 0: too late; 1000: none came).
    pub took: u64,
    pub ahead: i32,
    /// The other players, as they are (`PlayerState`, by their id on the network).
    pub players: Vec<(u32, PlayerState)>,
    /// What some of the ships you know were at the end of step `checks_at` (`sync::Digest`): the
    /// ones due then (`checked`). Yours of the same step, if it does not agree, asks for all of it
    /// (`Act::Resync`).
    pub checks_at: u64,
    pub checks: Vec<(u64, Digest)>,
    /// The structures that move, as they are (each `RigidState`'s id is the server's).
    pub things: Vec<RigidState>,
}

/// What is known of something only from afar (`docs/PLAN_AUTORITATIVO.md` §3.6, the track level):
/// a ship or a player past what a player knows in full, out to `interest::TRACK_REACH`: where it
/// is and how it goes, a few times a second; a dot in the sky, a name, a blip.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Far {
    pub what: FarWhat,
    pub pos: DVec3,
    pub vel: Vec3,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FarWhat {
    /// A ship: its structure's id and its kind (its place among the data's ship kinds).
    Ship { id: u64, kind: u16 },
    /// A player, by their id on the network.
    Player(u32),
}

/// What is known from afar at the end of step `step` into `out` (as much of `list` as fits in
/// `room` bytes, in its order; where each is, from `from`, to the centimetre): how many went.
pub fn write_tracks(step: u64, from: DVec3, list: &[Far], room: usize, out: &mut Vec<u8>) -> usize {
    let mut n = 0;
    framed(out, room, |w| {
        w.u8(TRACKS);
        w.var(step);
        from.to_array().iter().for_each(|x| w.f64(*x));
        for f in list {
            let before = w.mark();
            match f.what {
                FarWhat::Ship { id, kind } => {
                    w.u8(0);
                    w.var(id);
                    w.u16(kind);
                }
                FarWhat::Player(id) => {
                    w.u8(1);
                    w.var(u64::from(id));
                }
            }
            w.vec3((f.pos - from).as_vec3());
            w.vec3(f.vel);
            if !w.ok() {
                w.rewind(before);
                break;
            }
            n += 1;
        }
    });
    n
}

/// What `write_tracks` wrote, after its kind byte, into `out` (emptied first): its step.
pub fn read_tracks(r: &mut Reader, out: &mut Vec<Far>) -> Wire<u64> {
    out.clear();
    let step = r.var()?;
    let from = DVec3::new(r.f64()?, r.f64()?, r.f64()?);
    while !r.is_empty() {
        if out.len() >= 1024 {
            return Err(WireError::Long);
        }
        let what = match r.u8()? {
            0 => FarWhat::Ship { id: r.var()?, kind: r.u16()? },
            1 => FarWhat::Player(r.var32()?),
            _ => return Err(WireError::Value),
        };
        let pos = from + r.vec3()?.as_dvec3();
        out.push(Far { what, pos, vel: r.vec3()? });
    }
    Ok(step)
}

/// Every this many steps some ships' digests are taken, in the server and in every player's game
/// alike, of that same step (`Snap::checks`): the ones `checked` says, each one every
/// `CHECK_EVERY * CHECK_SPREAD` steps (4.3 s). Compared at the same step, what changes by itself
/// (a timer, a pump starting, the air warming) is the same in both; compared a few steps apart, it
/// would not be.
pub const CHECK_EVERY: u64 = 8;
pub const CHECK_SPREAD: u64 = 32;
/// The most digests a snapshot carries (more on the wire is refused).
pub const MOST_CHECKS: u64 = 256;

/// The most legs a structure at rest is told with (`Event::Rest`; more on the wire is refused).
pub const MOST_LEGS: u64 = 16;

/// Steps after it is told that what a hand of the server's own does takes effect (`Host::control`,
/// `Event::Control` told ahead): in every game at the same step, theirs being ahead of the
/// server's by less than this (half a second).
pub const HAND_LEAD: u64 = 30;
/// A ship's digest is compared only by whoever is aboard it or this near it (m): its systems
/// matter where they are seen and worked; from further, where it is is what matters, and that
/// the snapshots put right. In a battle every copy's autopilot and fire control work on what
/// its own radar sees, a hair apart: what they do is told by where the ships go.
pub const CHECK_NEAR: f64 = 100.0;

/// Whether ship structure `s` is compared by one whose eye is at `eye` and who is aboard `aboard`.
pub fn checks_near(s: &lunar_core::structure::state::Structure, eye: DVec3, aboard: Option<u64>) -> bool {
    aboard == Some(s.id) || s.to_world(s.center).distance(eye) - f64::from(s.radius) < CHECK_NEAR
}

/// Whether ship `id`'s digest is taken at the end of step `step`.
pub fn checked(id: u64, step: u64) -> bool {
    step % CHECK_EVERY == 0 && (id + step / CHECK_EVERY) % CHECK_SPREAD == 0
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
        w.var(s.checks.len() as u64);
        if !s.checks.is_empty() {
            w.var(s.step - s.checks_at.min(s.step));
            for (id, d) in &s.checks {
                w.var(*id);
                w.u32(d.hash);
                d.levels.iter().for_each(|l| w.f32(*l));
            }
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
    out.checks.clear();
    let n = r.var()?;
    if n > MOST_CHECKS {
        return Err(WireError::Long);
    }
    if n > 0 {
        out.checks_at = out.step.checked_sub(r.var()?).ok_or(WireError::Value)?;
        for _ in 0..n {
            let id = r.var()?;
            let hash = r.u32()?;
            let mut levels = [0.0; lunar_ship::sync::LEVELS];
            for l in &mut levels {
                *l = r.f32()?;
            }
            out.checks.push((id, Digest { hash, levels }));
        }
    }
    while !r.is_empty() {
        out.things.push(RigidState::decode(r)?);
    }
    Ok(())
}

/// What happened, as a player's game is told of it.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// You are in: the step the game is at, your id on the network, toward the sun, the region
    /// of the galaxy the server's game is of (0: the scenario's; each region is a game of its own,
    /// `docs/MUNDO.md` §14), and the key to come back with if you are cut off (`Act::Back`).
    Hello {
        step: u64,
        you: u32,
        sun: DVec3,
        region: u32,
        key: u64,
    },
    /// What `Act::Back` asked: your body as it waited for you, at the end of step `step`
    /// (`Pilot::write_state`; empty: there is none of that key, you start anew).
    Back {
        step: u64,
        state: Vec<u8>,
    },
    /// Your body as the server has it at the end of step `step` (`Pilot::write_state`): your game
    /// went astray there; put it so and step on from there with what you asked since.
    Correct {
        step: u64,
        state: Vec<u8>,
    },
    /// Structure `id` came into being (or into your knowing): its lineage and how many pieces
    /// came off it (`Structure::lineage`, `born`: a piece your game broke off the same is this
    /// one), of what it is made (`sync::write_make`), what is left of it (`sync::write_state`),
    /// where it is and how it moves, exactly (at rest: with no speed at all, to rest there in every
    /// game), and, if it is a ship of kind `ship`, the dice of its systems and how they are
    /// (`sync::write_ship`).
    Made {
        id: u64,
        lineage: u64,
        born: u32,
        ship: String,
        seed: u64,
        pos: DVec3,
        rot: Quat,
        vel: Vec3,
        spin: Vec3,
        resting: bool,
        held: Option<Held>,
        make: Vec<u8>,
        state: Vec<u8>,
        systems: Vec<u8>,
    },
    /// Structure `id` is gone (or out of your knowing).
    Gone {
        id: u64,
    },
    /// What was done to structures, as `told::STRIKES` says it (each named by its id).
    Strikes(Vec<u8>),
    /// What was let fly and where it ended, as `told::SEEN` says it.
    Seen(Vec<u8>),
    /// What a hand did to a ship (a control, a door, a clamp).
    Control {
        ship: u64,
        control: u16,
        value: f64,
    },
    Hand {
        ship: u64,
        act: Hand,
    },
    /// What a ship told whoever works it.
    Said {
        ship: u64,
        about: String,
        text: String,
        level: u8,
    },
    /// A ship's systems as they are (`sync::write_ship`): yours were found to differ.
    Systems {
        ship: u64,
        data: Vec<u8>,
    },
    /// What you asked could not be: why.
    Denied(String),
    /// What changed of structure `id`'s parts and joints not by a strike (a welder, a part put
    /// back): `sync::Shadow::delta`, for `sync::read_delta`.
    State {
        id: u64,
        delta: Vec<u8>,
    },
    /// A crater dug in the ground of body `body` (the ground is the server's: every game digs it
    /// the same, in the order it was dug).
    Crater {
        body: u16,
        crater: lunar_core::deform::Crater,
    },
    /// The craters of body `body` from the `from`-th on, oldest first, in place of what your game
    /// has from there (on coming in: the ground in pieces, `net::ground`).
    Ground {
        body: u16,
        from: u32,
        craters: Vec<lunar_core::deform::Crater>,
    },
    /// Structure `id` came to rest here, exactly (snapshots no longer tell of it until it moves),
    /// as it stands: whether on the ground, and its legs, how far in each is and what each carries
    /// (at rest it is not done again: a ship's flight computer reads its weight on its feet).
    Rest {
        id: u64,
        pos: DVec3,
        rot: Quat,
        grounded: bool,
        legs: Vec<(f32, f32)>,
    },
    /// Structure `id` is held so now (none: let go): by what, which bone of it, and where in it.
    Hold {
        id: u64,
        held: Option<Held>,
    },
    /// Your hands hold nothing: what you took in them another has (let it go in your game).
    Unheld,
    /// What you let fly as your `tag` was not let fly here (not what your hands carry, or not
    /// yet): gone from your game, as if it never was.
    Unfired { tag: u32 },
    /// All there was round you when you came in has been told (it comes after it): from here on
    /// the world is all there (what a game shows while it waits for this, `Online::ready`).
    Ready,
}

fn write_held(w: &mut Writer, h: &Option<Held>) {
    match h {
        Some(h) => {
            w.var(h.by + 1);
            w.u16(h.bone);
            w.vec3(h.pos);
            h.rot.to_array().iter().for_each(|x| w.f32(*x));
        }
        None => w.var(0),
    }
}

fn read_held(r: &mut Reader) -> Wire<Option<Held>> {
    Ok(match r.var()? {
        0 => None,
        by => {
            let bone = r.u16()?;
            let pos = r.vec3()?;
            let rot = Quat::from_xyzw(r.f32()?, r.f32()?, r.f32()?, r.f32()?);
            if !pos.is_finite() || !rot.is_finite() {
                return Err(WireError::Value);
            }
            Some(Held { by: by - 1, bone, pos, rot })
        }
    })
}

fn write_crater(w: &mut Writer, c: &lunar_core::deform::Crater) {
    for x in [c.dir.x, c.dir.y, c.dir.z, c.radius, c.depth, c.rim, c.seed, c.ground] {
        w.f64(x);
    }
}

fn read_crater(r: &mut Reader) -> Wire<lunar_core::deform::Crater> {
    let mut v = [0.0; 8];
    for x in &mut v {
        *x = r.f64()?;
        if !x.is_finite() {
            return Err(WireError::Value);
        }
    }
    // (as it was said, to the last bit, if it is a way at all)
    let dir = DVec3::new(v[0], v[1], v[2]);
    let dir = if (dir.length() - 1.0).abs() < 1e-6 { dir } else { dir.try_normalize().ok_or(WireError::Value)? };
    Ok(lunar_core::deform::Crater { dir, radius: v[3].clamp(0.0, 1e5), depth: v[4].clamp(-1e4, 1e4), rim: v[5].clamp(0.0, 1e4), seed: v[6].clamp(0.0, 1.0), ground: v[7] })
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

/// Craters told whole in one `Event::Ground` at most (each 64 bytes: well within a message).
pub const GROUND_MOST: usize = 800;

/// The ground of body `body` as it is, `craters` (oldest first), as events of `GROUND_MOST` craters
/// at most (`Event::Ground`: the first in place of what a game has, the rest after it), so that
/// none is too big to be told however much was dug.
pub fn ground(body: u16, craters: &[lunar_core::deform::Crater]) -> Vec<Event> {
    craters.chunks(GROUND_MOST).enumerate().map(|(k, c)| Event::Ground { body, from: (k * GROUND_MOST) as u32, craters: c.to_vec() }).collect()
}

/// Room an event takes on the wire, at most.
fn event_room(e: &Event) -> usize {
    64 + match e {
        Event::Correct { state, .. } | Event::Back { state, .. } => state.len() + 16,
        Event::Made { make, state, systems, .. } => make.len() + state.len() + systems.len() + 256,
        Event::Strikes(b) | Event::Seen(b) => b.len() + 8,
        Event::Systems { data, .. } => data.len() + 16,
        Event::Said { about, text, .. } => about.len() + text.len() + 32,
        Event::Denied(t) => t.len() + 8,
        Event::State { delta, .. } => delta.len() + 16,
        Event::Ground { craters, .. } => craters.len() * 64 + 16,
        Event::Crater { .. } => 72,
        Event::Rest { legs, .. } => 56 + legs.len() * 8,
        Event::Hold { .. } => 56,
        _ => 0,
    }
}

/// Events of the end of step `step` into `out`, as one message (the caller keeps it under
/// `lunar_net::MAX_TELL`).
pub fn write_events(step: u64, events: &[Event], out: &mut Vec<u8>) {
    let mut bytes = Vec::new();
    for e in events {
        append_event(e, &mut bytes);
    }
    events_message(step, events.len() as u32, &bytes, out);
}

/// One event appended to `out` as it goes in an `EVENTS` message: what whoever tells the same to
/// many encodes once (`events_message` puts them in one).
pub fn append_event(e: &Event, out: &mut Vec<u8>) {
    let from = out.len();
    // (an event is never left half written: with more room, again, until it fits)
    let mut room = event_room(e);
    loop {
        out.resize(from + room, 0);
        let mut w = Writer::new(&mut out[from..]);
        write_event(&mut w, e);
        if let Ok(n) = w.finish() {
            out.truncate(from + n);
            return;
        }
        room *= 2;
    }
}

/// `n` events appended by `append_event` (`bytes`), of the end of step `step` (what they say is
/// as things were then), as one message into `out`.
pub fn events_message(step: u64, n: u32, bytes: &[u8], out: &mut Vec<u8>) {
    framed(out, bytes.len() + 24, |w| {
        w.u8(EVENTS);
        w.var(step);
        w.var(u64::from(n));
        w.bytes(bytes);
    });
}

fn write_event(w: &mut Writer, e: &Event) {
    match e {
        Event::Hello { step, you, sun, region, key } => {
            w.u8(1);
            w.var(*step);
            w.var(u64::from(*you));
            w.f64(sun.x);
            w.f64(sun.y);
            w.f64(sun.z);
            w.var(u64::from(*region));
            w.u64(*key);
        }
        Event::Back { step, state } => {
            w.u8(17);
            w.var(*step);
            blob(w, state);
        }
        Event::Correct { step, state } => {
            w.u8(2);
            w.var(*step);
            blob(w, state);
        }
        Event::Made { id, lineage, born, ship, seed, pos, rot, vel, spin, resting, held, make, state, systems } => {
            w.u8(3);
            w.var(*id);
            w.var(*lineage);
            w.var(u64::from(*born));
            w.str(ship);
            w.u64(*seed);
            pos.to_array().iter().for_each(|x| w.f64(*x));
            rot.to_array().iter().for_each(|x| w.f32(*x));
            w.vec3(*vel);
            w.vec3(*spin);
            w.u8(u8::from(*resting));
            write_held(w, held);
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
        Event::State { id, delta } => {
            w.u8(12);
            w.var(*id);
            blob(w, delta);
        }
        Event::Crater { body, crater } => {
            w.u8(13);
            w.u16(*body);
            write_crater(w, crater);
        }
        Event::Ground { body, from, craters } => {
            w.u8(14);
            w.u16(*body);
            w.var(u64::from(*from));
            w.var(craters.len() as u64);
            craters.iter().for_each(|c| write_crater(w, c));
        }
        Event::Hold { id, held } => {
            w.u8(16);
            w.var(*id);
            write_held(w, held);
        }
        Event::Unheld => w.u8(18),
        Event::Unfired { tag } => {
            w.u8(19);
            w.u32(*tag);
        }
        Event::Ready => w.u8(20),
        Event::Rest { id, pos, rot, grounded, legs } => {
            w.u8(15);
            w.var(*id);
            pos.to_array().iter().for_each(|x| w.f64(*x));
            rot.to_array().iter().for_each(|x| w.f32(*x));
            w.u8(u8::from(*grounded));
            w.var(legs.len() as u64);
            for (x, load) in legs {
                w.f32(*x);
                w.f32(*load);
            }
        }
    }
}

/// What `write_events` wrote, after its kind byte, into `out`.
pub fn read_events(r: &mut Reader, out: &mut Vec<Event>) -> Wire<u64> {
    let step = r.var()?;
    let n = r.var()?;
    if n > 4096 {
        return Err(WireError::Long);
    }
    for _ in 0..n {
        out.push(read_event(r)?);
    }
    Ok(step)
}

/// One event as `append_event` wrote it.
pub fn read_event(r: &mut Reader) -> Wire<Event> {
    Ok(match r.u8()? {
        1 => {
            let (step, you) = (r.var()?, r.var32()?);
            let sun = DVec3::new(r.f64()?, r.f64()?, r.f64()?);
            Event::Hello { step, you, sun, region: r.var32()?, key: r.u64()? }
        }
        2 => Event::Correct { step: r.var()?, state: read_blob(r)? },
        3 => {
            let (id, lineage, born) = (r.var()?, r.var()?, r.var32()?);
            let ship = r.str(64)?.to_string();
            let seed = r.u64()?;
            let pos = DVec3::new(r.f64()?, r.f64()?, r.f64()?);
            let rot = Quat::from_xyzw(r.f32()?, r.f32()?, r.f32()?, r.f32()?);
            let (vel, spin, resting) = (r.vec3()?, r.vec3()?, r.u8()? != 0);
            if !pos.is_finite() || !rot.is_finite() || !vel.is_finite() || !spin.is_finite() {
                return Err(WireError::Value);
            }
            let held = read_held(r)?;
            Event::Made { id, lineage, born, ship, seed, pos, rot: rot.normalize(), vel, spin, resting, held, make: read_blob(r)?, state: read_blob(r)?, systems: read_blob(r)? }
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
        12 => Event::State { id: r.var()?, delta: read_blob(r)? },
        13 => Event::Crater { body: r.u16()?, crater: read_crater(r)? },
        14 => {
            let (body, from) = (r.u16()?, r.var32()?);
            let n = r.var()?;
            if n > 4096 {
                return Err(WireError::Long);
            }
            let mut craters = Vec::with_capacity(n as usize);
            for _ in 0..n {
                craters.push(read_crater(r)?);
            }
            Event::Ground { body, from, craters }
        }
        15 => {
            let id = r.var()?;
            let pos = DVec3::new(r.f64()?, r.f64()?, r.f64()?);
            let rot = Quat::from_xyzw(r.f32()?, r.f32()?, r.f32()?, r.f32()?);
            if !pos.is_finite() || !rot.is_finite() {
                return Err(WireError::Value);
            }
            let grounded = match r.u8()? {
                0 => false,
                1 => true,
                _ => return Err(WireError::Value),
            };
            let n = r.var()?;
            if n > MOST_LEGS {
                return Err(WireError::Long);
            }
            let mut legs = Vec::with_capacity(n as usize);
            for _ in 0..n {
                let (x, load) = (r.f32()?, r.f32()?);
                if !x.is_finite() || !load.is_finite() {
                    return Err(WireError::Value);
                }
                legs.push((x, load));
            }
            Event::Rest { id, pos, rot: rot.normalize(), grounded, legs }
        }
        16 => Event::Hold { id: r.var()?, held: read_held(r)? },
        17 => Event::Back { step: r.var()?, state: read_blob(r)? },
        18 => Event::Unheld,
        19 => Event::Unfired { tag: r.u32()? },
        20 => Event::Ready,
        _ => return Err(WireError::Value),
    })
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
                pack: true,
                steady: k != 2,
                lamps: false,
                fly: false,
                keys: if k == 0 { 0b1010 } else { 0 },
                frame: (k == 1).then_some([Vec3::new(0.0, 0.8, 0.6), Vec3::X]),
            })
            .collect();
        let mut out = Vec::new();
        let body = Summary { flags: 5, ride: Some(1 << 40), pos: DVec3::new(-3.25, 1.5, 12.0), vel: glam::Vec3::new(0.5, -1.0, 0.0), fuel: 0.75 }.travelled();
        write_cmds(&cmds, Some(Check { step: 996, body, fixes: 3 }), &mut out);
        let mut r = Reader::new(&out);
        assert_eq!(r.u8().unwrap(), CMDS);
        let mut back = Vec::new();
        let checked = read_cmds(&mut r, &mut back).unwrap();
        assert!(r.is_empty());
        assert_eq!(checked, Some(Check { step: 996, body, fixes: 3 }));
        for (a, b) in cmds.iter().zip(&back) {
            assert_eq!(a.travelled(), *b);
        }
        // (what both step with is the same: travelled twice is travelled once)
        assert_eq!(back[0].travelled(), back[0]);
        // as they come when playing: walking and looking round, a jump in the middle. The newest
        // whole, the other three each what it changes of the one after it
        let walking: Vec<Cmd> = (0..4).map(|k| Cmd { step: 2000 - k, input: Input { forward: 1.0, run: true, jump: k == 1, ..Input::default() }, yaw: 0.5 + k as f64 * 0.003, pitch: -0.1, ..Cmd::default() }).collect();
        let body = Summary { flags: 1, ride: None, pos: DVec3::new(1.7e6, 2.1e5, -4.0e4), vel: glam::Vec3::new(1.2, 0.0, 3.1), fuel: 1.0 };
        write_cmds(&walking, Some(Check { step: 1996, body, fixes: 0 }), &mut out);
        assert!(out.len() <= 72, "{} bytes for four commands and the body", out.len());
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
            Act::Launch(launch, 77),
            Act::Mend { structure: 9, part: 31, hp: 12.5 },
            Act::Rebuild { structure: 9, part: 32 },
            Act::Spawn { kind: "alcotan".into(), pos: DVec3::new(1.0, 2.0, 3.0), rot: Quat::IDENTITY },
            Act::Resync { id: 1 << 41 },
            Act::Back { key: 0xdead_beef_0123_4567 },
            Act::Body(vec![7; 300]),
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
            Event::Hello { step: 99, you: 3, sun: DVec3::Y, region: 7, key: u64::MAX - 5 },
            Event::Back { step: 97, state: vec![4, 5] },
            Event::Unheld,
            Event::Unfired { tag: 0x3FFF_FFFF },
            Event::Ready,
            Event::Correct { step: 98, state: vec![1, 2, 3] },
            Event::Made {
                id: 77,
                lineage: 5,
                born: 2,
                ship: "abejorro".into(),
                seed: 11,
                pos: DVec3::new(1.5e6, 2.0, 3.0),
                rot: Quat::IDENTITY,
                vel: Vec3::X,
                spin: Vec3::ZERO,
                resting: true,
                held: Some(Held { by: 6, bone: 2, pos: Vec3::Y, rot: Quat::IDENTITY }),
                make: vec![9; 40],
                state: vec![],
                systems: vec![1],
            },
            Event::Gone { id: 77 },
            Event::Strikes(vec![3, 1, 2]),
            Event::Seen(vec![4]),
            Event::Control { ship: 7, control: 3, value: 1.0 },
            Event::Hand { ship: 7, act: Hand::Clamp(0, false) },
            Event::Said { ship: 7, about: "reactor".into(), text: "en línea".into(), level: 1 },
            Event::Systems { ship: 7, data: vec![0; 300] },
            Event::Denied("no llegas".into()),
            Event::State { id: 4, delta: vec![2, 0, 0] },
            Event::Crater { body: 0, crater: lunar_core::deform::Crater { dir: DVec3::Y, radius: 6.0, depth: 2.0, rim: 0.25, seed: 0.5, ground: 12.0 } },
            Event::Rest { id: 9, pos: DVec3::new(1.7e6, 1.0, -2.0), rot: Quat::IDENTITY, grounded: true, legs: vec![(0.25, 31000.0), (0.0, 0.0)] },
            Event::Hold { id: 9, held: None },
            Event::Hold { id: 9, held: Some(Held { by: 1 << 40, bone: 0, pos: Vec3::new(1.0, -2.0, 3.0), rot: Quat::from_rotation_y(0.3) }) },
            Event::Ground { body: 1, from: 800, craters: vec![lunar_core::deform::Crater { dir: DVec3::X, radius: 1.0, depth: 0.5, rim: 0.1, seed: 0.1, ground: -3.0 }; 3] },
        ];
        let mut out = Vec::new();
        write_events(4321, &events, &mut out);
        let mut r = Reader::new(&out);
        assert_eq!(r.u8().unwrap(), EVENTS);
        let mut back = Vec::new();
        assert_eq!(read_events(&mut r, &mut back).unwrap(), 4321);
        assert_eq!(back, events);
    }

    #[test]
    fn a_snapshot_that_does_not_fit_leaves_what_does_not_for_next_time() {
        let things: Vec<RigidState> = (0..400).map(|k| RigidState { id: k, pos: DVec3::new(4e6 + k as f64, 3e6, 2e6), vel: Vec3::X, ..RigidState::default() }).collect();
        let snap = Snap { step: 5, took: 4, ahead: 2, players: vec![(1, PlayerState::default())], checks_at: 3, checks: vec![(9, Digest { hash: 77, levels: [0.5; lunar_ship::sync::LEVELS] }), (12, Digest::default())], things };
        let mut out = Vec::new();
        let sent = write_snap(&snap, 1100, &mut out);
        assert!(sent > 10 && sent < 400, "{sent} sent");
        assert!(out.len() <= 1100);
        let mut r = Reader::new(&out);
        assert_eq!(r.u8().unwrap(), SNAP);
        let mut back = Snap::default();
        read_snap(&mut r, &mut back).unwrap();
        assert_eq!(back.things.len(), sent);
        assert_eq!((back.step, back.took, back.ahead, back.checks_at, &back.checks), (5, 4, 2, 3, &snap.checks));
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
