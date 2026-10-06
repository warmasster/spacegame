//! A seat's own flight controls: a stick and a throttle beside every seat a ship is flown from
//! (a seat whose keys drive controls: `asientos[].mandos`), moved by what those keys fly, with
//! the hands of whoever sits there on them — the right on the stick, its index on the trigger
//! when something is fired, the left on the throttle.
//!
//! They are props, not parts of any ship: what they are, where they stand by the seat (from the
//! seat's eyes, in its own frame: any seat of any ship), their moving pieces and what moves each
//! is data (`assets/defs/manos.jsonc`, `cabina`). What is flown is told by name (`canales`):
//! each name says how to find it among a seat's keys — the first control its keys hold by an
//! axis, the control a key turns up, a key that presses something — so a ship says nothing new
//! for it. Their looks are models (`assets/models/<modelo>.glb`: its body and a mesh per piece);
//! without the model, plain shapes.
//!
//! `Cockpits` follows what is flown in the ships about (eased: a key held throws a control to
//! its stop at once) and draws the props; a body sat in such a seat asks it where its hands go.
use crate::{
    handwork::Target,
    holding::turned,
    ships::Ships,
};
use glam::{DVec3, Quat, Vec3};
use lunar_controls::intent::F_PRESSED;
use lunar_core::{
    anim::{Xf, skeleton::frame_turn, spring::Spring},
    props::{BOX, CYLINDER, Prop, PropFrame, PropScene, SPHERE},
    structure::set::Structures,
};
use lunar_ship::Ship;
use serde::Deserialize;
use std::collections::BTreeMap;

/// What is flown, by name: how to find it among a seat's keys.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChannelDef {
    /// This axis of the `de`-th control (from 0) the seat's keys hold by an axis: -1 .. 1.
    #[serde(default)]
    pub eje: Option<u8>,
    #[serde(default)]
    pub de: usize,
    /// Or the control a key works with this action: `subir` (its travel, 0 .. 1), `pulsar` (1
    /// while it is pressed).
    #[serde(default)]
    pub accion: Option<String>,
}

/// What a piece does as something is flown: it turns about `eje` by `grados` at the most, or
/// goes along it by `metros`; `desde`: where it is with nothing flown (degrees about `eje`: a
/// throttle at idle is back, not upright).
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MoveDef {
    pub canal: String,
    pub eje: [f32; 3],
    #[serde(default)]
    pub grados: f32,
    #[serde(default)]
    pub metros: f32,
    #[serde(default)]
    pub desde: f32,
}

/// A moving piece: where its pivot is on the prop (its frame: x left, y up, z ahead), the piece
/// it rides on, if any, and what moves it.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PieceDef {
    pub en: [f32; 3],
    #[serde(default)]
    pub padre: Option<String>,
    #[serde(default)]
    pub mueve: Vec<MoveDef>,
}

/// The finger on a trigger: what pulls it, and the hand's pose pulling.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PullDef {
    pub canal: String,
    pub pose: String,
}

/// Where a hand holds it: the middle of what it closes on, the way the palm faces, from index
/// to little finger (the prop's frame), the piece it goes with, the hand's pose.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HoldDef {
    pub en: [f32; 3],
    pub palma: [f32; 3],
    pub traves: [f32; 3],
    #[serde(default)]
    pub pieza: Option<String>,
    /// The hand goes where its piece goes but keeps facing as written: what it holds turns in
    /// it (the bar of a lever that swings fore and aft under a hand laid on it).
    #[serde(default)]
    pub libre: bool,
    pub pose: String,
    #[serde(default)]
    pub gatillo: Option<PullDef>,
}

/// A control beside the seat.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PropDef {
    pub id: String,
    /// `assets/models/<modelo>.glb`: its body (the mesh `cuerpo`) and a mesh per piece.
    #[serde(default)]
    pub modelo: Option<String>,
    /// Whose hand: `izq` or `der`.
    pub mano: String,
    /// Where its origin is from the seat's eyes, in the seat's own frame (x to the sitter's
    /// left, y up, z ahead; m), and how it is turned (degrees: nose up, nose to the left,
    /// rolled).
    pub en: [f32; 3],
    #[serde(default)]
    pub giro: [f32; 3],
    #[serde(default)]
    pub piezas: BTreeMap<String, PieceDef>,
    pub agarre: HoldDef,
}

