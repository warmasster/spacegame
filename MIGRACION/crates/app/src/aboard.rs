//! Hands on a ship: what the crosshair is on (a control within reach, a seat), the mouse on it
//! (click, hold, wheel, drag for levers), the keys of the seat you sit on, and the lines the HUD
//! shows for all that. Every ship works the same: its panels say where its controls are, its seats
//! what keys drive what.
use crate::{
    hud::{Card, Hud, Level, Prompt},
    pilot::{Pilot, Place, Seat},
    ships::Ships,
};
use glam::DVec3;
use lunar_controls::{Blocked, Event, Intent, Mods};
use lunar_core::structure::set::Structures;
use winit::keyboard::KeyCode;

/// Reach of a hand (m) and how far ships are looked into for it; how far an instrument is read.
use lunar_ship::panels::{READ, REACH};
/// Seconds a note stays in the HUD.
const NOTE: f32 = 2.5;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Target {
    Control { k: usize, elem: u8 },
    /// A gauge, display, lamp or screen (read, not worked).
    Indicator(usize),
    Seat(usize),
    /// A door, hatch or lid (`closures`).
    Closure(usize),
    /// A cargo clamp (`lunar_ship::cargo`).
    Clamp(usize),
}

#[derive(Clone, Copy, Debug)]
pub struct Aim {
    /// The ship's structure.
    pub structure: u64,
    pub target: Target,
}

#[derive(Clone, Copy, Debug)]
struct Held {
    structure: u64,
    k: usize,
    secs: f32,
    /// A lever: the mouse moves it instead of the head.
    drag: bool,
}

use lunar_ship::seat_keys::{self, Does};

#[derive(Clone, Copy, Debug)]
struct Binding {
    key: KeyCode,
    control: usize,
    does: Does,
}

#[derive(Default)]
pub struct Aboard {
    pub aim: Option<Aim>,
    held: Option<Held>,
    /// The seat's keys, and which are down.
    bindings: Vec<Binding>,
    down: Vec<KeyCode>,
    /// Last value sent to each (control, axis) by the keys.
    axes: Vec<(usize, u8, f64)>,
    note: Option<(String, f32, bool)>,
    /// Seconds since this began, and how fast the wheel is being spun.
    clock: f64,
    spin: lunar_controls::Spin,
    /// What the hand just worked sounds like (sound ids), taken by whoever plays them.
    pub heard: Vec<&'static str>,
    /// The controls the hand (or the seat's keys) changed: (ship's structure, control, the value
    /// it is at now), taken by whoever tells the other players.
    pub changed: Vec<(u64, u16, f64)>,
    /// What the hand worked that is no control of a panel (a door pushed, a clamp's lever): the
    /// ship's structure and what was done, taken by whoever tells the other players.
    pub acts: Vec<(u64, Act)>,
}

/// A hand on something of a ship that is not a control of its panels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Act {
    /// Closure `0` (a door, a hatch, a lid) pushed open (`1`) or shut.
    Closure(u16, bool),
    /// Clamp `0` worked: it held something and lets it go (`1`), or it shuts on what is loose on it.
    Clamp(u16, bool),
}

/// The signal a hand orders closure `c` of `sh` by.
fn closure_order(sh: &lunar_ship::Ship, c: usize) -> Option<String> {
    let plan = sh.kind.closures.get(c)?;
    Some(plan.order.clone().unwrap_or_else(|| format!("{}.mano", plan.id)))
}

/// A key name of the seat definitions ("W", "Mayús", "Espacio", "Flecha arriba"...).
pub fn key_named(name: &str) -> Option<KeyCode> {
    use KeyCode::*;
    const LETTERS: [KeyCode; 26] = [
        KeyA, KeyB, KeyC, KeyD, KeyE, KeyF, KeyG, KeyH, KeyI, KeyJ, KeyK, KeyL, KeyM, KeyN, KeyO, KeyP, KeyQ, KeyR, KeyS, KeyT, KeyU, KeyV, KeyW, KeyX, KeyY,
        KeyZ,
    ];
    const DIGITS: [KeyCode; 10] = [Digit0, Digit1, Digit2, Digit3, Digit4, Digit5, Digit6, Digit7, Digit8, Digit9];
    let n = name.trim().to_lowercase();
    let mut ch = n.chars();
    if let (Some(c), None) = (ch.next(), ch.next()) {
        return match c {
            'a'..='z' => Some(LETTERS[c as usize - 'a' as usize]),
            '0'..='9' => Some(DIGITS[c as usize - '0' as usize]),
            _ => None,
        };
    }
    Some(match n.as_str() {
        "mayús" | "mayus" | "shift" => ShiftLeft,
        "ctrl" | "control" => ControlLeft,
        "alt" => AltLeft,
        "espacio" | "space" => Space,
        "tab" | "tabulador" => Tab,
        "intro" | "enter" => Enter,
        "retroceso" => Backspace,
        "flecha arriba" => ArrowUp,
        "flecha abajo" => ArrowDown,
        "flecha izquierda" => ArrowLeft,
        "flecha derecha" => ArrowRight,
        "inicio" => Home,
        "fin" => End,
        "re pág" | "re pag" => PageUp,
        "av pág" | "av pag" => PageDown,
        _ => return None,
    })
}

