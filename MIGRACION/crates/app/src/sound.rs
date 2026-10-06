//! What the player hears (`lunar_audio`: every sound made, none recorded).
//!
//! The suit is always there (breath, fan, the pack's jets on the back). Everything else goes by
//! what it comes through: the air round the helmet — the room of the ship one is in, at its
//! pressure; outside, none — and what the body touches (the deck under the boots, the hull one
//! rides, the lever in the hand). So a cabin losing its air goes quiet as it empties, and out on
//! the regolith a ship's engines are a tremor in the boots or nothing at all.
//!
//! Nothing here is about one ship: steps by what is underfoot, engines by what pushes the hull,
//! mechanisms by any joint on the move, clamps by their state, and a ship's own sounds by its
//! signals `sonido.<id>` (the level of loop `<id>`: its master alarm, say).
use crate::{pilot::Pilot, ships::Ships};
use glam::DVec3;
use lunar_audio::{Cmd, LOOPS, SoundDefs, device::Output, medium::air_gain};
use lunar_core::structure::set::Structures;
use lunar_ship::cargo::Mode;
use std::{collections::HashMap, path::Path};

/// The loops the game holds, by slot; from `SHIP` on, the ship's own.
const BREATH: usize = 0;
const FAN: usize = 1;
const PACK: usize = 2;
const ENGINE: usize = 3;
const CABIN: usize = 4;
const MECHANISM: usize = 5;
const TOOL: usize = 6;
const SHIP: usize = 7;

/// Metres between steps, walking.
const STRIDE: f64 = 0.78;
/// Coming down faster than this (m/s) is heard.
const LANDING: f64 = 0.8;

/// What the feet did this frame (whoever raises dust wants to know): a step, coming down (how
/// hard, 0: not).
#[derive(Clone, Copy, Debug, Default)]
pub struct Feet {
    pub step: bool,
    pub landed: f32,
}

#[derive(Clone, Copy, PartialEq)]
struct Held {
    sound: u16,
    gain: f32,
    pitch: f32,
    touch: bool,
}

/// What a kind of ship sounds with: its `sonido.<id>` signals, and its clamps' state signals
/// (a magnet claps, a clamp latches).
struct KindSounds {
    loops: Vec<(String, u16)>,
    clamps: Vec<(String, bool)>,
}

pub struct Sounds {
    out: Option<Output>,
    ids: HashMap<String, u16>,
    held: [Option<Held>; LOOPS],
    air: f32,
    volume: f32,
    kinds: HashMap<String, KindSounds>,
    /// Steps: metres since the last one, the foot it was, where we were (what carried us, the
    /// place in its frame — or the world's).
    stride: f64,
    left: bool,
    last: Option<(Option<u64>, DVec3)>,
    grounded: bool,
    fall: f64,
    /// The clamps of the ship we are on as last heard, and whether the hands held something.
    clamps: (u64, Vec<bool>),
    holding: bool,
    /// Blows asked for since `heard` was last called (for a script's log).
    blows: Vec<u16>,
    /// The air round the head as of the last frame (kPa): what the suit reads outside itself.
    pub kpa: f32,
    /// The sound card, or why there is none.
    pub status: String,
}

impl Sounds {
    /// The game's sounds (`assets/defs/sounds.jsonc`) on the default output; `mute`: none asked
    /// for (scripts, benches). Without a sound card the game goes on, silent.
    pub fn new(root: &Path, mute: bool) -> Result<Sounds, String> {
        let defs: SoundDefs = lunar_core::defs::load(&root.join("assets/defs/sounds.jsonc")).map_err(|e| format!("{}: {}", e.file, e.message))?;
        let ids = defs.keys().enumerate().map(|(k, id)| (id.clone(), k as u16)).collect();
        let (out, status) = if mute {
            (None, "sin sonido (guion o banco de pruebas)".to_string())
        } else {
            match Output::open(&defs) {
                Ok(o) => {
                    let status = format!("{} · {} Hz · {} sonidos", o.name, o.rate, o.ids.len());
                    (Some(o), status)
                }
                Err(e) => (None, format!("sin sonido: {e}")),
            }
        };
        Ok(Sounds { out, ids, held: [None; LOOPS], air: -1.0, volume: -1.0, kinds: HashMap::new(), stride: 0.0, left: false, last: None, grounded: true, fall: 0.0, clamps: (0, Vec::new()), holding: false, blows: Vec::new(), kpa: 0.0, status })
    }

    fn send(&mut self, c: Cmd) {
        if let Some(out) = &mut self.out {
            out.send(c);
        }
    }

