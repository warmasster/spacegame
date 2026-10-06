//! The computer on the suit's wrist. A key raises the forearm that wears it in front of the
//! visor, the head turns to it, and its screen is read: the suit's oxygen and pressure, the gas
//! of its pack, the time out, and the ship one is in or nearest to — its electrical balance,
//! the air of its compartments. The index of the other hand taps it to turn the page.
//!
//! All of it is data (`assets/defs/muneca.jsonc`): where the arm goes when it is raised, where
//! the device sits (a point of the rig, on a forearm: `rigs/*.jsonc`), its model, its screen,
//! and its pages — a title and rows, each a label and what it shows: one of the suit's values
//! by name, a signal of the ship in its own unit, or a row for each of the ship's compartments.
//! The screen is drawn as every display aboard is: lit plates and glyphs of the silkscreen font
//! (`lunar_core::props`), in the device's own frame.
//!
//! The suit's oxygen is a clock for now (`Suit`): so many hours from full, filled again in air
//! that can be breathed. Nothing happens when it runs out.
use crate::{
    body::{Body, Stance},
    handwork::{Arm, Limits, Target, Tip, held, poke},
    holding::turned,
    rig::RigDef,
};
use glam::{DVec3, Quat, Vec3};
use lunar_core::{
    anim::skeleton::frame_turn,
    font::{Font, Placed},
    props::{BOX, GlyphQuad, Prop, PropFrame, PropScene},
};
use lunar_ship::Ship;
use lunar_signals::units::{self, Unit};
use serde::Deserialize;
use std::fmt::Write as _;

/// The suit's own values a row may show.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
pub enum SuitValue {
    /// What is left of its oxygen (a share), and for how long (s).
    #[serde(rename = "traje.oxigeno")]
    Oxygen,
    #[serde(rename = "traje.autonomia")]
    OxygenTime,
    /// Its own pressure (Pa).
    #[serde(rename = "traje.presion")]
    Pressure,
    /// The gas of its pack (a share).
    #[serde(rename = "traje.gas")]
    Gas,
    /// How long it has been out (s).
    #[serde(rename = "traje.reloj")]
    Clock,
    /// The air round it (Pa), and what things weigh there (m/s²).
    #[serde(rename = "exterior.presion")]
    Outside,
    #[serde(rename = "exterior.gravedad")]
    Gravity,
}

/// A row of a page. One of `valor` (the suit's), `senal` (the ship's) or `cada` (a row for
/// each of the ship's compartments: `senal` then names its signal, `{id}` its id).
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RowDef {
    #[serde(default)]
    pub rotulo: String,
    #[serde(default)]
    pub valor: Option<SuitValue>,
    #[serde(default)]
    pub senal: Option<String>,
    #[serde(default)]
    pub cada: Option<String>,
    /// The unit a suit's value is shown in (`reloj`: hours, minutes and seconds); a signal has
    /// its own.
    #[serde(default)]
    pub unidad: String,
    #[serde(default)]
    pub decimales: usize,
    /// Shown in amber and in red under these (in the unit shown); over them with `sobre`.
    #[serde(default)]
    pub ambar: Option<f64>,
    #[serde(default)]
    pub rojo: Option<f64>,
    #[serde(default)]
    pub sobre: bool,
    /// From this value on (SI), these words instead of the number (a battery that is not
    /// draining has no time left to show).
    #[serde(default)]
    pub desde: Option<(f64, String)>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PageDef {
    pub titulo: String,
    /// The ship's: shown only with a ship about (its name under the title).
    #[serde(default)]
    pub nave: bool,
    pub filas: Vec<RowDef>,
}

/// Its screen on the device, in the frame of the rig's point it sits at (x: the point's
/// `traves`, y: the way its `palma` faces, z: the two crossed; m of the world): its middle, its
/// size, the way a line of it runs and the way up it.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScreenDef {
    pub en: [f32; 3],
    pub tamano: [f32; 2],
    pub derecha: [f32; 3],
    pub arriba: [f32; 3],
}

/// Raised to be read: where the middle of its SCREEN goes from the eyes (x toward the side of
/// the arm that wears it, y up, z ahead; m), the way the elbow goes if it is to be said (else
/// wherever turns the screen most to the eyes), and the pose of that hand. The hand itself is
/// as its forearm has it: its wrist at ease.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RaisedDef {
    pub en: [f32; 3],
    #[serde(default)]
    pub codo: Option<[f32; 3]>,
    pub pose: String,
}

