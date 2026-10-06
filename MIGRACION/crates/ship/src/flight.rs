//! The flight computer: from the pilot's controls (stick, pedals, translation, throttle) and the
//! modes (stabiliser, hold), what each thruster and engine should do. Rate command: the stick asks
//! a turn rate and the computer pushes toward it; with the stabiliser off and the stick centred
//! the ship keeps turning. The wrench it wants (force and torque, ship frame) is shared among the
//! thrusters by where they are and which way they push *now* (a tilted nacelle pushes another way),
//! by a bounded least squares; without its computer (no power, wrecked) the ship flies direct:
//! no stabiliser, no hold, thrusters by sign. Works the same anywhere: gravity comes from the body
//! under the ship (or none, in deep space).
//!
//! A ship with momentum wheels (`ruedas`) turns with them first: they take the torque wanted as
//! far as they can give it (their torque, and nothing in the direction they are full in), the
//! thrusters the rest. Wheels more than half full give the hull the torque that empties them and
//! the thrusters cancel it: the momentum leaves as propellant. That is always so while the
//! computer runs and there are thrusters, unless the ship has a switch for it (`descarga`), and
//! then while that is on: wheels that only ever fill end up full, and then they turn nothing.
//!
//! What the engines turn the hull by as they push (a push that does not go through the centre of
//! mass: a few centimetres of it are hundreds of N·m) is asked of the thrusters as it is made —
//! by what they push now, not by what they were told — not left for the stabiliser to find once
//! the hull has turned.
//!
//! With the stabiliser on and the stick let go it keeps the attitude it was let go at, not only
//! its turn: whatever is left over (a push off centre, a wheel giving back) is put back, and the
//! hull does not creep away a few degrees a minute.
use crate::{
    kind::{ShipKind, resolve},
    ship::{Body, MachineRt},
};
use glam::Vec3;
use lunar_core::structure::state::Structure;
use lunar_signals::{SignalId, Store, Writer};

struct Thruster {
    machine: usize,
    order: SignalId,
    /// Rated thrust (N) and the machine's thrust axis is read from the machine plan.
    rated: f32,
    /// The most throttle it takes (an engine's `estrangulamiento`; the model's own without it).
    top: f32,
}

/// Engines that swing on a hinge the computer may order: the hinge's axis (ship frame), its
/// travel (rad, from `lo`), the signal that says where along it they are (0..1), and the two the
/// computer writes: where it wants them (0..1) and whether it has them (else the pilot's lever).
struct Vector {
    axis: Vec3,
    lo: f32,
    span: f32,
    pos: SignalId,
    order: SignalId,
    on: SignalId,
}

/// A cluster of momentum wheels: what it is asked (N·m per axis), what it holds, its limits.
struct Wheel {
    cmd: [SignalId; 3],
    h: [SignalId; 3],
    h_max: f32,
    t_max: f32,
}

/// Engines the computer trims against each other (a ship with more is not trimmed), and how far
/// from the common throttle it takes any of them (down to, up to).
const TRIMMED: usize = 8;
const TRIM: (f32, f32) = (0.6, 1.25);

/// Seconds clear of the ground before the stabiliser takes the hull again.
const AIRBORNE_AFTER: f32 = 1.0;
/// Holding, how fast it turns to stand on its engines: rad/s for each radian it leans (its sine),
/// and the most (rad/s). And the least gravity under which there is an upright to keep (m/s²).
const LEVEL: (f32, f32) = (0.8, 0.12);
/// The share of what its thrusters can push one way that is ever asked of them: the rest is
/// kept for turning the ship.
const RCS_SHARE: f32 = 0.7;
const MIN_GRAVITY: f32 = 0.02;
/// The flight profiles (`FlightDef::perfil`): how much of its turn rates and of its rate gain
/// each gives. Fine work, the ship as built, and combat.
const PROFILES: [(f32, f32); 3] = [(0.45, 0.85), (1.0, 1.0), (1.6, 1.3)];
/// Keeping the attitude the stick was let go at: rad/s of turn for each radian off, the most of
/// its turn rates it takes for it, and how slowly it must be turning (rad/s) for the attitude it
/// is at to be the one kept.
const KEEP: (f32, f32, f32) = (0.8, 0.25, 0.01);
/// Share of what a wheel holds that it gives back per second while unloading (1/s).
const UNLOAD: f32 = 0.25;

