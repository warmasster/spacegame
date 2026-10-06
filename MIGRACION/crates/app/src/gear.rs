//! What the suit carries in its hands (`assets/defs/gear.jsonc`), picked with the number keys:
//!
//! - the **welder-scanner**: what you look at within its reach shows on its own little screen —
//!   what it is, how sound it is —, the trigger mends it (or puts it back if it is gone, which
//!   takes a moment), and its view (right button) shows the integrity of everything round you:
//!   sound parts dim, damaged ones glowing red the more the worse, missing ones purple where they
//!   should be. Nothing of this is on the HUD: what is broken is known by looking;
//! - the **launcher**: a rocket where you aim, then a moment to load the next.
//!
//! Tools are data (a kind and its numbers); what each kind does is here. How a tool is held and
//! moves in the hands — its weight behind the look, its kick, its pieces, taking it out, firing,
//! loading it, where each hand is on it — is data too (`holding`: a tool's `"sujecion"`). The
//! tool in hand is drawn in the eye's frame, its screen lettered like any display.
use crate::{
    blasts::Blasts,
    body::{Body, Grip, Hand as BodyHand, Stance},
    builds::Builds,
    holding::{HandAt, HoldDef, Holding, Place, Steps, ToolPose},
    ships::Ships,
};
use glam::{DVec3, Quat, Vec3};
use lunar_core::{
    body::BodyRegistry,
    font::Font,
    props::{BOX, CYLINDER, GlyphQuad, Prop, PropFrame, PropScene, SPHERE},
    structure::schedule::LINGER,
};
use lunar_render::{Renderer, View};
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
pub struct ToolDef {
    pub id: String,
    pub nombre: String,
    /// What it looks like in the hands: a model of `assets/models` (its mesh `cuerpo`, in the
    /// frame it is held in: x to the left, y up, z ahead). Without one, a few plain shapes.
    #[serde(default)]
    pub modelo: Option<String>,
    /// How it is held and moves in the hands (`holding`).
    #[serde(default)]
    pub sujecion: HoldDef,
    /// Whose a click is with it in hand.
    #[serde(default)]
    pub clic: Click,
    #[serde(flatten)]
    pub kind: ToolKind,
}

/// Whose a click is with a tool in hand.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Click {
    /// What is aimed at first (a control, a door, a clamp); the tool's if there is nothing.
    #[default]
    Mundo,
    /// Always the tool's: whatever is under it is its work (a welder mends the door it is
    /// aimed at, it does not open it).
    Herramienta,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "tipo", rename_all = "snake_case", deny_unknown_fields)]
pub enum ToolKind {
    /// Reach (m); share of a part's hit points mended per second; seconds to put back a part
    /// that is gone; how far round you its view shows integrity (m).
    Soldador { alcance: f64, ritmo: f32, reconstruir: f32, vista: f64 },
    /// The shot it fires (`shots.jsonc`) and the seconds to load the next.
    Lanzador { tiro: String, recarga: f64 },
}

/// What the scanner reads of what you look at.
#[derive(Clone, Debug, PartialEq)]
pub struct Scan {
    pub structure: u64,
    pub part: u32,
    /// It is not there (destroyed, torn off): it can be put back.
    pub gone: bool,
    pub title: String,
    pub detail: String,
    /// 0 gone .. 1 sound.
    pub integrity: f32,
    pub at: DVec3,
}

#[derive(Default)]
pub struct Gear {
    pub tools: Vec<ToolDef>,
    /// The tool in hand.
    pub held: Option<usize>,
    pub trigger: bool,
    /// A launcher just fired (taken by whoever sounds it).
    pub fired: bool,
    /// The welder's integrity view is on.
    pub view: bool,
    pub scan: Option<Scan>,
    /// Putting back a part: which, and how far along (0..1).
    rebuilding: Option<(u64, u32, f32)>,
    /// Seconds until the launcher is loaded.
    loading: f64,
    sparks: f32,
    /// Structures shown by their integrity this frame.
    shown: Vec<u64>,
    /// For its screen's letters.
    font: Option<Font>,
    /// Each tool's model as the renderer knows it (`Prop::mesh`), if it has one: its body and
    /// its pieces (in the order of its `sujecion.piezas`).
    meshes: Vec<Option<(u8, Vec<Option<u8>>)>>,
    /// The tool in hand as it moves, and as it is this frame.
    holding: Holding,
    pose: ToolPose,
    /// The launcher has a rocket in.
    loaded: bool,
    /// Being put away to take this one (or none) next.
    next: Option<Option<usize>>,
    /// What the tool did that is heard (taken by whoever sounds it).
    pub heard: Vec<String>,
    /// What the tool did this frame to the parts of structures: (structure, part, what), for
    /// whoever tells the other players (what is simulated in another's game is theirs to mend).
    pub ops: Vec<(u64, u32, PartOp)>,
}

/// What a tool does to a part of a structure.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PartOp {
    /// So many hit points mended.
    Mend(f32),
    /// Put back, with this share of its hit points.
    Rebuild(f32),
    /// Left with this share of its hit points gone (1: destroyed): a tool of the game's makers.
    Wreck(f32),
}