/// The tap that turns the page: the other hand's pose, how long it takes (s) and how far off
/// the screen the finger starts and ends (m).
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TapDef {
    pub pose: String,
    pub dura: f32,
    pub separa: f32,
}

/// The suit's oxygen: hours from full, the seconds it takes to fill in air that can be
/// breathed (over `aire` kPa), and the suit's own pressure (kPa).
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SuitDef {
    pub oxigeno: f32,
    pub recarga: f32,
    pub aire: f32,
    pub presion: f32,
}

/// `muneca.jsonc`.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WristDef {
    /// `assets/models/<modelo>.glb` (its mesh `cuerpo`); without it, a plain box.
    #[serde(default)]
    pub modelo: Option<String>,
    /// The rig's point it sits at, and how the model is turned there (degrees).
    pub punto: String,
    #[serde(default)]
    pub giro: [f32; 3],
    /// Whose arm wears it: `izq` or `der`.
    pub brazo: String,
    pub pantalla: ScreenDef,
    pub arriba: RaisedDef,
    pub toque: TapDef,
    pub traje: SuitDef,
    /// How often what it shows is read again (s).
    pub refresco: f32,
    pub paginas: Vec<PageDef>,
}

#[derive(Clone, Debug)]
struct Row {
    def: RowDef,
    unit: Option<Unit>,
}

/// `WristDef` ready to use.
#[derive(Clone, Debug)]
pub struct WristKit {
    pub def: WristDef,
    /// The arm that wears it (0 left).
    pub arm: usize,
    raised: [f32; 5],
    tapping: [f32; 5],
    pages: Vec<Vec<Row>>,
    model_turn: Quat,
}

impl WristKit {
    pub fn new(def: &WristDef, rig: &RigDef) -> Result<WristKit, String> {
        let pose = |name: &str| rig.manos.get(name).copied().ok_or_else(|| format!("muneca: no hay pose de mano '{name}' en el esqueleto"));
        let arm = match def.brazo.as_str() {
            "izq" => 0,
            "der" => 1,
            other => return Err(format!("muneca: brazo '{other}' (izq o der)")),
        };
        if !rig.puntos.contains_key(&def.punto) {
            return Err(format!("muneca: el esqueleto no tiene el punto '{}'", def.punto));
        }
        if def.paginas.is_empty() || def.paginas.len() > 200 {
            return Err("muneca: de 1 a 200 páginas".into());
        }
        let mut pages = Vec::new();
        for p in &def.paginas {
            let mut rows = Vec::new();
            for r in &p.filas {
                let which = u8::from(r.valor.is_some()) + u8::from(r.senal.is_some() && r.cada.is_none()) + u8::from(r.cada.is_some());
                if which != 1 || (r.cada.is_some() && (r.senal.is_none() || r.cada.as_deref() != Some("compartimento"))) {
                    return Err(format!("muneca: página {}: una fila enseña un 'valor' del traje, una 'senal' de la nave o 'cada': \"compartimento\" con su 'senal'", p.titulo));
                }
                let unit = match (&r.valor, r.unidad.as_str()) {
                    (Some(_), "reloj") | (None, _) => None,
                    (Some(_), name) => Some(units::unit(name).map_err(|e| format!("muneca: página {}: {}", p.titulo, e.0))?),
                };
                rows.push(Row { def: r.clone(), unit });
            }
            pages.push(rows);
        }
        Ok(WristKit { def: def.clone(), arm, raised: pose(&def.arriba.pose)?, tapping: pose(&def.toque.pose)?, pages, model_turn: turned(def.giro) })
    }

    /// The pose of the hand that taps it.
    #[cfg(test)]
    pub fn tap_pose(&self) -> [f32; 5] {
        self.tapping
    }
}

/// The suit's oxygen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Suit {
    /// What is left (a share of full).
    pub oxygen: f32,
}

impl Default for Suit {
    fn default() -> Suit {
        Suit { oxygen: 1.0 }
    }
}

