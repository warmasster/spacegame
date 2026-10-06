//! The things of the world as the network names them: every structure (a ship, a build, a crate
//! come off a ship) has an id that is the same in every game — its own id for those the scenario
//! starts with, one made from its maker's player id for those made in play — and a record of
//! what `multi` knows of it: whether it is ours to simulate, what was last told of it, where it
//! lies if it is someone else's and at rest.
//!
//! And a thing as bytes (`write_record` / `Record`): how it is made, where it is, what holds it,
//! what is left of it, its ship's systems. One form for all the times a whole thing is told: a
//! structure made in play, the world for whoever comes late, a copy that no longer agrees.
use crate::{builds::Builds, ships::Ships};
use glam::{DVec3, Quat, Vec3};
use lunar_core::structure::{
    hold::{Held, Load},
    set::Structures,
    state::Structure,
};
use lunar_ship::sync::{self, In, Shadow, Sync, SyncError, put_bytes, put_dvec3, put_quat, put_str, put_var, put_vec3};
use std::collections::HashMap;

/// Why a thing is asked for (`Thing::claims`).
pub mod claim {
    /// We sit at its controls.
    pub const FLOWN: u8 = 1;
    /// It is in our hands.
    pub const HAND: u8 = 2;
    /// It lies in or on the ship we fly.
    pub const CARGO: u8 = 4;
    /// We let it go or made it (it came off something of ours), and it has not come to rest yet.
    pub const LOOSE: u8 = 8;
}

pub struct Thing {
    /// Its id on the wire and its structure's id in this game.
    pub net: u64,
    pub local: u64,
    /// It is ours to simulate and tell of.
    pub mine: bool,
    /// Why we ask for it (bits of `claim`); none: we do not.
    pub claims: u8,
    // ---- ours: what was last told of it
    pub shadow: Shadow,
    pub version: u64,
    pub looked: f64,
    /// We were telling where it is (it moved): its coming to rest is told once.
    pub moving: bool,
    pub joints: Vec<f32>,
    /// Its digest is owed once more when it goes quiet; when it last went, and when it last changed.
    pub owed: bool,
    pub digested: f64,
    pub changed: f64,
    /// What held it when that was last told or heard (a structure of this game), and whether that was ours.
    pub held: Option<u64>,
    // ---- theirs: what was last heard of it
    /// Where it lies at rest, and the moment (the server's clock) that was said.
    pub rest: Option<(DVec3, Quat)>,
    pub rest_stamp: f64,
    /// Digests that did not match in a row, when we last asked for the rest, and how long to wait before asking again.
    pub mismatches: u8,
    pub asked: f64,
    pub wait: f64,
}

impl Thing {
    fn new(net: u64, local: u64) -> Thing {
        Thing { net, local, mine: false, claims: 0, shadow: Shadow::unknown(), version: u64::MAX, looked: 0.0, moving: false, joints: Vec::new(), owed: false, digested: 0.0, changed: f64::MIN, held: None, rest: None, rest_stamp: f64::MIN, mismatches: 0, asked: f64::MIN, wait: 4.0 }
    }
}

/// Ids of the things made in play: the maker's player id over this many bits of its own count.
const MADE_BITS: u32 = 20;

#[derive(Default)]
pub struct Things {
    pub list: Vec<Thing>,
    by_net: HashMap<u64, usize>,
    by_local: HashMap<u64, usize>,
    /// Things we made so far (their ids go on from here).
    made: u64,
}

impl Things {
    /// The things the scenario starts with: the structures with ids up to `count`, which are the
    /// same in every game that starts with it (their id on the wire is their own).
    pub fn scenario(set: &Structures, count: u64) -> Things {
        let mut t = Things::default();
        for s in set.list.iter().filter(|s| s.id <= count) {
            t.add(s.id, s.id);
        }
        t
    }

    pub fn add(&mut self, net: u64, local: u64) -> usize {
        if let Some(&k) = self.by_net.get(&net) {
            // (the same thing on another structure: it was made again)
            self.by_local.remove(&self.list[k].local);
            self.list[k].local = local;
            self.by_local.insert(local, k);
            return k;
        }
        self.list.push(Thing::new(net, local));
        let k = self.list.len() - 1;
        self.by_net.insert(net, k);
        self.by_local.insert(local, k);
        k
    }

    /// An id for a thing made here by player `me`.
    pub fn fresh(&mut self, me: u32) -> u64 {
        self.made += 1;
        (u64::from(me) << MADE_BITS) | (self.made & ((1 << MADE_BITS) - 1))
    }

    pub fn forget(&mut self, k: usize) {
        let gone = self.list.swap_remove(k);
        self.by_net.remove(&gone.net);
        self.by_local.remove(&gone.local);
        if let Some(moved) = self.list.get(k) {
            self.by_net.insert(moved.net, k);
            self.by_local.insert(moved.local, k);
        }
    }

    pub fn of_net(&self, net: u64) -> Option<usize> {
        self.by_net.get(&net).copied()
    }
    pub fn of_local(&self, local: u64) -> Option<usize> {
        self.by_local.get(&local).copied()
    }
    /// The id on the wire of structure `local` of this game.
    pub fn net(&self, local: u64) -> Option<u64> {
        self.of_local(local).map(|k| self.list[k].net)
    }
    /// The structure of this game that thing `net` is.
    pub fn local(&self, net: u64) -> Option<u64> {
        self.of_net(net).map(|k| self.list[k].local)
    }
}

// ---------------------------------------------------------------------------------------------
// a thing as bytes