/// The keys of a ship's seats, for the list of controls: (seat, keys, what they do), as its data
/// says. Keys that do the same (the two ways of an axis) share a line.
pub fn seat_keys(kind: &lunar_ship::kind::ShipKind) -> Vec<(String, String, String)> {
    let mut out: Vec<(String, String, String)> = Vec::new();
    for s in &kind.seats {
        let seat = format!("{} — {}", kind.def.nombre, s.def.nombre);
        for b in &s.def.mandos {
            let what = b.ayuda.clone().unwrap_or_else(|| match (b.accion.as_deref(), b.eje) {
                (Some(a), _) => format!("{}: {a}", b.mando),
                (None, e) => format!("{}: eje {}", b.mando, e.unwrap_or(0)),
            });
            match out.iter_mut().find(|r| r.0 == seat && r.2 == what) {
                Some(r) => {
                    r.1.push_str(" / ");
                    r.1.push_str(&b.tecla);
                }
                None => out.push((seat.clone(), b.tecla.clone(), what)),
            }
        }
    }
    out
}

/// Parts of one component ("asiento_piloto.cojin", "asiento_piloto.respaldo").
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

fn same_piece(a: &str, b: &str) -> bool {
    a.split('.').next() == b.split('.').next()
}

impl Aboard {
    /// What the crosshair is on: the nearest control within reach not hidden behind a part, or a
    /// seat. Seated, seats are not offered.
    pub fn aim(&mut self, ships: &Ships, set: &Structures, eye: DVec3, dir: DVec3, seated: bool) {
        self.aim = None;
        let mut best = REACH;
        for sh in &ships.list {
            let Some(s) = set.get(sh.structure) else { continue };
            if s.to_world(s.center).distance(eye) > f64::from(s.radius + READ) {
                continue;
            }
            let (from, d) = (s.to_local(eye), s.dir_to_local(dir));
            let wall = s.raycast(from, d, REACH);
            if let Some((k, elem, t)) = sh.panels.pick(&sh.kind, s, from, d, best)
                && wall.is_none_or(|h| t <= h.t + 0.08)
            {
                best = t;
                self.aim = Some(Aim { structure: sh.structure, target: Target::Control { k, elem } });
            }
            // what can be read is looked at from farther than a hand reaches
            if self.aim.is_none()
                && let Some((i, t)) = sh.panels.pick_indicator(&sh.kind, s, from, d, READ)
                && wall.is_none_or(|h| t <= h.t + 0.08)
            {
                self.aim = Some(Aim { structure: sh.structure, target: Target::Indicator(i) });
            }
            if !seated
                && let Some(h) = wall
                && h.t < best
                && let Some(i) = sh.kind.seats.iter().position(|st| same_piece(&sh.kind.parts[st.part as usize], &sh.kind.parts[h.part as usize]))
            {
                best = h.t;
                self.aim = Some(Aim { structure: sh.structure, target: Target::Seat(i) });
            }
            if let Some(h) = wall
                && h.t < best
                && let Some(c) = sh.closure_of_part(h.part)
            {
                best = h.t;
                self.aim = Some(Aim { structure: sh.structure, target: Target::Closure(c) });
            }
            if let Some(h) = wall
                && h.t < best
                && let Some(c) = sh.clamp_of_part(h.part)
            {
                best = h.t;
                self.aim = Some(Aim { structure: sh.structure, target: Target::Clamp(c) });
            }
        }
    }

    fn act(&mut self, ships: &mut Ships, set: &Structures, structure: u64, k: usize, i: &Intent) -> bool {
        let Some(n) = ships.by_structure(structure) else { return false };
        let Some(s) = set.get(structure) else { return false };
        let sh = &mut ships.list[n];
        let kind = sh.kind.clone();
        let o = sh.panels.intent(k, i, s, &kind, &sh.store);
        if o.changed {
            let c = &sh.panels.controls[k];
            let value = c.mech.value(&c.st);
            // (one entry per control: where it is left is what matters)
            match self.changed.iter_mut().find(|e| e.0 == structure && usize::from(e.1) == k) {
                Some(e) => e.2 = value,
                None => self.changed.push((structure, k as u16, value)),
            }
        }
        if let Some(Event::Blocked(b)) = o.event
            && !matches!(b, Blocked::Stop)
        {
            self.note = Some((sh.panels.reason(b), NOTE, true));
        }
        // what the control did under the hand is heard: a detent, a stop, a spring, a cover
        let lever = sh.panels.controls[k].mech.kind() == "palanca";
        match o.event {
            Some(Event::Click | Event::Trip) => self.heard.push(if lever { "palanca" } else if sh.panels.controls[k].mech.kind() == "tapa" { "tapa" } else { "interruptor" }),
            Some(Event::Stop | Event::Spring) => self.heard.push("palanca"),
            Some(Event::Seal) => self.heard.push("tapa"),
            _ => {}
        }
        o.changed
    }