impl Suit {
    /// `dt` s in air of `kpa` round the suit.
    pub fn tick(&mut self, dt: f32, def: &SuitDef, kpa: f32) {
        let rate = if kpa >= def.aire { 1.0 / def.recarga.max(1.0) } else { -1.0 / (def.oxigeno.max(0.01) * 3600.0) };
        self.oxygen = (self.oxygen + rate * dt).clamp(0.0, 1.0);
    }
}

/// What the suit knows this frame (SI).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Readings {
    pub oxygen: f32,
    /// The pack's gas (a share), if the suit has one.
    pub gas: Option<f32>,
    /// Seconds out.
    pub clock: f64,
    /// The air round the suit (kPa) and the pull there (m/s²).
    pub outside: f32,
    pub gravity: f32,
}

/// The device on a body's wrist: up or down, the page it shows, a tap under way.
#[derive(Clone, Copy, Debug, Default)]
pub struct Wrist {
    pub up: bool,
    pub page: u8,
    /// Seconds into a tap (0: none).
    tap: f32,
    turned: bool,
}

/// Colours of its screen: glass, text, dim text, and the three levels.
const GLASS: [u8; 3] = [5, 12, 13];
const INK: [u8; 3] = [200, 236, 240];
const DIMMED: [u8; 3] = [96, 140, 150];
const LEVELS: [[u8; 3]; 3] = [[110, 255, 150], [255, 190, 60], [255, 70, 50]];

impl Wrist {
    /// The other hand's index taps the screen (it turns the page as it lands). Nothing with the
    /// device down.
    pub fn tap(&mut self) -> bool {
        if self.up && self.tap == 0.0 {
            (self.tap, self.turned) = (1e-6, false);
        }
        self.up
    }

    /// Where the middle of its screen is (world), the way out of it, the way its lines run and
    /// the way up them, on `body` as it was last posed.
    pub fn screen(kit: &WristKit, body: &Body, st: &Stance) -> Option<(DVec3, Vec3, Vec3, Vec3)> {
        let (at, normal, across) = body.point(&kit.def.punto, st)?;
        let rot = frame_turn(Vec3::Y, Vec3::X, normal, across);
        let s = &kit.def.pantalla;
        let right = (rot * Vec3::from(s.derecha)).normalize_or(across);
        let up = rot * Vec3::from(s.arriba);
        let up = (up - right * up.dot(right)).normalize_or(normal.cross(right));
        Some((at + (rot * Vec3::from(s.en)).as_dvec3(), right.cross(up), right, up))
    }

    /// A frame of it: where each hand (left, right) is asked to be — the arm that wears it
    /// raised, the other's index on its screen for a tap. `free`: the hands with nothing else
    /// in them.
    pub fn update(&mut self, dt: f32, kit: &WristKit, lim: &Limits, body: &Body, st: &Stance, arms: &[Arm; 2], free: [bool; 2]) -> [Option<Target>; 2] {
        let mut out = [None, None];
        if !self.up {
            self.tap = 0.0;
            return out;
        }
        let (worn, other) = (kit.arm, 1 - kit.arm);
        if !free[worn] {
            return out;
        }
        let arm = &arms[worn];
        let d = &kit.def.arriba;
        let side = -arm.inward();
        let world = |v: [f32; 3]| side * v[0] + arm.up() * v[1] + arm.ahead() * v[2];
        // (the eyes: where the stance has them — sat or crouched they are not a standing body's
        // height over its feet)
        let eyes = st.eye;
        let own = if worn == 0 { 1.0 } else { -1.0 };
        let screen = eyes + world(d.en).as_dvec3();
        let elbow = d.codo.map(|v| arm.rot * Vec3::new(v[0] * own, v[1], v[2]));
        out[worn] = Some(match raised(arm, kit, screen, eyes, elbow) {
            Some(t) => t,
            // (a rig whose arm cannot be told: the hand there, its palm down)
            None => held(arm, screen, -arm.up(), arm.inward(), kit.raised, None),
        });
        // the tap: the other index to the screen and away
        if self.tap > 0.0 {
            self.tap += dt;
            let k = self.tap / kit.def.toque.dura.max(0.05);
            if k >= 0.5 && !self.turned {
                self.turned = true;
                self.page = (self.page + 1) % kit.pages.len() as u8;
            }
            if k >= 1.0 {
                self.tap = 0.0;
            } else if free[other]
                && let Some((at, normal, ..)) = Wrist::screen(kit, body, st)
            {
                // (on it in the middle of the tap; off it before and after)
                let off = ((k - 0.5).abs() * 4.0 - 0.6).clamp(0.0, 1.0) * kit.def.toque.separa;
                out[other] = Some(poke(&arms[other], lim, Tip::Indice, kit.tapping, at, normal, off, 0.0).target);
            }
        }
        out
    }