/// A tool in someone's hands as it is drawn: which, how it is held this frame and what it shows.
#[derive(Clone, Copy)]
pub struct InHand<'a> {
    pub tool: usize,
    pub pose: &'a ToolPose,
    /// At work (a welder's tip lit).
    pub hot: bool,
    /// What it fires is in (a launcher's lamp).
    pub loaded: bool,
    /// What its screen reads, its second use on, and what it is putting back and how far along.
    pub scan: Option<&'a Scan>,
    pub view: bool,
    pub rebuilding: Option<(u64, u32, f32)>,
}

/// The frame of what is held in the hands: the eye's own (x left, y up of the view, z where it
/// looks), so a tool stays in its place in the view wherever you look — up at a ceiling, down at
/// the deck.
pub fn eye_frame(view: &View) -> Quat {
    let f = view.forward.normalize_or(DVec3::Z);
    let left = view.up.cross(f).normalize_or(f.any_orthonormal_vector());
    let up = f.cross(left);
    Quat::from_mat3(&glam::Mat3::from_cols(left.as_vec3(), up.as_vec3(), f.as_vec3()))
}

/// The same with the look level (x left, y the view's up, z ahead on the level): what a tool that
/// rests on the body turns about (`HoldDef::pivote`).
pub fn level_frame(view: &View) -> Quat {
    let f = view.forward.normalize_or(DVec3::Z);
    let up = view.up.normalize_or(DVec3::Y);
    let left = up.cross(f).normalize_or(f.any_orthonormal_vector());
    let ahead = left.cross(up);
    Quat::from_mat3(&glam::Mat3::from_cols(left.as_vec3(), up.as_vec3(), ahead.as_vec3()))
}

/// Sparks a second while welding.
const SPARK_RATE: f32 = 14.0;
/// A part put back starts with this share of its hit points (the welder mends the rest).
const REBUILT: f32 = 0.2;

impl Gear {
    pub fn load(path: &std::path::Path, font: Font) -> Result<Gear, String> {
        let tools: Vec<ToolDef> = if path.exists() { lunar_core::defs::load(path).map_err(|e| format!("{}: {}", e.file, e.message))? } else { Vec::new() };
        for t in &tools {
            t.sujecion.check().map_err(|e| format!("herramienta {}: {e}", t.id))?;
        }
        Ok(Gear { tools, font: Some(font), loaded: true, ..Gear::default() })
    }

    /// Gives the renderer the tools' models (those of `models` that the tools name).
    pub fn models(&mut self, models: &lunar_core::structure::models::Models, r: &mut Renderer) -> Result<(), String> {
        self.meshes.clear();
        let white = lunar_core::mesh::Material::new([255; 3], 128, 0);
        for t in &self.tools {
            let mesh = match &t.modelo {
                Some(name) => {
                    let model = models.get(&format!("{name}/{}", lunar_core::structure::catalog::BODY)).ok_or_else(|| format!("herramienta {}: no hay modelo '{name}' (assets/models/{name}.glb)", t.id))?;
                    let mut pieces = Vec::new();
                    for piece in t.sujecion.piezas.keys() {
                        let m = models.get(&format!("{name}/{piece}")).ok_or_else(|| format!("herramienta {}: su modelo no tiene la pieza '{piece}'", t.id))?;
                        pieces.push(r.prop_mesh(&m.tinted(white)));
                    }
                    r.prop_mesh(&model.tinted(white)).map(|body| (body, pieces))
                }
                None => None,
            };
            self.meshes.push(mesh);
        }
        Ok(())
    }

    pub fn tool(&self) -> Option<&ToolDef> {
        self.held.and_then(|k| self.tools.get(k))
    }

    /// With the tool in hand a click is always its own (`Click::Herramienta`): nothing of a
    /// ship is worked by it.
    pub fn owns_click(&self) -> bool {
        self.tool().is_some_and(|t| t.clic == Click::Herramienta)
    }

    /// The point of the body that carries the tool in hand (`HoldDef::apoyo`), if one does.
    pub fn rest(&self) -> Option<&str> {
        self.tool().and_then(|t| t.sujecion.apoyo.as_ref()).map(|a| a.punto.as_str())
    }

    /// Tool `k` in hand, or away if it was. False: there is no such tool.
    pub fn pick(&mut self, k: usize) -> bool {
        if k >= self.tools.len() {
            return false;
        }
        self.take(if self.held == Some(k) && self.next.is_none() { None } else { Some(k) });
        true
    }

    /// `what` in hand next (None: none): what is in hand is put away first, if it has a way of
    /// being put away (its clip `guardar`); then the other is taken out (its clip `sacar`).
    fn take(&mut self, what: Option<usize>) {
        self.trigger = false;
        self.rebuilding = None;
        self.view = false;
        match self.held {
            Some(k) if self.holding.play(&self.tools[k].sujecion, "guardar") => self.next = Some(what),
            _ => self.hold(what),
        }
    }

    fn hold(&mut self, what: Option<usize>) {
        self.held = what;
        self.next = None;
        self.holding = Holding::default();
        if let Some(k) = what {
            self.holding.play(&self.tools[k].sujecion, "sacar");
        }
    }

