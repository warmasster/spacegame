//! The seats of the ships, whoever sits in them: sitting down and getting up (where to, `exits`),
//! and what the seat's keys held do to its controls at each step (`Drive`). One place for a
//! player's own game and for the server, which does the same with the keys each player's
//! commands say are held (`net::Cmd::keys`: bit `k` is the seat's `k`-th key, as
//! `lunar_ship::seat_keys::keys` lists them).
use crate::{
    pilot::{Pilot, Place, Seat},
    ships::Ships,
};
use lunar_controls::Intent;
use lunar_core::structure::set::Structures;
use lunar_ship::seat_keys::{self, Does, SeatKey};

/// How far a seat may be from the eyes of whoever sits in it (m): no farther than a hand reaches.
pub const REACH: f64 = 2.5;

/// How far apart the spots tried in a place to get off at are (m).
const EXIT_STEP: f32 = 0.3;

/// Where one gets off a seat of a ship, in the order to try them: the seat's own exit, then the
/// ship's places to get off at (`bajadas`: the ones the seat names, else all, the nearest first),
/// every spot of each from the one nearest the seat.
pub fn exits(kind: &lunar_ship::kind::ShipKind, seat: usize) -> Vec<Place> {
    let Some(seat) = kind.seats.get(seat) else { return Vec::new() };
    let d = &seat.def;
    let eyes = glam::Vec3::from_array(d.ojos);
    let flat = |v: glam::Vec3| glam::Vec2::new(v.x - eyes.x, v.z - eyes.z).length_squared();
    // (its own exit, if it is in one of those places, is got off at as that place says)
    let own = glam::Vec3::from_array(d.salida);
    let within = kind.def.bajadas.iter().find(|z| (own.x - z.en[0]).abs() <= z.zona[0] * 0.5 + 0.01 && (own.z - z.en[2]).abs() <= z.zona[1] * 0.5 + 0.01 && (own.y - z.en[1]).abs() < 0.3);
    let mut out = vec![Place { feet: own, drop: within.map_or(0.3, |z| z.baja), heading: within.and_then(|z| z.rumbo).map(f32::to_radians) }];
    let mut zones: Vec<&lunar_ship::def::ExitDef> = match &d.bajadas {
        Some(named) => named.iter().filter_map(|id| kind.def.bajadas.iter().find(|z| &z.id == id)).collect(),
        None => kind.def.bajadas.iter().collect(),
    };
    if d.bajadas.is_none() {
        zones.sort_by(|a, b| flat(glam::Vec3::from_array(a.en)).total_cmp(&flat(glam::Vec3::from_array(b.en))));
    }
    for z in zones {
        let c = glam::Vec3::from_array(z.en);
        let n = [0, 1].map(|k| (z.zona[k] * 0.5 / EXIT_STEP).floor() as i32);
        let mut spots: Vec<glam::Vec3> = (-n[0]..=n[0]).flat_map(|i| (-n[1]..=n[1]).map(move |j| c + glam::Vec3::new(i as f32 * EXIT_STEP, 0.0, j as f32 * EXIT_STEP))).collect();
        // (its middle first; then the rest, nearest the seat first)
        spots.sort_by(|a, b| (*a != c, flat(*a)).partial_cmp(&(*b != c, flat(*b))).unwrap_or(std::cmp::Ordering::Equal));
        out.extend(spots.into_iter().map(|feet| Place { feet, drop: z.baja, heading: z.rumbo.map(f32::to_radians) }));
    }
    out
}

/// Why a seat cannot be sat in.
pub const NO_SHIP: &str = "esa nave no está";
pub const NO_SEAT: &str = "ese asiento no existe";
pub const TOO_FAR: &str = "el asiento está demasiado lejos";
pub const TAKEN: &str = "el asiento está ocupado";

/// `pilot` sits in seat `index` of the ship on `structure`, if it is near enough (`REACH`) and
/// `taken` (whether someone else sits there) says it is free.
pub fn sit(pilot: &mut Pilot, ships: &Ships, set: &Structures, structure: u64, index: usize, taken: impl Fn(u64, usize) -> bool) -> Result<(), &'static str> {
    let (Some(n), Some(s)) = (ships.by_structure(structure), set.get(structure)) else { return Err(NO_SHIP) };
    let sh = &ships.list[n];
    let Some(d) = sh.kind.seats.get(index).map(|x| &x.def) else { return Err(NO_SEAT) };
    let eyes = sh.seat_eyes(index);
    if s.to_world(eyes).distance(pilot.position) > REACH {
        return Err(TOO_FAR);
    }
    if taken(structure, index) {
        return Err(TAKEN);
    }
    let v = glam::Vec3::from_array;
    pilot.sit(set, Seat { structure, index, eyes, heading: d.rumbo.to_radians(), exit: v(d.salida) });
    Ok(())
}

/// `pilot` gets up from its seat, to where its ship says one gets off (`exits`; a seat that has
/// moved — a platform that lowered it out of the hull — took its own exit with it).
pub fn stand(pilot: &mut Pilot, ships: &Ships, set: &Structures) {
    let mut places = Vec::new();
    if let Some(seat) = pilot.seat
        && let Some(n) = ships.by_structure(seat.structure)
    {
        let ship = &ships.list[n];
        places = exits(&ship.kind, seat.index);
        let moved = ship.seat_eyes(seat.index) - glam::Vec3::from_array(ship.kind.seats[seat.index].def.ojos);
        if let Some(own) = places.first_mut() {
            own.feet += moved;
        }
    }
    pilot.stand(set, &places);
}