pub struct Flight {
    wheels: Vec<Wheel>,
    unload: Option<SignalId>,
    stick: [Option<SignalId>; 3],
    trans: [Option<SignalId>; 3],
    throttle: Option<SignalId>,
    sas: Option<SignalId>,
    hold: Option<SignalId>,
    computer: Option<SignalId>,
    /// How lively it flies (`FlightDef::perfil`): 0 fine, 1 normal, 2 combat.
    profile: Option<SignalId>,
    rcs: Vec<Thruster>,
    engines: Vec<Thruster>,
    /// Its engines are on a joint (nacelles that tilt): where they point is the pilot's to say,
    /// and holding keeps the hull upright instead of turning it to stand on them.
    tilting: bool,
    /// Engines on a hinge that the computer may swing by itself (`Vector`).
    vector: Option<Vector>,
    rates: Vec3,
    gain: f32,
    // scratch for the allocation
    cols: Vec<[f32; 6]>,
    u: Vec<f32>,
    /// What each thruster was given last tick: where the next share is sought from.
    warm: Vec<f32>,
    /// (scratch: what each thruster gives at full, squared)
    norms: Vec<f32>,
    /// Telemetry: what it asked (force and torque, ship frame) and its mode.
    o_mode: Option<SignalId>,
    /// `vuelo.tierra`: 1 standing on the ground (its weight on its feet).
    o_ground: Option<SignalId>,
    /// The attitude the stabiliser keeps (ship to world), since the stick was let go.
    kept: Option<glam::Quat>,
    /// Seconds since it last touched the ground (a ship just made counts as standing until it
    /// has been clear of it a moment: set down, its thrusters must not fire before its feet say
    /// so).
    aloft: f32,
}

fn opt(store: &mut Store, s: &Option<String>) -> Option<SignalId> {
    s.as_ref().map(|n| store.define(n))
}

fn rated(kind: &ShipKind, m: usize) -> f32 {
    crate::exhaust::rated(&kind.machines[m].def)
}

impl Flight {
    pub fn new(kind: &ShipKind, store: &mut Store) -> Result<Flight, String> {
        let Some(f) = &kind.def.vuelo else {
            return Ok(Flight {
                wheels: Vec::new(),
                unload: None,
                stick: [None; 3],
                trans: [None; 3],
                throttle: None,
                sas: None,
                hold: None,
                computer: None,
                profile: None,
                rcs: Vec::new(),
                engines: Vec::new(),
                tilting: false,
                vector: None,
                rates: Vec3::ONE,
                gain: 2.0,
                cols: Vec::new(),
                u: Vec::new(),
                warm: Vec::new(),
                norms: Vec::new(),
                o_mode: None,
                o_ground: None,
                kept: None,
                aloft: 0.0,
            });
        };
        let ids: Vec<String> = kind.machines.iter().map(|m| m.id.clone()).collect();
        let thrusters = |pats: &[String], role: &str, store: &mut Store| -> Result<Vec<Thruster>, String> {
            let mut out = Vec::new();
            for p in pats {
                let found = resolve(&ids, p);
                if found.is_empty() {
                    return Err(format!("vuelo: ninguna máquina '{p}'"));
                }
                for m in found {
                    let m = m as usize;
                    let name = kind.machines[m].def.ordenes.get(role).and_then(|v| v.as_str()).map_or(format!("{}.{role}", ids[m]), str::to_string);
                    let order = store.define(&name);
                    store.claim(order, Writer::World)?;
                    let top = kind.machines[m].def.params.get("estrangulamiento").and_then(|v| v.get(1)).and_then(serde_json::Value::as_f64).unwrap_or(1.05) as f32;
                    out.push(Thruster { machine: m, order, rated: rated(kind, m), top: top.max(0.1) });
                }
            }
            Ok(out)
        };
        let rcs = thrusters(&f.rcs, "orden", store)?;
        let engines = thrusters(&f.motores, "acelerador", store)?;
        let mut wheels = Vec::new();
        for p in &f.ruedas {
            let found = resolve(&ids, p);
            if found.is_empty() {
                return Err(format!("vuelo: ningún giróscopo '{p}'"));
            }
            for m in found {
                let (id, params) = (&ids[m as usize], &kind.machines[m as usize].def.params);
                let num = |key: &str, or: f64| params.get(key).and_then(serde_json::Value::as_f64).unwrap_or(or) as f32;
                let sig = |what: &str, store: &mut Store| -> Result<[SignalId; 3], String> {
                    let mut out = [SignalId::default(); 3];
                    for (k, axis) in ["x", "y", "z"].iter().enumerate() {
                        out[k] = store.define(&format!("{id}.{what}_{axis}"));
                    }
                    Ok(out)
                };
                let cmd = sig("par", store)?;
                for c in cmd {
                    store.claim(c, Writer::World)?;
                }
                wheels.push(Wheel { cmd, h: sig("h", store)?, h_max: num("momento", 2000.0), t_max: num("par", 200.0) });
            }
        }
        let r = f.giro.unwrap_or([25.0, 30.0, 20.0]);
        let o_mode = store.define("vuelo.modo");
        store.claim(o_mode, Writer::World)?;
        let o_ground = store.define("vuelo.tierra");
        store.claim(o_ground, Writer::World)?;
        // engines on a hinge whose order the ship takes from the computer (something of its
        // data — a derived signal, an actuator — names `vuelo.vector`): the computer swings them
        // to where the push is wanted while it holds or the autopilot flies
        let wired = kind.def.derivadas.values().any(|e| e.contains("vuelo.vector")) || kind.actuators.iter().any(|a| a.def.control.orden.as_deref().is_some_and(|o| o.starts_with("vuelo.vector")));
        let hinge = engines.iter().find_map(|e| kind.machines[e.machine].part.and_then(|p| kind.joints.iter().find(|j| j.hinge && j.parts.contains(&p))));
        let vector = match hinge.filter(|j| wired && j.hi > j.lo) {
            Some(j) => {
                let (order, on) = (store.define("vuelo.vector"), store.define("vuelo.vector_on"));
                store.claim(order, Writer::World)?;
                store.claim(on, Writer::World)?;
                Some(Vector { axis: j.axis.normalize_or_zero(), lo: j.lo, span: j.hi - j.lo, pos: store.define(&format!("{}.pos", j.id)), order, on })
            }
            None => None,
        };
        Ok(Flight {
            vector,
            wheels,
            unload: opt(store, &f.descarga),
            stick: [opt(store, &f.cabeceo), opt(store, &f.alabeo), opt(store, &f.guinada)],
            trans: match &f.traslacion {
                Some([x, y, z]) => [Some(store.define(x)), Some(store.define(y)), Some(store.define(z))],
                None => [None; 3],
            },
            throttle: opt(store, &f.acelerador),
            sas: opt(store, &f.estabilizador),
            hold: opt(store, &f.mantener),
            computer: f.ordenador.as_ref().map(|c| store.define(&format!("{c}.on"))),
            profile: opt(store, &f.perfil),
            rcs,
            tilting: engines.iter().any(|e| kind.machines[e.machine].part.is_some_and(|p| kind.joints.iter().any(|j| j.parts.contains(&p)))),
            engines,
            rates: Vec3::new(r[0], r[1], r[2]) * std::f32::consts::PI / 180.0,
            gain: f.ganancia.unwrap_or(2.5),
            cols: Vec::new(),
            u: Vec::new(),
            warm: Vec::new(),
            norms: Vec::new(),
            o_mode: Some(o_mode),
            o_ground: Some(o_ground),
            kept: None,
            aloft: 0.0,
        })
    }