    /// A control set by another player's hand (told over the network): put where they left it.
    /// Nothing of it is heard or noted here, and it is not told on again.
    pub fn remote(ships: &mut Ships, set: &Structures, structure: u64, k: usize, value: f64) -> bool {
        let (Some(n), Some(s)) = (ships.by_structure(structure), set.get(structure)) else { return false };
        let sh = &mut ships.list[n];
        if k >= sh.panels.controls.len() {
            return false;
        }
        let kind = sh.kind.clone();
        sh.panels.intent(k, &Intent::Set { value }, s, &kind, &sh.store).changed
    }

    /// What another player's hand did to a ship (told over the network), done to our copy of it.
    /// Nothing of it is heard or noted here, and it is not told on again.
    pub fn remote_act(ships: &mut Ships, structure: u64, act: Act) {
        let Some(n) = ships.by_structure(structure) else { return };
        let sh = &mut ships.list[n];
        match act {
            Act::Closure(c, open) => {
                if let Some(order) = closure_order(sh, usize::from(c)) {
                    sh.set_signal(&order, f64::from(u8::from(open)));
                    sh.touch();
                }
            }
            Act::Clamp(c, holding) if usize::from(c) < sh.kind.clamps.len() => sh.work_clamp(usize::from(c), holding),
            Act::Clamp(..) => {}
        }
    }

    /// The control aimed at (its ship's structure, which, which of its keys) and the one held
    /// down: what the body's hand goes to (`handwork`).
    pub fn hand(&self) -> (Option<(u64, u16, u8)>, Option<(u64, u16)>) {
        let aim = self.aim.and_then(|a| match a.target {
            Target::Control { k, elem } => Some((a.structure, k as u16, elem)),
            _ => None,
        });
        (aim, self.held.map(|h| (h.structure, h.k as u16)))
    }

    /// Left button down on what is aimed at. True when it was a control (or a hand-worked door).
    pub fn press(&mut self, ships: &mut Ships, set: &Structures) -> bool {
        if let Some(Aim { structure, target: Target::Closure(c) }) = self.aim {
            let Some(n) = ships.by_structure(structure) else { return false };
            let sh = &mut ships.list[n];
            if sh.toggle_closure(c) {
                self.heard.push("palanca");
                // (told as where it is left, not as "pushed": every copy ends the same)
                let open = closure_order(sh, c).and_then(|o| sh.signal(&o)).is_some_and(|v| v >= 0.5);
                self.acts.push((structure, Act::Closure(c as u16, open)));
            } else {
                self.note = Some((format!("{}: se acciona desde su panel", sh.kind.closures[c].name), NOTE, false));
            }
            return true;
        }
        if let Some(Aim { structure, target: Target::Clamp(c) }) = self.aim {
            // holding, it opens; open with something loose on it, it shuts on it (the ship does
            // either on its next tick)
            let Some(n) = ships.by_structure(structure) else { return false };
            let state = clamp_state(&ships.list[n], set, c);
            let sh = &mut ships.list[n];
            match state {
                ClampState::Holding(what) => {
                    sh.work_clamp(c, true);
                    self.acts.push((structure, Act::Clamp(c as u16, true)));
                    self.note = Some((format!("Suelta: {what}"), NOTE, false));
                }
                ClampState::Ready(what) => {
                    sh.work_clamp(c, false);
                    self.acts.push((structure, Act::Clamp(c as u16, false)));
                    self.note = Some((format!("Anclada: {what}"), NOTE, false));
                }
                ClampState::Empty => self.note = Some(("No hay carga suelta sobre el anclaje".into(), NOTE, false)),
            }
            return true;
        }
        let Some(Aim { structure, target: Target::Control { k, elem }, .. }) = self.aim else { return false };
        self.act(ships, set, structure, k, &Intent::Press { elem });
        let drag = ships.by_structure(structure).is_some_and(|n| ships.list[n].panels.controls[k].mech.kind() == "palanca");
        self.held = Some(Held { structure, k, secs: 0.0, drag });
        true
    }

    /// Left button up.
    pub fn release(&mut self, ships: &mut Ships, set: &Structures) {
        if let Some(h) = self.held.take() {
            self.act(ships, set, h.structure, h.k, &Intent::Release);
        }
    }

    /// Mouse moved: true when a held lever took it.
    pub fn motion(&mut self, ships: &mut Ships, set: &Structures, dx: f64, dy: f64, m: Mods) -> bool {
        let Some(h) = self.held.filter(|h| h.drag) else { return false };
        let i = Intent::Drag { dx: (-dx * 0.004) as f32, dy: (-dy * 0.004) as f32, m };
        self.act(ships, set, h.structure, h.k, &i);
        true
    }