/// `manos.jsonc`, `cabina`.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CockpitDef {
    /// Half-life with which the props follow what is flown (s).
    pub sigue: f32,
    pub canales: BTreeMap<String, ChannelDef>,
    pub mandos: Vec<PropDef>,
}

/// The most things flown a seat has (`canales`).
pub const CHANNELS: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Read {
    Axis(u8),
    Level,
    Press,
}

/// How a channel is found among a seat's keys.
#[derive(Clone, Debug)]
struct Channel {
    read: Read,
    nth: usize,
    action: Option<String>,
}

#[derive(Clone, Debug)]
struct Piece {
    name: String,
    pivot: Vec3,
    parent: Option<usize>,
    /// (channel, axis, rad, m, rad with nothing flown).
    moves: Vec<(usize, Vec3, f32, f32, f32)>,
}

#[derive(Clone, Debug)]
pub struct PropKit {
    pub id: String,
    pub model: Option<String>,
    pub side: usize,
    at: Vec3,
    turn: Quat,
    /// Parents before what rides on them.
    pieces: Vec<Piece>,
    grip: (Vec3, Vec3, Vec3),
    grip_piece: Option<usize>,
    /// The hand keeps its own way as its piece turns.
    grip_free: bool,
    pose: [f32; 5],
    /// The channel that pulls its trigger and the hand's pose pulling.
    pull: Option<(usize, [f32; 5])>,
}

/// `CockpitDef` ready to use.
#[derive(Clone, Debug)]
pub struct CockpitKit {
    follow: f32,
    channels: Vec<Channel>,
    pub props: Vec<PropKit>,
}

impl CockpitKit {
    pub fn new(def: &CockpitDef, poses: &BTreeMap<String, [f32; 5]>) -> Result<CockpitKit, String> {
        if def.canales.len() > CHANNELS {
            return Err(format!("cabina: {} canales (caben {CHANNELS})", def.canales.len()));
        }
        let pose = |name: &str| poses.get(name).copied().ok_or_else(|| format!("cabina: no hay pose de mano '{name}' en el esqueleto"));
        let names: Vec<&String> = def.canales.keys().collect();
        let channel = |name: &str| names.iter().position(|n| *n == name).ok_or_else(|| format!("cabina: no hay canal '{name}'"));
        let mut channels = Vec::new();
        for (name, c) in &def.canales {
            let read = match (c.eje, c.accion.as_deref()) {
                (Some(axis), None) => Read::Axis(axis),
                (None, Some("pulsar")) => Read::Press,
                (None, Some(_)) => Read::Level,
                _ => return Err(format!("cabina: el canal '{name}' es un 'eje' o una 'accion'")),
            };
            channels.push(Channel { read, nth: c.de, action: c.accion.clone() });
        }
        let mut props = Vec::new();
        for p in &def.mandos {
            let side = match p.mano.as_str() {
                "izq" => 0,
                "der" => 1,
                other => return Err(format!("cabina: {}: mano '{other}' (izq o der)", p.id)),
            };
            // (what rides on another after it)
            let mut order: Vec<&String> = p.piezas.keys().collect();
            order.sort_by_key(|n| p.piezas[*n].padre.is_some());
            let mut pieces = Vec::new();
            for name in &order {
                let d = &p.piezas[*name];
                let parent = match &d.padre {
                    Some(over) => Some(order.iter().position(|n| *n == over).filter(|k| p.piezas[order[*k]].padre.is_none()).ok_or_else(|| format!("cabina: {}: la pieza '{name}' va sobre '{over}', que no hay o va sobre otra", p.id))?),
                    None => None,
                };
                let mut moves = Vec::new();
                for m in &d.mueve {
                    moves.push((channel(&m.canal)?, Vec3::from(m.eje).normalize_or(Vec3::X), m.grados.to_radians(), m.metros, m.desde.to_radians()));
                }
                pieces.push(Piece { name: (*name).clone(), pivot: Vec3::from(d.en), parent, moves });
            }
            let g = &p.agarre;
            let grip_piece = match &g.pieza {
                Some(name) => Some(pieces.iter().position(|x| x.name == *name).ok_or_else(|| format!("cabina: {}: su agarre va con la pieza '{name}', que no hay", p.id))?),
                None => None,
            };
            let normal = Vec3::from(g.palma).normalize_or(Vec3::X);
            let across = Vec3::from(g.traves);
            let across = (across - normal * across.dot(normal)).normalize_or(normal.any_orthonormal_vector());
            let pull = match &g.gatillo {
                Some(t) => Some((channel(&t.canal)?, pose(&t.pose)?)),
                None => None,
            };
            props.push(PropKit { id: p.id.clone(), model: p.modelo.clone(), side, at: Vec3::from(p.en), turn: turned(p.giro), pieces, grip: (Vec3::from(g.en), normal, across), grip_piece, grip_free: g.libre, pose: pose(&g.pose)?, pull });
        }
        Ok(CockpitKit { follow: def.sigue.max(1e-3), channels, props })
    }