    /// How much of the common throttle each engine gets so that together they push through the
    /// centre of mass, wherever the cargo has put it: the engines are trimmed against each other
    /// (as little as does it — the least change, by least squares — and within `TRIM`) until the
    /// torque they make across their own push is nothing, their sum unchanged. What trimming
    /// cannot take (two engines abreast cannot pitch) is left to the wheels and the thrusters.
    /// An engine that gives nothing while others do is out, and the rest are trimmed without it.
    fn trim(&self, machines: &[MachineRt], s: &Structure) -> [f32; TRIMMED] {
        let mut w = [1.0f32; TRIMMED];
        let n = self.engines.len();
        if !(2..=TRIMMED).contains(&n) {
            return w;
        }
        // the torque each makes per unit of throttle, and which of them run
        let (mut torque, mut live) = ([Vec3::ZERO; TRIMMED], [true; TRIMMED]);
        let (mut push, mut any) = (Vec3::ZERO, false);
        for (k, e) in self.engines.iter().enumerate() {
            let m = &machines[e.machine];
            let Some(p) = m.part.filter(|&p| s.parts[p as usize].alive) else {
                live[k] = false;
                continue;
            };
            let part = &s.parts[p as usize];
            let f = part.local.transform_vector3(m.thrust_axis).normalize_or_zero() * e.rated;
            torque[k] = (part.center - s.com).cross(f);
            push += f;
            live[k] = m.m.thrust() > 0.0;
            any |= live[k];
        }
        if !any {
            // (none lit yet: trimmed as if all were about to be)
            live = [true; TRIMMED];
        }
        let along = push.normalize_or_zero();
        if along == Vec3::ZERO {
            return w;
        }
        // across their push, two ways; in units of a full engine at the hull's own size
        let u = along.any_orthonormal_vector();
        let v = along.cross(u);
        let unit = (push.length() / n as f32 * s.radius.max(1.0)).max(1.0);
        let mut a = [[0.0f32; TRIMMED]; 3];
        let mut b = Vec3::ZERO;
        let count = (0..n).filter(|&k| live[k]).count().max(1) as f32;
        for k in (0..n).filter(|&k| live[k]) {
            a[0][k] = 1.0;
            a[1][k] = torque[k].dot(u) / unit;
            a[2][k] = torque[k].dot(v) / unit;
            b -= Vec3::new(0.0, a[1][k], a[2][k]);
        }
        // what trimming can change of each torque is only what the engines make of it apart from
        // one another: the part they all make alike changes with the sum, which stays (two
        // engines abreast make the same pitch: none of it is trimmed, and the roll is trimmed
        // whole instead of traded against a pitch it cannot reach)
        for row in a.iter_mut().skip(1) {
            let mean = (0..n).filter(|&k| live[k]).map(|k| row[k]).sum::<f32>() / count;
            for k in (0..n).filter(|&k| live[k]) {
                row[k] -= mean;
            }
        }
        // the least change that does it: δ = Aᵀ (A Aᵀ + λ)⁻¹ b
        let mut m = glam::Mat3::ZERO;
        for i in 0..3 {
            for j in 0..3 {
                let dot: f32 = (0..n).map(|k| a[i][k] * a[j][k]).sum();
                m.col_mut(j)[i] = dot + if i == j { 1e-4 } else { 0.0 };
            }
        }
        if m.determinant().abs() < 1e-12 {
            return w;
        }
        let y = m.inverse() * b;
        for k in (0..n).filter(|&k| live[k]) {
            w[k] = (1.0 + a[0][k] * y.x + a[1][k] * y.y + a[2][k] * y.z).clamp(TRIM.0, TRIM.1);
        }
        w
    }