    /// Wheel notches: true when a control took them.
    pub fn wheel(&mut self, ships: &mut Ships, set: &Structures, notches: f32, m: Mods) -> bool {
        let Some(Aim { structure, target: Target::Control { k, .. }, .. }) = self.aim else { return false };
        // (how fast it is spun, by when its notches come: a notch on its own, not at all)
        let rate = self.spin.rate(self.clock, notches);
        self.act(ships, set, structure, k, &Intent::Turn { notches, rate, m });
        true
    }

    /// E: sit on the seat aimed at.
    pub fn use_key(&mut self, pilot: &mut Pilot, ships: &Ships, set: &Structures) -> bool {
        let Some(Aim { structure, target: Target::Seat(i), .. }) = self.aim else { return false };
        let Some(n) = ships.by_structure(structure) else { return false };
        let kind = &ships.list[n].kind;
        let d = &kind.seats[i].def;
        let v = |a: [f32; 3]| glam::Vec3::from_array(a);
        pilot.sit(set, Seat { structure, index: i, eyes: ships.list[n].seat_eyes(i), heading: d.rumbo.to_radians(), exit: v(d.salida) });
        // the seat's keys
        self.bindings.clear();
        self.down.clear();
        self.axes.clear();
        let (keys, bad) = seat_keys::keys(&ships.list[n], i);
        for b in bad {
            eprintln!("{b}");
        }
        for k in keys {
            let Some(key) = key_named(&k.key) else {
                eprintln!("asiento {}: tecla '{}' desconocida", d.id, k.key);
                continue;
            };
            self.bindings.push(Binding { key, control: k.control, does: k.does });
        }
        self.aim = None;
        true
    }

    /// Get up (the seat's keys let go): to where its ship says one gets off (`exits`).
    pub fn stand(&mut self, pilot: &mut Pilot, ships: &mut Ships, set: &Structures) {
        let mut places = Vec::new();
        if let Some(seat) = pilot.seat {
            self.down.clear();
            self.send_axes(ships, set, seat.structure);
            if let Some(n) = ships.by_structure(seat.structure) {
                let ship = &ships.list[n];
                places = exits(&ship.kind, seat.index);
                // (a seat that has moved — a platform that lowered it out of the hull — took its
                // own exit with it: one does not get up into the cabin it is no longer in)
                let moved = ship.seat_eyes(seat.index) - glam::Vec3::from_array(ship.kind.seats[seat.index].def.ojos);
                if let Some(own) = places.first_mut() {
                    own.feet += moved;
                }
            }
        }
        self.bindings.clear();
        self.axes.clear();
        pilot.stand(set, &places);
    }

    /// A key while seated: true when the seat took it.
    pub fn key(&mut self, key: KeyCode, pressed: bool, pilot: &Pilot, ships: &mut Ships, set: &Structures) -> bool {
        let Some(seat) = pilot.seat else { return false };
        let mut took = false;
        for b in self.bindings.clone() {
            if b.key != key {
                continue;
            }
            took = true;
            match b.does {
                Does::Zero if pressed => {
                    self.act(ships, set, seat.structure, b.control, &Intent::Set { value: 0.0 });
                }
                Does::Press => {
                    let i = if pressed { Intent::Press { elem: 0 } } else { Intent::Release };
                    self.act(ships, set, seat.structure, b.control, &i);
                }
                _ => {}
            }
        }
        if took {
            self.down.retain(|k| *k != key);
            if pressed {
                self.down.push(key);
            }
        }
        took
    }

    /// The stick follows the keys held: each (control, axis) gets the sum of its keys down.
    fn send_axes(&mut self, ships: &mut Ships, set: &Structures, structure: u64) {
        let mut want: Vec<(usize, u8, f64)> = Vec::new();
        for b in &self.bindings {
            if let Does::Axis { axis, value } = b.does {
                let v = if self.down.contains(&b.key) { value } else { 0.0 };
                match want.iter_mut().find(|w| w.0 == b.control && w.1 == axis) {
                    Some(w) => w.2 += v,
                    None => want.push((b.control, axis, v)),
                }
            }
        }
        for (c, axis, v) in want {
            let v = v.clamp(-1.0, 1.0);
            let last = self.axes.iter_mut().find(|a| a.0 == c && a.1 == axis);
            let changed = last.as_ref().is_none_or(|a| (a.2 - v).abs() > 1e-9);
            if changed {
                self.act(ships, set, structure, c, &Intent::Axis { axis, value: v });
                match self.axes.iter_mut().find(|a| a.0 == c && a.1 == axis) {
                    Some(a) => a.2 = v,
                    None => self.axes.push((c, axis, v)),
                }
            }
        }
    }