    /// The pieces of prop `k`, by name, in the order they are kept (for whoever gives it its
    /// meshes).
    pub fn pieces(&self, k: usize) -> impl Iterator<Item = &str> {
        self.props[k].pieces.iter().map(|p| p.name.as_str())
    }
}

/// One thing flown from a seat: the control, how it is read, its range.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Flown {
    control: u16,
    read: Read,
    lo: f32,
    hi: f32,
}

/// A seat a ship is flown from: what each channel reads, and every control its keys work.
#[derive(Clone, Copy, Debug, Default)]
pub struct SeatRig {
    flown: [Option<Flown>; CHANNELS],
    bound: [u16; 12],
    bounds: u8,
}

impl SeatRig {
    /// Seat `seat` of `ship`, if a ship is flown from it.
    pub fn of(kit: &CockpitKit, ship: &Ship, seat: usize) -> Option<SeatRig> {
        let d = &ship.kind.seats.get(seat)?.def;
        if d.mandos.is_empty() {
            return None;
        }
        let controls = &ship.panels.controls;
        let find = |id: &str| controls.iter().position(|c| c.id == id).map(|k| k as u16);
        let mut rig = SeatRig::default();
        // every control its keys work, in the order they are first named; the ones held by an
        // axis, in theirs
        let mut by_axis = [0u16; 12];
        let mut axes = 0;
        for b in &d.mandos {
            let Some(k) = find(&b.mando) else { continue };
            if !rig.bound[..usize::from(rig.bounds)].contains(&k) && usize::from(rig.bounds) < rig.bound.len() {
                rig.bound[usize::from(rig.bounds)] = k;
                rig.bounds += 1;
            }
            if b.accion.is_none() && !by_axis[..axes].contains(&k) && axes < by_axis.len() {
                by_axis[axes] = k;
                axes += 1;
            }
        }
        for (n, ch) in kit.channels.iter().enumerate() {
            let control = match &ch.action {
                None => by_axis[..axes].get(ch.nth).copied(),
                Some(action) => d.mandos.iter().filter(|b| b.accion.as_deref() == Some(action.as_str())).nth(ch.nth).and_then(|b| find(&b.mando)),
            };
            let Some(control) = control else { continue };
            let c = &controls[usize::from(control)];
            let def = &ship.kind.panels[c.panel].def.mandos[c.index];
            // (an axis the control has not is not flown)
            if matches!(ch.read, Read::Axis(a) if a >= def.ejes.unwrap_or(1)) {
                continue;
            }
            let (lo, hi) = match &def.rango {
                Some([a, b]) => (a.si().unwrap_or(0.0) as f32, b.si().unwrap_or(1.0) as f32),
                None => (0.0, 1.0),
            };
            rig.flown[n] = Some(Flown { control, read: ch.read, lo, hi });
        }
        Some(rig)
    }

    /// The seat's keys work control `k`.
    pub fn flies(&self, k: u16) -> bool {
        self.bound[..usize::from(self.bounds)].contains(&k)
    }

    /// What is flown now, channel by channel: axes -1 .. 1, travels and presses 0 .. 1.
    fn read(&self, ship: &Ship) -> [f32; CHANNELS] {
        std::array::from_fn(|n| {
            let Some(f) = self.flown[n] else { return 0.0 };
            let Some(c) = ship.panels.controls.get(usize::from(f.control)) else { return 0.0 };
            let share = |v: f64| ((v as f32 - f.lo) / (f.hi - f.lo).max(1e-6)).clamp(0.0, 1.0);
            match f.read {
                Read::Axis(0) => share(c.mech.value(&c.st)) * 2.0 - 1.0,
                Read::Axis(_) => c.mech.value2(&c.st).map_or(0.0, |v| share(v) * 2.0 - 1.0),
                Read::Level => share(c.mech.value(&c.st)),
                Read::Press => f32::from(u8::from(c.st.has(F_PRESSED))),
            }
        })
    }
}