    /// The most torque (N·m, ship frame) its thrusters and wheels can turn it by each way about
    /// each axis now: (positive, negative).
    fn can_turn(&self, machines: &[MachineRt], s: &Structure) -> (Vec3, Vec3) {
        let (mut most, mut least) = (Vec3::ZERO, Vec3::ZERO);
        for r in &self.rcs {
            let m = &machines[r.machine];
            let Some(p) = m.part.filter(|&p| s.parts[p as usize].alive) else { continue };
            let part = &s.parts[p as usize];
            let t = (part.center - s.com).cross(part.local.transform_vector3(m.thrust_axis).normalize_or_zero() * r.rated);
            most += t.max(Vec3::ZERO);
            least += t.min(Vec3::ZERO);
        }
        for w in &self.wheels {
            most += Vec3::splat(w.t_max);
            least -= Vec3::splat(w.t_max);
        }
        // (a ship that has none of these turns by its engines alone: nothing is held back)
        (Vec3::select(most.cmpgt(Vec3::ZERO), most, Vec3::splat(1.0e30)), Vec3::select(least.cmplt(Vec3::ZERO), least, Vec3::splat(-1.0e30)))
    }

    /// Its computer runs (a ship that asks for none always has one).
    pub fn computer_on(&self, store: &Store) -> bool {
        self.computer.is_none_or(|c| store.on(c))
    }

    /// What its main engines push with together at full throttle (N).
    pub fn push(&self) -> f32 {
        self.engines.iter().map(|e| e.rated).sum()
    }

    /// How fast it turns at full stick now (rad/s per axis): what it was built for, by the
    /// profile the pilot chose.
    pub fn rates(&self, store: &Store) -> Vec3 {
        self.rates * self.profile.map_or(1.0, |p| PROFILES[(store.get(p).round().max(0.0) as usize).min(PROFILES.len() - 1)].0)
    }

    /// It swings its engines by itself: its hull need not turn to push another way.
    pub fn vectoring(&self) -> bool {
        self.vector.is_some()
    }

    /// The translation keys as they are held (−1..1 each way, ship frame).
    pub fn keys(&self, store: &Store) -> Vec3 {
        let get = |o: Option<SignalId>| o.map_or(0.0, |s| store.get(s) as f32);
        Vec3::new(get(self.trans[0]), get(self.trans[1]), get(self.trans[2])).clamp(Vec3::splat(-1.0), Vec3::splat(1.0))
    }

