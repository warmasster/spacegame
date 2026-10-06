//! Camera scripts (`--guion FICHERO`): steps run in play without hands, to see what the game draws
//! where it matters and to work the ships: the camera anywhere (a point of a ship's frame, aimed
//! at a control, a panel or a part), the ship worked (controls, signals, joints), waits, pictures,
//! signal dumps, shots. The program ends after the last step. For tooling and for checking things
//! by eye (`tools/camara/*.jsonc`).
use crate::{blasts::Blasts, builds::Builds, ships::Ships};
use glam::Vec3;
use lunar_controls::Intent;
use lunar_render::View;
use lunar_ship::panels::Panels;
use serde::Deserialize;
use std::{fmt::Write as _, path::PathBuf};

/// Frames a camera move waits before a picture (shadows, exposure and temporal filters settle).
const SETTLE: u32 = 8;
/// Seconds `pulsar` holds a control down.
const PRESS: f64 = 0.2;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScriptDef {
    pub pasos: Vec<StepDef>,
    /// Where `volcar` writes (default `out/guion.log`).
    #[serde(default)]
    pub registro: Option<String>,
    /// The ships of the scenario this script wants made (kind ids); without it, all of them. A
    /// measure of performance says which, so that what the scenario gains does not change it.
    #[serde(default)]
    pub naves: Option<Vec<String>>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum StepDef {
    /// Seconds of play.
    Esperar(f64),
    /// The ship the next steps are about: a kind id (its first ship) or "kind#n".
    Nave(String),
    /// Put the camera (it stays until moved again).
    Camara(CameraDef),
    /// The camera at a point of the site (metres east and north of it, and over the ground
    /// there), looking at another: for what is not about a ship (the traffic, the fields).
    CamaraSitio { en: [f64; 3], mira: [f64; 3], #[serde(default)] fov: Option<f32> },
    /// The camera back to the player's.
    Jugador,
    /// A picture of the next frame (PNG, relative to the game's folder).
    Foto(String),
    /// Press and release a control ("panel/control"; "panel/control#n": its button n, of a
    /// bezel or a keypad).
    Pulsar(String),
    /// Set a control's value (levers, knobs, throttles).
    Ajustar { mando: String, valor: f64 },
    /// Hold one axis of a sprung lever where a seat's key would (a stick, the translation
    /// lever): it stays there until it is put back at its centre (`valor` 0).
    Eje { mando: String, #[serde(default)] eje: u8, valor: f64 },
    /// Set a signal the data owns.
    Senal { nombre: String, valor: f64 },
    /// Set a joint straight.
    Articulacion { id: String, q: f64 },
    /// Write signals to the log: exact names, or "prefix*" for every one under it.
    Volcar(Vec<String>),
    /// Write a line to the log.
    Nota(String),
    /// Fire a shot (`assets/defs/shots.jsonc`) along the camera's view.
    Disparar(String),
    /// Tool: structures tinted by their level of detail (F7).
    NivelesDetalle(bool),
    /// The player's helmet lamps on or off (L).
    Linterna(bool),
    /// The player crouched or standing (C).
    Agachado(bool),
    /// The HUD in the pictures (a script's are without it unless it says so).
    Hud(bool),
    /// The suit's jet pack on or off (J), and its push up held or let go (Space).
    Mochila(bool),
    Subir(bool),
    /// Where the player looks: heading and pitch (degrees).
    Mirar([f64; 2]),
    /// The player's walking keys held: ahead (1, back -1) and to the right (1, left -1); [0, 0]
    /// lets them go. And the run key, the jump (once) and the pack's push down.
    Andar([f64; 2]),
    Correr(bool),
    Saltar,
    Bajar(bool),
    /// The player's feet at a point of the ship (its frame, m), standing; and the player looking
    /// at a point of it.
    Ir([f32; 3]),
    MirarA([f32; 3]),
    /// The camera by the player, going with them: eye and target in the player's own frame (m:
    /// x to their left, y up from their feet, z the way they face).
    CamaraJugador { en: [f64; 3], mira: [f64; 3], #[serde(default)] fov: Option<f32> },
    /// The view from outside the player (V), from so many metres behind; 0: from their eyes.
    Tercera(f64),
    /// The look turned away from where the body faces, as with Alt held: degrees to the right
    /// and up (the head; from outside, the camera). [0, 0] lets go.
    Cabeza([f64; 2]),
    /// A gesture by its id ("": none), and the wrist computer up or down.
    Gesto(String),
    Muneca(bool),
    /// The hands' own axes drawn on them (as F8), and how each wrist is to the log: how far it
    /// bends, leans and turns, how far its arm is stretched, whether it is held at its stop.
    EjesManos(bool),
    Manos,
    /// The start menu shown (as the game opens with), or away; and the place chosen in it (its
    /// number down the list, from 0).
    Inicio(bool),
    Lugar(usize),
    /// Out of the start menu and into play at the place chosen (as JUGAR does).
    Jugar,
    /// The menu open at a tab ("controles", "graficos", "escena"); an empty name shuts it.
    Menu(String),
    /// A tool window open ("catalogo", "inspector", "cifras"), for a picture of it; an empty
    /// name shuts them all.
    Ventana(String),
    /// Bare hands: take what the camera looks at (as a click held would), or let it go.
    Coger(bool),
    /// Write to the log what is sounding: the air round the head, the loops held, the blows since
    /// the last time ("oir").
    Oir,
    /// The player sat in a seat of the ship (its id), as E on it would; an empty name: up again.
    Sentar(String),
    /// Write to the log what the rangefinder reads and how the ship is drawn (level, hidden,
    /// what the GPU holds).
    Medir,
    /// Write to the log where the player's feet are in the ship's frame, what carries them (the
    /// ship, something they go along with in the air, nothing) and how fast they go from it.
    Donde,
    /// The ship editor (F6): one of its operations (`lunar_editor::Op`, as the MCP takes them).
    Editar(serde_json::Value),
    /// The edited ship rebuilt in its place.
    AplicarEdicion,
    /// More ships: a grid of them round a point of the site, standing or dropped from a height.
    Flota(FleetDef),
    /// The ship put somewhere else as it is, with whoever is aboard: over a point of any body
    /// (or of none), going as said.
    Poner(PutDef),
    /// Shots at the ships from all round them, so many a second for so long.
    Fuego(FireDef),
    /// Record what every frame costs from now (`perf`), into this file (JSON).
    Perfil(String),
    /// Stop recording: the file is written and the summary goes to the log.
    FinPerfil,
    /// The hand aimed at a control ("panel/control") as a player's would be (it shows on its
    /// panel); an empty name: at nothing.
    Apuntar(String),
    /// A tool in hand (`gear.jsonc`, by id); an empty name: hands free.
    Equipo(String),
    /// The tool's trigger held or let go, and the welder's integrity view on or off.
    Gatillo(bool),
    VistaIntegridad(bool),
    /// Damage to a part of the ship (its id, or a prefix ending in `*`): `dano` of its hit points
    /// gone (1: destroyed).
    Danar { pieza: String, dano: f32 },
    /// The visor's welding filter held down or let go (whatever the welder asks), and the
    /// effort so far (0..1: breath on the visor), each if said.
    Visor { #[serde(default)] filtro: Option<bool>, #[serde(default)] esfuerzo: Option<f32> },
    /// Write to the log the marks on the ground (kept, near the eye, written) and what the
    /// visor is doing.
    Huellas,
}

/// `flota`: `cuenta` ships of kind `nave` (columns by rows, `paso` m apart) round the point
/// `este`, `norte` m of the site, on the ground or `altura` m over it (they fall).
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FleetDef {
    pub nave: String,
    pub cuenta: [u32; 2],
    pub paso: f64,
    #[serde(default)]
    pub altura: f64,
    #[serde(default)]
    pub este: f64,
    #[serde(default)]
    pub norte: f64,
}

/// `poner`: the ship `altura` m over the datum of body `cuerpo` (its id) — over its ground
/// there with `sobre_suelo` —, along `hacia` from its centre (any length; without it, straight
/// over where the ship is now), going `velocidad` m/s (east, north, up there) and turning `giro`
/// rad/s (the same axes). Its deck is laid level there, nose to the north, unless `volcada`
/// (degrees it is rolled over).
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PutDef {
    pub cuerpo: String,
    pub altura: f64,
    #[serde(default)]
    pub sobre_suelo: bool,
    #[serde(default)]
    pub hacia: Option<[f64; 3]>,
    #[serde(default)]
    pub velocidad: [f64; 3],
    #[serde(default)]
    pub giro: [f32; 3],
    #[serde(default)]
    pub volcada: f32,
}

/// `fuego`: shot `tiro` at the ships, `por_segundo` rounds a second for `segundos`.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FireDef {
    pub tiro: String,
    pub por_segundo: f64,
    pub segundos: f64,
}

/// What a step asks of the game round it (play.rs does it).
#[derive(Clone, Debug, PartialEq)]
pub enum Request {
    LodTint(bool),
    Lamp(bool),
    Crouch(bool),
    Pack(bool),
    Thrust(bool),
    Look([f64; 2]),
    Walk([f64; 2]),
    Run(bool),
    Jump,
    Down(bool),
    /// The player's feet at a point of a structure (its frame); the player looking at one.
    Go(u64, [f32; 3]),
    LookAt(u64, [f32; 3]),
    Menu(Option<String>),
    /// The view from outside from so far behind (None: from the eyes); the start menu up or away.
    Chase(Option<f64>),
    Head([f64; 2]),
    /// A gesture by its id (empty: none), and the wrist computer up or down.
    Gesture(String),
    Wrist(bool),
    /// The hands' axes drawn or not; how each wrist is, to the log.
    HandAxes(bool),
    Hands,
    Start(bool),
    StartPlace(usize),
    Begin,
    Window(String),
    Grab(bool),
    /// Log what is sounding.
    Hear,
    /// Sit in seat `1` of structure `0` (None: stand up).
    Sit(Option<(u64, usize)>),
    Measure,
    /// Where the player is, told in the frame of this structure.
    Where(Option<u64>),
    Edit(serde_json::Value),
    ApplyEdit,
    Fleet(FleetDef),
    /// A structure put somewhere else.
    Put(u64, PutDef),
    Fire(FireDef),
    Profile(Option<String>),
    /// Aim at control `k` of the script's ship (None: at nothing).
    Aim(Option<usize>),
    Tool(String),
    Trigger(bool),
    IntegrityView(bool),
    Damage { part: String, share: f32 },
    /// The visor's filter down or up, the effort so far; and the marks and the visor to the log.
    Visor(Option<bool>, Option<f32>),
    Marks,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CameraDef {
    /// Eye and the point looked at, in the ship's frame (m).
    #[serde(default)]
    pub en: Option<[f32; 3]>,
    #[serde(default)]
    pub mira: Option<[f32; 3]>,
    /// Or aimed at a control ("panel/control"), a panel or a part, from `distancia` m along
    /// `desde` (ship frame; by default out of the panel's face, or from the ship's centre).
    #[serde(default)]
    pub mando: Option<String>,
    #[serde(default)]
    pub panel: Option<String>,
    #[serde(default)]
    pub pieza: Option<String>,
    #[serde(default)]
    pub distancia: Option<f32>,
    #[serde(default)]
    pub desde: Option<[f32; 3]>,
    /// Vertical field of view (degrees); the player's by default.
    #[serde(default)]
    pub fov: Option<f32>,
}

pub struct Script {
    steps: Vec<StepDef>,
    next: usize,
    wait: f64,
    /// The ship's structure the steps are about.
    pub ship: Option<u64>,
    camera: Option<CameraDef>,
    /// A camera of the site: eye, target (east, north, height) and field of view (deg).
    pub site_camera: Option<([f64; 3], [f64; 3], Option<f32>)>,
    /// A camera by the player: eye, target (the player's frame) and field of view (deg).
    pub player_camera: Option<([f64; 3], [f64; 3], Option<f32>)>,
    settle: u32,
    /// This frame's picture.
    pub shot: Option<PathBuf>,
    log: String,
    log_path: PathBuf,
    /// Controls pressed by `pulsar`, let go after a moment: (control, seconds left).
    releases: Vec<(String, f64)>,
    pub done: bool,
    /// What the steps of this frame ask of the game.
    pub requests: Vec<Request>,
    /// The HUD is shown (`hud`).
    pub hud: bool,
    /// The scenario's ships to make (`ScriptDef::naves`).
    pub only: Option<Vec<String>>,
    /// The controls a step changed: (ship's structure, control, the value it is at now), as a
    /// hand's are told (`Aboard::changed`): with other players about, they are told of them.
    pub changed: Vec<(u64, u16, f64)>,
}

impl Script {
    pub fn load(path: &std::path::Path) -> Result<Script, String> {
        let def: ScriptDef = lunar_core::defs::load(path).map_err(|e| format!("{}: {}", e.file, e.message))?;
        let root = crate::root();
        let log_path = root.join(def.registro.as_deref().unwrap_or("out/guion.log"));
        Ok(Script { steps: def.pasos, next: 0, wait: 0.0, ship: None, camera: None, site_camera: None, player_camera: None, settle: 0, shot: None, log: String::new(), log_path, releases: Vec::new(), done: false, requests: Vec::new(), hud: false, only: def.naves, changed: Vec::new() })
    }

    fn ship_index(&self, ships: &Ships) -> Option<usize> {
        match self.ship {
            Some(id) => ships.by_structure(id),
            None => (!ships.list.is_empty()).then_some(0),
        }
    }

    pub fn note(&mut self, line: &str) {
        self.log.push_str(line);
        self.log.push('\n');
    }

    /// Run the steps due this frame; the camera the script holds (None: the player's).
    pub fn update(&mut self, dt: f64, ships: &mut Ships, builds: &Builds, blasts: &mut Blasts, view: View) -> Option<View> {
        self.shot = None;
        self.settle = self.settle.saturating_sub(1);
        self.wait -= dt;
        // a press is held a moment, as a hand would (edges and toggles see it)
        for r in &mut self.releases {
            r.1 -= dt;
        }
        let due: Vec<String> = self.releases.iter().filter(|r| r.1 <= 0.0).map(|r| r.0.clone()).collect();
        self.releases.retain(|r| r.1 > 0.0);
        for m in due {
            let _ = self.intent(ships, builds, &m, &Intent::Release);
        }
        while !self.done && self.wait <= 0.0 {
            let Some(step) = self.steps.get(self.next).cloned() else {
                self.done = true;
                self.flush();
                break;
            };
            if let StepDef::Foto(_) = step
                && self.settle > 0
            {
                break;
            }
            self.next += 1;
            if let Err(e) = self.run(step, ships, builds, blasts, &view) {
                self.note(&format!("ERROR paso {}: {e}", self.next));
                self.flush();
            }
            if self.shot.is_some() {
                // the picture is of this frame: the next steps wait for the next one
                break;
            }
        }
        let cam = self.camera.clone()?;
        let n = self.ship_index(ships)?;
        let s = builds.set.get(ships.list[n].structure)?;
        let (eye, at) = self.place(&cam, ships, n, s).ok()?;
        let (e, a) = (s.to_world(eye), s.to_world(at));
        let forward = (a - e).normalize_or(view.forward);
        let up = (s.rot * Vec3::Y).as_dvec3();
        let up = if forward.cross(up).length_squared() < 1e-6 { (s.rot * Vec3::Z).as_dvec3() } else { up };
        Some(View { eye: e, forward, up, fov_y: cam.fov.map_or(view.fov_y, f32::to_radians), near: 0.03 })
    }

    /// Eye and target of a camera in the ship's frame.
    fn place(&self, c: &CameraDef, ships: &Ships, n: usize, s: &lunar_core::structure::state::Structure) -> Result<(Vec3, Vec3), String> {
        let sh = &ships.list[n];
        let kind = &sh.kind;
        let v = |a: [f32; 3]| Vec3::from_array(a);
        let dist = c.distancia;
        // the point looked at and the way out of it
        let (at, out, d0) = if let Some(m) = &c.mando {
            let k = sh.panels.controls.iter().position(|x| &x.id == m).ok_or_else(|| format!("no hay mando '{m}'"))?;
            let ctl = &sh.panels.controls[k];
            let f = Panels::frame(kind, s, &kind.panels[ctl.panel]);
            let r = ctl.rect;
            let p = f.transform_point3(Vec3::new((r.x + r.w * 0.5) / 1000.0, (r.y + r.h * 0.5) / 1000.0, 0.02));
            (p, f.transform_vector3(Vec3::Z).normalize(), 0.45)
        } else if let Some(name) = &c.panel {
            let plan = kind.panels.iter().find(|p| &p.id == name).ok_or_else(|| format!("no hay panel '{name}'"))?;
            let f = Panels::frame(kind, s, plan);
            let [w, h] = plan.layout.size;
            let p = f.transform_point3(Vec3::new(w / 2000.0, h / 2000.0, 0.0));
            (p, f.transform_vector3(Vec3::Z).normalize(), (w.max(h) / 1000.0 * 1.4).max(0.5))
        } else if let Some(name) = &c.pieza {
            let k = kind.parts.iter().position(|p| p == name || p.starts_with(&format!("{name}."))).ok_or_else(|| format!("no hay pieza '{name}'"))?;
            let p = s.parts[k].center;
            let away = (p - s.center).normalize_or(Vec3::Y);
            (p, away, (s.parts[k].radius * 3.0).max(1.0))
        } else {
            let eye = v(c.en.ok_or("la cámara necesita 'en', 'mando', 'panel' o 'pieza'")?);
            return Ok((eye, c.mira.map_or(eye + Vec3::Z, v)));
        };
        let out = c.desde.map_or(out, |d| v(d).normalize_or(out));
        let eye = c.en.map_or(at + out * dist.unwrap_or(d0), v);
        Ok((eye, c.mira.map_or(at, v)))
    }

    fn run(&mut self, step: StepDef, ships: &mut Ships, builds: &Builds, blasts: &mut Blasts, view: &View) -> Result<(), String> {
        match step {
            StepDef::Esperar(t) => self.wait = t,
            StepDef::Nave(name) => {
                let (kind, nth) = name.split_once('#').map_or((name.as_str(), 0), |(k, n)| (k, n.parse().unwrap_or(0)));
                let sh = ships.list.iter().filter(|s| s.kind.id == kind).nth(nth).ok_or_else(|| format!("no hay nave '{name}'"))?;
                self.ship = Some(sh.structure);
            }
            StepDef::Camara(c) => {
                self.camera = Some(c);
                self.site_camera = None;
                self.settle = SETTLE;
            }
            StepDef::CamaraSitio { en, mira, fov } => {
                self.camera = None;
                self.player_camera = None;
                self.site_camera = Some((en, mira, fov));
                self.settle = SETTLE;
            }
            StepDef::CamaraJugador { en, mira, fov } => {
                self.camera = None;
                self.site_camera = None;
                self.player_camera = Some((en, mira, fov));
                self.settle = SETTLE;
            }
            StepDef::Jugador => {
                self.camera = None;
                self.site_camera = None;
                self.player_camera = None;
                self.settle = SETTLE;
            }
            StepDef::Foto(p) => {
                let path = crate::root().join(&p);
                self.note(&format!("foto {}", path.display()));
                self.shot = Some(path);
            }
            StepDef::Pulsar(m) => {
                let (id, elem) = m.split_once('#').map_or((m.as_str(), 0), |(id, e)| (id, e.parse().unwrap_or(0)));
                self.intent(ships, builds, id, &Intent::Press { elem })?;
                self.releases.push((id.to_string(), PRESS));
            }
            StepDef::Ajustar { mando, valor } => self.intent(ships, builds, &mando, &Intent::Set { value: valor })?,
            StepDef::Eje { mando, eje, valor } => self.intent(ships, builds, &mando, &Intent::Axis { axis: eje, value: valor })?,
            StepDef::Senal { nombre, valor } => {
                let n = self.ship_index(ships).ok_or("no hay nave")?;
                ships.list[n].set_signal(&nombre, valor);
            }
            StepDef::Articulacion { id, q } => {
                let n = self.ship_index(ships).ok_or("no hay nave")?;
                ships.list[n].set_joint(&id, q);
            }
            StepDef::Volcar(names) => {
                let n = self.ship_index(ships).ok_or("no hay nave")?;
                let sh = &ships.list[n];
                let mut out = String::new();
                for name in &names {
                    if let Some(prefix) = name.strip_suffix('*') {
                        for id in sh.store.ids() {
                            let s = sh.store.name(id);
                            if s.starts_with(prefix) {
                                let _ = writeln!(out, "  {s} = {}", sh.store.get(id));
                            }
                        }
                    } else {
                        match sh.signal(name) {
                            Some(x) => {
                                let _ = writeln!(out, "  {name} = {x}");
                            }
                            None => {
                                let _ = writeln!(out, "  {name}: no existe");
                            }
                        }
                    }
                }
                self.note(&format!("t {:.1} s:\n{}", sh.t, out.trim_end()));
            }
            StepDef::Nota(t) => self.note(&t),
            StepDef::Disparar(id) => {
                if !blasts.shoot_named(&id) {
                    return Err(format!("no hay disparo '{id}'"));
                }
                let _ = view;
            }
            StepDef::NivelesDetalle(on) => {
                self.requests.push(Request::LodTint(on));
                self.settle = SETTLE;
            }
            StepDef::Linterna(on) => self.requests.push(Request::Lamp(on)),
            StepDef::Agachado(on) => self.requests.push(Request::Crouch(on)),
            StepDef::Hud(on) => self.hud = on,
            StepDef::Mochila(on) => self.requests.push(Request::Pack(on)),
            StepDef::Subir(on) => self.requests.push(Request::Thrust(on)),
            StepDef::Mirar(a) => self.requests.push(Request::Look(a)),
            StepDef::Andar(a) => self.requests.push(Request::Walk(a)),
            StepDef::Correr(on) => self.requests.push(Request::Run(on)),
            StepDef::Saltar => self.requests.push(Request::Jump),
            StepDef::Bajar(on) => self.requests.push(Request::Down(on)),
            StepDef::Ir(at) => {
                let id = self.ship.ok_or("ir: sin nave")?;
                self.requests.push(Request::Go(id, at));
            }
            StepDef::MirarA(at) => {
                let id = self.ship.ok_or("mirar_a: sin nave")?;
                self.requests.push(Request::LookAt(id, at));
            }
            StepDef::Coger(on) => self.requests.push(Request::Grab(on)),
            StepDef::Oir => self.requests.push(Request::Hear),
            StepDef::Sentar(seat) if seat.is_empty() => self.requests.push(Request::Sit(None)),
            StepDef::Sentar(seat) => {
                let n = self.ship_index(ships).ok_or("sentar: sin nave")?;
                let k = ships.list[n].kind.seats.iter().position(|s| s.def.id == seat).ok_or_else(|| format!("no hay asiento '{seat}'"))?;
                self.requests.push(Request::Sit(Some((ships.list[n].structure, k))));
            }
            StepDef::Tercera(m) => {
                self.requests.push(Request::Chase((m > 0.0).then_some(m)));
                self.settle = SETTLE;
            }
            StepDef::Cabeza(a) => {
                self.requests.push(Request::Head(a));
                self.settle = SETTLE;
            }
            StepDef::Gesto(id) => self.requests.push(Request::Gesture(id)),
            StepDef::EjesManos(on) => self.requests.push(Request::HandAxes(on)),
            StepDef::Manos => self.requests.push(Request::Hands),
            StepDef::Muneca(up) => {
                self.requests.push(Request::Wrist(up));
                self.settle = SETTLE;
            }
            StepDef::Inicio(on) => {
                self.requests.push(Request::Start(on));
                self.settle = SETTLE;
            }
            StepDef::Lugar(n) => self.requests.push(Request::StartPlace(n)),
            StepDef::Jugar => {
                self.requests.push(Request::Begin);
                self.settle = SETTLE;
            }
            StepDef::Menu(tab) => {
                self.requests.push(Request::Menu((!tab.is_empty()).then_some(tab)));
                self.settle = SETTLE;
            }
            StepDef::Ventana(name) => {
                self.requests.push(Request::Window(name));
                self.settle = SETTLE;
            }
            StepDef::Medir => self.requests.push(Request::Measure),
            StepDef::Donde => self.requests.push(Request::Where(self.ship)),
            StepDef::Visor { filtro, esfuerzo } => self.requests.push(Request::Visor(filtro, esfuerzo)),
            StepDef::Huellas => self.requests.push(Request::Marks),
            StepDef::Editar(op) => self.requests.push(Request::Edit(op)),
            StepDef::AplicarEdicion => {
                self.requests.push(Request::ApplyEdit);
                self.settle = SETTLE;
            }
            StepDef::Poner(p) => {
                if let Some(id) = self.ship {
                    self.requests.push(Request::Put(id, p));
                }
            }
            StepDef::Flota(f) => {
                self.requests.push(Request::Fleet(f));
                self.settle = SETTLE;
            }
            StepDef::Fuego(f) => self.requests.push(Request::Fire(f)),
            StepDef::Perfil(p) => self.requests.push(Request::Profile(Some(p))),
            StepDef::FinPerfil => self.requests.push(Request::Profile(None)),
            StepDef::Equipo(id) => self.requests.push(Request::Tool(id)),
            StepDef::Gatillo(on) => self.requests.push(Request::Trigger(on)),
            StepDef::VistaIntegridad(on) => {
                self.requests.push(Request::IntegrityView(on));
                self.settle = SETTLE;
            }
            StepDef::Danar { pieza, dano } => {
                self.requests.push(Request::Damage { part: pieza, share: dano });
                self.settle = SETTLE;
            }
            StepDef::Apuntar(m) => {
                let k = if m.is_empty() {
                    None
                } else {
                    let n = self.ship_index(ships).ok_or("no hay nave")?;
                    Some(ships.list[n].panels.controls.iter().position(|c| c.id == m).ok_or_else(|| format!("no hay mando '{m}'"))?)
                };
                self.requests.push(Request::Aim(k));
                self.settle = SETTLE;
            }
        }
        Ok(())
    }

    fn intent(&mut self, ships: &mut Ships, builds: &Builds, id: &str, i: &Intent) -> Result<(), String> {
        let n = self.ship_index(ships).ok_or("no hay nave")?;
        let sh = &mut ships.list[n];
        let s = builds.set.get(sh.structure).ok_or("la nave no tiene estructura")?;
        let k = sh.panels.controls.iter().position(|c| c.id == id).ok_or_else(|| format!("no hay mando '{id}'"))?;
        let kind = sh.kind.clone();
        let o = sh.panels.intent(k, i, s, &kind, &sh.store);
        if o.changed {
            let c = &sh.panels.controls[k];
            self.changed.push((sh.structure, k as u16, c.mech.value(&c.st)));
        }
        let line = format!("{id}: {i:?} -> {o:?}");
        self.note(&line);
        Ok(())
    }

    /// Write the log so far.
    pub fn flush(&self) {
        if let Some(dir) = self.log_path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(&self.log_path, &self.log);
    }
}