/// The keys of the seat someone sits in, as they work its controls step by step: which are held
/// (a mask, `net::Cmd::keys`), what they did at the last step (to press and let go once, to set
/// an axis only when it changes). The same in a player's game and in the server.
#[derive(Clone, Debug, Default)]
pub struct Drive {
    /// The seat (its ship's structure and its index) the keys are of.
    seat: Option<(u64, usize)>,
    keys: Vec<SeatKey>,
    held: u32,
    /// Each (control, axis) as it was left.
    axes: Vec<(usize, u8, f64)>,
    /// (reused)
    want: Vec<(usize, u8, f64)>,
}

/// Keys of one seat at most (one bit each in a command).
pub const MOST_KEYS: usize = 32;

impl Drive {
    /// The seat's keys (as `seat_keys::keys` lists them), for whoever turns key presses into
    /// the mask (its `k`-th is bit `k`).
    pub fn keys(&self) -> &[SeatKey] {
        &self.keys
    }

    /// One step with the keys `held` of the seat `pilot` sits in: what they do to its controls.
    /// Each control a key moved is put in `moved` (with where it was left), for whoever tells it.
    pub fn step(&mut self, pilot: &Pilot, held: u32, ships: &mut Ships, set: &Structures, dt: f32, moved: &mut Vec<(u64, usize, f64)>) {
        let seat = pilot.seat.map(|s| (s.structure, s.index));
        if seat != self.seat {
            // (got up, or into another: what the old one's keys held is let go)
            if let Some((structure, _)) = self.seat {
                self.drive(0, ships, set, structure, dt, moved);
            }
            self.seat = seat;
            self.keys.clear();
            (self.held, self.axes) = (0, Vec::new());
            if let Some((structure, index)) = seat
                && let Some(n) = ships.by_structure(structure)
            {
                let (keys, _) = seat_keys::keys(&ships.list[n], index);
                self.keys = keys;
                self.keys.truncate(MOST_KEYS);
            }
        }
        if let Some((structure, _)) = self.seat {
            self.drive(held, ships, set, structure, dt, moved);
        }
    }

    fn drive(&mut self, held: u32, ships: &mut Ships, set: &Structures, structure: u64, dt: f32, moved: &mut Vec<(u64, usize, f64)>) {
        let was = self.held;
        self.held = held;
        let on = |mask: u32, k: usize| mask & (1 << k) != 0;
        // pressed now: to its least, or a finger on it; let go: the finger off
        for k in 0..self.keys.len() {
            let (control, does) = (self.keys[k].control, self.keys[k].does);
            let (now, before) = (on(held, k), on(was, k));
            let intent = match does {
                Does::Zero if now && !before => Intent::Set { value: 0.0 },
                Does::Press if now && !before => Intent::Press { elem: 0 },
                Does::Press if !now && before => Intent::Release,
                Does::Up if now => seat_keys::held(1.0, dt),
                Does::Down if now => seat_keys::held(-1.0, dt),
                _ => continue,
            };
            act(ships, set, structure, control, &intent, moved);
        }
        // the stick follows the keys held: each (control, axis) gets the sum of its keys down
        self.want.clear();
        for (k, key) in self.keys.iter().enumerate() {
            if let Does::Axis { axis, value } = key.does {
                let v = if on(held, k) { value } else { 0.0 };
                match self.want.iter_mut().find(|w| w.0 == key.control && w.1 == axis) {
                    Some(w) => w.2 += v,
                    None => self.want.push((key.control, axis, v)),
                }
            }
        }
        for w in 0..self.want.len() {
            let (c, axis, v) = self.want[w];
            let v = v.clamp(-1.0, 1.0);
            let last = self.axes.iter().position(|a| a.0 == c && a.1 == axis);
            if last.is_none_or(|i| (self.axes[i].2 - v).abs() > 1e-9) {
                act(ships, set, structure, c, &Intent::Axis { axis, value: v }, moved);
                match last {
                    Some(i) => self.axes[i].2 = v,
                    None => self.axes.push((c, axis, v)),
                }
            }
        }
    }
}

/// `intent` on control `k` of the ship on `structure`; where it was left into `moved` if it moved.
fn act(ships: &mut Ships, set: &Structures, structure: u64, k: usize, intent: &Intent, moved: &mut Vec<(u64, usize, f64)>) {
    let (Some(n), Some(s)) = (ships.by_structure(structure), set.get(structure)) else { return };
    let sh = &mut ships.list[n];
    if k >= sh.panels.controls.len() {
        return;
    }
    let kind = sh.kind.clone();
    if sh.panels.intent(k, intent, s, &kind, &sh.store).changed {
        let c = &sh.panels.controls[k];
        let value = c.mech.value(&c.st);
        match moved.iter_mut().find(|m| m.0 == structure && m.1 == k) {
            Some(m) => m.2 = value,
            None => moved.push((structure, k, value)),
        }
    }
}