/// The seat's frame in its ship's: its eyes, x to the sitter's left, y up, z the way it faces.
fn seat_frame(ship: &Ship, seat: usize) -> Xf {
    let d = &ship.kind.seats[seat].def;
    // (where the seat is now: one on a platform goes down and up with it)
    Xf::new(ship.seat_eyes(seat), Quat::from_rotation_y(d.rumbo.to_radians()))
}

impl PropKit {
    /// Where it is in the ship's frame, by a seat there.
    fn place(&self, seat: Xf) -> Xf {
        seat.then(Xf::new(self.at, self.turn))
    }

    /// Piece `k` in the prop's frame (where its pivot is and how it is turned) with what is
    /// flown at `v`.
    fn piece(&self, k: usize, v: &[f32; CHANNELS]) -> Xf {
        let own = |p: &Piece| {
            let (mut rot, mut slide) = (Quat::IDENTITY, Vec3::ZERO);
            for &(ch, axis, rad, m, from) in &p.moves {
                rot *= Quat::from_axis_angle(axis, from + rad * v[ch]);
                slide += axis * (m * v[ch]);
            }
            (rot, slide)
        };
        let p = &self.pieces[k];
        let (rot, slide) = own(p);
        match p.parent {
            Some(over) => {
                let base = &self.pieces[over];
                let (r, s) = own(base);
                Xf::new(base.pivot + s + r * (p.pivot - base.pivot + slide), (r * rot).normalize())
            }
            None => Xf::new(p.pivot + slide, rot),
        }
    }

    /// Where a hand holds it (the prop's frame: place, the way the palm faces, from index to
    /// little finger) and how its fingers are, with what is flown at `v`.
    fn hold(&self, v: &[f32; CHANNELS]) -> (Vec3, Vec3, Vec3, [f32; 5]) {
        let (at, normal, across) = self.grip;
        let fingers = match self.pull {
            Some((ch, pulling)) => std::array::from_fn(|i| self.pose[i] + (pulling[i] - self.pose[i]) * v[ch].clamp(0.0, 1.0)),
            None => self.pose,
        };
        match self.grip_piece {
            Some(k) => {
                let x = self.piece(k, v);
                let turn = if self.grip_free { Quat::IDENTITY } else { x.rot };
                (x.pos + x.rot * (at - self.pieces[k].pivot), turn * normal, turn * across, fingers)
            }
            None => (at, normal, across, fingers),
        }
    }
}

/// A seat being flown: its ship, what is read of it, what is flown as the props have it.
#[derive(Clone, Copy, Debug)]
pub struct Seated {
    pub structure: u64,
    pub seat: usize,
    rig: SeatRig,
    v: [Spring; CHANNELS],
    /// Seen this frame.
    here: bool,
}

impl Seated {
    pub fn flies(&self, control: u16) -> bool {
        self.rig.flies(control)
    }

    /// What is flown, as the props have it (eased).
    pub fn flown(&self) -> [f32; CHANNELS] {
        self.v.map(|s| s.x)
    }
}

/// Where the hands of whoever sits in `seat` go (left, right): on the props, each its own, if
/// that hand is `free`. `ship_at`: the ship's place and turn in the world.
pub fn hands(kit: &CockpitKit, s: &Seated, ship: &Ship, ship_at: (DVec3, Quat), free: [bool; 2]) -> [Option<Target>; 2] {
    let v = s.flown();
    let seat = seat_frame(ship, s.seat);
    let mut out = [None, None];
    for p in &kit.props {
        if !free[p.side] || out[p.side].is_some() {
            continue;
        }
        let x = p.place(seat);
        let (at, normal, across, fingers) = p.hold(&v);
        let rot = ship_at.1 * x.rot;
        out[p.side] = Some(Target { at: ship_at.0 + (ship_at.1 * x.point(at)).as_dvec3(), rot: frame_turn(Vec3::Y, Vec3::X, rot * normal, rot * across), fingers, pole: None });
    }
    out
}

