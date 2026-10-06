//! Hands on a ship's controls: whoever works a control of a panel has a hand go to it and do it.
//! The index presses a button's cap and comes back with it, thumb and index take a toggle and
//! go where it goes, a hand closes round a knob and turns with it, a fist takes a lever, both
//! hands take a valve's wheel by its rim and go round with it. The control itself acts at once,
//! as it always did: the hand is what is seen of it, and never holds it back.
//!
//! Nothing here knows a control kind, a ship or a panel. What a control is to a hand comes from
//! the ship (`lunar_ship::scene::Handle`: the point of its moving part a hand works, the way out
//! of it, how it lies, how far it is turned), and how a hand works each kind is data
//! (`assets/defs/manos.jsonc`, `mandos`): one of two ways of taking something — with the tips
//! of the fingers (`dedo`) or in the fist (`puno`) —, which part of the hand goes on it, the
//! poses of the fingers on it and before it, and whether both hands go.
//!
//! A hand works one control at a time (`Job`): it goes to it, stays on it while it is held and
//! a moment more (`toca`), follows its moving part wherever that goes, then waits by it
//! (`separa` off it) for a while (`queda`) in case there is more to do — at the control aimed
//! at, if any — and goes back to where it was. The fingertip is put on the control's own face
//! and comes to it from outside: it is never behind it.
use crate::{
    handwork::{Arm, Limits, Target, Tip, fist, poke},
    ships::Ships,
};
use glam::{DVec3, Quat, Vec3};
use lunar_controls::Pose;
use lunar_core::{anim::spring::Spring, structure::set::Structures};
use serde::Deserialize;
use std::collections::BTreeMap;

/// How a hand takes something.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Take {
    /// With the tips of its fingers, which come at it from outside.
    #[default]
    Dedo,
    /// In the fist, closed round it.
    Puno,
}

/// The way what a fist closes round runs.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Along {
    /// The way out of the control: a stick.
    #[default]
    Fuera,
    /// The control's own axis: the bar of a T handle.
    Eje,
    /// Round the control, `radius` from its middle: the rim of a wheel.
    Aro,
}

/// How a hand works a kind of control (`mandos`).
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkDef {
    #[serde(default)]
    pub toma: Take,
    /// With the fingertips: which part of the hand goes on it.
    #[serde(default)]
    pub punta: Tip,
    /// The fingers on it, and before taking it (hand poses of the rig; none: the same).
    pub pose: String,
    #[serde(default)]
    pub antes: Option<String>,
    #[serde(default)]
    pub largo: Along,
    /// Both hands take it, if both are free.
    #[serde(default)]
    pub dos_manos: bool,
    /// How far a hand turns with it before it lets go and takes it again (degrees; 0: it does
    /// not turn with it).
    #[serde(default)]
    pub gira: f32,
}

/// Hands on controls, as the data has it (`manos.jsonc`).
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReachDef {
    /// How far off a control a hand waits before and after working it (m).
    pub separa: f32,
    /// The least a hand stays on what it worked (s), and how long it waits by it after (s).
    pub toca: f32,
    pub queda: f32,
    /// Half-lives (s): of the way from off a control to on it, and of the hand after a moving
    /// part that jumps (a toggle thrown).
    pub entra: f32,
    pub sigue: f32,
    /// How far the way a fingertip comes leans from the control's own way out toward the arm
    /// (degrees), the most a wrist bends (degrees), the share of its length an arm stretches.
    pub inclina: f32,
    pub muneca: f32,
    pub brazo: f32,
    /// The share of its length an arm is held out toward a control well out of its reach.
    #[serde(default = "far")]
    pub lejos: f32,
    /// Waiting by a panel, the hand goes to the control aimed at.
    #[serde(default)]
    pub mira: bool,
    /// By control kind (`<kind>` or `<kind>/<axes>`), and any other.
    pub mandos: BTreeMap<String, WorkDef>,
    pub otro: WorkDef,
}

fn far() -> f32 {
    0.8
}