    /// How far the look turns to read it, from where `view` looks: to the right and up (rad).
    pub fn look(kit: &WristKit, body: &Body, st: &Stance, view: &lunar_render::View) -> Option<(f64, f64)> {
        let (at, ..) = Wrist::screen(kit, body, st)?;
        let to = (at - view.eye).normalize_or_zero();
        let up = view.up;
        let level = |v: DVec3| (v - up * v.dot(up)).normalize_or_zero();
        let (a, b) = (level(view.forward), level(to));
        if to == DVec3::ZERO || a == DVec3::ZERO || b == DVec3::ZERO {
            return None;
        }
        let yaw = a.cross(up).dot(b).atan2(a.dot(b));
        Some((yaw, to.dot(up).clamp(-1.0, 1.0).asin() - view.forward.dot(up).clamp(-1.0, 1.0).asin()))
    }
}

/// Where the hand of the arm that wears it goes for the middle of its screen to be at `screen`
/// (world) and to face `eyes` as far as the arm allows. A forearm does not twist here: what is
/// worn on it faces one way out of the plane the arm bends in (`body::hinge`), so it is that
/// plane that is turned — the elbow goes round the line from shoulder to wrist to where the
/// screen looks most at the eyes (`elbow`: the way it goes instead, if said; world), never
/// across the chest nor up over the shoulder for it. The hand is as its forearm has it.
pub fn raised(arm: &Arm, kit: &WristKit, screen: DVec3, eyes: DVec3, elbow: Option<Vec3>) -> Option<Target> {
    let (rig, sk) = (arm.rig, &arm.rig.skeleton);
    let limb = rig.arms[arm.side];
    let (b, c) = (sk.bind[limb.lower].pos, sk.bind[limb.end].pos);
    let along0 = (c - b).normalize_or_zero();
    let (bone, rel) = rig.points.get(&kit.def.punto)?;
    let point = sk.bind[*bone].then(*rel);
    let out0 = point.rot * Vec3::Y;
    let out0 = (out0 - along0 * out0.dot(along0)).normalize_or_zero();
    if along0 == Vec3::ZERO || out0 == Vec3::ZERO {
        return None;
    }
    // how far round the forearm from its hinge side the screen looks, how far the screen's
    // middle is from the wrist along the forearm and from the forearm's axis (m of the world)
    let hinge = crate::body::hinge(rig, arm.side);
    let round = hinge.cross(out0).dot(along0).atan2(hinge.dot(out0));
    let s = &kit.def.pantalla;
    let back = (c - point.pos).dot(along0) * arm.scale - s.en[0];
    let off = s.en[1];
    let (l1, l2) = {
        let (a, b) = limb.lengths(sk);
        (a * arm.scale, b * arm.scale)
    };
    let to = (eyes - screen).as_vec3().normalize_or(arm.up());
    let (up, inward) = (arm.up(), arm.inward());
    // (the forearm's way is not known until the elbow is: from a first guess, a few times over)
    let mut fore = (inward * 0.85 + arm.ahead() * 0.45).normalize();
    let mut found = None;
    for _ in 0..4 {
        let wrist = screen - (to * off).as_dvec3() + (fore * back).as_dvec3();
        let reach = (wrist - arm.shoulder).as_vec3();
        let far = reach.length();
        if far < 1e-4 {
            return None;
        }
        let dir = reach / far;
        let d = far.clamp((l1 - l2).abs() + 1e-4, (l1 + l2) * 0.999);
        let on = (l1 * l1 + d * d - l2 * l2) / (2.0 * d);
        let out = (l1 * l1 - on * on).max(0.0).sqrt();
        let u = dir.any_orthonormal_vector();
        let v = dir.cross(u);
        let at = |side: Vec3| {
            let mid = dir * on + side * out;
            let fore = (dir * d - mid).normalize_or(dir);
            let normal = side.cross(dir);
            (fore, normal, Quat::from_axis_angle(fore, round) * normal)
        };
        let mut best = (f32::MIN, Vec3::ZERO);
        match elbow {
            Some(e) => best.1 = (e - dir * e.dot(dir)).normalize_or(u),
            None => {
                for k in 0..48 {
                    let a = k as f32 / 48.0 * std::f32::consts::TAU;
                    let side = u * a.cos() + v * a.sin();
                    let score = at(side).2.dot(to) - 2.0 * (side.dot(inward) - 0.1).max(0.0).powi(2) - 1.5 * (side.dot(up) - 0.35).max(0.0).powi(2);
                    if score > best.0 {
                        best = (score, side);
                    }
                }
            }
        }
        let (f, normal, _) = at(best.1);
        fore = f;
        found = Some((wrist, best.1, normal));
    }
    let (wrist, side, normal) = found?;
    // the hand: its forearm's turn from rest, and no more (its wrist at ease)
    let turn = frame_turn(along0, hinge, fore, normal);
    let palm = &rig.palms[arm.side];
    Some(Target { at: wrist + (turn * (palm.at - c) * arm.scale).as_dvec3(), rot: (turn * frame_turn(Vec3::Y, Vec3::X, palm.normal, palm.across)).normalize(), fingers: kit.raised, pole: Some(arm.rot.inverse() * side) })
}