/// The first piece a prop names that its model has not, said (a prop without its model at all is
/// drawn as plain shapes: nothing to say).
pub fn missing(kit: &CockpitKit, models: &lunar_core::structure::models::Models) -> Result<(), String> {
    for (k, p) in kit.props.iter().enumerate() {
        let Some(name) = p.model.as_ref().filter(|name| models.get(&format!("{name}/{}", lunar_core::structure::catalog::BODY)).is_some()) else { continue };
        if let Some(piece) = kit.pieces(k).find(|piece| models.get(&format!("{name}/{piece}")).is_none()) {
            return Err(format!("cabina: {}: su modelo '{name}' no tiene la pieza '{piece}' (assets/models/{name}.glb)", p.id));
        }
    }
    Ok(())
}

/// How far from the eye a ship's seats show their props (m).
const SHOWN: f64 = 60.0;

/// The props of the ships about: what each follows, and their looks.
#[derive(Default)]
pub struct Cockpits {
    seats: Vec<Seated>,
    /// Each prop's meshes as the renderer knows them: its body and its pieces (none: plain
    /// shapes).
    meshes: Vec<Option<(u8, Vec<Option<u8>>)>>,
}

impl Cockpits {
    /// Gives the renderer the props' models (those of `models` they name; a prop whose model is
    /// not there is drawn as plain shapes). A model that is there has every piece its prop
    /// names: a piece by another name would not be drawn, and the hand would hold nothing.
    pub fn models(&mut self, kit: &CockpitKit, models: &lunar_core::structure::models::Models, r: &mut lunar_render::Renderer) -> Result<(), String> {
        missing(kit, models)?;
        let white = lunar_core::mesh::Material::new([255; 3], 128, 0);
        self.meshes.clear();
        for (k, p) in kit.props.iter().enumerate() {
            let found = p.model.as_ref().and_then(|name| {
                let body = models.get(&format!("{name}/{}", lunar_core::structure::catalog::BODY))?;
                let pieces = kit.pieces(k).map(|piece| models.get(&format!("{name}/{piece}")).and_then(|m| r.prop_mesh(&m.tinted(white)))).collect();
                r.prop_mesh(&body.tinted(white)).map(|b| (b, pieces))
            });
            self.meshes.push(found);
        }
        Ok(())
    }

    /// The seat `seat` of the ship on `structure`, if it is being followed.
    pub fn seat(&self, structure: u64, seat: usize) -> Option<&Seated> {
        self.seats.iter().find(|s| s.structure == structure && s.seat == seat)
    }

    /// A frame gone by: every seat a ship is flown from, in the ships near `eye` (and in
    /// `aboard`, wherever it is), follows what is flown.
    pub fn update(&mut self, dt: f32, kit: &CockpitKit, ships: &Ships, set: &Structures, eye: DVec3, aboard: Option<u64>) {
        for s in &mut self.seats {
            s.here = false;
        }
        if kit.props.is_empty() {
            return;
        }
        for sh in &ships.list {
            let Some(st) = set.get(sh.structure) else { continue };
            if aboard != Some(sh.structure) && st.to_world(st.center).distance(eye) - f64::from(st.radius) > SHOWN {
                continue;
            }
            for (k, seat) in sh.kind.seats.iter().enumerate() {
                if seat.def.mandos.is_empty() {
                    continue;
                }
                let known = self.seats.iter().position(|s| s.structure == sh.structure && s.seat == k);
                let n = match known {
                    Some(n) => n,
                    None => {
                        let Some(rig) = SeatRig::of(kit, sh, k) else { continue };
                        // (as it is now: nothing to catch up with)
                        self.seats.push(Seated { structure: sh.structure, seat: k, rig, v: rig.read(sh).map(Spring::at), here: true });
                        self.seats.len() - 1
                    }
                };
                let s = &mut self.seats[n];
                s.here = true;
                let now = s.rig.read(sh);
                for (v, goal) in s.v.iter_mut().zip(now) {
                    v.step(goal, kit.follow, dt);
                }
            }
        }
        self.seats.retain(|s| s.here);
    }