    /// The tool in hand is being taken out, put away or loaded: it does nothing else meanwhile.
    pub fn busy(&self) -> bool {
        self.holding.clip().is_some_and(|c| matches!(c, "sacar" | "guardar" | "recarga"))
    }

    /// What tool `k` does with each button, in a line.
    pub fn help(&self, k: usize) -> String {
        match self.tools.get(k).map(|t| &t.kind) {
            Some(ToolKind::Soldador { .. }) => "Clic: soldar o reponer lo que miras (con él en la mano el clic no acciona mandos ni puertas) · Botón derecho: vista de integridad y filtro de soldadura".into(),
            Some(ToolKind::Lanzador { .. }) => "Clic: disparar · Botón derecho: apuntar".into(),
            None => String::new(),
        }
    }

    /// The suit's tools as the HUD shows them: a slot each, in the order of their keys.
    pub fn slots(&self, out: &mut Vec<crate::hud::Slot>) {
        for (k, t) in self.tools.iter().enumerate() {
            let held = self.held == Some(k);
            let (bar, note) = match &t.kind {
                ToolKind::Lanzador { recarga, .. } if !self.loaded => (Some((1.0 - self.loading / recarga.max(1e-3)).clamp(0.0, 1.0) as f32), String::new()),
                ToolKind::Lanzador { .. } if held => (None, "LISTO".into()),
                ToolKind::Soldador { .. } if held && self.view => (None, "VISTA DE INTEGRIDAD".into()),
                ToolKind::Soldador { .. } if held && self.rebuilding.is_some() => (self.rebuilding.map(|r| r.2), String::new()),
                _ => (None, String::new()),
            };
            let icon = match t.kind {
                ToolKind::Soldador { .. } => crate::hud::Icon::Welder,
                ToolKind::Lanzador { .. } => crate::hud::Icon::Launcher,
            };
            out.push(crate::hud::Slot { key: (k + 1).to_string(), name: t.nombre.clone(), held, bar, note, icon });
        }
    }

    /// By its id (scripts); None: hands free.
    pub fn pick_named(&mut self, id: Option<&str>) -> bool {
        match id {
            None => {
                self.hold(None);
                self.view = false;
                true
            }
            // (a script's: in hand at once, as it is held)
            Some(id) => match self.tools.iter().position(|t| t.id == id) {
                Some(k) => {
                    if self.held != Some(k) {
                        self.trigger = false;
                        self.rebuilding = None;
                        self.view = false;
                        self.hold(Some(k));
                        self.holding.playing = None;
                    }
                    true
                }
                None => false,
            },
        }
    }

    /// The right button with a tool in hand: the welder's view. False: it is not ours (zoom).
    pub fn secondary(&mut self) -> bool {
        match self.tool().map(|t| &t.kind) {
            Some(ToolKind::Soldador { .. }) => {
                self.view = !self.view;
                true
            }
            _ => false,
        }
    }

    /// What the scanner reads along the view within `reach`.
    fn read(view: &View, reach: f64, ships: &Ships, builds: &Builds) -> Option<Scan> {
        let set = &builds.set;
        let live = set.raycast(view.eye, view.forward, reach).map(|(k, h, p)| (k, h.part, f64::from(h.t), p, false));
        // what is gone, where it would be: nearer than what is there, it is what you look at
        let mut best = live;
        for (k, s) in set.list.iter().enumerate() {
            if s.to_world(s.center).distance(view.eye) > f64::from(s.radius) + reach {
                continue;
            }
            let limit = best.map_or(reach, |b| b.2) as f32;
            if let Some(h) = s.raycast_gone(s.to_local(view.eye), s.dir_to_local(view.forward), limit) {
                best = Some((k, h.part, f64::from(h.t), view.eye + view.forward * f64::from(h.t), true));
            }
        }
        let (k, part, _, at, gone) = best?;
        let s = &set.list[k];
        let p = &s.parts[part as usize];
        let cat = &set.lib.catalog;
        let (title, detail) = match ships.by_structure(s.id) {
            // (a container says what is left in it and what it weighs now)
            Some(n) => lunar_ship::cargo::scan(&ships.list[n].kind, s, cat, part as usize),
            None => {
                // (a container says what is in it; anything else, what it is made of)
                let kind = &cat.parts[usize::from(p.kind)];
                (kind.def.name.replace('_', " "), s.cargo_card(cat).or_else(|| kind.def.contents.clone()).unwrap_or_else(|| cat.materials[usize::from(kind.material)].1.name.clone()))
            }
        };
        Some(Scan { structure: s.id, part, gone, title, detail, integrity: if gone { 0.0 } else { 1.0 - p.damage() }, at })
    }

