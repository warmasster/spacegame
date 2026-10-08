//! A game kept on disk and taken up again (`docs/PLAN_AUTORITATIVO.md` fase 9): its world as a
//! game that comes in is told it (`Event::Made` of every structure that is not as the scenario
//! sets it, `Event::Ground`, `Event::Gone`), its clock, and its players' bodies with the keys they
//! are theirs by (each comes back to theirs: `Act::Back`), and what flies the moment it is kept
//! (rounds, missiles, guided missiles, decoys: `Blasts::keep`), to the last bit. Particles and
//! flashes are not: they are only seen.
//!
//! The bytes: `MAGIC`, `VERSION`, the build and the scenario's fingerprint it is of (no other
//! takes it up), how many times it was kept (of two slots, the newest is the one with more), the
//! clock (step, the structures' time and bursts, strikes done, the next free id, the sun), the
//! events, the bodies, what flies and a checksum of all that before it (a slot cut short or
//! garbled is not taken). Little-endian; the events as they go on the wire (`net::append_event`).
use crate::{
    game::Game,
    net::{self, Event},
    online,
};
use glam::DVec3;
use lunar_net::Reader;

pub const MAGIC: [u8; 4] = *b"LPAR";
pub const VERSION: u16 = 3;

/// What a game taken up again had besides its world: how many times it was kept, and its
/// players' bodies (`Pilot::write_state`) by their keys.
#[derive(Debug, Default)]
pub struct Kept {
    pub saves: u64,
    pub bodies: Vec<(u64, Vec<u8>)>,
}

/// FNV-1a, 64 bits: what tells a slot written whole from one cut short.
fn checksum(b: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &x in b {
        h = (h ^ u64::from(x)).wrapping_mul(0x100_0000_01b3);
    }
    h
}

/// `game` kept into `out` (emptied first), as kept for the `saves`-th time, of `scenario` (the
/// fingerprint of its data), with `bodies` (key, body as `Pilot::write_state` says it).
pub fn write<'a>(game: &Game, saves: u64, scenario: u32, bodies: impl Iterator<Item = (u64, &'a [u8])>, out: &mut Vec<u8>) {
    out.clear();
    out.extend_from_slice(&MAGIC);
    out.extend_from_slice(&VERSION.to_le_bytes());
    let build = net::BUILD.as_bytes();
    out.extend_from_slice(&(build.len() as u16).to_le_bytes());
    out.extend_from_slice(build);
    out.extend_from_slice(&scenario.to_le_bytes());
    out.extend_from_slice(&saves.to_le_bytes());
    let (now, bursts) = game.builds.clock();
    out.extend_from_slice(&game.step.to_le_bytes());
    out.extend_from_slice(&now.to_le_bytes());
    out.extend_from_slice(&bursts.to_le_bytes());
    out.extend_from_slice(&game.struck.to_le_bytes());
    out.extend_from_slice(&game.builds.set.next_free().to_le_bytes());
    for x in game.sun.to_array() {
        out.extend_from_slice(&x.to_le_bytes());
    }
    // ---- the world, as a game that comes in is told it
    let count_at = out.len();
    out.extend_from_slice(&0u32.to_le_bytes());
    let mut n = 0u32;
    for (b, body) in game.bodies.iter() {
        for e in net::ground(b, body.deform().craters()) {
            net::append_event(&e, out);
            n += 1;
        }
    }
    let set = &game.builds.set;
    let end = game.builds.scenario_end;
    for gone in 1..end {
        if set.index_of(gone).is_none() {
            net::append_event(&Event::Gone { id: gone }, out);
            n += 1;
        }
    }
    for s in &set.list {
        // (what the scenario set, anchored and untouched: as every game starts with it)
        if s.id < end && s.anchored && s.version == 0 && game.ships.by_structure(s.id).is_none() {
            continue;
        }
        if let Some(e) = crate::host::made(game, s.id) {
            net::append_event(&e, out);
            n += 1;
        }
    }
    out[count_at..count_at + 4].copy_from_slice(&n.to_le_bytes());
    // ---- the bodies
    let count_at = out.len();
    out.extend_from_slice(&0u32.to_le_bytes());
    let mut n = 0u32;
    for (key, state) in bodies {
        out.extend_from_slice(&key.to_le_bytes());
        out.extend_from_slice(&(state.len() as u32).to_le_bytes());
        out.extend_from_slice(state);
        n += 1;
    }
    out[count_at..count_at + 4].copy_from_slice(&n.to_le_bytes());
    // ---- what flies
    game.blasts.keep(out);
    let sum = checksum(out);
    out.extend_from_slice(&sum.to_le_bytes());
}