/// A kind's way, its poses found.
#[derive(Clone, Debug)]
pub struct Work {
    kind: String,
    axes: Option<u8>,
    pub take: Take,
    pub tip: Tip,
    pub pose: [f32; 5],
    pub before: [f32; 5],
    pub along: Along,
    pub two: bool,
    /// rad.
    pub turn: f32,
}

/// `ReachDef` ready to use: its poses found in a rig's.
#[derive(Clone, Debug)]
pub struct ReachKit {
    pub gap: f32,
    pub touch: f32,
    pub stay: f32,
    pub enter: f32,
    pub follow: f32,
    pub limits: Limits,
    pub aim: bool,
    /// The last is for a kind not listed.
    pub works: Vec<Work>,
}

impl ReachKit {
    pub fn new(def: &ReachDef, poses: &BTreeMap<String, [f32; 5]>) -> Result<ReachKit, String> {
        let pose = |name: &str| poses.get(name).copied().ok_or_else(|| format!("manos: no hay pose de mano '{name}' en el esqueleto"));
        let work = |key: &str, w: &WorkDef| -> Result<Work, String> {
            let (kind, axes) = match key.split_once('/') {
                Some((k, n)) => (k, Some(n.parse::<u8>().map_err(|_| format!("manos: '{key}' (es <tipo> o <tipo>/<ejes>)"))?)),
                None => (key, None),
            };
            let on = pose(&w.pose)?;
            Ok(Work { kind: kind.to_string(), axes, take: w.toma, tip: w.punta, pose: on, before: w.antes.as_deref().map(pose).transpose()?.unwrap_or(on), along: w.largo, two: w.dos_manos, turn: w.gira.to_radians() })
        };
        let mut works = def.mandos.iter().map(|(k, w)| work(k, w)).collect::<Result<Vec<_>, _>>()?;
        // (the ones that say how many axes before the ones that do not)
        works.sort_by_key(|w| w.axes.is_none());
        works.push(work("", &def.otro)?);
        if !(def.brazo > 0.5 && def.brazo <= 1.0) || def.separa < 0.0 || def.toca < 0.0 {
            return Err("manos: 'brazo' es la parte del brazo que se estira (de 0,5 a 1); 'separa' y 'toca' no son negativos".into());
        }
        Ok(ReachKit { gap: def.separa, touch: def.toca, stay: def.queda, enter: def.entra, follow: def.sigue, limits: Limits { lean: def.inclina.to_radians(), wrist: def.muneca.to_radians(), arm: def.brazo, far: def.lejos.clamp(0.4, 1.0) }, aim: def.mira, works })
    }

    /// The way a hand works a control of `kind` with so many axes: its place in `works`.
    pub fn work_of(&self, kind: &str, axes: u8) -> usize {
        self.works.iter().position(|w| w.kind == kind && w.axes.is_none_or(|a| a == axes)).unwrap_or(self.works.len() - 1)
    }
}

/// A control: its ship's structure, which of its controls, which element of it.
pub type Control = (u64, u16, u8);

/// The control a hand is at.
#[derive(Clone, Copy, Debug)]
struct Job {
    structure: u64,
    control: u16,
    elem: u8,
    work: usize,
    hands: [bool; 2],
    /// On it (else waiting by it), and for how long.
    touching: bool,
    t: f32,
    /// Its moving part as the hand follows it (`Pose::e`).
    e: [Spring; 4],
    /// 0 on it .. 1 off it by `gap`.
    off: Spring,
    /// The turn at which the hand took it, how far it has turned with it since, and how long
    /// ago it let go to take it again (s).
    base: f32,
    roll: Spring,
    retake: f32,
}

/// What a body's hands do at the controls.
#[derive(Clone, Debug, Default)]
pub struct Reach {
    job: Option<Job>,
    /// Worked since the last frame: (structure, control, element if known).
    asked: Option<(u64, u16, Option<u8>)>,
    /// How far short of the control the hand is left (m; 0: on it): for whoever checks reach.
    pub short: [f32; 2],
}