    /// A blow: `id` at `gain`, to one side (−1..1), `touch`: we touch what makes it.
    pub fn blow(&mut self, id: &str, gain: f32, pan: f32, touch: bool) {
        if let Some(&sound) = self.ids.get(id) {
            self.send(Cmd::Play { sound, gain, pitch: 1.0, pan, touch });
            if self.blows.len() < 64 {
                self.blows.push(sound);
            }
        }
    }

    /// What is sounding, in words (a script's log): the air, every loop held and its level, the
    /// blows since the last time it was asked.
    pub fn heard(&mut self) -> String {
        let name = |k: u16| self.ids.iter().find(|(_, v)| **v == k).map_or("?", |(n, _)| n.as_str());
        let loops: Vec<String> = self.held.iter().flatten().filter(|h| h.gain > 0.0).map(|h| format!("{} {:.2}", name(h.sound), h.gain)).collect();
        let blows: Vec<&str> = self.blows.iter().map(|&k| name(k)).collect();
        let text = format!("aire {:.2}; bucles: {}; golpes: {}", self.air.max(0.0), if loops.is_empty() { "ninguno".into() } else { loops.join(", ") }, if blows.is_empty() { "ninguno".into() } else { blows.join(", ") });
        self.blows.clear();
        text
    }

    /// The level of a loop (asked again only when it changes).
    fn hold(&mut self, slot: usize, sound: Option<u16>, gain: f32, pitch: f32, touch: bool) {
        let Some(sound) = sound else { return };
        let gain = if gain < 0.004 { 0.0 } else { gain };
        let now = Held { sound, gain, pitch, touch };
        let changed = match self.held[slot] {
            Some(h) => h.sound != sound || h.touch != touch || (h.gain - gain).abs() > 0.01 || (h.gain > 0.0) != (gain > 0.0) || (h.pitch - pitch).abs() > 0.01,
            None => gain > 0.0,
        };
        if changed {
            self.send(Cmd::Loop { slot: slot as u8, sound, gain, pitch, touch });
            self.held[slot] = Some(now);
        }
    }

    fn id(&self, name: &str) -> Option<u16> {
        self.ids.get(name).copied()
    }

    /// This frame's: the air round the head, the suit, the ship one is on, one's own steps.
    /// `tool`: the tool in hand is working; `holding`: the hands hold something.
    #[allow(clippy::too_many_arguments)]
    /// `footfalls`: the feet the body set down this frame (bit 0 the left, bit 1 the right), if
    /// a body walks for us (else steps are told by the ground covered).
    #[allow(clippy::too_many_arguments)]
    pub fn frame(&mut self, volume: f32, pilot: &Pilot, ships: &Ships, set: &Structures, tool: bool, holding: bool, paused: bool, footfalls: Option<u8>) -> Feet {
        let volume = if paused { 0.0 } else { volume };
        if (volume - self.volume).abs() > 0.004 {
            self.volume = volume;
            self.send(Cmd::Master(volume));
        }
        let on = pilot.ride.map(|r| r.id).or(pilot.seat.map(|s| s.structure));
        let ship = on.and_then(|id| Some((ships.by_structure(id)?, set.get(id)?)));
        // the air round the head: the room's, if we are in one
        let kpa = ship
            .and_then(|(n, s)| {
                let sh = &ships.list[n];
                let room = lunar_ship::atmos::room_of(&sh.kind, s.to_local(pilot.eye()))?;
                Some((sh.atmos.pressure(room) / 1000.0) as f32)
            })
            .unwrap_or(0.0);
        self.kpa = kpa;
        let air = air_gain(kpa);
        if (air - self.air).abs() > 0.004 {
            self.air = air;
            self.send(Cmd::Air(air));
        }
        // the suit
        self.hold(BREATH, self.id("respiracion"), 1.0, 1.0, false);
        self.hold(FAN, self.id("ventilador"), 1.0, 1.0, false);
        self.hold(PACK, self.id("mochila"), if pilot.pack_on { pilot.jet as f32 } else { 0.0 }, 1.0, false);
        self.hold(TOOL, self.id("herramienta"), if tool { 1.0 } else { 0.0 }, 1.0, true);
        // the ship under us: what pushes it, its air, what moves in it
        let (mut engine, mut cabin, mut mech) = (0.0f32, 0.0f32, 0.0f32);
        if let Some((n, s)) = ship {
            let sh = &ships.list[n];
            let push = s.force.length() / s.mass.max(1.0);
            let turn = s.torque.length() / (s.mass.max(1.0) * s.radius.max(1.0));
            engine = ((push / 3.0).sqrt() + (turn / 2.0).sqrt() * 0.5).min(1.0);
            cabin = if kpa > 3.0 { 1.0 } else { 0.0 };
            mech = sh.joints.iter().map(|j| (j.qd.abs() / if j.hinge { 0.25 } else { 0.15 }) as f32).fold(0.0, f32::max).min(1.0);
        }
        self.hold(ENGINE, self.id("motor"), engine, 0.85 + 0.3 * engine, true);
        self.hold(CABIN, self.id("cabina"), cabin, 1.0, false);
        self.hold(MECHANISM, self.id("mecanismo"), if mech > 0.05 { 0.4 + 0.6 * mech } else { 0.0 }, 0.8 + 0.4 * mech, true);
        self.ship(ship.map(|(n, s)| (&ships.list[n], s.id)));
        if holding != self.holding {
            self.holding = holding;
            self.blow("agarre", 0.8, 0.0, true);
        }
        self.steps(pilot, footfalls)
    }