    /// The tool at work this frame. `steps`: the steps of whoever holds it; `short`: how far its
    /// hands fell short of it last frame (world).
    #[allow(clippy::too_many_arguments)]
    pub fn update(&mut self, dt: f64, view: &View, ships: &mut Ships, builds: &mut Builds, blasts: &mut Blasts, bodies: &BodyRegistry, r: &mut Renderer, steps: Steps, short: Vec3) {
        self.loading = (self.loading - dt).max(0.0);
        self.shown.clear();
        self.scan = None;
        self.ops.clear();
        let Some(tool) = self.tool().cloned() else {
            r.set_integrity_view(&[]);
            self.pose = ToolPose::default();
            return;
        };
        // how it is in the hands now
        let look = eye_frame(view);
        let pulling = self.trigger && !self.busy() && matches!(tool.kind, ToolKind::Soldador { .. });
        self.pose = self.holding.update(dt as f32, &tool.sujecion, look, level_frame(view), steps, look.inverse() * short, pulling);
        for e in std::mem::take(&mut self.holding.events) {
            match e.as_str() {
                "cargado" => (self.loaded, self.loading) = (true, 0.0),
                other => {
                    if let Some(sound) = other.strip_prefix("sonido:") {
                        self.heard.push(sound.to_string());
                    }
                }
            }
        }
        // put away: the next one out
        if let (Some(next), None) = (self.next, self.holding.clip()) {
            self.hold(next);
            r.set_integrity_view(&[]);
            return;
        }
        if self.busy() {
            self.rebuilding = None;
            r.set_integrity_view(&self.shown);
            if !matches!(tool.kind, ToolKind::Lanzador { .. }) || self.holding.clip() != Some("recarga") {
                return;
            }
        }
        match tool.kind {
            ToolKind::Soldador { alcance, ritmo, reconstruir, vista } => {
                self.scan = Gear::read(view, alcance, ships, builds);
                if self.view {
                    self.shown.extend(builds.set.list.iter().filter(|s| s.to_world(s.center).distance(view.eye) - f64::from(s.radius) < vista).map(|s| s.id));
                }
                let working = self.trigger.then(|| self.scan.clone()).flatten();
                if let Some(sc) = &working {
                    let now = builds.set.now;
                    let lib = builds.set.lib.clone();
                    if let Some(s) = builds.set.list.iter_mut().find(|s| s.id == sc.structure) {
                        s.awake_until = s.awake_until.max(now + LINGER);
                        if sc.gone {
                            // putting it back takes a moment; then it is there, to be mended
                            let done = match self.rebuilding {
                                Some((id, p, k)) if id == sc.structure && p == sc.part => k,
                                _ => 0.0,
                            } + dt as f32 / reconstruir.max(0.05);
                            if done >= 1.0 {
                                s.rebuild(&lib.catalog, sc.part as usize, REBUILT);
                                self.ops.push((sc.structure, sc.part, PartOp::Rebuild(REBUILT)));
                                self.rebuilding = None;
                            } else {
                                self.rebuilding = Some((sc.structure, sc.part, done));
                            }
                        } else {
                            self.rebuilding = None;
                            let hp = s.parts[sc.part as usize].max_hp * ritmo * dt as f32;
                            s.mend(&lib.catalog, sc.part as usize, hp);
                            self.ops.push((sc.structure, sc.part, PartOp::Mend(hp)));
                        }
                    }
                    if let Some(n) = ships.by_structure(sc.structure) {
                        ships.list[n].touch();
                    }
                    // the arc: sparks where it works
                    self.sparks += SPARK_RATE * dt as f32;
                    while self.sparks >= 1.0 {
                        self.sparks -= 1.0;
                        let _ = blasts.fx.explode_scaled("soldadura", bodies, bodies.dominant(sc.at), sc.at - view.forward * 0.03, 0.5);
                    }
                } else {
                    self.rebuilding = None;
                }
            }
            ToolKind::Lanzador { ref tiro, recarga } => {
                let hold = &tool.sujecion;
                // (a launcher with no way of being loaded written: loaded when its time is up)
                if !self.loaded && self.holding.clip().is_none() && !self.holding.play(hold, "recarga") && self.loading <= 0.0 {
                    self.loaded = true;
                }
                if self.trigger && self.loaded && !self.busy() {
                    // out of its muzzle, where one aims
                    let muzzle = match hold.puntos.get("boca") {
                        Some(p) => view.eye + (look * self.pose.at.point(Vec3::from(p.en))).as_dvec3(),
                        None => view.eye + view.forward * 0.9 + view.forward.cross(view.up).normalize_or(DVec3::X) * 0.22 - view.up * 0.1,
                    };
                    if blasts.fire_from(tiro, muzzle, view.forward, bodies, builds) {
                        (self.loading, self.loaded, self.fired) = (recarga, false, true);
                        self.holding.fired(hold);
                        self.holding.play(hold, "disparo");
                    }
                }
                self.trigger = false;
            }
        }
        r.set_integrity_view(&self.shown);
    }

    /// Where the hands are asked to be this frame (left, right): on the tool, or wherever its
    /// clip has them (`holding`).
    pub fn grips(&self, view: &View, body: &Body, stance: &Stance) -> [Option<Grip>; 2] {
        if self.held.is_none() {
            return [None, None];
        }
        Gear::grips_of(&self.pose, view, body, stance)
    }