/// Seconds a hand has its fingers open while it takes a turning thing again.
const RETAKE: f32 = 0.12;
/// With both hands free, the one that already has it keeps it unless the other is nearer by
/// this (m).
const KEEP: f32 = 0.12;

/// Which hands go to something `d` m from each shoulder: both if it takes two and both are
/// free, else the nearer free one (`keep`: the ones already there).
pub fn choose(two: bool, free: [bool; 2], d: [f32; 2], keep: [bool; 2]) -> [bool; 2] {
    match free {
        [true, true] if two => [true, true],
        [true, true] => {
            let d = [d[0] - if keep[0] { KEEP } else { 0.0 }, d[1] - if keep[1] { KEEP } else { 0.0 }];
            if d[0] <= d[1] { [true, false] } else { [false, true] }
        }
        free => free,
    }
}

/// What a frame of it needs of the world.
pub struct Scene<'a> {
    pub ships: &'a Ships,
    pub set: &'a Structures,
    /// The control aimed at and the one held down (the player's own; none for anyone else).
    pub aim: Option<Control>,
    pub held: Option<(u64, u16)>,
    /// The seat the body is in, if a ship is flown from it: what its keys fly is worked from
    /// the stick and the throttle, and no hand goes to it.
    pub flown: Option<&'a crate::cockpit::Seated>,
}

impl Reach {
    /// The body's player worked this control (a click, a turn of the wheel): `elem` its element
    /// if known (a bezel's button, a keypad's key).
    pub fn worked(&mut self, structure: u64, control: u16, elem: Option<u8>) {
        self.asked = Some((structure, control, elem));
    }

    /// A hand is at a control, or about to be.
    pub fn active(&self) -> bool {
        self.job.is_some() || self.asked.is_some()
    }

    /// The control the hands are at.
    pub fn at(&self) -> Option<Control> {
        self.job.map(|j| (j.structure, j.control, j.elem))
    }

    /// On it (not waiting by it).
    pub fn touching(&self) -> bool {
        self.job.is_some_and(|j| j.touching)
    }

    /// Nothing at hand any more.
    pub fn drop(&mut self) {
        self.job = None;
        self.asked = None;
    }

    fn start(kit: &ReachKit, sc: &Scene, (structure, control, elem): (u64, u16, Option<u8>), touching: bool, hands: [bool; 2]) -> Option<Job> {
        let sh = &sc.ships.list[sc.ships.by_structure(structure)?];
        let c = sh.panels.controls.get(usize::from(control))?;
        let d = &sh.kind.panels[c.panel].def.mandos[c.index];
        // (not told which element: a bezel's is the button of the page it shows, a keypad's its ENTER)
        let elem = elem.unwrap_or(match d.kind.as_str() {
            "bisel" => c.st.x as u8,
            "teclado" => 12,
            _ => 0,
        });
        let pose = sh.control_pose(usize::from(control));
        let h = sh.handle(sc.set.get(structure)?, usize::from(control), elem, &pose)?;
        Some(Job { structure, control, elem, work: kit.work_of(&d.kind, d.ejes.unwrap_or(1)), hands, touching, t: 0.0, e: pose.e.map(Spring::at), off: Spring::at(1.0), base: h.turn, roll: Spring::default(), retake: RETAKE })
    }