/// What the screen says, kept between frames and read again every so often: nothing is made
/// anew each frame.
#[derive(Default)]
pub struct Screen {
    title: String,
    under: String,
    /// Label, value, level (0 normal, 1 caution, 2 warning, 3 no reading).
    rows: Vec<(String, String, u8)>,
    used: usize,
    age: f32,
    page: Option<u8>,
    placed: Vec<Placed>,
    /// The device's mesh as the renderer knows it.
    pub mesh: Option<u8>,
}

/// The most rows a page shows.
const ROWS: usize = 6;

impl Screen {
    /// Gives the renderer the device's model, if it is there.
    pub fn model(&mut self, kit: &WristKit, models: &lunar_core::structure::models::Models, r: &mut lunar_render::Renderer) {
        let white = lunar_core::mesh::Material::new([255; 3], 128, 0);
        self.mesh = kit.def.modelo.as_ref().and_then(|name| models.get(&format!("{name}/{}", lunar_core::structure::catalog::BODY))).and_then(|m| r.prop_mesh(&m.tinted(white)));
    }

    fn row(&mut self) -> &mut (String, String, u8) {
        if self.used == self.rows.len() {
            self.rows.push((String::new(), String::new(), 0));
        }
        self.used += 1;
        let r = &mut self.rows[self.used - 1];
        r.0.clear();
        r.1.clear();
        r.2 = 0;
        r
    }