    /// The same for a tool held as `pose` says by whoever looks along `view` (another player's).
    pub fn grips_of(pose: &ToolPose, view: &View, body: &Body, stance: &Stance) -> [Option<Grip>; 2] {
        let look = eye_frame(view);
        let tool = pose.at;
        let poses = &body.rig.def.manos;
        let fingers = |name: &str| poses.get(name).copied().unwrap_or([0.0; 5]);
        let place = |p: &Place| -> Option<(DVec3, Vec3, Vec3)> {
            match p {
                Place::Tool(at, n, a) => Some((view.eye + (look * tool.point(*at)).as_dvec3(), look * (tool.rot * *n), look * (tool.rot * *a))),
                Place::Body(name) => body.point(name, stance),
                Place::Free => None,
            }
        };
        let grip = |h: &HandAt| -> Option<Grip> {
            let (a, b) = (place(&h.from), place(&h.to));
            let (fa, fb) = (fingers(&h.poses.0), fingers(&h.poses.1));
            let fingers = std::array::from_fn(|i| fa[i] + (fb[i] - fa[i]) * h.k);
            match (a, b) {
                (Some(a), Some(b)) => {
                    // (a hand going from one place to another goes round, not through)
                    let way = (b.0 - a.0).as_vec3();
                    let out = (a.1 + b.1) * -0.5;
                    let arc = out * (way.length() * 0.18 * (std::f32::consts::PI * h.k).sin());
                    Some(Grip { at: a.0.lerp(b.0, f64::from(h.k)) + arc.as_dvec3(), normal: a.1.lerp(b.1, h.k).normalize_or(b.1), across: a.2.lerp(b.2, h.k).normalize_or(b.2), fingers })
                }
                (Some(a), None) if h.k < 0.5 => Some(Grip { at: a.0, normal: a.1, across: a.2, fingers }),
                (None, Some(b)) if h.k >= 0.5 => Some(Grip { at: b.0, normal: b.1, across: b.2, fingers }),
                _ => None,
            }
        };
        std::array::from_fn(|i| pose.hands[i].as_ref().and_then(grip))
    }

    /// Tool `k` in another's hands this frame: how it is held (their look, their steps, how far
    /// their hands fell short of it), moved on `dt` in `holding` (theirs: kept by whoever draws them).
    pub fn pose_of(&self, k: usize, holding: &mut Holding, dt: f32, view: &View, steps: Steps, short: Vec3, pulling: bool) -> Option<ToolPose> {
        let tool = self.tools.get(k)?;
        let look = eye_frame(view);
        let pose = holding.update(dt, &tool.sujecion, look, level_frame(view), steps, look.inverse() * short, pulling && matches!(tool.kind, ToolKind::Soldador { .. }));
        holding.events.clear();
        Some(pose)
    }

    /// Tool `k` just taken in another's hands, or fired by them: its movement starts (`sacar`, `disparo`).
    pub fn play_of(&self, k: usize, holding: &mut Holding, clip: &str) {
        if let Some(tool) = self.tools.get(k) {
            if clip == "disparo" {
                holding.fired(&tool.sujecion);
            }
            holding.play(&tool.sujecion, clip);
        }
    }

    /// What the tool in hand is at work on this frame: the structure and the point of it (world).
    pub fn work(&self) -> Option<(u64, DVec3)> {
        self.scan.as_ref().filter(|_| self.trigger && self.held.is_some() && !self.busy()).map(|s| (s.structure, s.at))
    }

    /// Where the tool in hand is now, in the eye's frame (x left, y up, z ahead; m).
    pub fn tool_at(&self) -> Option<Vec3> {
        self.held.map(|_| self.pose.at.pos)
    }

    /// The launcher has a rocket in.
    pub fn loaded(&self) -> bool {
        self.loaded
    }

    /// The tool in hand is at work on something (a welder mending).
    pub fn working(&self) -> bool {
        self.trigger && self.scan.is_some() && self.held.is_some()
    }

    /// The welder's right button is on with it in hand: its view of integrity, and with it the
    /// visor's welding filter (`visor`). It never comes down by itself.
    pub fn filtering(&self) -> bool {
        self.view && matches!(self.tool().map(|t| &t.kind), Some(ToolKind::Soldador { .. }))
    }

    /// The tool in hand, in the eye's frame (`inside`: under a ship's lamps). `hands`: where the
    /// holder's hands are (what one of them carries goes with it).
    pub fn show(&self, out: &mut PropScene, view: &View, inside: bool, hands: Option<&[BodyHand; 2]>) {
        let Some(tool) = self.held else { return };
        let hot = self.trigger && self.scan.is_some() && !self.busy();
        self.show_held(out, &InHand { tool, pose: &self.pose, hot, loaded: self.loaded, scan: self.scan.as_ref(), view: self.view, rebuilding: self.rebuilding }, view, inside, hands);
    }