const SHIP: u8 = 1;
const HELD: u8 = 2;
const RESTING: u8 = 4;

/// Thing `net` (structure `s`, with its ship's systems if it is one) as bytes. False (and nothing
/// written) if it cannot be told (a piece of a broken part: it is of no blueprint).
pub fn write_record(net: u64, s: &Structure, ships: &Ships, set: &Structures, things: &Things, out: &mut Vec<u8>) -> bool {
    let mut make = Vec::new();
    if sync::write_make(s, &set.lib, &mut make).is_err() {
        return false;
    }
    let ship = ships.by_structure(s.id).map(|n| &ships.list[n]);
    let held = s.held.and_then(|h| Some((things.net(h.by)?, h)));
    put_var(out, net);
    out.push(if ship.is_some() { SHIP } else { 0 } | if held.is_some() { HELD } else { 0 } | if s.resting { RESTING } else { 0 });
    put_bytes(out, &make);
    if let Some(sh) = ship {
        put_str(out, &sh.kind.id);
        put_var(out, sh.seed);
    }
    // (a byte the record still has for the body a thing was at: nothing is ruled by it)
    out.push(0);
    put_dvec3(out, s.pos);
    put_quat(out, s.rot);
    put_dvec3(out, s.vel);
    put_vec3(out, s.spin);
    if let Some((by, h)) = held {
        put_var(out, by);
        // (which of its holder's clamps has it, if one does)
        let clamp = ships.by_structure(h.by).and_then(|n| ships.list[n].clamp_held.iter().position(|c| c.contains(&s.id)));
        put_var(out, clamp.map_or(0, |c| c as u64 + 1));
        put_var(out, u64::from(h.bone));
        put_vec3(out, h.pos);
        put_quat(out, h.rot);
    }
    let mut state = Vec::new();
    sync::write_state(s, &mut state);
    put_bytes(out, &state);
    if let Some(sh) = ship {
        state.clear();
        sync::write_ship(sh, &|id| things.net(id), &mut state);
        put_bytes(out, &state);
    }
    true
}

/// What holds a thing, as told: its holder (on the wire), which of its clamps (if one), the bone
/// and the place in it.
#[derive(Clone, Copy, Debug)]
pub struct HeldBy {
    pub by: u64,
    pub clamp: Option<usize>,
    pub bone: u16,
    pub pos: Vec3,
    pub rot: Quat,
}

/// A thing as it was told (`write_record`), its parts still as bytes.
pub struct Record<'a> {
    pub net: u64,
    pub make: &'a [u8],
    /// Its ship's kind and what its pieces were drawn from, if it is one.
    pub ship: Option<(&'a str, u64)>,
    pub body: u8,
    pub pos: DVec3,
    pub rot: Quat,
    pub vel: DVec3,
    pub spin: Vec3,
    pub resting: bool,
    pub held: Option<HeldBy>,
    pub state: &'a [u8],
    pub systems: Option<&'a [u8]>,
}

impl<'a> Record<'a> {
    pub fn read(inp: &mut In<'a>) -> Sync<Record<'a>> {
        let net = inp.var()?;
        let flags = inp.u8()?;
        if flags & !(SHIP | HELD | RESTING) != 0 {
            return Err(SyncError::Value);
        }
        let make = inp.block()?;
        let ship = if flags & SHIP != 0 { Some((inp.str()?, inp.var()?)) } else { None };
        let (body, pos, rot, vel, spin) = (inp.u8()?, inp.dvec3()?, inp.quat()?, inp.dvec3()?, inp.vec3()?);
        let held = if flags & HELD != 0 {
            let (by, clamp, bone) = (inp.var()?, inp.var()?, inp.var()?);
            Some(HeldBy { by, clamp: clamp.checked_sub(1).map(|c| c as usize), bone: bone as u16, pos: inp.vec3()?, rot: inp.quat()? })
        } else {
            None
        };
        let state = inp.block()?;
        let systems = if flags & SHIP != 0 { Some(inp.block()?) } else { None };
        Ok(Record { net, make, ship, body, pos, rot, vel, spin, resting: flags & RESTING != 0, held, state, systems })
    }
}

/// Structure `id` (and its ship, if it is one) out of the game.
pub fn remove(id: u64, ships: &mut Ships, builds: &mut Builds) {
    // (what it held is free; what held it carries it no more: `follow` sees to both)
    builds.set.remove(id);
    ships.list.retain(|sh| sh.structure != id);
    builds.set.follow();
}

/// Structure `x` held by `by` at the place told (in the bone's frame), as `Structures::hold_at`
/// leaves it; whatever held it before lets it go first.
pub fn hold(set: &mut Structures, x: u64, h: Held) -> bool {
    if set.get(x).is_some_and(|s| s.held.is_some()) {
        set.let_go(x);
    }
    let (Some(a), Some(b)) = (set.index_of(x), set.index_of(h.by)) else { return false };
    if a == b || set.list[a].anchored || set.list[b].held.is_some_and(|up| up.by == x) {
        return false;
    }
    let (at, turn) = h.place(&set.list[b]);
    let s = &mut set.list[a];
    s.held = Some(h);
    (s.force, s.torque) = (Vec3::ZERO, Vec3::ZERO);
    let load = Load { id: x, mass: s.mass, at: at + turn * s.com };
    let holder = &mut set.list[b];
    holder.loads.retain(|l| l.id != x);
    holder.loads.push(load);
    (holder.resting, holder.still) = (false, 0.0);
    holder.refresh();
    set.follow();
    true
}