    /// Every frame: held controls count their time, seat keys drive their controls, notes fade.
    pub fn update(&mut self, dt: f32, pilot: &Pilot, ships: &mut Ships, set: &Structures) {
        self.clock += f64::from(dt);
        if let Some(h) = &mut self.held {
            h.secs += dt;
            let (s, k, secs) = (h.structure, h.k, h.secs);
            self.act(ships, set, s, k, &Intent::Hold { secs });
        }
        if let Some(seat) = pilot.seat {
            self.send_axes(ships, set, seat.structure);
            for b in self.bindings.clone() {
                let sign = match b.does {
                    Does::Up => 1.0,
                    Does::Down => -1.0,
                    _ => continue,
                };
                if self.down.contains(&b.key) {
                    self.act(ships, set, seat.structure, b.control, &seat_keys::held(sign, dt));
                }
            }
        }
        if let Some(n) = &mut self.note {
            n.1 -= dt;
            if n.1 <= 0.0 {
                self.note = None;
            }
        }
    }

    /// What the HUD shows of the hands and the seat: under the crosshair, in a few words, what is
    /// aimed at and what works it; to the right, its card (what it is, how it stands, what it
    /// does); over the tools, how to get up. `hand`: a click is the hand's (else it is the
    /// tool's in it: what is aimed at is named, and no click is offered on it).
    pub fn hud(&mut self, pilot: &Pilot, ships: &Ships, set: &Structures, hand: bool, out: &mut Hud) {
        let click = |on: bool| if on && hand { "Clic".to_string() } else { String::new() };
        if let Some(a) = self.aim
            && let Some(n) = ships.by_structure(a.structure)
        {
            let sh = &ships.list[n];
            let short = |c: &lunar_ship::panels::Card| if c.value.is_empty() { c.title.clone() } else { format!("{} · {}", c.title, c.value) };
            match a.target {
                Target::Control { k, .. } => {
                    let c = sh.panels.card_control(&sh.kind, &sh.store, k);
                    out.prompt = Some(Prompt { key: click(true), text: short(&c), level: Level::of(c.level) });
                    out.card = Some(card(c, if hand { "Clic: accionar (mantener: tirar, armar) · Rueda: girar (Mayús grueso, Ctrl fino)" } else { "Con esta herramienta en la mano el clic es suyo: guárdala para accionar · Rueda: girar" }));
                }
                Target::Indicator(i) => {
                    let c = sh.panels.card_indicator(&sh.kind, &sh.store, i);
                    out.prompt = Some(Prompt { key: String::new(), text: short(&c), level: Level::of(c.level) });
                    out.card = Some(card(c, ""));
                }
                Target::Seat(i) => out.prompt = Some(Prompt { key: "E".into(), text: format!("Sentarse · {}", sh.kind.seats[i].def.nombre), level: Level::Normal }),
                Target::Closure(c) => {
                    let plan = &sh.kind.closures[c];
                    let open = sh.signal(&format!("{}.abierta", plan.id)).unwrap_or(0.0);
                    let state = if sh.latched[c] { "cerrada y trabada".to_string() } else if open > 0.98 { "abierta".into() } else { format!("{:.0} % abierta", open * 100.0) };
                    out.prompt = Some(Prompt { key: click(plan.hand), text: format!("{} · {state}", plan.name), level: Level::Normal });
                }
                Target::Clamp(c) => {
                    out.prompt = Some(match clamp_state(sh, set, c) {
                        ClampState::Holding(what) => Prompt { key: click(true), text: format!("Soltar · {what}"), level: Level::Caution },
                        ClampState::Ready(what) => Prompt { key: click(true), text: format!("Anclar · {what}"), level: Level::Good },
                        ClampState::Empty => Prompt { key: String::new(), text: format!("{} · vacío (pon una carga encima)", sh.kind.clamps[c].name), level: Level::Off },
                    });
                }
            }
        }
        if let Some(seat) = pilot.seat
            && let Some(n) = ships.by_structure(seat.structure)
        {
            let d = &ships.list[n].kind.seats[seat.index].def;
            let keys = if self.bindings.is_empty() { "" } else { " · Sus teclas de vuelo: Esc, CONTROLES" };
            out.hints.push(format!("{} · Espacio: levantarse{keys}", d.nombre));
        }
        // what the hand was just told (why a control will not move): where it is looking
        if let Some((t, _, warn)) = &self.note {
            out.prompt = Some(Prompt { key: String::new(), text: t.clone(), level: if *warn { Level::Warning } else { Level::Caution } });
        }
    }
}

/// A clamp as a hand finds it.
enum ClampState {
    /// It holds this.
    Holding(String),
    /// It holds nothing, and this is loose in its zone: shut, it takes it.
    Ready(String),
    Empty,
}

fn clamp_state(sh: &lunar_ship::Ship, set: &Structures, c: usize) -> ClampState {
    let plan = &sh.kind.clamps[c];
    let name = |ids: &[u64]| ids.iter().filter_map(|id| set.get(*id)).map(|s| s.label(&set.lib.catalog)).collect::<Vec<_>>().join(", ");
    if !sh.clamp_held[c].is_empty() {
        return ClampState::Holding(name(&sh.clamp_held[c]));
    }
    if sh.signal(&format!("{}.sujeta", plan.id)).unwrap_or(0.0) >= 0.5 {
        return ClampState::Holding(plan.holds.join(", "));
    }
    let mut ids = Vec::new();
    if let Some(z) = set.get(sh.structure).and_then(|s| sh.clamp_zone(s, c)) {
        set.loose_in(sh.structure, z.centre, z.half, z.max, &mut ids);
        ids.truncate(z.count as usize);
    }
    if ids.is_empty() { ClampState::Empty } else { ClampState::Ready(name(&ids)) }
}