    /// The props into `out`, each in its ship's frame.
    pub fn show(&self, kit: &CockpitKit, ships: &Ships, set: &Structures, out: &mut PropScene) {
        let mut last: Option<(u64, u16)> = None;
        for s in &self.seats {
            let (Some(n), Some(st)) = (ships.by_structure(s.structure), set.get(s.structure)) else { continue };
            let sh = &ships.list[n];
            // one frame for all of a ship's (they come one ship after another)
            let frame = match last {
                Some((id, f)) if id == s.structure => f,
                _ => {
                    out.frames.push(PropFrame { pos: st.pos, rot: st.rot, inside: lunar_ship::atmos::room_of(&sh.kind, Vec3::from_array(sh.kind.seats[s.seat].def.ojos)).is_some() });
                    (out.frames.len() - 1) as u16
                }
            };
            last = Some((s.structure, frame));
            let v = s.flown();
            let seat = seat_frame(sh, s.seat);
            for (k, p) in kit.props.iter().enumerate() {
                let x = p.place(seat);
                let model = |mesh: u8, at: Xf| Prop { frame, mesh, pos: at.pos, rot: at.rot, size: Vec3::ONE, color: [255; 3], emissive: 0.0, rough: 128, metal: 0 };
                match self.meshes.get(k).and_then(Option::as_ref) {
                    Some((body, pieces)) => {
                        out.props.push(model(*body, x));
                        for (j, mesh) in pieces.iter().enumerate() {
                            if let Some(mesh) = mesh {
                                out.props.push(model(*mesh, x.then(p.piece(j, &v))));
                            }
                        }
                    }
                    // no model: a foot, and each piece a rod with a knob on it
                    None => {
                        let shape = |mesh: u8, at: Xf, size: Vec3, color: [u8; 3]| Prop { frame, mesh, pos: at.pos, rot: at.rot, size, color, emissive: 0.0, rough: 150, metal: 40 };
                        out.props.push(shape(BOX, x.then(Xf::new(Vec3::new(0.0, -0.02, 0.0), Quat::IDENTITY)), Vec3::new(0.09, 0.04, 0.12), [44, 46, 50]));
                        for j in 0..p.pieces.len() {
                            let at = x.then(p.piece(j, &v));
                            let tall = if p.pieces[j].parent.is_some() { 0.03 } else { 0.13 };
                            out.props.push(shape(CYLINDER, at.then(Xf::new(Vec3::Y * tall * 0.5, Quat::IDENTITY)), Vec3::new(0.034, tall, 0.034), [30, 30, 32]));
                            out.props.push(shape(SPHERE, at.then(Xf::new(Vec3::Y * tall, Quat::IDENTITY)), Vec3::splat(0.04), [150, 34, 28]));
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kit() -> CockpitKit {
        let def: CockpitDef = lunar_core::defs::parse(
            "cabina",
            r#"{ "sigue": 0.03,
                 "canales": { "alabeo": { "eje": 1 }, "cabeceo": { "eje": 0 }, "disparo": { "accion": "pulsar" }, "gases": { "accion": "subir" } },
                 "mandos": [
                    { "id": "palanca", "mano": "der", "en": [-0.3, -0.6, 0.2],
                      "piezas": { "mango": { "en": [0, 0.02, 0], "mueve": [ { "canal": "cabeceo", "eje": [1, 0, 0], "grados": 20 }, { "canal": "alabeo", "eje": [0, 0, 1], "grados": 20 } ] },
                                  "gatillo": { "en": [0, 0.1, 0.02], "padre": "mango", "mueve": [ { "canal": "disparo", "eje": [1, 0, 0], "grados": 15 } ] } },
                      "agarre": { "en": [0, 0.1, 0], "palma": [1, 0, 0], "traves": [0, -1, 0], "pieza": "mango", "pose": "agarre", "gatillo": { "canal": "disparo", "pose": "gatillo" } } },
                    { "id": "gases", "mano": "izq", "en": [0.3, -0.6, 0.2],
                      "piezas": { "maneta": { "en": [0, 0.05, 0], "mueve": [ { "canal": "gases", "eje": [0, 0, 1], "metros": 0.08 } ] } },
                      "agarre": { "en": [0, 0.1, 0], "palma": [-1, 0, 0], "traves": [0, -1, 0], "pieza": "maneta", "pose": "agarre" } }
                 ] }"#,
        )
        .unwrap();
        let poses: BTreeMap<String, [f32; 5]> = [("agarre", [0.5, 0.8, 0.9, 0.9, 0.8]), ("gatillo", [0.5, 0.2, 0.9, 0.9, 0.8])].into_iter().map(|(k, v)| (k.to_string(), v)).collect();
        CockpitKit::new(&def, &poses).unwrap()
    }

    #[test]
    fn pieces_turn_and_slide_with_what_is_flown_and_carry_what_rides_on_them() {
        let kit = kit();
        let stick = &kit.props[0];
        // (channels in the order of their names: alabeo, cabeceo, disparo, gases)
        let mut v = [0.0; CHANNELS];
        let grip = stick.hold(&v);
        assert!((grip.0 - Vec3::new(0.0, 0.1, 0.0)).length() < 1e-6 && grip.3 == [0.5, 0.8, 0.9, 0.9, 0.8]);
        // full pitch: the grip, 8 cm over the pivot, goes round it 20 degrees about x
        v[1] = 1.0;
        let grip = stick.hold(&v);
        let want = Vec3::new(0.0, 0.02, 0.0) + Quat::from_rotation_x(20f32.to_radians()) * Vec3::new(0.0, 0.08, 0.0);
        assert!((grip.0 - want).length() < 1e-5, "{:?} / {want:?}", grip.0);
        assert!((grip.1 - Vec3::X).length() < 1e-5 && grip.2.z < -0.3, "the hand turns with it: {:?}", grip.2);
        // the trigger rides on the stick, and turns on its own pin when pulled
        let trigger = stick.pieces.iter().position(|p| p.name == "gatillo").unwrap();
        let at = stick.piece(trigger, &v);
        let pin = Vec3::new(0.0, 0.02, 0.0) + Quat::from_rotation_x(20f32.to_radians()) * Vec3::new(0.0, 0.08, 0.02);
        assert!((at.pos - pin).length() < 1e-5);
        v[2] = 1.0;
        let pulled = stick.piece(trigger, &v);
        assert!((pulled.pos - pin).length() < 1e-5 && pulled.rot.angle_between(at.rot) > 0.25);
        // ... and the index goes to it
        assert!(stick.hold(&v).3.iter().zip([0.5, 0.2, 0.9, 0.9, 0.8]).all(|(a, b)| (a - b).abs() < 1e-5), "{:?}", stick.hold(&v).3);
        // the throttle's handle slides with its travel
        let gas = &kit.props[1];
        v[3] = 0.5;
        assert!((gas.hold(&v).0 - Vec3::new(0.0, 0.1, 0.04)).length() < 1e-6);
        assert_eq!(gas.side, 0);
    }

    #[test]
    fn the_games_stick_and_throttle_are_their_models() {
        // (the stick was not drawn and the left hand was on backwards: the data named pieces
        // and grips the models did not have)
        let root = crate::root();
        let (_, _, kit) = crate::handwork::test_kit();
        let models = lunar_core::structure::models::Models::load(&root.join("assets/models")).unwrap();
        missing(&kit.cockpit, &models).unwrap_or_else(|e| panic!("{e}"));
        for (k, p) in kit.cockpit.props.iter().enumerate() {
            let name = p.model.as_ref().unwrap_or_else(|| panic!("{}: sin modelo", p.id));
            // what its recipe wrote of it (tools/modelos/datos/<name>.json): every piece's pivot
            // and the grip are those
            let text = std::fs::read_to_string(root.join(format!("tools/modelos/datos/{name}.json"))).unwrap_or_else(|e| panic!("{name}: {e}"));
            let data: serde_json::Value = serde_json::from_str(&text).unwrap();
            let v3 = |v: &serde_json::Value| Vec3::new(v[0].as_f64().unwrap() as f32, v[1].as_f64().unwrap() as f32, v[2].as_f64().unwrap() as f32);
            for (j, piece) in kit.cockpit.pieces(k).enumerate() {
                let at = v3(&data["piezas"][piece]["en"]);
                assert!((p.pieces[j].pivot - at).length() < 1e-3, "{}: la pieza '{piece}' gira en {:?}, su modelo en {at:?}", p.id, p.pieces[j].pivot);
            }
            // (every piece of the model is drawn: none left out)
            let named: Vec<&str> = kit.cockpit.pieces(k).collect();
            for piece in data["piezas"].as_object().unwrap().keys().filter(|n| n.as_str() != "cuerpo") {
                assert!(named.contains(&piece.as_str()), "{}: la pieza '{piece}' de su modelo no está en manos.jsonc", p.id);
            }
            let grip = &data["puntos"]["agarre"];
            assert_eq!(grip["mano"].as_str(), Some(if p.side == 0 { "izq" } else { "der" }), "{}", p.id);
            let (at, palm, across) = (v3(&grip["en"]), v3(&grip["palma"]).normalize(), v3(&grip["traves"]).normalize());
            assert!((p.grip.0 - at).length() < 1e-3 && p.grip.2.dot(across) > 0.99, "{}: su agarre {:?} no es el de su modelo {:?}", p.id, p.grip, (at, palm, across));
            // (a hand that keeps its own way on what turns in it faces as the data says)
            assert!(p.grip_free || p.grip.1.dot(palm) > 0.999, "{}: la palma a {:?}, su modelo a {palm:?}", p.id, p.grip.1);
        }
        // the throttle at idle is back, at full ahead: as far each way as its model goes
        let gas = kit.cockpit.props.iter().find(|p| p.id == "gases").unwrap();
        let names: Vec<&str> = ["alabeo", "cabeceo", "disparo", "gases", "guinada"].to_vec();
        let ch = names.iter().position(|n| *n == "gases").unwrap();
        let mut v = [0.0; CHANNELS];
        let idle = gas.hold(&v).0;
        v[ch] = 1.0;
        let full = gas.hold(&v).0;
        assert!(idle.z < -0.05 && full.z > 0.05 && (idle.z + full.z).abs() < 1e-3 && (idle.y - full.y).abs() < 1e-3, "{idle:?} .. {full:?}");
    }

    #[test]
    fn a_piece_by_a_name_its_model_has_not_is_said() {
        let root = crate::root();
        let (data, rig, _) = crate::handwork::test_kit();
        let models = lunar_core::structure::models::Models::load(&root.join("assets/models")).unwrap();
        let mut def = data.cockpit.clone();
        let stick = def.mandos.iter_mut().find(|p| p.id == "palanca").unwrap();
        let piece = stick.piezas.remove("seta").unwrap();
        stick.piezas.insert("sombrerete".into(), piece);
        let kit = CockpitKit::new(&def, &rig.manos).unwrap();
        let e = missing(&kit, &models).unwrap_err();
        assert!(e.contains("sombrerete") && e.contains("palanca_vuelo"), "{e}");
    }

    #[test]
    fn what_is_written_wrong_is_said() {
        let poses: BTreeMap<String, [f32; 5]> = [("agarre".to_string(), [0.5; 5])].into();
        let parse = |text: &str| CockpitKit::new(&lunar_core::defs::parse::<CockpitDef>("cabina", text).unwrap(), &poses);
        let prop = |piece: &str, hold: &str| format!(r#"{{ "sigue": 0.03, "canales": {{ "gases": {{ "accion": "subir" }} }}, "mandos": [ {{ "id": "g", "mano": "izq", "en": [0, 0, 0], "piezas": {{ {piece} }}, "agarre": {{ "en": [0, 0, 0], "palma": [1, 0, 0], "traves": [0, -1, 0], {hold} }} }} ] }}"#);
        assert!(parse(&prop(r#""a": { "en": [0, 0, 0] }"#, r#""pose": "agarre""#)).is_ok());
        assert!(parse(&prop(r#""a": { "en": [0, 0, 0], "mueve": [ { "canal": "nada", "eje": [1, 0, 0] } ] }"#, r#""pose": "agarre""#)).unwrap_err().contains("nada"));
        assert!(parse(&prop(r#""a": { "en": [0, 0, 0] }"#, r#""pose": "agarre", "pieza": "b""#)).unwrap_err().contains("'b'"));
        assert!(parse(&prop(r#""a": { "en": [0, 0, 0], "padre": "c" }"#, r#""pose": "agarre""#)).unwrap_err().contains("'c'"));
        assert!(parse(&prop(r#""a": { "en": [0, 0, 0] }"#, r#""pose": "puno""#)).unwrap_err().contains("puno"));
    }
}