/// How many times the game in `data` was kept, if it is a game kept whole (none: not one, or cut
/// short, or garbled).
pub fn saves(data: &[u8]) -> Option<u64> {
    let body = whole(data)?;
    let mut r = Reader::new(body);
    header(&mut r).ok().map(|h| h.saves)
}

fn whole(data: &[u8]) -> Option<&[u8]> {
    if data.len() < 8 + MAGIC.len() || data[..4] != MAGIC {
        return None;
    }
    let (body, sum) = data.split_at(data.len() - 8);
    (checksum(body) == u64::from_le_bytes(sum.try_into().ok()?)).then_some(body)
}

struct Header {
    version: u16,
    build: String,
    scenario: u32,
    saves: u64,
}

fn le<const N: usize>(r: &mut Reader) -> Result<[u8; N], lunar_net::WireError> {
    let b = r.bytes(N)?;
    let mut a = [0u8; N];
    a.copy_from_slice(b);
    Ok(a)
}

fn header(r: &mut Reader) -> Result<Header, lunar_net::WireError> {
    let _magic = r.bytes(4)?;
    let version = u16::from_le_bytes(le(r)?);
    let n = u16::from_le_bytes(le(r)?) as usize;
    let build = String::from_utf8_lossy(r.bytes(n)?).into_owned();
    let scenario = u32::from_le_bytes(le(r)?);
    let saves = u64::from_le_bytes(le(r)?);
    Ok(Header { version, build, scenario, saves })
}

/// The game kept in `data` put into `game` (made of the same scenario, as it starts): its
/// world and clock as they were kept; what else it had is given back. `scenario`: the
/// fingerprint of the data `game` is of (a game kept of other data is not taken up).
pub fn read(game: &mut Game, scenario: u32, data: &[u8]) -> Result<Kept, String> {
    let body = whole(data).ok_or("no es una partida guardada entera (cortada o estropeada)")?;
    let mut r = Reader::new(body);
    let bad = |_| "la partida guardada está estropeada".to_string();
    let h = header(&mut r).map_err(bad)?;
    if h.version != VERSION {
        return Err(format!("la partida se guardó con otra versión del formato ({}, esta lee la {VERSION})", h.version));
    }
    if h.build != net::BUILD {
        return Err(format!("la partida es de otra versión del juego ({}, este es {})", h.build, net::BUILD));
    }
    if h.scenario != scenario {
        return Err(format!("la partida es de otros datos del juego ({:08x}, estos son {scenario:08x})", h.scenario));
    }
    let step = u64::from_le_bytes(le(&mut r).map_err(bad)?);
    let now = f64::from_le_bytes(le(&mut r).map_err(bad)?);
    let bursts = u64::from_le_bytes(le(&mut r).map_err(bad)?);
    let struck = u64::from_le_bytes(le(&mut r).map_err(bad)?);
    let next = u64::from_le_bytes(le(&mut r).map_err(bad)?);
    let mut sun = [0.0; 3];
    for x in &mut sun {
        *x = f64::from_le_bytes(le(&mut r).map_err(bad)?);
    }
    (game.step, game.struck, game.sun) = (step, struck, DVec3::from_array(sun));
    game.builds.resume((now, bursts));
    game.builds.set.reserve(next);
    let n = u32::from_le_bytes(le(&mut r).map_err(bad)?);
    let mut died = Vec::new();
    for _ in 0..n {
        match net::read_event(&mut r).map_err(bad)? {
            Event::Ground { body, from, craters } if usize::from(body) < game.bodies.len() => game.bodies.get(body).edit(|d| d.put_from(from as usize, &craters)),
            Event::Gone { id } => online::remove(game, id),
            e @ Event::Made { .. } => {
                if !online::put_made(game, step, &e, &mut died) {
                    return Err("la partida guardada tiene una estructura que no se puede rehacer".to_string());
                }
            }
            _ => return Err("la partida guardada está estropeada".to_string()),
        }
    }
    let n = u32::from_le_bytes(le(&mut r).map_err(bad)?);
    let mut kept = Kept { saves: h.saves, bodies: Vec::with_capacity(n.min(1024) as usize) };
    for _ in 0..n {
        let key = u64::from_le_bytes(le(&mut r).map_err(bad)?);
        let len = u32::from_le_bytes(le(&mut r).map_err(bad)?) as usize;
        kept.bodies.push((key, r.bytes(len).map_err(bad)?.to_vec()));
    }
    game.blasts.take_up(&mut r).map_err(bad)?;
    if !r.is_empty() {
        return Err("la partida guardada está estropeada".to_string());
    }
    Ok(kept)
}