/// A panel's card as the HUD's: what it is, how it stands, what it does (a few lines).
fn card(c: lunar_ship::panels::Card, hint: &str) -> Card {
    Card { title: c.title, value: c.value, level: Level::of(c.level), lines: c.lines.into_iter().take(4).collect(), hint: hint.to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_names() {
        assert_eq!(key_named("W"), Some(KeyCode::KeyW));
        assert_eq!(key_named("Mayús"), Some(KeyCode::ShiftLeft));
        assert_eq!(key_named("Flecha arriba"), Some(KeyCode::ArrowUp));
        assert_eq!(key_named("nada"), None);
    }

    #[test]
    fn the_list_of_a_seats_keys_is_its_data() {
        let defs = crate::root().join("assets/defs");
        let mut lib = lunar_core::structure::Library::load(&defs.join("structures")).unwrap();
        let (ships, _) = lunar_ship::ShipLibrary::load(&defs, &mut lib.catalog).unwrap_or_else(|e| panic!("{}: {}", e.file, e.message));
        let kind = ships.get("alcotan").unwrap();
        let rows = seat_keys(kind);
        // every key of every seat is a key the game knows, and says what it does in words
        for s in &kind.seats {
            for b in &s.def.mandos {
                assert!(key_named(&b.tecla).is_some(), "tecla '{}'", b.tecla);
                assert!(b.ayuda.is_some(), "la tecla {} del asiento {} no dice qué hace", b.tecla, s.def.id);
            }
        }
        // the two ways of an axis share a line
        let pitch = rows.iter().find(|r| r.1 == "W / S").expect("W / S");
        assert!(pitch.0.contains("piloto") && pitch.2.contains("cabeceo"), "{pitch:?}");
        assert!(rows.iter().any(|r| r.1 == "X" && r.2.contains("cero")));
        assert!(rows.len() < kind.seats.iter().map(|s| s.def.mandos.len()).sum::<usize>());
    }

    use lunar_core::{
        body::{Body as Body_, BodyDef, BodyRegistry},
        defs,
        scenario::ScenarioDef,
        scene::Site,
    };
    use std::sync::Arc;

    /// The Moon and a ship of a kind standing on the scenario's site, posed as it stands; a
    /// player to sit in it.
    fn standing(kind: &str) -> (Structures, Pilot, Arc<lunar_ship::ShipKind>, u64) {
        let root = crate::root().join("assets/defs");
        let mut lib = lunar_core::structure::Library::load(&root.join("structures")).unwrap();
        let (ships, bps) = lunar_ship::ShipLibrary::load(&root, &mut lib.catalog).unwrap_or_else(|e| panic!("{}: {}", e.file, e.message));
        lib.blueprints.extend(bps);
        let moon: BodyDef = defs::parse("luna", include_str!("../../../assets/defs/bodies/luna.jsonc")).unwrap();
        let sc: ScenarioDef = defs::parse("scenario", include_str!("../../../assets/defs/scenario.jsonc")).unwrap();
        let bodies = Arc::new(BodyRegistry::new(vec![Body_::from_def("luna", &moon).unwrap()]));
        let site = Site::from_def(&sc.site, &bodies).unwrap();
        let kind = ships.get(kind).unwrap().clone();
        let mut set = Structures::new(Arc::new(lib));
        let id = set.place(&kind.blueprint, &bodies, 0, site.at(40.0, 40.0), 0.0, f64::from(kind.lift)).unwrap();
        let mut ship = lunar_ship::Ship::new(kind.clone(), id, 7).unwrap_or_else(|e| panic!("{e}"));
        ship.update(&mut set.list[0], &lunar_ship::World::default(), 0.0);
        set.rest_on_ground(id, &bodies);
        (set, Pilot::new(bodies, &site, sc.player), kind, id)
    }

    /// Sat in seat `i` and up again: where the feet are (the ship's frame).
    fn off(set: &Structures, pilot: &mut Pilot, kind: &lunar_ship::ShipKind, id: u64, i: usize) -> glam::Vec3 {
        let d = &kind.seats[i].def;
        let v = |a: [f32; 3]| glam::Vec3::from_array(a);
        pilot.sit(set, Seat { structure: id, index: i, eyes: v(d.ojos), heading: d.rumbo.to_radians(), exit: v(d.salida) });
        pilot.stand(set, &exits(kind, i));
        set.get(id).unwrap().to_local(pilot.feet().0)
    }

    #[test]
    fn sat_in_any_seat_the_body_is_on_its_cushion_with_its_feet_on_the_floor() {
        use crate::{
            body::{Body, Stance},
            rig::{Rig, RigDef, Rigged},
        };
        let root = crate::root();
        let def: RigDef = defs::load(&root.join("assets/defs/rigs/astronauta.jsonc")).unwrap_or_else(|e| panic!("{}: {}", e.file, e.message));
        let Ok(model) = Rigged::load(&root.join("assets/models").join(format!("{}.glb", def.modelo))) else { return };
        let moon: BodyDef = defs::parse("luna", include_str!("../../../assets/defs/bodies/luna.jsonc")).unwrap();
        let bodies = BodyRegistry::new(vec![Body_::from_def("luna", &moon).unwrap()]);
        for name in ["alcotan", "abejorro", "cachalote"] {
            let (set, _, kind, id) = standing(name);
            let s = set.get(id).unwrap();
            for seat in &kind.seats {
                let d = &seat.def;
                let mut body = Body::new(Rig::new(def.clone(), &model).unwrap(), None, None);
                let yaw = d.rumbo.to_radians();
                let (ahead, side) = (glam::Vec3::new(yaw.sin(), 0.0, yaw.cos()), glam::Vec3::new(yaw.cos(), 0.0, -yaw.sin()));
                let stance = Stance {
                    eye: s.to_world(glam::Vec3::from_array(d.ojos)),
                    up: (s.rot * glam::Vec3::Y).as_dvec3(),
                    ahead: (s.rot * ahead).as_dvec3(),
                    eye_h: 1.2,
                    vel: DVec3::ZERO,
                    grounded: true,
                    g: 1.62,
                    ride: Some(id),
                    seated: true,
                    inside: true,
                    own_eyes: false,
                };
                for _ in 0..90 {
                    body.update(1.0 / 60.0, &stance, &set, &bodies, [None, None]);
                }
                // its cushion, in the ship's frame: its top, its front edge, its sides
                let p = &s.parts[seat.part as usize];
                let verts: Vec<glam::Vec3> = p.shape.verts().map(|v| p.local.transform_point3(v)).collect();
                let top = verts.iter().map(|v| v.y).fold(f32::MIN, f32::max);
                let front = verts.iter().map(|v| v.dot(ahead)).fold(f32::MIN, f32::max);
                let at = |bone: usize| s.to_local(body.joint(bone, &stance));
                let who = format!("{name}, asiento {}", d.id);
                let rig = &body.rig;
                for i in 0..2 {
                    let (hip, knee, ankle) = (at(rig.legs[i].upper), at(rig.legs[i].lower), at(rig.legs[i].end));
                    // the hips over the cushion by the thigh's own thickness: neither sunk in it nor over it
                    let over = hip.y - top;
                    assert!((0.09..=0.14).contains(&over), "{who}: la cadera a {over:.3} m sobre el cojín");
                    // the thigh along it, level, the knee just past its front edge
                    assert!((knee.y - hip.y).abs() < 0.08 && (0.0..=0.2).contains(&(knee.dot(ahead) - front)), "{who}: rodilla en {knee:.2?}, borde del cojín a {front:.2}");
                    // the foot down on the deck under it: its ankle as high over it as it is standing
                    let floor = set.raycast_solid(s.to_world(ankle), -stance.up, 0.6, 0.06).map(|(_, t, _)| t).unwrap_or(f64::MAX);
                    assert!((floor - 0.132).abs() < 0.04, "{who}: el tobillo a {floor:.3} m del suelo");
                    // and its hand on its thigh
                    let hand = at(rig.arms[i].end);
                    let (d_hip, d_knee) = ((hand - hip).length(), (hand - knee).length());
                    assert!(hand.y > top + 0.12 && hand.y < top + 0.36 && d_hip < 0.42 && d_knee < 0.42 && (hand - hip).dot(side).abs() < 0.2, "{who}: la mano en {hand:.2?} (cadera {hip:.2?}, rodilla {knee:.2?})");
                }
            }
        }
    }

    #[test]
    fn off_every_seat_of_every_ship_one_stands_on_a_floor_with_room() {
        for name in ["alcotan", "abejorro", "cachalote"] {
            let (set, mut pilot, kind, id) = standing(name);
            for i in 0..kind.seats.len() {
                let d = &kind.seats[i].def;
                let at = off(&set, &mut pilot, &kind, id, i);
                let (feet, up) = pilot.feet();
                // on a floor (it is a hand under the feet at most), nothing where the body is
                assert!(pilot.room(&set, feet + up * 0.02, up, 0.08).is_some(), "{name}, asiento {}: de pie en {at:.2?} no cabe o no pisa nada", d.id);
                // and that is its own way out: as the ship stands, nothing is in it
                assert!(at.distance(glam::Vec3::from_array(d.salida)) < 0.3, "{name}, asiento {}: su salida {:?} está ocupada, sale por {at:.2?}", d.id, d.salida);
                assert!(pilot.seat.is_none() && pilot.ride.is_some_and(|r| r.id == id));
            }
        }
    }

    #[test]
    fn off_the_tug_one_steps_on_to_its_step_and_a_step_taken_is_passed_over() {
        let (mut set, mut pilot, kind, id) = standing("abejorro");
        // its port step, facing out (to port: the ship's +x)
        let at = off(&set, &mut pilot, &kind, id, 0);
        assert!((at - glam::Vec3::new(1.05, -0.675, 1.0)).length() < 0.05, "{at:.2?}");
        let s = set.get(id).unwrap();
        let port = (s.rot * glam::Vec3::X).as_dvec3();
        assert!(pilot.heading().dot(port) > 0.95, "no mira hacia fuera");
        // another tug set down with its seat on that step: off by the other side
        let rot = s.rot;
        let on = |x: f32| s.to_world(glam::Vec3::new(x, -0.7, 0.14));
        let (a, b) = (on(1.05), on(-1.05));
        set.spawn(&kind.blueprint, a, rot).unwrap();
        let at = off(&set, &mut pilot, &kind, id, 0);
        assert!((at - glam::Vec3::new(-1.05, -0.675, 1.0)).length() < 0.4, "con el estribo de babor ocupado: {at:.2?}");
        assert!(pilot.heading().dot(port) < -0.95);
        // both taken: down to the ground beside it
        set.spawn(&kind.blueprint, b, rot).unwrap();
        let at = off(&set, &mut pilot, &kind, id, 0);
        let (feet, up) = pilot.feet();
        let high = set.get(id).unwrap().to_world(glam::Vec3::ZERO).distance(feet + up * f64::from(-at.y)) ;
        assert!(at.x.abs() > 1.5 && at.y < -1.2, "con los dos ocupados: {at:.2?} ({high:.2})");
        assert!(pilot.room(&set, feet + up * 0.02, up, 0.08).is_some(), "en el suelo, en {at:.2?}, no cabe");
    }

    #[test]
    fn with_a_tool_whose_click_is_its_own_nothing_of_a_ship_is_offered_a_click() {
        // (a welder aimed at a door said "click: the door": it was the door that took the click)
        let (mut set, pilot, kind, id) = standing("alcotan");
        let font = lunar_core::font::Font::parse(&std::fs::read_to_string(crate::root().join("assets/fonts/serigrafia.json")).unwrap()).unwrap();
        let mut ships = Ships::new(vec![kind.clone()], font);
        let mut ship = lunar_ship::Ship::new(kind.clone(), id, 7).unwrap_or_else(|e| panic!("{e}"));
        ship.update(&mut set.list[0], &lunar_ship::World::default(), 0.0);
        ships.list.push(ship);
        let mut aboard = Aboard::default();
        // a control of a panel, and a door (worked by hand if the ship has one such; else one
        // worked from its panel, which is offered no click anyway)
        let door = kind.closures.iter().position(|c| c.hand).unwrap_or(0);
        let targets = [Target::Control { k: 0, elem: 0 }, Target::Closure(door)];
        for target in targets {
            aboard.aim = Some(Aim { structure: id, target });
            // bare hands, or a tool that lets what is aimed at go first: a click is offered
            let mut hud = Hud::default();
            aboard.hud(&pilot, &ships, &set, true, &mut hud);
            let prompt = hud.prompt.expect("what is aimed at is named");
            let by_hand = !matches!(target, Target::Closure(c) if !kind.closures[c].hand);
            assert_eq!(prompt.key, if by_hand { "Clic" } else { "" }, "{target:?}");
            // the welder in hand: it is still named, with no click on it
            let mut hud = Hud::default();
            aboard.hud(&pilot, &ships, &set, false, &mut hud);
            let with_tool = hud.prompt.expect("what is aimed at is named");
            assert!(with_tool.key.is_empty() && with_tool.text == prompt.text, "{target:?}: {:?}", with_tool.key);
            if let Some(card) = hud.card {
                assert!(card.hint.contains("herramienta") && !card.hint.contains("accionar ("), "{}", card.hint);
            }
        }
    }

    #[test]
    fn the_places_to_get_off_are_tried_in_order_from_the_nearest() {
        let (_, _, kind, _) = standing("abejorro");
        let list = exits(&kind, 0);
        // its own first; then each step (its middle, then the rest of it), then the ground
        assert_eq!(list[0].feet, glam::Vec3::from_array(kind.seats[0].def.salida));
        let xs: Vec<f32> = list.iter().map(|p| p.feet.x).collect();
        let first_ground = xs.iter().position(|x| x.abs() > 1.5).expect("the ground beside it");
        assert!(xs[1..first_ground].iter().all(|x| (x.abs() - 1.05).abs() < 0.2), "{xs:?}");
        assert!(xs[1] > 0.0 && xs[first_ground - 1] < 0.0, "port first, then starboard: {xs:?}");
        assert!(list[first_ground..].iter().all(|p| p.drop > 2.0) && list[1].heading.is_some());
    }
}