    /// A frame of it: where each hand (left, right) is asked to be, if at a control. `free`:
    /// the hands with nothing else in them.
    pub fn update(&mut self, dt: f32, kit: &ReachKit, sc: &Scene, arms: &[Arm; 2], free: [bool; 2]) -> [Option<Target>; 2] {
        self.short = [0.0; 2];
        let flown = |structure: u64, control: u16| sc.flown.is_some_and(|s| s.structure == structure && s.flies(control));
        // worked: on it (the same control worked again: on it for longer)
        let mut taken = false;
        if let Some(asked) = self.asked.take().filter(|a| !flown(a.0, a.1)) {
            match self.job {
                Some(mut j) if (j.structure, j.control) == (asked.0, asked.1) => {
                    (j.touching, j.t) = (true, 0.0);
                    j.elem = asked.2.unwrap_or(j.elem);
                    self.job = Some(j);
                }
                was => (self.job, taken) = (Reach::start(kit, sc, asked, true, was.map_or([false; 2], |j| j.hands)), true),
            }
        }
        let Some(mut j) = self.job else { return [None, None] };
        // on it while it is held and a moment more; then by it for a while, at what is aimed at
        j.t += dt;
        let held = sc.held == Some((j.structure, j.control));
        if j.touching && !held && j.t >= kit.touch {
            (j.touching, j.t) = (false, 0.0);
        }
        if !j.touching {
            if j.t >= kit.stay {
                self.job = None;
                return [None, None];
            }
            if let Some(aim) = sc.aim.filter(|a| kit.aim && (a.0, a.1, a.2) != (j.structure, j.control, j.elem) && !flown(a.0, a.1))
                && let Some(next) = Reach::start(kit, sc, (aim.0, aim.1, Some(aim.2)), false, j.hands)
            {
                (j, taken) = (Job { t: j.t, ..next }, true);
            }
        }
        let (Some(n), Some(s)) = (sc.ships.by_structure(j.structure), sc.set.get(j.structure)) else {
            self.job = None;
            return [None, None];
        };
        let sh = &sc.ships.list[n];
        // its moving part, as the hand follows it
        let now = sh.control_pose(usize::from(j.control));
        let mut pose = Pose::default();
        for k in 0..4 {
            pose.e[k] = j.e[k].step(now.e[k], kit.follow, dt);
        }
        let Some(h) = sh.handle(s, usize::from(j.control), j.elem, &pose) else {
            self.job = None;
            return [None, None];
        };
        let w = &kit.works[j.work];
        let (at, out, axis, normal) = (s.to_world(h.at), s.rot * h.out, s.rot * h.axis, s.rot * h.normal);
        // whose hands: decided as it is taken, and again if one of them is no longer free
        let d = [0, 1].map(|i| (at - arms[i].shoulder).length() as f32);
        if taken || j.hands == [false; 2] || (j.hands[0] && !free[0]) || (j.hands[1] && !free[1]) || (w.two && j.hands != free) {
            j.hands = choose(w.two, free, d, j.hands);
        }
        if j.hands == [false; 2] {
            // (both busy: the control is worked all the same, by no hand)
            self.job = None;
            return [None, None];
        }
        // how far it has turned in the hand: so far and no more, then the hand takes it again
        j.retake += dt;
        if w.turn > 0.0 && j.touching {
            if (h.turn - j.base).abs() > w.turn {
                (j.base, j.retake) = (h.turn, 0.0);
            }
        } else {
            j.base = h.turn;
        }
        let roll = j.roll.step(h.turn - j.base, kit.follow, dt);
        let closed = j.touching && j.retake >= RETAKE;
        let off = j.off.step(if j.touching { 0.0 } else { 1.0 }, kit.enter, dt).clamp(0.0, 1.0) * kit.gap;
        let fingers = if closed { w.pose } else { w.before };
        let mut targets = [None, None];
        for i in 0..2 {
            if !j.hands[i] {
                continue;
            }
            let arm = &arms[i];
            let placed = match (w.take, w.along) {
                (Take::Dedo, _) => poke(arm, &kit.limits, w.tip, fingers, at, out, off, roll),
                // (the palm inward round a stick; down on a bar, and toward what it stands on)
                (Take::Puno, Along::Fuera) => fist(arm, &kit.limits, fingers, at, out, arm.inward() + arm.ahead() * 0.3, h.radius, off),
                (Take::Puno, Along::Eje) => fist(arm, &kit.limits, fingers, at, axis, -arm.up() * 0.8 - out * 0.5 + arm.inward() * 0.3, h.radius, off),
                (Take::Puno, Along::Aro) => {
                    // its own side of the rim (the only hand: the side it is nearer), gone round with it
                    let left = arm.left();
                    let across = (left - normal * left.dot(normal)).normalize_or(axis);
                    let own = if i == 0 { 1.0 } else { -1.0 };
                    let spoke = Quat::from_axis_angle(normal, roll) * across * own;
                    let on = at + (spoke * h.radius).as_dvec3();
                    fist(arm, &kit.limits, fingers, on, normal.cross(spoke), -spoke * 0.6 - normal * 0.8, RIM, off)
                }
            };
            let mut t = placed.target;
            // never behind the plate it stands on
            let deep = (placed.point - s.to_world(h.plate)).as_vec3().dot(normal);
            if deep < 0.0 {
                t.at -= (normal * deep).as_dvec3();
            }
            self.short[i] = placed.short;
            targets[i] = Some(t);
        }
        self.job = Some(j);
        targets
    }
}