    /// The ship's own: its `sonido.<id>` loops, its clamps as they take and let go.
    fn ship(&mut self, ship: Option<(&lunar_ship::Ship, u64)>) {
        let Some((sh, id)) = ship else {
            for slot in SHIP..LOOPS {
                if let Some(h) = self.held[slot] {
                    self.hold(slot, Some(h.sound), 0.0, 1.0, false);
                }
            }
            self.clamps.0 = 0;
            return;
        };
        if !self.kinds.contains_key(&sh.kind.id) {
            let loops = self.ids.iter().filter(|(name, _)| sh.signal(&format!("sonido.{name}")).is_some()).map(|(name, &k)| (format!("sonido.{name}"), k)).take(LOOPS - SHIP).collect();
            let clamps = sh.kind.clamps.iter().map(|c| (format!("{}.sujeta", c.id), c.zone.is_some_and(|z| z.mode == Mode::Grip))).collect();
            self.kinds.insert(sh.kind.id.clone(), KindSounds { loops, clamps });
        }
        let kind = &self.kinds[&sh.kind.id];
        let levels: Vec<(u16, f32)> = kind.loops.iter().map(|(signal, sound)| (*sound, sh.signal(signal).unwrap_or(0.0) as f32)).collect();
        let clamps: Vec<(bool, bool)> = kind.clamps.iter().map(|(signal, grip)| (sh.signal(signal).unwrap_or(0.0) > 0.5, *grip)).collect();
        for slot in SHIP..LOOPS {
            match levels.get(slot - SHIP) {
                Some(&(sound, level)) => self.hold(slot, Some(sound), level.clamp(0.0, 1.0), 1.0, false),
                None => {
                    if let Some(h) = self.held[slot] {
                        self.hold(slot, Some(h.sound), 0.0, 1.0, false);
                    }
                }
            }
        }
        // (a ship just boarded: its clamps as they are, unheard)
        if self.clamps.0 == id && self.clamps.1.len() == clamps.len() {
            for (k, &(holds, grip)) in clamps.iter().enumerate() {
                if holds != self.clamps.1[k] {
                    self.blow(if grip { "iman" } else { "anclaje" }, 1.0, 0.0, true);
                }
            }
        }
        self.clamps = (id, clamps.iter().map(|c| c.0).collect());
    }

    /// Our own steps on what is underfoot, and coming down on it.
    fn steps(&mut self, pilot: &Pilot, footfalls: Option<u8>) -> Feet {
        let mut feet = Feet::default();
        let walking = !pilot.flying && pilot.seat.is_none() && pilot.grounded;
        let here = match pilot.ride {
            Some(r) => (Some(r.id), r.local.as_dvec3()),
            None => (None, pilot.position),
        };
        if let Some((on, at)) = self.last
            && on == here.0
            && walking
        {
            let d = here.1.distance(at);
            if d < 0.6 {
                self.stride += d;
            }
        }
        self.last = Some(here);
        let (deck, pan) = (pilot.ride.is_some(), if self.left { -0.18 } else { 0.18 });
        let step = if deck { "paso_cubierta" } else { "paso_regolito" };
        let coming_down = pilot.grounded && !self.grounded;
        match footfalls {
            // a body walks for us: each foot is heard as it comes down, on its side
            Some(bits) => {
                self.stride = 0.0;
                if walking && !coming_down {
                    for (i, pan) in [(0, -0.18), (1, 0.18)] {
                        if bits & (1 << i) != 0 {
                            self.blow(step, 0.55, pan, true);
                            feet.step = true;
                        }
                    }
                }
            }
            None if self.stride >= STRIDE => {
                self.stride = 0.0;
                self.left = !self.left;
                self.blow(step, 0.55, pan, true);
                feet.step = true;
            }
            None => {}
        }
        if pilot.grounded && !self.grounded && !pilot.flying && self.fall < -LANDING {
            feet.landed = ((-self.fall) / 4.0).clamp(0.3, 1.2) as f32;
            self.blow("caida", feet.landed, 0.0, true);
            self.stride = 0.0;
        }
        if !pilot.grounded {
            self.fall = pilot.vertical_speed();
        }
        self.grounded = pilot.grounded;
        feet
    }
}