    /// The way its engines push together as they stand now (the hull's frame; a tilted nacelle
    /// pushes another way): zero if it has none.
    pub fn axis(&self, machines: &[MachineRt], s: &Structure) -> Vec3 {
        let mut push = Vec3::ZERO;
        for e in &self.engines {
            let m = &machines[e.machine];
            if let Some(p) = m.part.filter(|&p| s.parts[p as usize].alive) {
                push += s.parts[p as usize].local.transform_vector3(m.thrust_axis).normalize_or_zero() * e.rated;
            }
        }
        push.normalize_or_zero()
    }

    /// Commands for this tick, from the controls and how the ship moves.
    /// `ap`: what the autopilot wants (`autopilot::Demand`), if a mode is flying.
    pub fn command(&mut self, kind: &ShipKind, store: &mut Store, machines: &[MachineRt], s: &Structure, b: &Body, ap: Option<&crate::autopilot::Demand>) {
        if self.rcs.is_empty() && self.engines.is_empty() && self.wheels.is_empty() {
            return;
        }
        let get = |o: Option<SignalId>| o.map_or(0.0, |s| store.get(s) as f32);
        let computer = self.computer.is_none_or(|c| store.on(c));
        let sas = computer && self.sas.is_some_and(|s| store.on(s));
        let hold = computer && self.hold.is_some_and(|s| store.on(s));
        // stick: pitch (about x), yaw (about y), roll (about z); deflections −1..1
        let stick = Vec3::new(get(self.stick[0]), get(self.stick[2]), get(self.stick[1]));
        let ap = ap.filter(|_| computer);
        let trans = Vec3::new(get(self.trans[0]), get(self.trans[1]), get(self.trans[2])).clamp(Vec3::splat(-1.0), Vec3::splat(1.0));
        let throttle = get(self.throttle).clamp(0.0, 1.1);
        // what the autopilot wants it to gain over the ground (m/s²): the computer then flies
        // the engines and the thrusters as when it holds, and the lever does not count
        let auto = ap.and_then(|d| d.accel);
        let holding = hold || auto.is_some();
        let (rates, gain) = match self.profile {
            Some(p) => {
                let (r, g) = PROFILES[(store.get(p).round().max(0.0) as usize).min(PROFILES.len() - 1)];
                (self.rates * r, self.gain * g)
            }
            None => (self.rates, self.gain),
        };
        // the mode's turn, unless the pilot has the stick off its centre
        let ap_turn = ap.and_then(|d| d.rate).filter(|_| stick.length_squared() <= 1e-4).map(|r| r.clamp(-rates, rates));
        if let Some(m) = self.o_mode {
            store.set(
                m,
                if !computer {
                    0.0
                } else if holding {
                    3.0
                } else if sas {
                    2.0
                } else {
                    1.0
                },
            );
        }
        let inertia = b.inertia;
        let mass = b.mass as f32;
        // the turn it wants: rate command with the stabiliser, else torque by stick
        // (standing on the ground the stabiliser lets go: the ground holds it, and thrusters
        // fighting the ground walk a ship about. The stick still turns it)
        // (and it counts as standing until it has been clear of the ground a moment: a hull
        // rocking on its feet is not flying)
        self.aloft = if s.grounded { 0.0 } else { (self.aloft + crate::ship::TICK as f32).min(60.0) };
        let landed = self.aloft < AIRBORNE_AFTER;
        if let Some(g) = self.o_ground {
            store.set(g, if landed { 1.0 } else { 0.0 });
        }
        let torque = if computer {
            let mut want = ap_turn.unwrap_or(stick * rates);
            // holding, it stands on its engines: with the stick let go it turns to put their push
            // against gravity (the faster the further off, and no faster than `LEVEL.1`), so that
            // it is they that carry it and not the thrusters that chase the drift of a hull
            // left leaning. (Where nothing weighs there is no upright to keep.)
            if hold && !landed && stick.length_squared() <= 1e-4 && ap_turn.is_none() {
                // (engines that tilt: it is the hull that is kept upright — nacelles laid flat to
                // fly forward must not stand the ship on its tail)
                let (up, along) = ((-b.gravity).normalize_or_zero(), if self.tilting { Vec3::Y } else { self.axis(machines, s) });
                if b.gravity.length_squared() > MIN_GRAVITY * MIN_GRAVITY && along != Vec3::ZERO {
                    want += (along.cross(up) * LEVEL.0).clamp_length_max(LEVEL.1);
                }
            }
            // the stabiliser alone, the stick let go: the attitude it was let go at, kept
            let keeping = sas && !holding && !landed && stick.length_squared() <= 1e-4 && ap_turn.is_none();
            if keeping {
                // (first it stops turning; the attitude it keeps is the one it stopped at)
                if self.kept.is_none() && b.spin.length() < KEEP.2 {
                    self.kept = Some(s.rot);
                }
                if let Some(kept) = self.kept {
                    let (axis, angle) = (s.rot.inverse() * kept).to_axis_angle();
                    let angle = if angle > std::f32::consts::PI { angle - std::f32::consts::TAU } else { angle };
                    want += (axis * angle * KEEP.0).clamp(-rates * KEEP.1, rates * KEEP.1);
                }
            } else {
                self.kept = None;
            }
            let err = if (sas && !landed) || holding && !landed || stick.length_squared() > 1e-4 || (ap_turn.is_some() && !landed) { want - b.spin } else { Vec3::ZERO };
            // (no more turn asked on an axis than its thrusters and wheels can give it in good time:
            // a turn asked that they cannot give would, through the hull's cross inertia, drown
            // what they must do on the other axes — a yaw they cannot make faster rolls it)
            let (most, least) = self.can_turn(machines, s);
            let diag = Vec3::new(inertia.x_axis.x, inertia.y_axis.y, inertia.z_axis.z).max(Vec3::splat(1.0));
            let err = err.clamp(least / diag / gain, most / diag / gain);
            inertia * (err * gain)
        } else {
            stick * 1e6
        };
        // the push it wants: translation by the stick; holding, it fights drift and gravity
        let mut force = trans * mass * 3.0;
        if let Some(a) = auto {
            force = (a - b.gravity) * mass;
        } else if hold {
            let v_want = trans * 4.0;
            force = (v_want - b.vel) * mass * 1.2 - b.gravity * mass;
        }
        // ---- engines that swing: to where the push is wanted, as far as their hinge goes ----
        if let Some(v) = &self.vector {
            store.set(v.on, if holding { 1.0 } else { 0.0 });
            let along = self.axis(machines, s);
            let (want, now) = (force - v.axis * force.dot(v.axis), along - v.axis * along.dot(v.axis));
            // (a push of less than a fiftieth of its weight on the Moon: they stay as they are)
            if holding && want.length() > 0.03 * mass && now.length() > 0.1 {
                let (want, now) = (want.normalize(), now.normalize());
                let q = v.lo + (store.get(v.pos) as f32).clamp(0.0, 1.0) * v.span;
                let to = q + now.cross(want).dot(v.axis).atan2(now.dot(want));
                // (the same way a whole turn round, if that is the one its hinge allows)
                let inside = [to, to - std::f32::consts::TAU, to + std::f32::consts::TAU].into_iter().find(|a| (v.lo..=v.lo + v.span).contains(a));
                let to = inside.unwrap_or(if (to - v.lo).rem_euclid(std::f32::consts::TAU) > std::f32::consts::PI + v.span * 0.5 { v.lo } else { v.lo + v.span });
                store.set(v.order, f64::from(((to - v.lo) / v.span).clamp(0.0, 1.0)));
            }
        }
        // ---- engines: the throttle, or what holding needs along their axes ----
        let com = s.com;
        let up_need = if holding { force } else { Vec3::ZERO };
        let mut engine_force = Vec3::ZERO;
        let mut engine_torque = Vec3::ZERO;
        let trim = if computer { self.trim(machines, s) } else { [1.0; TRIMMED] };
        // what each is asked, trimmed. An engine gives its full throttle and no more: where the
        // trim asks one for more than that (a ship at full power whose centre of mass is not
        // amid its engines: its tanks, its cargo), all of them are taken down together, so that
        // what they give still goes through the centre of mass. It loses a little push for it;
        // it does not pitch
        self.u.clear();
        let mut over = 1.0f32;
        for (k, e) in self.engines.iter().enumerate() {
            let m = &machines[e.machine];
            let dir = m.part.map_or(Vec3::ZERO, |p| s.parts[p as usize].local.transform_vector3(m.thrust_axis).normalize_or_zero());
            let t = if holding {
                // its share of the force along its axis
                (up_need.dot(dir).max(0.0) / (self.engines.len() as f32 * e.rated).max(1.0)).clamp(0.0, 1.05)
            } else {
                throttle
            };
            let t = t * trim.get(k).copied().unwrap_or(1.0);
            over = over.max(t / e.top);
            self.u.push(t);
        }
        for (k, e) in self.engines.iter().enumerate() {
            let m = &machines[e.machine];
            let Some(p) = m.part else { continue };
            let part = &s.parts[p as usize];
            let dir = part.local.transform_vector3(m.thrust_axis).normalize_or_zero();
            let t = self.u[k] / over;
            store.set(e.order, f64::from(t));
            engine_force += dir * t * e.rated;
            // (what it turns the hull by is what it pushes now: an engine spooling up or down
            // pushes other than it was told)
            engine_torque += (part.center - com).cross(dir * m.m.thrust() as f32);
        }
        // ---- wheels: the turn, as far as they can give it ----
        let unload = computer && !self.rcs.is_empty() && self.unload.is_none_or(|s| store.on(s));
        let mut by_wheels = Vec3::ZERO;
        for w in &self.wheels {
            let rest = torque - by_wheels;
            for k in 0..3 {
                let h = store.get(w.h[k]) as f32;
                let mut want = rest[k] + if unload && h.abs() > 0.5 * w.h_max { h * UNLOAD } else { 0.0 };
                want = want.clamp(-w.t_max, w.t_max);
                // full that way (a torque on the hull takes the opposite out of the wheel)
                if h.abs() >= w.h_max * 0.999 && want * h < 0.0 {
                    want = 0.0;
                }
                store.set(w.cmd[k], f64::from(want));
                by_wheels[k] += want;
            }
        }
        // ---- RCS: the rest of the wrench, shared by bounded least squares ----
        let want_f = if holding { force - engine_force } else { force };
        // (what the engines turn it by as they are trimmed — two abreast cannot trim their pitch —
        // is taken out here as it is made, not after it has turned the ship)
        let want_t = torque - by_wheels - if computer { engine_torque } else { Vec3::ZERO };
        let n = self.rcs.len();
        if n == 0 {
            return;
        }
        self.cols.clear();
        let scale = s.radius.max(1.0);
        for r in &self.rcs {
            let m = &machines[r.machine];
            let (pos, dir) = match m.part {
                Some(p) if s.parts[p as usize].alive => {
                    let part = &s.parts[p as usize];
                    (part.center, part.local.transform_vector3(m.thrust_axis).normalize_or_zero())
                }
                _ => (Vec3::ZERO, Vec3::ZERO),
            };
            let f = dir * r.rated;
            let t = (pos - com).cross(f) / scale * TURN_WEIGHT;
            self.cols.push([f.x, f.y, f.z, t.x, t.y, t.z]);
        }
        // (no more push asked of them than they can give that way and still turn the ship: a
        // push far past what they have — the engines swinging round to brake, say — would
        // drown the turning in the sum, and the ship would tumble)
        let want_f = match want_f.try_normalize() {
            Some(d) => want_f.clamp_length_max(RCS_SHARE * self.cols.iter().map(|c| (c[0] * d.x + c[1] * d.y + c[2] * d.z).max(0.0)).sum::<f32>()),
            None => want_f,
        };
        let w = [want_f.x, want_f.y, want_f.z, want_t.x / scale * TURN_WEIGHT, want_t.y / scale * TURN_WEIGHT, want_t.z / scale * TURN_WEIGHT];
        self.u.clear();
        self.u.resize(n, 0.0);
        if !computer {
            // direct: each thruster fires by the sign of its share
            for (k, c) in self.cols.iter().enumerate() {
                let d: f32 = (0..6).map(|i| c[i] * w[i]).sum();
                self.u[k] = if d > 0.0 { 1.0 } else { 0.0 };
            }
        } else {
            allocate(&self.cols, &w, &mut self.warm, &mut self.norms, &mut self.u);
        }
        for (k, r) in self.rcs.iter().enumerate() {
            // (what is asked is what is sent, however little: a nozzle asked for less than its
            // shortest pulse fires in pulses that give it on the whole — `rcs` model)
            store.set(r.order, f64::from(self.u[k]));
        }
        let _ = kind;
    }
}