    /// Reads page `page` again if it is time: the suit's values from `suit`, the ship's from
    /// `ship` (its signals, its compartments).
    pub fn read(&mut self, dt: f32, kit: &WristKit, page: u8, suit: &Readings, ship: Option<&Ship>) {
        self.age += dt;
        if self.page == Some(page) && self.age < kit.def.refresco {
            return;
        }
        (self.age, self.page, self.used) = (0.0, Some(page), 0);
        let k = usize::from(page).min(kit.pages.len() - 1);
        let def = &kit.def.paginas[k];
        self.title.clear();
        self.title.push_str(&def.titulo);
        self.under.clear();
        if def.nave {
            match ship {
                Some(sh) => self.under.push_str(&sh.kind.def.nombre),
                None => self.under.push_str("SIN NAVE CERCA"),
            }
        }
        let _ = write!(self.under, "{}{}/{}", if self.under.is_empty() { "" } else { "  ·  " }, k + 1, kit.pages.len());
        for r in &kit.pages[k] {
            if self.used >= ROWS {
                break;
            }
            let d = &r.def;
            let level = |shown: f64| {
                let past = |limit: Option<f64>| limit.is_some_and(|l| if d.sobre { shown > l } else { shown < l });
                if past(d.rojo) { 2 } else { u8::from(past(d.ambar)) }
            };
            if let Some(v) = d.valor {
                let si = match v {
                    SuitValue::Oxygen => Some(f64::from(suit.oxygen)),
                    SuitValue::OxygenTime => Some(f64::from(suit.oxygen * kit.def.traje.oxigeno) * 3600.0),
                    SuitValue::Pressure => Some(f64::from(kit.def.traje.presion) * 1000.0),
                    SuitValue::Gas => suit.gas.map(f64::from),
                    SuitValue::Clock => Some(suit.clock),
                    SuitValue::Outside => Some(f64::from(suit.outside) * 1000.0),
                    SuitValue::Gravity => Some(f64::from(suit.gravity)),
                };
                let row = self.row();
                row.0.push_str(&d.rotulo);
                match (si, &r.unit) {
                    (None, _) => {
                        row.1.push_str("NO LLEVA");
                        row.2 = 3;
                    }
                    (Some(v), None) => {
                        let t = v.max(0.0) as u64;
                        let _ = write!(row.1, "{:02}:{:02}:{:02}", t / 3600, t / 60 % 60, t % 60);
                    }
                    (Some(v), Some(u)) => {
                        units::format(v, &d.unidad, u, d.decimales, &mut row.1);
                        row.2 = level(u.from_si(v));
                    }
                }
                continue;
            }
            let Some(sh) = ship else { continue };
            let Some(name) = d.senal.as_deref() else { continue };
            let mut show = |label: &str, sig: Option<lunar_signals::SignalId>, this: &mut Screen| {
                let row = this.row();
                row.0.push_str(label);
                let Some(id) = sig else {
                    row.1.push_str("—");
                    row.2 = 3;
                    return;
                };
                let v = sh.store.get(id);
                match &d.desde {
                    Some((from, words)) if v >= *from => row.1.push_str(words),
                    _ => {
                        sh.store.format(id, d.decimales, &mut row.1);
                        row.2 = level(sh.store.meta(id).unit.from_si(v));
                    }
                }
            };
            if d.cada.is_some() {
                // a row for each compartment: its name, its own signal
                let mut key = String::new();
                for c in &sh.kind.def.compartimentos {
                    if self.used >= ROWS {
                        break;
                    }
                    key.clear();
                    key.push_str(&name.replace("{id}", &c.id));
                    let label = c.nombre.to_uppercase();
                    show(&label, sh.store.find(&key), self);
                }
            } else {
                show(&d.rotulo, sh.store.find(name), self);
            }
        }
    }

    /// The device on `body`'s wrist and, with `lit`, what its screen says (`read`), into
    /// `out`. `inside`: under a hull's lamps.
    pub fn show(&mut self, kit: &WristKit, body: &Body, st: &Stance, font: &Font, lit: bool, inside: bool, out: &mut PropScene) {
        let Some((at, normal, across)) = body.point(&kit.def.punto, st) else { return };
        let frame = out.frames.len() as u16;
        let rot = frame_turn(Vec3::Y, Vec3::X, normal, across);
        out.frames.push(PropFrame { pos: at, rot, inside });
        let s = &kit.def.pantalla;
        let (w, h) = (s.tamano[0], s.tamano[1]);
        let c = Vec3::from(s.en);
        let u = Vec3::from(s.derecha).normalize_or(Vec3::X);
        let v = Vec3::from(s.arriba);
        let v = (v - u * v.dot(u)).normalize_or(Vec3::Z);
        let n = u.cross(v);
        let face = Quat::from_mat3(&glam::Mat3::from_cols(u, v, n));
        match self.mesh {
            Some(mesh) => out.props.push(Prop { frame, mesh, pos: Vec3::ZERO, rot: kit.model_turn, size: Vec3::ONE, color: [255; 3], emissive: 0.0, rough: 128, metal: 0 }),
            // no model: a housing under its screen, a strap round the arm
            None => {
                out.props.push(Prop { frame, mesh: BOX, pos: c - n * 0.009, rot: face, size: Vec3::new(w + 0.016, h + 0.014, 0.016), color: [34, 36, 40], emissive: 0.0, rough: 150, metal: 60 });
                out.props.push(Prop { frame, mesh: BOX, pos: c - n * 0.017, rot: face, size: Vec3::new(w * 0.5, h + 0.05, 0.004), color: [58, 60, 66], emissive: 0.0, rough: 200, metal: 0 });
            }
        }
        // its glass: dark, a little light of its own when it is on (a model has its own glass:
        // only its light is laid over it)
        if self.mesh.is_none() || lit {
            out.props.push(Prop { frame, mesh: BOX, pos: c + n * 0.0003, rot: face, size: Vec3::new(w, h, 0.0006), color: GLASS, emissive: if lit { -0.3 } else { 0.0 }, rough: 40, metal: 0 });
        }
        if !lit {
            return;
        }
        let mut pen = Pen { out, frame, font, placed: &mut self.placed, c: c + n * 0.0011, u, v, w, h };
        let line = h / (ROWS as f32 + 3.2);
        pen.text(&self.title, -0.94, 1.0 - line / h * 1.5, line * 0.95, INK, 0.0, 2.2);
        pen.text(&self.under, 0.94, 1.0 - line / h * 1.5, line * 0.62, DIMMED, 1.0, 1.6);
        pen.bar(0.0, 1.0 - line / h * 2.9, 1.88, line * 0.06, DIMMED);
        for (i, (label, value, level)) in self.rows[..self.used].iter().enumerate() {
            let y = 1.0 - line / h * (4.3 + 2.0 * i as f32);
            let color = match level {
                0 => LEVELS[0],
                1 => LEVELS[1],
                2 => LEVELS[2],
                _ => DIMMED,
            };
            pen.text(label, -0.94, y, line * 0.7, DIMMED, 0.0, 1.5);
            pen.text(value, 0.94, y, line * 0.82, color, 1.0, 2.0);
        }
    }
}