/// Half the thickness of a wheel's rim in a hand (m).
const RIM: f32 = 0.012;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_nearer_free_hand_is_chosen_and_both_for_what_takes_two() {
        // both free: the nearer
        assert_eq!(choose(false, [true, true], [0.4, 0.6], [false; 2]), [true, false]);
        assert_eq!(choose(false, [true, true], [0.7, 0.6], [false; 2]), [false, true]);
        // the one that has it keeps it unless the other is clearly nearer
        assert_eq!(choose(false, [true, true], [0.65, 0.6], [true, false]), [true, false]);
        assert_eq!(choose(false, [true, true], [0.9, 0.6], [true, false]), [false, true]);
        // one busy (a tool in it): the other, however far; both busy: none
        assert_eq!(choose(false, [false, true], [0.2, 0.9], [false; 2]), [false, true]);
        assert_eq!(choose(true, [true, false], [0.9, 0.2], [false; 2]), [true, false]);
        assert_eq!(choose(true, [false, false], [0.2, 0.2], [true, true]), [false, false]);
        // a wheel: both, if both are free
        assert_eq!(choose(true, [true, true], [0.4, 0.6], [false; 2]), [true, true]);
    }

    #[test]
    fn a_kind_is_found_by_its_name_and_its_axes_and_any_other_has_a_way_too() {
        let def: ReachDef = lunar_core::defs::parse(
            "manos",
            r#"{ "separa": 0.05, "toca": 0.15, "queda": 1.0, "entra": 0.02, "sigue": 0.02, "inclina": 40, "muneca": 55, "brazo": 0.97,
                 "mandos": { "palanca": { "toma": "puno", "pose": "agarre", "largo": "eje" }, "palanca/2": { "toma": "puno", "pose": "agarre" }, "rueda": { "punta": "garra", "pose": "pomo", "antes": "abierta", "gira": 45 } },
                 "otro": { "pose": "apuntar" } }"#,
        )
        .unwrap();
        let poses: BTreeMap<String, [f32; 5]> = [("agarre", [0.5; 5]), ("pomo", [0.3; 5]), ("abierta", [-0.8; 5]), ("apuntar", [0.6, -0.9, 0.9, 0.9, 0.9])].into_iter().map(|(k, v)| (k.to_string(), v)).collect();
        let kit = ReachKit::new(&def, &poses).unwrap();
        let stick = &kit.works[kit.work_of("palanca", 2)];
        assert!(stick.take == Take::Puno && stick.along == Along::Fuera);
        assert_eq!(kit.works[kit.work_of("palanca", 1)].along, Along::Eje);
        let knob = &kit.works[kit.work_of("rueda", 1)];
        assert!(knob.tip == Tip::Garra && knob.before == [-0.8; 5] && (knob.turn - 45f32.to_radians()).abs() < 1e-6);
        // a kind nobody wrote: the index on it
        let other = &kit.works[kit.work_of("manivela", 1)];
        assert!(other.take == Take::Dedo && other.tip == Tip::Indice && other.pose[1] < 0.0);
        // a pose the rig has not is said
        let mut bad = def.clone();
        bad.otro.pose = "nada".into();
        assert!(ReachKit::new(&bad, &poses).unwrap_err().contains("nada"));
    }
}