/// How much more a torque missed counts than a push missed, in the share among the thrusters
/// (torques over the hull's size): a ship whose thrusters cannot turn it without pushing it a
/// little turns, and is pushed a little, rather than let its attitude go.
const TURN_WEIGHT: f32 = 3.0;

/// Sweeps of the share among the thrusters (each thruster set, in turn, to what best makes up
/// what is still missing with the others as they are).
const ALLOC_SWEEPS: usize = 40;
/// What a thruster's share costs in the share (a fraction of what it gives at full): enough that
/// none fires to no purpose, too little to leave anything wanted ungiven.
const ALLOC_THRIFT: f32 = 0.0005;

/// The share of each thruster (0..1, into `u`) that comes nearest to the wrench `w` (force and
/// torque over size, ship frame), each thruster's column of what it gives at full in `cols`: the
/// bounded least squares |A u − w|², solved by cyclic coordinate descent — each thruster in turn
/// set exactly to what best makes up what is missing, within 0..1 — started from what was given
/// last tick (`warm`; `norms`: scratch), each share costing a little (`ALLOC_THRIFT`). Whatever is asked past what they can give one way or the
/// other on an axis is first taken down to what they can (so that a turn they cannot make faster
/// does not drag the other axes along with what it makes on them). A thruster's own share is
/// exact at every step, so it converges in a few sweeps whatever the layout and however unlike
/// the axes are in size.
fn allocate(cols: &[[f32; 6]], w: &[f32; 6], warm: &mut Vec<f32>, norms: &mut Vec<f32>, u: &mut [f32]) {
    let n = cols.len();
    if warm.len() != n {
        warm.clear();
        warm.resize(n, 0.0);
    }
    let mut w = *w;
    for i in 0..6 {
        let (most, least) = cols.iter().fold((0.0f32, 0.0f32), |(hi, lo), c| (hi + c[i].max(0.0), lo + c[i].min(0.0)));
        w[i] = w[i].clamp(least, most);
    }
    norms.clear();
    norms.extend(cols.iter().map(|c| c.iter().map(|x| x * x).sum::<f32>()));
    u.copy_from_slice(&warm[..n]);
    // what is missing: w − A u
    let mut r = w;
    for (k, c) in cols.iter().enumerate() {
        for i in 0..6 {
            r[i] -= c[i] * u[k];
        }
    }
    for _ in 0..ALLOC_SWEEPS {
        let mut moved = 0.0f32;
        for (k, c) in cols.iter().enumerate() {
            if norms[k] < 1e-12 {
                continue;
            }
            // (what it burns counts a little: no two thrusters pushing against each other, and
            // none firing for what is not worth it)
            let g: f32 = (0..6).map(|i| c[i] * r[i]).sum::<f32>() - ALLOC_THRIFT * norms[k];
            let next = (u[k] + g / norms[k]).clamp(0.0, 1.0);
            let d = next - u[k];
            if d != 0.0 {
                for i in 0..6 {
                    r[i] -= c[i] * d;
                }
                u[k] = next;
                moved = moved.max(d.abs());
            }
        }
        if moved < 1e-5 {
            break;
        }
    }
    warm[..n].copy_from_slice(u);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sixteen thrusters in four quads at the corners of a hull 12 m long and 4 wide, as a
    /// fighter's: (where, which way it pushes), and their columns as the computer builds them.
    fn quads() -> Vec<[f32; 6]> {
        let mut cols = Vec::new();
        for (x, z) in [(1.0f32, 5.0f32), (-1.0, 5.0), (1.0, -5.0), (-1.0, -5.0)] {
            for d in [Vec3::Y, -Vec3::Y, Vec3::X * x.signum(), Vec3::Z * z.signum()] {
                let (f, t) = (d * 2200.0, Vec3::new(x, 0.3, z).cross(d * 2200.0) / 6.0 * TURN_WEIGHT);
                cols.push([f.x, f.y, f.z, t.x, t.y, t.z]);
            }
        }
        cols
    }

    #[test]
    fn a_small_turn_is_given_whole_and_pushes_nothing() {
        let cols = quads();
        let (mut warm, mut norms, mut u) = (Vec::new(), Vec::new(), vec![0.0; cols.len()]);
        // a little pitch (what a push 4 mm off the centre of mass makes), no push
        let w = [0.0, 0.0, 0.0, 440.0 / 6.0 * TURN_WEIGHT, 0.0, 0.0];
        let t0 = std::time::Instant::now();
        let n = 10_000;
        for _ in 0..n {
            allocate(&cols, &w, &mut warm, &mut norms, &mut u);
        }
        let each = t0.elapsed().as_secs_f64() / f64::from(n) * 1e6;
        let mut got = [0.0f32; 6];
        for (k, c) in cols.iter().enumerate() {
            for i in 0..6 {
                got[i] += c[i] * u[k];
            }
        }
        eprintln!("reparto de 16 toberas: {each:.2} µs; da {got:.1?} de {w:.1?}");
        assert!((got[3] - w[3]).abs() < 0.02 * w[3], "da {:.1} de {:.1} de cabeceo", got[3], w[3]);
        assert!(got[0].abs() + got[1].abs() + got[2].abs() < 50.0, "empuja {:.1?} sin que se pida", &got[..3]);
        // and from cold (no share of the last tick to start from) as well as warm
        let (mut warm, mut u2) = (Vec::new(), vec![0.0; cols.len()]);
        allocate(&cols, &w, &mut warm, &mut norms, &mut u2);
        let given: f32 = cols.iter().zip(&u2).map(|(c, x)| c[3] * x).sum();
        assert!((given - w[3]).abs() < 0.02 * w[3], "en frío da {given:.1} de {:.1}", w[3]);
    }
}