    /// A tool in anyone's hands (`it`: which and how it is held), in the frame of the eyes that
    /// look along `view`.
    pub fn show_held(&self, out: &mut PropScene, it: &InHand, view: &View, inside: bool, hands: Option<&[BodyHand; 2]>) {
        let (Some(tool), Some(font)) = (self.tools.get(it.tool), &self.font) else { return };
        let frame = out.frames.len() as u16;
        let look = eye_frame(view);
        out.frames.push(PropFrame { pos: view.eye, rot: look, inside });
        let mut pen = Hand { out, frame, font, at: it.pose.at };
        let model = self.meshes.get(it.tool).cloned().flatten();
        let hold = &tool.sujecion;
        let point = |name: &str, or: Vec3| hold.puntos.get(name).map_or(or, |p| Vec3::from(p.en));
        if let Some((body, pieces)) = &model {
            pen.model(*body, Vec3::ZERO, Quat::IDENTITY);
            for (i, mesh) in pieces.iter().enumerate() {
                let (Some(mesh), Some(&(x, shows))) = (mesh, it.pose.pieces.get(i)) else { continue };
                if !shows {
                    continue;
                }
                match (it.pose.carried.get(i).copied().flatten(), hands) {
                    // in a hand: held by its point, wherever the hand is
                    (Some((side, held_by)), Some(hands)) => {
                        let rot = hands[side].rot * held_by.rot.inverse();
                        let at = hands[side].at - (rot * held_by.pos).as_dvec3();
                        pen.out.props.push(Prop { frame, mesh: *mesh, pos: look.inverse() * (at - view.eye).as_vec3(), rot: look.inverse() * rot, size: Vec3::ONE, color: [255; 3], emissive: 0.0, rough: 128, metal: 0 });
                    }
                    _ => pen.model(*mesh, x.pos, x.rot),
                }
            }
        }
        match &tool.kind {
            ToolKind::Soldador { .. } => {
                let hot = it.hot;
                let tip = point("punta", Vec3::new(0.0, 0.008, 0.178));
                match model {
                    // its tip, lit while it works
                    Some(_) if hot => pen.ball(tip, 0.012, [190, 220, 255], 5.0),
                    Some(_) => {}
                    None => {
                        let o = Vec3::ZERO;
                        pen.boxed(o, Vec3::new(0.05, 0.062, 0.16), [204, 152, 22], 0.0, 120, 30);
                        pen.boxed(o + Vec3::new(0.0, -0.06, -0.035), Vec3::new(0.026, 0.075, 0.034), [26, 26, 28], 0.0, 200, 0);
                        pen.rod(o + Vec3::new(0.0, 0.008, 0.115), 0.018, 0.08, [150, 152, 158], 0.0);
                        pen.ball(o + Vec3::new(0.0, 0.008, 0.16), 0.014, if hot { [190, 220, 255] } else { [60, 62, 66] }, if hot { 5.0 } else { 0.0 });
                    }
                }
                // its screen, turned to where the eye is when it is held still (a model has its
                // housing)
                let c = point("pantalla", WELDER_SCREEN);
                let n = (-(Vec3::from(hold.en) + c)).normalize();
                let u = (Vec3::NEG_X - n * Vec3::NEG_X.dot(n)).normalize();
                let v = n.cross(u);
                let (w, h) = (0.09, 0.056);
                if model.is_none() {
                    pen.plate(c - n * 0.006, u, v, n, w + 0.012, h + 0.012, 0.01, [22, 22, 24], 0.0);
                }
                pen.plate(c, u, v, n, w, h, 0.002, [6, 14, 12], -0.25);
                let at = |x: f32, y: f32| c + u * (x * w * 0.5) + v * (y * h * 0.5) + n * 0.002;
                match it.scan {
                    Some(sc) => {
                        let (state, color) = match sc.integrity {
                            _ if sc.gone => ("FALTA", [200, 110, 255]),
                            i if i >= 0.97 => ("EN BUEN ESTADO", [110, 255, 150]),
                            i if i >= 0.6 => ("DAÑADA", [255, 210, 80]),
                            i if i >= 0.25 => ("MUY DAÑADA", [255, 140, 50]),
                            _ => ("CASI DESTRUIDA", [255, 70, 50]),
                        };
                        pen.text(&fit(&sc.title, 22), at(-0.92, 0.66), u, v, 0.0074, [215, 245, 230], 0.0);
                        pen.text(&fit(&sc.detail, 30), at(-0.92, 0.28), u, v, 0.0052, [130, 170, 155], 0.0);
                        pen.text(state, at(-0.92, -0.14), u, v, 0.0062, color, 0.0);
                        pen.text(&format!("{:.0} %", sc.integrity * 100.0), at(0.92, -0.14), u, v, 0.0062, color, 1.0);
                        // the bar: its integrity; while it is put back, how far along
                        let (fill, bar) = match it.rebuilding {
                            Some((id, p, k)) if id == sc.structure && p == sc.part => (k, [200, 110, 255]),
                            _ => (sc.integrity, color),
                        };
                        let bc = at(0.0, -0.52);
                        pen.plate(bc, u, v, n, w * 0.92, h * 0.16, 0.001, [30, 44, 40], -0.3);
                        let fw = w * 0.92 * fill.clamp(0.0, 1.0);
                        if fw > 0.001 {
                            pen.plate(bc - u * ((w * 0.92 - fw) * 0.5) + n * 0.001, u, v, n, fw, h * 0.16, 0.001, bar, -1.6);
                        }
                    }
                    None => pen.text("SIN LECTURA", at(-0.92, 0.1), u, v, 0.0068, [110, 140, 130], 0.0),
                }
                if it.view {
                    pen.text("VISTA DE INTEGRIDAD", at(0.92, -0.9), u, v, 0.0034, [200, 110, 255], 1.0);
                }
            }
            ToolKind::Lanzador { .. } => {
                // on the shoulder: most of it behind the eye
                if model.is_none() {
                    let o = Vec3::ZERO;
                    pen.rod(o, 0.085, 0.8, [72, 82, 60], 0.0);
                    pen.rod(o + Vec3::new(0.0, 0.0, 0.4), 0.096, 0.04, [40, 44, 36], 0.0);
                    pen.boxed(o + Vec3::new(0.0, -0.075, 0.26), Vec3::new(0.026, 0.085, 0.034), [26, 26, 28], 0.0, 200, 0);
                    pen.boxed(o + Vec3::new(0.06, 0.05, 0.3), Vec3::new(0.022, 0.026, 0.05), [30, 30, 32], 0.0, 160, 20);
                }
                // its lamp: green when loaded, amber while it loads
                pen.ball(point("luz", LAUNCHER_LAMP), 0.007, if it.loaded { [70, 255, 110] } else { [255, 170, 40] }, -1.5);
            }
        }
    }
}