/// Writes on the screen: places are given as shares of its half width and half height from its
/// middle (-1 .. 1).
struct Pen<'a> {
    out: &'a mut PropScene,
    frame: u16,
    font: &'a Font,
    placed: &'a mut Vec<Placed>,
    c: Vec3,
    u: Vec3,
    v: Vec3,
    w: f32,
    h: f32,
}

impl Pen<'_> {
    fn at(&self, x: f32, y: f32) -> Vec3 {
        self.c + self.u * (x * self.w * 0.5) + self.v * (y * self.h * 0.5)
    }

    /// Lit text `size` m tall from (x, y): its left end, or its right with `align` 1.
    #[allow(clippy::too_many_arguments)]
    fn text(&mut self, s: &str, x: f32, y: f32, size: f32, color: [u8; 3], align: f32, glow: f32) {
        self.placed.clear();
        self.font.layout(s, align, self.placed);
        let at = self.at(x, y);
        for g in self.placed.iter() {
            let c = at + self.u * (g.center[0] * size) + self.v * (g.center[1] * size);
            self.out.glyphs.push(GlyphQuad { frame: self.frame, center: c, u: self.u * (g.half[0] * size), v: self.v * (g.half[1] * size), uv: g.uv, color, emissive: glow, relief: 0.0 });
        }
    }

    /// A lit line across: its middle at (x, y), `wide` of the half width, `thick` m.
    fn bar(&mut self, x: f32, y: f32, wide: f32, thick: f32, color: [u8; 3]) {
        let rot = Quat::from_mat3(&glam::Mat3::from_cols(self.u, self.v, self.u.cross(self.v)));
        self.out.props.push(Prop { frame: self.frame, mesh: BOX, pos: self.at(x, y), rot, size: Vec3::new(wide * self.w * 0.5, thick, 0.0004), color, emissive: -1.2, rough: 200, metal: 0 });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_suits_oxygen_goes_down_out_of_air_and_fills_in_it() {
        let def = SuitDef { oxigeno: 8.0, recarga: 30.0, aire: 40.0, presion: 29.6 };
        let mut s = Suit::default();
        // an hour in the vacuum: an eighth of it gone
        for _ in 0..3600 {
            s.tick(1.0, &def, 0.0);
        }
        assert!((s.oxygen - 0.875).abs() < 1e-3, "{}", s.oxygen);
        // thin air does not fill it; a cabin's does, in half a minute at the most
        s.tick(10.0, &def, 20.0);
        assert!(s.oxygen < 0.875);
        for _ in 0..30 {
            s.tick(1.0, &def, 70.0);
        }
        assert_eq!(s.oxygen, 1.0);
        // it never goes under nothing
        let mut s = Suit { oxygen: 0.0001 };
        s.tick(100.0, &def, 0.0);
        assert_eq!(s.oxygen, 0.0);
    }
}