/// Where the welder's screen and the launcher's lamp are on them (the tool's frame) when their
/// data does not say (`sujecion.puntos`: `pantalla`, `luz`).
pub const WELDER_SCREEN: Vec3 = Vec3::new(0.03, 0.078, -0.02);
pub const LAUNCHER_LAMP: Vec3 = Vec3::new(0.06, 0.068, 0.285);

/// `s` cut to `n` letters.
fn fit(s: &str, n: usize) -> String {
    if s.chars().count() <= n { s.to_string() } else { s.chars().take(n - 1).chain(std::iter::once('…')).collect() }
}

/// Draws on the tool: everything is given in the tool's own frame and goes to the eye's by where
/// the tool is in it now (`at`).
struct Hand<'a> {
    out: &'a mut PropScene,
    frame: u16,
    font: &'a Font,
    at: lunar_core::anim::Xf,
}

impl Hand<'_> {
    /// A model (a mesh the renderer was given), as it is, its origin at `at`, turned by `rot`.
    fn model(&mut self, mesh: u8, at: Vec3, rot: Quat) {
        self.out.props.push(Prop { frame: self.frame, mesh, pos: self.at.point(at), rot: self.at.rot * rot, size: Vec3::ONE, color: [255; 3], emissive: 0.0, rough: 128, metal: 0 });
    }

    #[allow(clippy::too_many_arguments)]
    fn boxed(&mut self, at: Vec3, size: Vec3, color: [u8; 3], emissive: f32, rough: u8, metal: u8) {
        self.out.props.push(Prop { frame: self.frame, mesh: BOX, pos: self.at.point(at), rot: self.at.rot, size, color, emissive, rough, metal });
    }

    /// A cylinder along z (ahead), `d` across and `len` long.
    fn rod(&mut self, at: Vec3, d: f32, len: f32, color: [u8; 3], emissive: f32) {
        self.out.props.push(Prop { frame: self.frame, mesh: CYLINDER, pos: self.at.point(at), rot: self.at.rot * Quat::from_rotation_x(std::f32::consts::FRAC_PI_2), size: Vec3::new(d, len, d), color, emissive, rough: 110, metal: 120 });
    }

    fn ball(&mut self, at: Vec3, d: f32, color: [u8; 3], emissive: f32) {
        self.out.props.push(Prop { frame: self.frame, mesh: SPHERE, pos: self.at.point(at), rot: Quat::IDENTITY, size: Vec3::splat(d), color, emissive, rough: 80, metal: 0 });
    }

    /// A thin box in the plane (u, v) facing `n`, `w` by `h`.
    #[allow(clippy::too_many_arguments)]
    fn plate(&mut self, at: Vec3, u: Vec3, v: Vec3, n: Vec3, w: f32, h: f32, t: f32, color: [u8; 3], emissive: f32) {
        let rot = Quat::from_mat3(&glam::Mat3::from_cols(u, v, n));
        self.out.props.push(Prop { frame: self.frame, mesh: BOX, pos: self.at.point(at), rot: self.at.rot * rot, size: Vec3::new(w, h, t), color, emissive, rough: 60, metal: 0 });
    }

    /// Lit text from `at` (its left end, or its right with `align` 1), `size` m tall.
    #[allow(clippy::too_many_arguments)]
    fn text(&mut self, s: &str, at: Vec3, u: Vec3, v: Vec3, size: f32, color: [u8; 3], align: f32) {
        let mut placed = Vec::new();
        self.font.layout(s, align, &mut placed);
        let (u, v) = (self.at.rot * u, self.at.rot * v);
        let at = self.at.point(at);
        for g in &placed {
            let c = at + u * (g.center[0] * size) + v * (g.center[1] * size);
            self.out.glyphs.push(GlyphQuad { frame: self.frame, center: c, u: u * (g.half[0] * size), v: v * (g.half[1] * size), uv: g.uv, color, emissive: 2.0, relief: 0.0 });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tool_has_the_model_it_names() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let tools: Vec<ToolDef> = lunar_core::defs::load(&root.join("defs/gear.jsonc")).unwrap();
        let models = lunar_core::structure::models::Models::load(&root.join("models")).unwrap();
        for t in &tools {
            let name = t.modelo.as_ref().unwrap_or_else(|| panic!("la herramienta {} no tiene modelo", t.id));
            let m = models.get(&format!("{name}/{}", lunar_core::structure::catalog::BODY)).unwrap_or_else(|| panic!("no hay modelo {name}"));
            // held in the hands: within arm's reach of where it is held, and nothing of it a tint
            let (lo, hi) = m.bounds();
            assert!(lo.min_element() > -0.6 && hi.max_element() < 0.6 && m.mesh.tris() > 500, "{name}: {lo:?}..{hi:?}, {} triángulos", m.mesh.tris());
            assert!(m.tint.iter().all(|&t| t == 0), "{name}: una herramienta lleva sus propios colores");
        }
    }

    #[test]
    fn tools_are_picked_and_put_away() {
        let tools: Vec<ToolDef> = lunar_core::defs::parse("gear", include_str!("../../../assets/defs/gear.jsonc")).unwrap();
        assert!(tools.iter().any(|t| matches!(t.kind, ToolKind::Soldador { .. })) && tools.iter().any(|t| matches!(t.kind, ToolKind::Lanzador { .. })));
        for t in &tools {
            t.sujecion.check().unwrap_or_else(|e| panic!("{}: {e}", t.id));
        }
        // (taken and put away at once: what moves them in the hands has its own tests)
        let tools: Vec<ToolDef> = tools.into_iter().map(|t| ToolDef { sujecion: HoldDef { clips: Default::default(), ..t.sujecion }, ..t }).collect();
        let mut g = Gear { tools, ..Gear::default() };
        assert!(g.tool().is_none() && !g.secondary());
        assert!(g.pick(0));
        assert_eq!(g.held, Some(0));
        // the welder's right button is its view; the launcher's is not its own
        assert!(g.pick_named(Some("soldador")) && g.secondary() && g.view);
        assert!(g.pick_named(Some("lanzacohetes")) && !g.view && !g.secondary());
        assert!(!g.pick_named(Some("nada")));
        // a click with the welder in hand is the welder's, whatever it is aimed at; with the
        // launcher, what is aimed at comes first
        assert!(g.pick_named(Some("soldador")) && g.owns_click());
        assert!(g.pick_named(Some("lanzacohetes")) && !g.owns_click());
        // the launcher rests on a shoulder, a point the rig has
        let rig: crate::rig::RigDef = lunar_core::defs::parse("astronauta", include_str!("../../../assets/defs/rigs/astronauta.jsonc")).unwrap();
        for t in &g.tools {
            if let Some(rest) = &t.sujecion.apoyo {
                assert!(rig.puntos.contains_key(&rest.punto), "{}: se apoya en '{}', que el esqueleto no tiene", t.id, rest.punto);
            }
        }
        assert_eq!(g.rest(), Some("hombro_der"));
        // the same key again puts it away
        let k = g.held.unwrap();
        assert!(g.pick(k));
        assert!(g.held.is_none() && !g.pick(99));
        // a slot each for the HUD, in the order of their keys; what each does, in a line
        let mut slots = Vec::new();
        g.slots(&mut slots);
        assert_eq!(slots.len(), g.tools.len());
        assert!(slots[0].key == "1" && !slots[0].held && !g.help(0).is_empty() && g.help(99).is_empty());
        g.pick(1);
        slots.clear();
        g.slots(&mut slots);
        assert!(slots[1].held && !slots[0].held);
        assert_eq!(fit("Unidad hidráulica de babor", 10), "Unidad hi…");
    }

    #[test]
    fn what_is_in_the_hands_follows_the_look_up_and_down() {
        // the tool's frame is the eye's: looking at the ceiling or at the deck, what is drawn
        // 0.46 m ahead and 0.2 m under the line of sight is still there in the view
        let up = DVec3::Y;
        for pitch in [-80.0f64, -30.0, 0.0, 45.0, 85.0] {
            let p = pitch.to_radians();
            let forward = DVec3::new(0.0, p.sin(), p.cos());
            let view = View { eye: DVec3::ZERO, forward, up, fov_y: 1.0, near: 0.1 };
            let rot = eye_frame(&view);
            let at = |v: Vec3| (rot * v).as_dvec3();
            assert!(at(Vec3::Z).distance(forward) < 1e-5, "a {pitch}°: no mira adonde miras");
            // the view's own up and left: perpendicular to the look, upright
            let (l, u) = (at(Vec3::X), at(Vec3::Y));
            assert!(u.dot(forward).abs() < 1e-5 && l.dot(forward).abs() < 1e-5 && l.dot(up).abs() < 1e-5);
            assert!(u.dot(up) > 0.0, "a {pitch}°: boca abajo");
            let tool = at(Vec3::new(-0.23, -0.2, 0.46));
            assert!((tool.dot(forward) - 0.46).abs() < 1e-5 && (tool.dot(u) + 0.2).abs() < 1e-5, "a {pitch}°: la herramienta no está en su sitio de la vista");
        }
    }
}
