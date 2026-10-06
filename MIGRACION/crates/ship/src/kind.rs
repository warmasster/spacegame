//! A kind of ship, assembled from its data once at load: every part generated (hull, decks,
//! bulkheads, components, the trunks and outlets of its conduits), registered in the
//! structure catalog with its look, joined where parts touch; networks laid out (junctions,
//! conduit edges, switches, ports); machines, actuators, joints, panels, compartments and seats
//! resolved to part and node indices. Instances (`Ship`) are built from this.
use crate::{
    components::{self, Components, euler, mirror, mirror_name},
    def::*,
    geom::{self, GenPart, Role},
};
use glam::{Affine3A, Vec2, Vec3};
use lunar_controls::layout::{self, PanelDef, PanelLayout};
use lunar_core::structure::{
    blueprint::{Blueprint, Joined, Placed},
    catalog::{Catalog, PartKindDef, ShapeDef},
};
use lunar_machines::{
    actuator::ActuatorDef,
    net::{EdgeKind, Medium},
};
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub struct EdgePlan {
    pub a: u32,
    pub b: u32,
    pub kind: EdgeKind,
    /// The conduit segment (or the switch's body) it lives in.
    pub part: Option<u32>,
    /// Switches: closed while this signal is on.
    pub signal: Option<String>,
    pub measure: bool,
    pub id: String,
}

#[derive(Clone, Debug)]
pub struct NetPlan {
    pub name: String,
    pub medium: Medium,
    pub nominal: f64,
    pub leak: f64,
    pub nodes: Vec<Vec3>,
    pub names: BTreeMap<String, u32>,
    pub edges: Vec<EdgePlan>,
    pub color: [u8; 3],
    /// Kind of conduit for its runs and stubs.
    pub conducto: String,
}

#[derive(Clone, Debug)]
pub struct PortPlan {
    pub role: String,
    pub net: u16,
    pub node: u32,
}

#[derive(Clone, Debug)]
pub struct MachinePlan {
    pub id: String,
    pub def: MachineDef,
    pub part: Option<u32>,
    pub ports: Vec<PortPlan>,
    /// Thrust axis (part frame): the push of an engine or thruster.
    pub thrust: Vec3,
    /// Its nozzle, if it is a thruster (`exhaust`).
    pub jet: Option<crate::exhaust::Jet>,
    pub light: Option<LightDef>,
    /// Its lamp is inside the hull (at rest): in a compartment or, where it is and a little way
    /// along where it shines, within the hull's outline — what `far::interior` says of the parts
    /// it lights. A ship with no air of its own has an inside too.
    pub lit_inside: bool,
}

#[derive(Clone, Debug)]
pub struct ActuatorPlan {
    pub id: String,
    pub def: ActuatorDef,
    pub joint: usize,
    pub part: Option<u32>,
    pub ports: Vec<PortPlan>,
}

#[derive(Clone, Debug)]
pub struct JointPlan {
    pub id: String,
    pub name: String,
    pub hinge: bool,
    pub axis: Vec3,
    pub pivot: Vec3,
    pub lo: f32,
    pub hi: f32,
    pub init: f32,
    pub parent: Option<usize>,
    pub parts: Vec<u32>,
    pub damping: f32,
    pub opening: Option<JointOpening>,
    /// A sprung leg: its foot's sole with the leg full out (ship frame, the joints at rest) and
    /// what it is built to (`def::SpringDef`).
    pub spring: Option<(Vec3, crate::def::SpringDef)>,
    /// It gives way to what pushes it (`def::JointDef::cede`).
    pub gives: bool,
    /// It goes where another joint is, times this (`def::JointDef::sigue`).
    pub follows: Option<(usize, f32)>,
}

/// What a joint opens as it moves: between compartments `a` and `b` (or `SPACE`), `area` m² at
/// full open, at a place (ship frame) with its normal from `a` toward `b`.
#[derive(Clone, Copy, Debug)]
pub struct JointOpening {
    pub a: usize,
    pub b: usize,
    pub area: f32,
    pub at: Vec3,
    pub n: Vec3,
}

#[derive(Clone, Debug)]
pub struct PanelPlan {
    pub id: String,
    pub part: u32,
    /// Panel plane → ship frame (at rest): origin at the panel's bottom-left corner, x right,
    /// y up, z out of the face; metres.
    pub frame: Affine3A,
    pub def: PanelDef,
    pub layout: PanelLayout,
    pub power: Option<PortPlan>,
    pub data: Option<PortPlan>,
}

/// A decal laid: its atlas rectangle, centre and half extents (ship frame, at rest), the part
/// it is on.
#[derive(Clone, Debug)]
pub struct DecalPlan {
    pub uv: [f32; 4],
    pub center: Vec3,
    pub u: Vec3,
    pub v: Vec3,
    pub part: Option<u32>,
    pub tint: [u8; 3],
    pub emissive: f32,
}

/// A closure (`closures`): its leaves' joints and parts, what orders it, its latches.
#[derive(Clone, Debug)]
pub struct ClosurePlan {
    pub id: String,
    pub name: String,
    pub joints: Vec<usize>,
    pub parts: Vec<u32>,
    /// Signal ordering it open; none: worked by hand (`<id>.mano`).
    pub order: Option<String>,
    pub latch: bool,
    /// From shut (rad or m) where the latches catch it.
    pub capture: f64,
    /// No actuator moves it: a hand does.
    pub hand: bool,
}

#[derive(Clone, Debug)]
pub struct CompartmentPlan {
    pub id: String,
    pub name: String,
    pub boxes: Vec<[Vec3; 2]>,
    pub volume: f32,
    pub pressure: f64,
    /// Parts between it and space.
    pub hull: Vec<u32>,
    /// Parts between it and another compartment.
    pub walls: Vec<(u32, usize)>,
    /// Openings a signal works (`VentPlan`).
    pub vents: Vec<VentPlan>,
}

/// An opening of a compartment worked by a signal: `area` m² toward `to` (another compartment or
/// `SPACE`) while the signal is on, or the share of it the signal says (0..1: a valve worked by
/// a wheel). `at`: where it lets the gas out and which way (ship frame), if it has a place.
#[derive(Clone, Debug)]
pub struct VentPlan {
    pub signal: String,
    pub area: f32,
    pub to: usize,
    pub at: Option<(Vec3, Vec3)>,
}

/// Index of the outside in compartment pairs.
pub const SPACE: usize = usize::MAX;

#[derive(Clone, Debug)]
pub struct SeatPlan {
    pub def: SeatDef,
    pub part: u32,
}

pub struct ShipKind {
    pub id: String,
    pub def: ShipDef,
    /// Part ids: the structure's part index is the position here.
    pub parts: Vec<String>,
    /// Centre of each part at rest (ship frame).
    pub centers: Vec<Vec3>,
    pub roles: Vec<Role>,
    pub nets: Vec<NetPlan>,
    pub machines: Vec<MachinePlan>,
    pub actuators: Vec<ActuatorPlan>,
    pub joints: Vec<JointPlan>,
    pub panels: Vec<PanelPlan>,
    pub compartments: Vec<CompartmentPlan>,
    /// The boxes of all its compartments: its inside, as its structure says it
    /// (`lunar_core::structure::state::Structure::rooms`).
    pub rooms: std::sync::Arc<[[Vec3; 2]]>,
    pub seats: Vec<SeatPlan>,
    pub closures: Vec<ClosurePlan>,
    pub decals: Vec<DecalPlan>,
    /// The blueprint's name in the structure library.
    pub blueprint: String,
    /// Lift over the ground of its lowest point when placed.
    pub lift: f32,
    /// Parts that spit sparks when badly hurt and fed (outlets, junction boxes).
    pub sparks: Vec<u32>,
    /// What each component placed is called (its kind's name), by the id it was placed as.
    pub names: BTreeMap<String, String>,
    /// What each piece of cargo placed holds, by the id it was placed as.
    pub cargo: BTreeMap<String, String>,
    /// The ways of what is only seen (the drops of the outlets, the pipes), ship frame.
    pub seen: Vec<Vec<Vec3>>,
    /// The clamps that hold its cargo (`cargo`).
    pub clamps: Vec<crate::cargo::ClampPlan>,
    /// Its components that hold something (`contents`: drums, pallets, tanks).
    pub holders: Vec<Holder>,
}

/// A component that holds something (`contents`): the parts of it that do, and all its parts.
#[derive(Clone, Debug)]
pub struct Holder {
    /// The id it was placed as.
    pub id: String,
    pub holds: Vec<u32>,
    pub parts: Vec<u32>,
    /// The signal that says how many kg it holds (a tank's machine: what it weighs follows it),
    /// and the share of that in each of the parts that hold.
    pub level: Option<String>,
    pub shares: Vec<f32>,
    /// The substance (its id in the registry), kg of it at most and as built, and kg that fit
    /// to the brim in all the parts that hold.
    pub substance: String,
    pub capacity: f32,
    pub built: f32,
    pub brim: f32,
    /// Its machine takes how much there is room for from here (`contents::Fitted::machine`):
    /// the two cannot disagree.
    pub tied: bool,
}

impl ShipKind {
    /// What part `i` is, for whoever looks at it with a scanner: its name ("Unidad hidráulica",
    /// "Panel de casco", "Tronco de cables") and where or which ("hpu a · cuerpo", "bodega,
    /// babor").
    pub fn describe(&self, i: usize) -> (String, String) {
        let id = self.parts.get(i).map_or("", String::as_str);
        let nice = |s: &str| s.replace(['_', '.'], " ");
        // a machine's part: its component's name
        if let Some(m) = self.machines.iter().find(|m| m.part == Some(i as u32)) {
            let name = self.component_name(&m.id).unwrap_or_else(|| nice(&m.id));
            return (name, nice(id));
        }
        if let Some(p) = self.panels.iter().find(|p| p.part == i as u32) {
            return (format!("Panel: {}", p.def.name), nice(id));
        }
        let head = id.split('.').next().unwrap_or(id);
        if let Some(name) = self.component_name(head) {
            // cargo says what it holds
            return (name, self.cargo.get(head).cloned().unwrap_or_else(|| nice(id)));
        }
        let role = match self.roles.get(i) {
            Some(Role::Hull) => "Panel de casco",
            Some(Role::Glass) => "Ventana",
            Some(Role::Floor) => "Placa de suelo",
            Some(Role::Bulkhead) => "Mamparo",
            Some(Role::Conduit) if id.starts_with("toma.") => "Toma de corriente",
            Some(Role::Conduit) if id.starts_with("canal.paso") => "Paso de cables por mamparo",
            Some(Role::Conduit) if id.starts_with("canal.") => "Tronco de cables",
            Some(Role::Conduit) if id.starts_with("tuberia.") => "Tubería",
            Some(Role::Conduit) => "Conducto de aire",
            _ => "Pieza",
        };
        // a closure's leaf, a joint's part
        if let Some(c) = self.closures.iter().find(|c| c.parts.contains(&(i as u32))) {
            return (c.name.clone(), nice(id));
        }
        (role.to_string(), nice(id.strip_prefix("canal.").unwrap_or(id)))
    }

    /// The name of the component placed as `id` ("hpu_a" → "Unidad hidráulica"), if it is one.
    fn component_name(&self, id: &str) -> Option<String> {
        self.names.get(id).cloned()
    }
}

/// The colour of a run: the network's own when the data gives one; else cables of the circuit's
/// colour (a palette, so a bundle reads), pipes and ducts their kind's.
fn run_color(n: &NetPlan, kind: &str, k: usize) -> [u8; 3] {
    const POWER: [[u8; 3]; 8] = [[28, 28, 30], [168, 32, 28], [222, 220, 212], [214, 170, 30], [40, 118, 62], [52, 92, 176], [214, 112, 32], [110, 64, 140]];
    const DATA: [[u8; 3]; 4] = [[40, 90, 180], [40, 150, 170], [220, 120, 160], [230, 230, 230]];
    if n.color != medium_color(n.medium) {
        return n.color;
    }
    if kind.starts_with("tubo") || kind.starts_with("conducto") {
        return geom::conduit_look(kind, None).color;
    }
    match n.medium {
        Medium::Electrico => POWER[k % POWER.len()],
        Medium::Datos => DATA[k % DATA.len()],
        _ => n.color,
    }
}

/// Border of a panel's own box round its plate (m).
const PANEL_BEZEL: f32 = 0.012;
/// How far along where a lamp shines it must still be within the hull's outline to be a lamp of
/// its inside (m): a light on the skin shining out is not.
const LAMP_AHEAD: f32 = 0.3;

/// A panel's plane: across, up, out of its face (ship frame).
fn panel_axes(m: &crate::def::PanelMount) -> (Vec3, Vec3, Vec3) {
    let n = Vec3::from_array(m.normal).normalize_or(Vec3::Z);
    let up0 = Vec3::from_array(m.arriba.unwrap_or([0.0, 1.0, 0.0]));
    let mut v = (up0 - n * up0.dot(n)).normalize_or(n.any_orthonormal_vector());
    if v.length_squared() < 0.5 {
        v = n.any_orthonormal_vector();
    }
    (v.cross(n), v, n)
}

/// A wall panel seated on its wall: how far out of where it was asked its face must come for the
/// wall not to cut it anywhere (0 where it already stands clear), and how deep its box must be to
/// reach the wall behind all of it (`fondo` at least). The wall is what is built so far of hull,
/// bulkheads and decks, measured over the box's face (`w` by `h` round `c`).
#[allow(clippy::too_many_arguments)]
fn seat_on_wall(parts: &[GenPart], c: Vec3, u: Vec3, v: Vec3, n: Vec3, w: f32, h: f32, fondo: f32) -> (f32, f32) {
    /// Rays start this far in front of the face (m) and look this far behind it; the face clears
    /// the wall by this much; no box is deeper than this.
    const OUT: f32 = 0.3;
    const BEHIND: f32 = 0.4;
    const CLEAR: f32 = 0.004;
    const DEEPEST: f32 = 0.35;
    const N: usize = 8;
    let walls: Vec<(&GenPart, Affine3A)> = parts.iter().filter(|p| matches!(p.role, Role::Hull | Role::Bulkhead | Role::Floor)).map(|p| (p, p.at.inverse())).collect();
    // where the wall's surface is along the normal, from the face: over it (> 0) or behind it
    let (mut front, mut back) = (f32::MIN, f32::MAX);
    for i in 0..=N {
        for j in 0..=N {
            let o = c + u * (w * (i as f32 / N as f32 - 0.5)) + v * (h * (j as f32 / N as f32 - 0.5)) + n * OUT;
            let hit = walls.iter().filter_map(|(p, inv)| p.shape.raycast(inv.transform_point3(o), inv.transform_vector3(-n), OUT + BEHIND).map(|x| x.0)).fold(f32::MAX, f32::min);
            if hit < f32::MAX {
                front = front.max(OUT - hit);
                back = back.min(OUT - hit);
            }
        }
    }
    if front == f32::MIN {
        return (0.0, fondo);
    }
    let out = (front + CLEAR).max(0.0);
    (out, fondo.max(out - back + 0.01).min(DEEPEST))
}

/// Every part reference of the data resolved: an id, or a prefix ending in `*`.
pub fn resolve(ids: &[String], pat: &str) -> Vec<u32> {
    if let Some(prefix) = pat.strip_suffix('*') { ids.iter().enumerate().filter(|(_, i)| i.starts_with(prefix)).map(|(k, _)| k as u32).collect() } else { ids.iter().position(|i| i == pat).map(|k| vec![k as u32]).unwrap_or_default() }
}

/// What the builder needs from outside: the structure catalog (to register parts and read
/// materials), the component kinds and the panel definitions (raw text, for templates).
pub struct Inputs<'a> {
    pub catalog: &'a mut Catalog,
    pub components: &'a Components,
    pub panels: &'a BTreeMap<String, String>,
    pub decals: &'a lunar_core::props::DecalAtlas,
}

struct Builder<'a> {
    id: String,
    parts: Vec<GenPart>,
    /// Runs waiting to be laid all at once (`conduits`).
    runs: Vec<crate::conduits::Run>,
    nets: Vec<NetPlan>,
    /// Bones by part id, from the joints.
    anchors: BTreeMap<String, BTreeMap<String, Vec3>>,
    inputs: Inputs<'a>,
}

impl Builder<'_> {
    fn part_index(&self, id: &str) -> Result<u32, String> {
        self.parts.iter().position(|p| p.id == id).map(|k| k as u32).ok_or_else(|| format!("nave {}: no hay pieza '{id}'", self.id))
    }

    fn net(&self, name: &str) -> Result<u16, String> {
        self.nets.iter().position(|n| n.name == name).map(|k| k as u16).ok_or_else(|| format!("nave {}: red desconocida '{name}'", self.id))
    }

    /// A run of conduit `kind` on network `net` from node `from` (at `a`) to node `to` (at `b`): a
    /// device's own connection (`stub`) or a run between junctions. Laid later with all the
    /// others (`lay_runs`).
    #[allow(clippy::too_many_arguments)]
    fn plan(&mut self, net: u16, from: u32, to: u32, a: Vec3, b: Vec3, kind: &str, prefix: String, stub: bool) {
        let n = &self.nets[usize::from(net)];
        let k = self.runs.iter().filter(|r| r.net == net).count();
        let color = run_color(n, kind, k);
        self.runs.push(crate::conduits::Run { net, from, to, a, b, kind: kind.to_string(), color, stub, prefix, via: Vec::new() });
    }

    /// Every planned run laid at once (`conduits`): their parts, their edges and junctions.
    fn lay_runs(&mut self, def: &ShipDef, keep: &[crate::conduits::Keep]) -> Vec<Vec<Vec3>> {
        let runs = std::mem::take(&mut self.runs);
        let rooms = def
            .compartimentos
            .iter()
            .flat_map(|c| c.cajas.iter().enumerate().map(|(k, [a, z])| (if k == 0 { c.id.clone() } else { format!("{}{k}", c.id) }, [Vec3::from_array(*a).min(Vec3::from_array(*z)), Vec3::from_array(*a).max(Vec3::from_array(*z))])))
            .collect();
        let site = crate::conduits::Site { hull: def.casco.as_ref(), bulkheads: &def.mamparos, rooms, parts: &self.parts, cfg: &def.canalizaciones, keep };
        let laid = crate::conduits::lay(&site, &runs);
        let seen = laid.seen;
        let base = self.parts.len();
        self.parts.extend(laid.parts);
        for crate::conduits::Chain { run, pts, owners } in laid.chains {
            let r = &runs[run];
            let net = &mut self.nets[usize::from(r.net)];
            let mut last = r.from;
            let n = pts.len().saturating_sub(1);
            for k in 0..n {
                let next = if k + 1 == n {
                    r.to
                } else {
                    net.nodes.push(pts[k + 1]);
                    (net.nodes.len() - 1) as u32
                };
                let (kind, part) = match owners[k] {
                    Some(p) => (EdgeKind::Conduit, Some((base + p) as u32)),
                    None => (EdgeKind::Fixed, None),
                };
                net.edges.push(EdgePlan { a: last, b: next, kind, part, signal: None, measure: false, id: String::new() });
                last = next;
            }
            if n == 0 {
                net.edges.push(EdgePlan { a: r.from, b: r.to, kind: EdgeKind::Fixed, part: None, signal: None, measure: false, id: String::new() });
            }
        }
        seen
    }

    /// A device's port on "net:node": its own connection from `at` to the junction (an outlet on
    /// the nearest trunk and a drop to it), unless it is right there. Returns the port.
    ///
    /// Not `wired` (placed so): connected right to the junction, no conduit.
    fn port(&mut self, spec: &str, at: Vec3, owner: &str, role: &str, wired: bool) -> Result<PortPlan, String> {
        let (net_name, node_name) = spec.split_once(':').ok_or_else(|| format!("{owner}: puerto '{spec}' (red:nodo)"))?;
        let net = self.net(net_name)?;
        let n = &self.nets[usize::from(net)];
        let junction = *n.names.get(node_name).ok_or_else(|| format!("{owner}: la red {net_name} no tiene el nodo '{node_name}'"))?;
        let target = n.nodes[junction as usize];
        let kind = n.conducto.clone();
        if !wired || at.distance(target) < 0.12 {
            return Ok(PortPlan { role: role.to_string(), net, node: junction });
        }
        // the device's own end; its stub laid with every other run
        let dev = {
            let n = &mut self.nets[usize::from(net)];
            n.nodes.push(at);
            (n.nodes.len() - 1) as u32
        };
        self.plan(net, dev, junction, at, target, &kind, format!("{owner}.acometida.{role}"), true);
        Ok(PortPlan { role: role.to_string(), net, node: dev })
    }
}

impl NetPlan {
    fn kind_default(medium: Medium) -> &'static str {
        match medium {
            Medium::Electrico => "cable",
            Medium::Datos => "datos",
            Medium::Hidraulico => "tubo_alta",
            Medium::Aire => "conducto",
            Medium::Termico | Medium::Refrigerante => "tubo",
            _ => "tubo",
        }
    }
}

/// Conventional colours of conduits by medium (sRGB).
fn medium_color(m: Medium) -> [u8; 3] {
    match m {
        Medium::Electrico => [38, 38, 42],
        Medium::Datos => [40, 80, 160],
        Medium::Hidraulico => [180, 120, 40],
        Medium::Neumatico => [60, 140, 200],
        Medium::Propelente => [190, 60, 40],
        Medium::Gas => [70, 170, 90],
        Medium::Aire => [180, 182, 184],
        Medium::Refrigerante | Medium::Termico => [60, 110, 200],
    }
}

/// Index of compartment `name` of `def` (`SPACE` for "vacio").
fn comp_index(def: &ShipDef, name: &str) -> Result<usize, String> {
    if name == "vacio" || name == "vacío" {
        return Ok(SPACE);
    }
    def.compartimentos.iter().position(|c| c.id == name).ok_or_else(|| format!("compartimento desconocido '{name}'"))
}

/// Assemble kind `id` from its definition. Registers its parts in the catalog and returns the
/// kind and its blueprint.
pub fn build(id: &str, def: ShipDef, inputs: Inputs) -> Result<(ShipKind, Blueprint), String> {
    let err = |e: String| format!("nave {id}: {e}");
    let mut b = Builder { id: id.to_string(), parts: Vec::new(), runs: Vec::new(), nets: Vec::new(), anchors: BTreeMap::new(), inputs };
    // ---- panels laid out first: one with no part of its own brings its box, its size ----
    let mut laid: Vec<(PanelDef, PanelLayout)> = Vec::new();
    for m in &def.paneles {
        let raw = b.inputs.panels.get(&m.panel).ok_or_else(|| err(format!("panel {}: definición desconocida '{}'", m.id, m.panel)))?;
        let vars: Vec<(String, String)> = m.prefijo.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        let text = lunar_signals::expr::substitute(raw, &vars);
        let mut pdef: PanelDef = lunar_core::defs::parse(&m.panel, &text).map_err(|e| err(e.to_string()))?;
        if !m.sin.is_empty() {
            for s in &m.sin {
                if !pdef.mandos.iter().any(|c| &c.id == s) {
                    return Err(err(format!("panel {}: 'sin' cita '{s}', que no está en {}", m.id, m.panel)));
                }
            }
            pdef.mandos.retain(|c| !m.sin.contains(&c.id));
            let ids: Vec<String> = pdef.mandos.iter().map(|c| c.id.clone()).collect();
            pdef.mandos.retain(|c| c.kind != "tapa" || c.protege.iter().any(|p| ids.contains(p)));
        }
        // multi-function displays: the pages the ship makes, and their bezels
        crate::mfd_auto::fill(&mut pdef, &def, b.inputs.components).map_err(|e| err(format!("panel {}: {e}", m.id)))?;
        lunar_controls::mfd::expand(&mut pdef);
        let lay = layout::layout(&pdef).map_err(|e| err(format!("panel {}: {}", m.id, e.0)))?;
        laid.push((pdef, lay));
    }
    // ---- structure: hull, decks, bulkheads ----
    if let Some(h) = &def.casco {
        if h.estaciones.windows(2).any(|w| w[1].z <= w[0].z) {
            return Err(err("las estaciones del casco van de popa a proa (z creciente)".into()));
        }
        b.parts.extend(geom::hull(h, "casco").map_err(err)?);
        for m in &def.mamparos {
            b.parts.extend(geom::bulkhead(h, m).map_err(err)?);
        }
    }
    for f in &def.suelos {
        b.parts.extend(geom::floor(f, def.casco.as_ref()));
    }
    // ---- a panel with no part of its own brings its box, seated on the wall it is on: its face
    // comes out of where it was asked as far as the wall needs (a hull wall leans and turns, and
    // would cut a corner off it), and its box goes back as deep as the wall is behind it ----
    let mut panel_at: Vec<Vec3> = Vec::new();
    let mut keep_depth: Vec<f32> = Vec::new();
    for (m, (_, lay)) in def.paneles.iter().zip(&laid) {
        let mut c = Vec3::from_array(m.en);
        let mut depth = m.fondo.unwrap_or(0.06);
        if m.pieza.is_none() {
            let (u, v, n) = panel_axes(m);
            let (w, h) = (lay.size[0] / 1000.0 + 2.0 * PANEL_BEZEL, lay.size[1] / 1000.0 + 2.0 * PANEL_BEZEL);
            let (out, deep) = seat_on_wall(&b.parts, c, u, v, n, w, h, depth);
            c += n * out;
            depth = deep;
            let mut pts = Vec::new();
            for a in [-0.5, 0.5] {
                for e in [-0.5, 0.5] {
                    for z in [0.0, -depth] {
                        pts.push(c + u * (a * w) + v * (e * h) + n * z);
                    }
                }
            }
            let mut part = GenPart::from_points(format!("{}.caja", m.id), "electronica", Role::Component, &pts).ok_or_else(|| err(format!("panel {}: caja sin forma", m.id)))?;
            part.color = Some([46, 48, 52]);
            b.parts.push(part);
        }
        panel_at.push(c);
        keep_depth.push(depth);
    }
    // ---- closures: leaves shaped to their openings, seals, fixed rests ----
    let mut leaves: Vec<(usize, crate::closures::Leaf)> = Vec::new();
    for (ci, c) in def.cierres.iter().enumerate() {
        let built = crate::closures::build(c, def.casco.as_ref(), &def.mamparos).map_err(err)?;
        b.parts.extend(built.parts);
        leaves.extend(built.leaves.into_iter().map(|l| (ci, l)));
    }
    // ---- components (and their inline machines and lights) ----
    let mut inline: Vec<(String, MachineDef)> = Vec::new();
    // lights in the frame of their component as placed (ship frame), turned into the frame of
    // their machine's part once that is known
    let mut lights: BTreeMap<String, (LightDef, Affine3A)> = BTreeMap::new();
    let materials = |m: &str| -> Option<([u8; 3], u8, u8, u8)> {
        let i = b.inputs.catalog.materials.iter().position(|(n, _)| n == m)?;
        let d = &b.inputs.catalog.materials[i].1;
        Some((d.color, d.rough, d.metal, d.finish))
    };
    let mut placed: Vec<(GenPart, String)> = Vec::new();
    let mut comp_names: BTreeMap<String, String> = BTreeMap::new();
    let mut cargo: BTreeMap<String, String> = BTreeMap::new();
    // what holds something (`contents`): by part, what it holds and what that lets go of if the
    // part is destroyed; by component, the parts that hold, all its parts, its signal and shares
    let mut containers: BTreeMap<String, (lunar_core::structure::contents::ContainerDef, Option<lunar_core::structure::catalog::BurstDef>)> = BTreeMap::new();
    let mut holding: Vec<(String, Vec<String>, Vec<String>, Option<String>, Vec<f32>, (String, f32, f32, f32, bool))> = Vec::new();
    // the component each part is of, and the clamp that holds each component that is cargo
    let mut part_comp: BTreeMap<String, String> = BTreeMap::new();
    let mut held: BTreeMap<String, String> = BTreeMap::new();
    // the clamps placed (components of a kind that is one), each with its zone
    let mut clamp_zones: BTreeMap<String, crate::cargo::Zone> = BTreeMap::new();
    // decals of the components, in the ship frame, with the component's first part
    let mut fitted: Vec<(DecalDef, Affine3A, String)> = Vec::new();
    // and their labels: with the component's id and name too (what their text may say)
    let mut lettered: Vec<(crate::def::LabelDef, Affine3A, String, String, String, Option<String>)> = Vec::new();
    for c in &def.componentes {
        let base = Affine3A::from_rotation_translation(euler(c.rot), Vec3::from_array(c.en));
        let (count, step) = c.repetir.unwrap_or((1, [0.0; 3]));
        for k in 0..count.max(1) {
            let at = Affine3A::from_translation(Vec3::from_array(step) * k as f32) * base;
            let cid = if count > 1 { format!("{}#{k}", c.id) } else { c.id.clone() };
            for (side, at, cid) in [(false, at, cid.clone()), (true, mirror(at), mirror_name(&cid))] {
                if side && !c.espejo {
                    continue;
                }
                let cid = if side && cid == c.id { format!("{cid}.e") } else { cid };
                let (parts, anchors) = components::place(c, &cid, at, side, b.inputs.components, &materials).map_err(err)?;
                let first = parts[0].id.clone();
                let pieces: Vec<String> = parts.iter().map(|p| p.id.clone()).collect();
                part_comp.extend(pieces.iter().map(|p| (p.clone(), cid.clone())));
                if let Some(clamp) = &c.anclaje {
                    held.insert(cid.clone(), clamp.clone());
                }
                if let Some(z) = c.tipo.as_ref().and_then(|t| b.inputs.components.get(t)).and_then(|k| k.anclaje.as_ref()) {
                    clamp_zones.insert(cid.clone(), crate::cargo::Zone::new(z, at).map_err(|e| err(format!("componente {cid}: {e}")))?);
                }
                for p in parts {
                    placed.push((p, c.articulacion.as_ref().map_or(String::new(), |j| if side { mirror_name(j) } else { j.clone() })));
                }
                b.anchors.insert(cid.clone(), anchors);
                let kind = c.tipo.as_ref().and_then(|t| b.inputs.components.get(t));
                comp_names.insert(cid.clone(), kind.map_or_else(|| cid.replace('_', " "), |k| k.nombre.clone()));
                // what it holds (`contents`): the pieces of it that are containers, what a scanner
                // reads of it as built, what its labels write of it
                let within = crate::contents::fit(&cid, c, kind, b.inputs.catalog).map_err(err)?;
                match &within {
                    Some(w) => {
                        for (i, holds, burst) in &w.pieces {
                            containers.insert(pieces[*i].clone(), (holds.clone(), burst.clone()));
                        }
                        // (what a signal counts is told by whatever counts it: a tank's machine)
                        if w.level.is_none() {
                            cargo.insert(cid.clone(), w.text.clone());
                        }
                        let shares = w.pieces.iter().map(|p| p.1.capacity.unwrap_or(0.0) / w.capacity.max(1e-9)).collect();
                        holding.push((cid.clone(), w.pieces.iter().map(|p| pieces[p.0].clone()).collect(), pieces.clone(), w.level.clone(), shares, (w.substance.clone(), w.capacity, w.mass, w.brim, !w.machine.is_empty())));
                    }
                    None => {
                        if let Some(r) = kind.and_then(|k| k.recurso.clone()) {
                            cargo.insert(cid.clone(), r);
                        }
                    }
                }
                if let Some(k) = kind {
                    fitted.extend(k.calcas.iter().map(|d| (d.clone(), at, first.clone())));
                    // (a label that says what is held is left out of what holds nothing)
                    lettered.extend(k.rotulos.iter().filter_map(|l| {
                        let texto = crate::contents::lettered(&l.texto, within.as_ref())?;
                        Some((crate::def::LabelDef { texto, ..l.clone() }, at, first.clone(), cid.clone(), k.nombre.clone(), c.etiqueta.clone()))
                    }));
                }
                // the machine: the component's own, the kind's default under it
                let mdef = match (&c.maquina, kind.and_then(|k| k.maquina.as_ref())) {
                    (Some(m), Some(k)) => {
                        let mut m = m.clone();
                        for (key, v) in &k.params {
                            m.params.entry(key.clone()).or_insert_with(|| v.clone());
                        }
                        for (key, v) in &k.ordenes {
                            m.ordenes.entry(key.clone()).or_insert_with(|| v.clone());
                        }
                        for (key, v) in &k.puertos {
                            m.puertos.entry(key.clone()).or_insert_with(|| v.clone());
                        }
                        if m.modelo.is_empty() {
                            m.modelo.clone_from(&k.modelo);
                        }
                        if m.pieza.is_none() {
                            m.pieza.clone_from(&k.pieza);
                        }
                        if m.eje.is_none() {
                            m.eje = k.eje;
                        }
                        if m.empuje.is_none() {
                            m.empuje = k.empuje;
                        }
                        m.chorro = crate::exhaust::JetDef::over(m.chorro.take(), k.chorro.as_ref());
                        if m.fabricante.is_none() {
                            m.fabricante.clone_from(&k.fabricante);
                        }
                        Some(m)
                    }
                    (Some(m), None) => Some(m.clone()),
                    (None, Some(k)) => Some(k.clone()),
                    (None, None) => None,
                };
                if let Some(mut m) = mdef {
                    // (how much it holds is one figure for its container and for it: `contents`)
                    for (key, v) in within.iter().flat_map(|w| &w.machine) {
                        m.params.insert(key.clone(), serde_json::json!(v));
                    }
                    if side {
                        m.puertos = m.puertos.iter().map(|(r, p)| (r.clone(), mirror_name(p))).collect();
                        m.ordenes = m.ordenes.iter().map(|(r, v)| (r.clone(), v.as_str().map_or(v.clone(), |s| serde_json::Value::String(mirror_name(s))))).collect();
                        m.params = m.params.iter().map(|(r, v)| (r.clone(), v.as_str().map_or(v.clone(), |s| serde_json::Value::String(mirror_name(s))))).collect();
                    }
                    // its part: a piece of the component by its own name, or the first
                    m.pieza = Some(match &m.pieza {
                        Some(p) if pieces.iter().any(|g| *g == format!("{cid}.{p}")) => {
                            format!("{cid}.{p}")
                        }
                        Some(p) => p.clone(),
                        None => first.clone(),
                    });
                    if m.fabricante.is_none() {
                        m.fabricante = kind.and_then(|k| k.fabricante.clone());
                    }
                    if m.cableado.is_none() {
                        m.cableado = c.cableado;
                    }
                    inline.push((cid.clone(), m));
                }
                if let Some(l) = c.luz.clone().or_else(|| kind.and_then(|k| k.luz.clone())) {
                    lights.insert(cid.clone(), (l, at));
                }
            }
        }
    }
    let joint_of_part: Vec<(String, String)> = placed.iter().filter(|(_, j)| !j.is_empty()).map(|(p, j)| (p.id.clone(), j.clone())).collect();
    b.parts.extend(placed.into_iter().map(|(p, _)| p));
    let t_routes = std::time::Instant::now();
    // ---- networks: junctions, runs, switches ----
    for (name, n) in &def.redes {
        let medium = Medium::parse(&n.medio).ok_or_else(|| err(format!("red {name}: medio desconocido '{}'", n.medio)))?;
        let nominal = match &n.nominal {
            Some(q) => q.si().map_err(|e| err(e.0))?,
            None => 0.0,
        };
        let leak = match &n.fuga {
            Some(q) => q.si().map_err(|e| err(e.0))?,
            None => 0.0,
        };
        let kind = n.conducto.clone().unwrap_or_else(|| NetPlan::kind_default(medium).to_string());
        let mut plan = NetPlan { name: name.clone(), medium, nominal, leak, nodes: Vec::new(), names: BTreeMap::new(), edges: Vec::new(), color: n.color.unwrap_or(medium_color(medium)), conducto: kind.clone() };
        for (node, at) in &n.nodos {
            plan.names.insert(node.clone(), plan.nodes.len() as u32);
            plan.nodes.push(Vec3::from_array(*at));
        }
        b.nets.push(plan);
        let net = (b.nets.len() - 1) as u16;
        for (k, r) in n.tramos.iter().enumerate() {
            let names = &b.nets[usize::from(net)].names;
            let (a, z) = (*names.get(&r.de).ok_or_else(|| err(format!("red {name}: nodo '{}'", r.de)))?, *names.get(&r.a).ok_or_else(|| err(format!("red {name}: nodo '{}'", r.a)))?);
            let nodes = &b.nets[usize::from(net)].nodes;
            let (pa, pz) = (nodes[a as usize], nodes[z as usize]);
            let ckind = r.tipo.clone().unwrap_or_else(|| kind.clone());
            // with every other run, along the trunks, by the points the data gives (if any)
            b.plan(net, a, z, pa, pz, &ckind, format!("{name}.tramo{k}"), false);
            if let Some(run) = b.runs.last_mut() {
                run.via = r.por.iter().map(|p| Vec3::from_array(*p)).collect();
            }
        }
        for s in &n.interruptores {
            let names = &b.nets[usize::from(net)].names;
            let (a, z) = (*names.get(&s.de).ok_or_else(|| err(format!("{}: nodo '{}'", s.id, s.de)))?, *names.get(&s.a).ok_or_else(|| err(format!("{}: nodo '{}'", s.id, s.a)))?);
            let part = match &s.pieza {
                Some(p) => Some(b.part_index(p)?),
                None => None,
            };
            b.nets[usize::from(net)].edges.push(EdgePlan { a, b: z, kind: EdgeKind::Switch, part, signal: Some(s.senal.clone().unwrap_or_else(|| s.id.clone())), measure: s.medir, id: s.id.clone() });
        }
    }
    // ---- machines: their ports, each with its stub ----
    let mut machines = Vec::new();
    let all: Vec<(String, MachineDef)> = inline.into_iter().chain(def.maquinas.iter().map(|(k, v)| (k.clone(), v.clone()))).collect();
    for (mid, m) in all {
        if m.modelo.is_empty() {
            return Err(err(format!("máquina {mid}: falta 'modelo'")));
        }
        let part = match &m.pieza {
            Some(p) => Some(b.part_index(p).or_else(|_| b.part_index(&format!("{p}.cuerpo")))?),
            None => None,
        };
        let at = b.anchors.get(&mid).and_then(|a| a.get("puerto").copied()).or_else(|| part.map(|p| Vec3::from(b.parts[p as usize].at.translation))).unwrap_or(Vec3::ZERO);
        let mut ports = Vec::new();
        for (role, spec) in &m.puertos {
            ports.push(b.port(spec, at, &mid, role, m.cableado != Some(false))?);
        }
        // (the axis of a machine that pushes is the way it pushes; of one that looks or fires,
        // the way it does)
        let thrust = Vec3::from_array(m.empuje.or(m.eje).unwrap_or([0.0, 0.0, -1.0])).normalize_or(Vec3::NEG_Z);
        // the light was written in its component's frame: into its part's
        let light = lights.remove(&mid).map(|(mut l, comp)| {
            let to_part = part.map_or(Affine3A::IDENTITY, |p| b.parts[p as usize].at.inverse()) * comp;
            l.en = to_part.transform_point3(Vec3::from_array(l.en)).to_array();
            l.dir = l.dir.map(|d| to_part.transform_vector3(Vec3::from_array(d)).normalize_or(Vec3::Y).to_array());
            l
        });
        let jet = crate::exhaust::Jet::plan(&b.parts, &mid, &m, part, thrust);
        machines.push(MachinePlan { light, lit_inside: false, id: mid, def: m, part, ports, thrust, jet });
    }
    // lights without a machine of their own are always on, fed by nothing: refuse (every light is wired)
    if let Some((l, _)) = lights.iter().next() {
        return Err(err(format!("la luz '{l}' no tiene máquina (modelo \"luz\") que la alimente")));
    }
    // ---- joints ----
    let mut joints: Vec<JointPlan> = Vec::new();
    let names: Vec<String> = def.articulaciones.keys().cloned().chain(leaves.iter().map(|(_, l)| l.joint.clone())).collect();
    let ids: Vec<String> = b.parts.iter().map(|p| p.id.clone()).collect();
    for (jid, j) in &def.articulaciones {
        let hinge = match j.tipo.as_str() {
            "bisagra" => true,
            "corredera" => false,
            t => {
                return Err(err(format!("articulación {jid}: tipo '{t}' (bisagra o corredera)")));
            }
        };
        let conv = |v: f32| if hinge { v.to_radians() } else { v };
        let mut parts: Vec<u32> = Vec::new();
        for p in &j.piezas {
            let found = resolve(&ids, p);
            if found.is_empty() {
                return Err(err(format!("articulación {jid}: ninguna pieza '{p}'")));
            }
            parts.extend(found);
        }
        for (pid, jn) in &joint_of_part {
            if jn == jid {
                parts.extend(resolve(&ids, pid));
            }
        }
        parts.sort_unstable();
        parts.dedup();
        let parent = match &j.padre {
            Some(p) => Some(names.iter().position(|n| n == p).ok_or_else(|| err(format!("articulación {jid}: padre desconocido '{p}'")))?),
            None => None,
        };
        // a sprung leg: its foot is the farthest of its parts against the way it is pressed in
        let spring = match &j.muelle {
            Some(m) => {
                if hinge || parts.is_empty() || j.limites[0].min(j.limites[1]).abs() > 1e-6 {
                    return Err(err(format!("articulación {jid}: un muelle es una corredera con piezas y recorrido desde 0")));
                }
                let axis = Vec3::from_array(j.eje).normalize_or(Vec3::Y);
                let mut sole: Option<(f32, Vec3)> = None;
                for &p in &parts {
                    let part = &b.parts[p as usize];
                    let reach = part.shape.verts().map(|v| part.at.transform_point3(v).dot(axis)).fold(f32::MAX, f32::min);
                    if sole.is_none_or(|s| reach < s.0) {
                        let c = Vec3::from(part.at.translation);
                        sole = Some((reach, c + axis * (reach - c.dot(axis))));
                    }
                }
                sole.map(|s| (s.1, m.clone()))
            }
            None => None,
        };
        let follows = match &j.sigue {
            Some(f) => {
                let of = names.iter().position(|n| *n == f.de).filter(|&o| names[o] != *jid).ok_or_else(|| err(format!("articulación {jid}: sigue a una desconocida '{}'", f.de)))?;
                // (the one followed may be a closure's: a ramp's own joint is not of the ship's data)
                if def.articulaciones.get(&f.de).is_some_and(|d| d.sigue.is_some()) || j.muelle.is_some() {
                    return Err(err(format!("articulación {jid}: no puede seguir a una que sigue a otra, ni ser un muelle")));
                }
                Some((of, f.razon))
            }
            None => None,
        };
        joints.push(JointPlan {
            spring,
            follows,
            gives: j.cede,
            id: jid.clone(),
            name: j.nombre.clone().unwrap_or_else(|| jid.clone()),
            hinge,
            axis: Vec3::from_array(j.eje).normalize_or(Vec3::X),
            pivot: Vec3::from_array(j.pivote),
            lo: conv(j.limites[0].min(j.limites[1])),
            hi: conv(j.limites[0].max(j.limites[1])),
            init: conv(j.inicial),
            parent,
            parts,
            damping: j.amortiguamiento.unwrap_or(if hinge { 400.0 } else { 200.0 }),
            opening: None,
        });
    }
    // the closures' leaves: shut at 0, open at their limit
    let mut closures: Vec<ClosurePlan> =
        def.cierres.iter().map(|c| ClosurePlan { id: c.id.clone(), name: c.nombre.clone().unwrap_or_else(|| c.id.clone()), joints: Vec::new(), parts: Vec::new(), order: c.orden.clone(), latch: c.pestillos, capture: 0.0, hand: true }).collect();
    for (ci, l) in &leaves {
        let c = &def.cierres[*ci];
        let mut parts = Vec::new();
        for p in &l.parts {
            parts.extend(resolve(&ids, p));
        }
        let opening = match &c.entre {
            Some([a, z]) => Some(JointOpening { a: comp_index(&def, a).map_err(err)?, b: comp_index(&def, z).map_err(err)?, area: l.area, at: l.at, n: l.n }),
            None => None,
        };
        closures[*ci].joints.push(joints.len());
        closures[*ci].parts.extend(parts.iter().copied());
        closures[*ci].capture = f64::from(c.captura.map_or(if l.hinge { 5f32.to_radians() } else { 0.03 }, |x| if l.hinge { x.to_radians() } else { x }));
        joints.push(JointPlan {
            spring: None,
            follows: None,
            // (a leaf hinged at its bottom lies on the ground when it is down: a ramp)
            gives: matches!(&c.mueve, crate::closures::MoveDef::Bisagra { borde, .. } if borde == "abajo"),
            id: l.joint.clone(),
            name: c.nombre.clone().unwrap_or_else(|| c.id.clone()),
            hinge: l.hinge,
            axis: l.axis,
            pivot: l.pivot,
            lo: 0.0,
            hi: l.open,
            init: if c.abierta { l.open } else { 0.0 },
            parent: None,
            parts,
            damping: c.amortiguamiento.unwrap_or(if l.hinge { 600.0 } else { 200.0 }),
            opening,
        });
    }
    // ---- actuators ----
    let mut actuators = Vec::new();
    for (aid, a) in &def.actuadores {
        let joint = names.iter().position(|n| *n == a.articulacion).ok_or_else(|| err(format!("actuador {aid}: articulación desconocida '{}'", a.articulacion)))?;
        let part = match &a.pieza {
            Some(p) => Some(b.part_index(p)?),
            None => None,
        };
        let at = part.map_or(joints[joint].pivot, |p| Vec3::from(b.parts[p as usize].at.translation));
        let mut ports = Vec::new();
        for (role, spec) in &a.redes {
            ports.push(b.port(spec, at, aid, role, a.cableado != Some(false))?);
        }
        actuators.push(ActuatorPlan { id: aid.clone(), def: a.clone(), joint, part, ports });
    }
    for c in &mut closures {
        c.hand = !actuators.iter().any(|a| c.joints.contains(&a.joint));
    }
    // ---- panels ----
    let mut panels = Vec::new();
    for ((m, (pdef, lay)), center) in def.paneles.iter().zip(laid).zip(panel_at) {
        let part = b.part_index(&m.pieza.clone().unwrap_or_else(|| format!("{}.caja", m.id)))?;
        let (u, v, n) = panel_axes(m);
        let size = Vec3::new(lay.size[0] / 1000.0, lay.size[1] / 1000.0, 0.0);
        let corner = center - u * size.x * 0.5 - v * size.y * 0.5;
        let frame = Affine3A::from_cols(u.into(), v.into(), n.into(), corner.into());
        // its wires come in at the back of its box (of the console under it), not across its face
        let back = match m.pieza {
            None => center - n * (m.fondo.unwrap_or(0.06) * 0.5),
            Some(_) => center - n * 0.15 - v * (size.y * 0.5),
        };
        let power = match &m.energia {
            Some(s) => Some(b.port(s, back, &m.id, "energia", m.cableado != Some(false))?),
            None => None,
        };
        let data = match &m.datos {
            Some(s) => Some(b.port(s, back, &m.id, "datos", m.cableado != Some(false))?),
            None => None,
        };
        panels.push(PanelPlan { id: m.id.clone(), part, frame, def: pdef, layout: lay, power, data });
    }
    // ---- every run laid at once, along the trunks they make ----
    // nothing only seen (a drop, a pipe) runs across a panel's face
    let keep: Vec<crate::conduits::Keep> = panels.iter().zip(&keep_depth).map(|(p, d)| crate::conduits::Keep { inv: p.frame.inverse(), size: Vec2::new(p.layout.size[0] / 1000.0, p.layout.size[1] / 1000.0), depth: *d }).collect();
    let seen = b.lay_runs(&def, &keep);
    if std::env::var_os("LUNAR_PERFIL").is_some() {
        eprintln!("canalizaciones: {:.1} ms, {} piezas", t_routes.elapsed().as_secs_f32() * 1000.0, b.parts.iter().filter(|p| p.role == Role::Conduit).count());
    }
    // ---- compartments ----
    let mut compartments: Vec<CompartmentPlan> = Vec::new();
    for c in &def.compartimentos {
        let boxes: Vec<[Vec3; 2]> = c.cajas.iter().map(|[a, z]| [Vec3::from_array(*a).min(Vec3::from_array(*z)), Vec3::from_array(*a).max(Vec3::from_array(*z))]).collect();
        let volume = c.volumen.unwrap_or_else(|| boxes.iter().map(|[a, z]| (*z - *a).element_product()).sum());
        let pressure = match &c.presion {
            Some(q) => q.si_as("Pa").map_err(|e| err(e.0))?,
            None => 0.0,
        };
        compartments.push(CompartmentPlan { id: c.id.clone(), name: c.nombre.clone(), boxes, volume, pressure, hull: Vec::new(), walls: Vec::new(), vents: Vec::new() });
    }
    for m in &mut machines {
        let Some(l) = &m.light else { continue };
        let to_ship = m.part.map_or(Affine3A::IDENTITY, |p| b.parts[p as usize].at);
        let at = to_ship.transform_point3(Vec3::from_array(l.en));
        let ahead = at + l.dir.map_or(Vec3::ZERO, |d| to_ship.transform_vector3(Vec3::from_array(d)).normalize_or_zero() * LAMP_AHEAD);
        let in_room = compartments.iter().any(|c| c.boxes.iter().any(|[lo, hi]| at.cmpge(*lo).all() && at.cmple(*hi).all()));
        m.lit_inside = in_room || def.casco.as_ref().is_some_and(|h| crate::far::in_hull(h, at) && crate::far::in_hull(h, ahead));
    }
    let comp_ids: Vec<String> = compartments.iter().map(|c| c.id.clone()).collect();
    let comp = |name: &str| -> Result<usize, String> {
        if name == "vacio" || name == "vacío" {
            return Ok(SPACE);
        }
        comp_ids.iter().position(|c| c == name).ok_or_else(|| format!("nave {id}: compartimento desconocido '{name}'"))
    };
    let mut hulls: Vec<(usize, u32)> = Vec::new();
    let mut walls: Vec<(usize, u32, usize)> = Vec::new();
    for (k, p) in b.parts.iter().enumerate() {
        match p.bounds.as_slice() {
            [a] => hulls.push((comp(a)?, k as u32)),
            [a, z] => {
                let (a, z) = (comp(a)?, comp(z)?);
                if a == SPACE {
                    hulls.push((z, k as u32));
                } else if z == SPACE {
                    hulls.push((a, k as u32));
                } else {
                    walls.push((a, k as u32, z));
                }
            }
            _ => {}
        }
    }
    let mut vents = Vec::new();
    for c in &def.compartimentos {
        let ci = comp(&c.id)?;
        for l in &c.limites {
            for k in resolve(&ids, l) {
                hulls.push((ci, k));
            }
        }
        for v in &c.aberturas {
            let at = v.en.map(|p| (Vec3::from_array(p), Vec3::from_array(v.normal.unwrap_or([0.0, 1.0, 0.0])).normalize_or(Vec3::Y)));
            vents.push((ci, VentPlan { signal: v.senal.clone(), area: v.area, to: comp(&v.hacia)?, at }));
        }
    }
    for (ci, v) in vents {
        compartments[ci].vents.push(v);
    }
    for (c, k) in hulls {
        if c != SPACE {
            compartments[c].hull.push(k);
        }
    }
    for (a, k, z) in walls {
        compartments[a].walls.push((k, z));
        compartments[z].walls.push((k, a));
    }
    // where each compartment is, to say which way an opening goes
    let centre = |c: usize| {
        let bx = &compartments[c].boxes;
        bx.iter().map(|[a, z]| (*a + *z) * 0.5).sum::<Vec3>() / bx.len().max(1) as f32
    };
    let inside = |c: usize, p: Vec3| c != SPACE && compartments[c].boxes.iter().any(|[lo, hi]| p.cmpge(*lo).all() && p.cmple(*hi).all());
    for (jid, j) in &def.articulaciones {
        if let Some(o) = &j.abre {
            let ji = names.iter().position(|n| n == jid).unwrap_or(0);
            let (a, z) = (comp(&o.entre[0])?, comp(&o.entre[1])?);
            let at = joints[ji].pivot;
            let n = match (a == SPACE, z == SPACE) {
                (false, false) => centre(z) - centre(a),
                (false, true) => at - centre(a),
                (true, false) => centre(z) - at,
                _ => Vec3::Y,
            };
            joints[ji].opening = Some(JointOpening { a, b: z, area: o.area, at, n: n.normalize_or(Vec3::Y) });
        }
    }
    // a closure's normal is its opening's: turned to go from a toward b
    for j in &mut joints {
        if let Some(o) = &mut j.opening {
            let back = o.at - o.n * 0.3;
            let ahead = o.at + o.n * 0.3;
            let a_behind = if o.a == SPACE { !compartments.iter().enumerate().any(|(c, _)| inside(c, back)) } else { inside(o.a, back) };
            let b_ahead = if o.b == SPACE { !compartments.iter().enumerate().any(|(c, _)| inside(c, ahead)) } else { inside(o.b, ahead) };
            if !a_behind && !b_ahead {
                o.n = -o.n;
            }
        }
    }
    // ---- decals ----
    let mut decals = Vec::new();
    {
        let ids: Vec<String> = b.parts.iter().map(|p| p.id.clone()).collect();
        let mut lay = |d: &DecalDef, at: Affine3A, part: Option<u32>| -> Result<(), String> {
            let e = b.inputs.decals.calcas.get(&d.imagen).ok_or_else(|| format!("calca desconocida '{}' (assets/textures/calcas.json)", d.imagen))?;
            let n = at.transform_vector3(Vec3::from_array(d.normal)).normalize_or(Vec3::Z);
            let up0 = at.transform_vector3(Vec3::from_array(d.arriba.unwrap_or([0.0, 1.0, 0.0])));
            let mut up = (up0 - n * up0.dot(n)).normalize_or(Vec3::ZERO);
            if up.length_squared() < 0.5 {
                up = n.any_orthonormal_vector();
            }
            // read from the front even on a mirrored component: across is up × out
            let across = up.cross(n);
            let half = Vec3::new(d.ancho * 0.5, d.ancho * 0.5 / e.aspecto.max(0.01), 0.0);
            // a hair off the surface
            let center = at.transform_point3(Vec3::from_array(d.en)) + n * 0.003;
            let part = match (&d.pieza, part) {
                (Some(p), _) => Some(*resolve(&ids, p).first().ok_or_else(|| format!("calca {}: no hay pieza '{p}'", d.imagen))?),
                (None, Some(p)) => Some(p),
                // the part right under it
                (None, None) => b.parts.iter().enumerate().map(|(k, p)| (k, p.shape.closest(p.at.inverse().transform_point3(center)).0.abs())).filter(|(_, d)| *d < 0.06).min_by(|a, b| a.1.total_cmp(&b.1)).map(|(k, _)| k as u32),
            };
            decals.push(DecalPlan { uv: e.uv, center, u: across * half.x, v: up * half.y, part, tint: d.tinte.unwrap_or([255, 255, 255]), emissive: d.brillo.unwrap_or(0.0) });
            Ok(())
        };
        for (d, at, first) in &fitted {
            let p = ids.iter().position(|i| i == first).map(|k| k as u32);
            lay(d, *at, p).map_err(err)?;
        }
        for d in &def.calcas {
            lay(d, Affine3A::IDENTITY, None).map_err(err)?;
        }
    }
    // ---- labels: each becomes its part's (`lunar_core::structure::labels`), in that part's
    // frame, so it goes where the part goes ----
    let mut labels: Vec<Vec<lunar_core::structure::catalog::Label>> = vec![Vec::new(); b.parts.len()];
    {
        let ids: Vec<String> = b.parts.iter().map(|p| p.id.clone()).collect();
        let mut lay = |l: &crate::def::LabelDef, at: Affine3A, part: Option<u32>, comp: &str, name: &str, mark: Option<&str>| -> Result<(), String> {
            if l.texto.contains("{etiqueta}") && mark.is_none() {
                return Ok(());
            }
            let n = at.transform_vector3(Vec3::from_array(l.normal)).normalize_or(Vec3::Z);
            let up0 = at.transform_vector3(Vec3::from_array(l.arriba.unwrap_or([0.0, 1.0, 0.0])));
            let mut up = (up0 - n * up0.dot(n)).normalize_or(Vec3::ZERO);
            if up.length_squared() < 0.5 {
                up = n.any_orthonormal_vector();
            }
            // read from the front even on a mirrored component: across is up × out
            let across = up.cross(n);
            let origin = at.transform_point3(Vec3::from_array(l.en)) + n * 0.002;
            let part = match (&l.pieza, part) {
                (Some(p), _) => Some(*resolve(&ids, p).first().ok_or_else(|| format!("rótulo '{}': no hay pieza '{p}'", l.texto))?),
                (None, Some(p)) => Some(p),
                (None, None) => b.parts.iter().enumerate().map(|(k, p)| (k, p.shape.closest(p.at.inverse().transform_point3(origin)).0.abs())).filter(|(_, d)| *d < 0.06).min_by(|a, b| a.1.total_cmp(&b.1)).map(|(k, _)| k as u32),
            };
            let align = match l.alinear.as_deref() {
                None | Some("centro") => 0.5,
                Some("izq") => 0.0,
                Some("der") => 1.0,
                Some(e) => return Err(format!("rótulo '{}': alinear '{e}' (izq, centro, der)", l.texto)),
            };
            // a serial of its own: the same every time, different on every one
            let serial = comp.bytes().chain(id.bytes()).fold(2166136261u32, |h, c| (h ^ u32::from(c)).wrapping_mul(16777619)) % 9000 + 1000;
            let text = l.texto.replace("{etiqueta}", mark.unwrap_or("")).replace("{id}", &comp.replace('_', " ").to_uppercase()).replace("{nombre}", &name.to_uppercase()).replace("{matricula}", def.matricula.as_deref().unwrap_or("")).replace("{nave}", &def.nombre.to_uppercase()).replace("{serie}", &serial.to_string());
            let Some(part) = part else { return Err(format!("rótulo '{}': no está sobre ninguna pieza", l.texto)) };
            let to_part = b.parts[part as usize].at.inverse();
            labels[part as usize].push(lunar_core::structure::catalog::Label {
                text,
                at: to_part.transform_point3(origin).to_array(),
                u: to_part.transform_vector3(across * l.alto).to_array(),
                v: to_part.transform_vector3(up * l.alto).to_array(),
                align,
                color: l.color.unwrap_or([18, 18, 20]),
                relief: l.relieve,
                emissive: l.brillo.unwrap_or(0.0),
                radius: l.radio.unwrap_or(0.0).max(0.0),
            });
            Ok(())
        };
        for (l, at, first, comp, name, mark) in &lettered {
            let p = ids.iter().position(|i| i == first).map(|k| k as u32);
            lay(l, *at, p, comp, name, mark.as_deref()).map_err(err)?;
        }
        for l in &def.rotulos {
            lay(l, Affine3A::IDENTITY, None, "", "", None).map_err(err)?;
        }
    }
    // ---- seats ----
    let mut seats = Vec::new();
    for s in &def.asientos {
        for e in s.bajadas.iter().flatten() {
            if !def.bajadas.iter().any(|x| &x.id == e) {
                return Err(format!("asiento {}: no hay bajada '{e}'", s.id));
            }
        }
        seats.push(SeatPlan { part: b.part_index(&s.pieza)?, def: s.clone() });
    }
    // ---- into the structure catalog: one kind per part, its look; bones; joints ----
    let mut bones = vec![0u16; b.parts.len()];
    for (k, j) in joints.iter().enumerate() {
        for &p in &j.parts {
            bones[p as usize] = (k + 1) as u16;
        }
    }
    // cargo and its clamps: who holds what, part by part
    let comp: Vec<Option<&str>> = b.parts.iter().map(|p| part_comp.get(&p.id).map(String::as_str)).collect();
    let empty: Vec<String> = clamp_zones.keys().filter(|c| !held.values().any(|h| h == *c)).cloned().collect();
    let holds = crate::cargo::Holds::new(&comp, &held, &empty).map_err(err)?;
    let mut placed_parts = Vec::with_capacity(b.parts.len());
    let rooms: Vec<[Vec3; 2]> = compartments.iter().flat_map(|c| c.boxes.iter().copied()).collect();
    let interior: Vec<bool> = b.parts.iter().map(|p| crate::far::interior(p, def.casco.as_ref(), &rooms)).collect();
    for (k, p) in b.parts.iter().enumerate() {
        // its plain look (what it was made with), and the one for close by if it has one: a
        // model wearing its surface, else its shape with its edges broken
        let plain = p.look.as_ref().map(|mesh| {
            let name = format!("{id}/{}", p.id);
            b.inputs.catalog.add_look(&name, mesh.clone());
            name
        });
        let model = p.model.as_ref().and_then(|m| b.inputs.catalog.models.get(m).map(|model| (m, if p.mirrored { model.mirrored().tinted(p.surface.unwrap_or_default()) } else { model.tinted(p.surface.unwrap_or_default()) })));
        let fine = match (model, &p.fine) {
            (Some((m, mesh)), _) => {
                // (one look per model and surface: every blue drum shares it)
                let s = p.surface.unwrap_or_default();
                let name = format!("{m}{}#{:02x}{:02x}{:02x}.{}.{}.{}.{}.{}", if p.mirrored { "~" } else { "" }, s.albedo[0], s.albedo[1], s.albedo[2], s.rough, s.metal, s.emissive, s.panel, s.finish);
                if !b.inputs.catalog.looks.iter().any(|(n, _)| *n == name) {
                    b.inputs.catalog.add_look(&name, mesh);
                }
                Some(name)
            }
            (None, Some(mesh)) => {
                let name = format!("{id}/{}+", p.id);
                b.inputs.catalog.add_look(&name, mesh.clone());
                Some(name)
            }
            _ => None,
        };
        let (look, basic) = match fine {
            Some(f) => (Some(f), plain),
            None => (plain, None),
        };
        let kdef = PartKindDef {
            // (cargo is called what it is: it may leave, and whoever finds it reads this)
            name: comp[k].filter(|_| holds.cargo(k)).and_then(|c| comp_names.get(c)).cloned().unwrap_or_else(|| p.id.clone()),
            shape: ShapeDef::Box { size: [0.1; 3] },
            material: p.material.clone(),
            ports: Vec::new(),
            module: None,
            function: 0.5,
            color: p.color,
            glow: p.glow,
            hollow: p.hollow,
            look,
            basic,
            fragility: match p.role {
                Role::Conduit => 3.0,
                Role::Glass => 2.0,
                _ => 1.0,
            },
            interior: interior[k],
            wiring: p.role == Role::Conduit,
            ghost: p.flags & geom::GHOST != 0,
            no_collide: p.flags & geom::NO_COLLIDE != 0,
            sparks: p.flags & geom::SPARKS != 0,
            // (what it says itself; else what the substance it holds does)
            burst: p.burst.clone().or_else(|| containers.get(&p.id).and_then(|c| c.1.clone())),
            carried: holds.cargo(k),
            contents: comp[k].filter(|_| holds.cargo(k)).and_then(|c| cargo.get(c)).cloned(),
            holds: containers.get(&p.id).map(|c| c.0.clone()),
            labels: std::mem::take(&mut labels[k]),
        };
        let kind = b.inputs.catalog.add_part(format!("{id}/{}", p.id), kdef, Some(p.shape.clone())).map_err(err)?;
        placed_parts.push(Placed { kind, local: p.at, bone: bones[k] });
    }
    // joints where parts touch (spheres first)
    let cat = &*b.inputs.catalog;
    let shapes: Vec<_> = b.parts.iter().map(|p| p.shape.transformed(p.at)).collect();
    let spheres: Vec<_> = shapes.iter().map(|s| s.sphere()).collect();
    let weld = cat.joint(def.casco.as_ref().map_or("soldadura", |h| h.union.as_str())).or_else(|| cat.joint("soldadura")).ok_or_else(|| err("no hay uniones en el catálogo".into()))?;
    let hinge = cat.joint("bisagra").unwrap_or(weld);
    let lashing = cat.joint("amarre").unwrap_or(weld);
    let mut joined = Vec::new();
    const TOUCH: f32 = 0.03;
    for a in 0..b.parts.len() {
        for z in a + 1..b.parts.len() {
            if spheres[a].0.distance(spheres[z].0) > spheres[a].1 + spheres[z].1 + TOUCH {
                continue;
            }
            // parts on different bones hold by hinges (they move apart: the hinge is their pivot)
            let near = shapes[a].verts().filter(|&v| shapes[z].distance(v) <= TOUCH).chain(shapes[z].verts().filter(|&v| shapes[a].distance(v) <= TOUCH)).fold((Vec3::ZERO, 0), |(s, n), v| (s + v, n + 1));
            if near.1 > 0 {
                // cargo holds to its clamp and to nothing else (`cargo`)
                let kind = match holds.join(a, z) {
                    crate::cargo::Join::Never => continue,
                    crate::cargo::Join::Lashing => lashing,
                    crate::cargo::Join::Any if bones[a] != bones[z] => hinge,
                    crate::cargo::Join::Any => weld,
                };
                joined.push(Joined { a: a as u32, b: z as u32, kind, at: near.0 / near.1 as f32, networks: 0 });
            }
        }
    }
    // cargo that touches its clamp nowhere (a crate on other crates) is lashed to it all the same
    let tied: Vec<(u32, u32)> = joined.iter().map(|j| (j.a, j.b)).collect();
    for (a, z) in holds.unlashed(&tied, &spheres) {
        joined.push(Joined { a: a.min(z) as u32, b: a.max(z) as u32, kind: lashing, at: (spheres[a].0 + spheres[z].0) * 0.5, networks: 0 });
    }
    // every part, the stubs laid last included
    // one piece: whatever touches nothing (a lamp off its ceiling, a duct under it, a console's
    // face a hair off its body) is fixed to the nearest part of the rest, of its own joint if
    // there is one. Otherwise the first hit anywhere would send each of them flying.
    {
        let n = b.parts.len();
        let mut group: Vec<usize> = (0..n).collect();
        fn root(g: &mut [usize], mut a: usize) -> usize {
            while g[a] != a {
                g[a] = g[g[a]];
                a = g[a];
            }
            a
        }
        for j in &joined {
            let (x, y) = (root(&mut group, j.a as usize), root(&mut group, j.b as usize));
            group[x.max(y)] = x.min(y);
        }
        loop {
            // the biggest group, and a part of any other
            let mut size = vec![0usize; n];
            for k in 0..n {
                size[root(&mut group, k)] += 1;
            }
            let main = (0..n).max_by_key(|&k| size[k]).unwrap_or(0);
            let roots: Vec<usize> = (0..n).map(|k| root(&mut group, k)).collect();
            let Some(&g) = roots.iter().find(|&&r| r != main) else {
                break;
            };
            // the nearest pair between that group and the rest (its own joint first)
            let mut best: Option<(usize, usize, f32)> = None;
            // (never by its cargo, nor to another's: cargo holds to its clamp alone)
            for a in (0..n).filter(|&a| roots[a] == g && !holds.cargo(a)) {
                for z in 0..n {
                    if roots[z] == g || holds.cargo(z) {
                        continue;
                    }
                    let d = (spheres[a].0.distance(spheres[z].0) - spheres[a].1 - spheres[z].1).max(0.0) + if bones[a] == bones[z] { 0.0 } else { 0.5 };
                    if best.is_none_or(|b| d < b.2) {
                        best = Some((a, z, d));
                    }
                }
            }
            let Some((a, z, _)) = best else { break };
            let kind = if bones[a] != bones[z] { hinge } else { weld };
            joined.push(Joined { a: a.min(z) as u32, b: a.max(z) as u32, kind, at: (spheres[a].0 + spheres[z].0) * 0.5, networks: 0 });
            let (x, y) = (root(&mut group, a), root(&mut group, z));
            group[x.max(y)] = x.min(y);
        }
    }
    let ids: Vec<String> = b.parts.iter().map(|p| p.id.clone()).collect();
    let bp = Blueprint { name: def.nombre.clone(), anchored: false, ids: ids.clone(), parts: placed_parts, joints: joined };
    // how it looks from far away
    b.inputs.catalog.add_far(&bp.name, crate::far::far_shape(def.casco.as_ref(), &b.parts, &interior));
    let roles = b.parts.iter().map(|p| p.role).collect();
    let sparks = b.parts.iter().enumerate().filter(|(_, p)| p.flags & geom::SPARKS != 0).map(|(k, _)| k as u32).collect();
    let centers = b.parts.iter().map(|p| p.at.transform_point3(p.shape.sphere().0)).collect();
    let lift = def.alzado.unwrap_or(0.0);
    let index = |names: &[String]| -> Vec<u32> { names.iter().filter_map(|n| ids.iter().position(|i| i == n)).map(|k| k as u32).collect() };
    let holders = holding.into_iter().map(|(id, held_in, parts, level, shares, (substance, capacity, built, brim, tied))| Holder { id, holds: index(&held_in), parts: index(&parts), level, shares, substance, capacity, built, brim, tied }).collect();
    let inside: std::sync::Arc<[[Vec3; 2]]> = compartments.iter().flat_map(|c| c.boxes.iter().copied()).collect();
    let kind = ShipKind {
        holders,
        id: id.to_string(),
        parts: ids,
        centers,
        roles,
        nets: b.nets,
        machines,
        actuators,
        joints,
        panels,
        compartments,
        rooms: inside,
        seats,
        closures,
        decals,
        blueprint: id.to_string(),
        lift,
        sparks,
        clamps: holds.plans(&|c| comp_names.get(c).cloned().unwrap_or_else(|| c.replace('_', " ")), &|c| clamp_zones.get(c).copied(), &bones),
        names: comp_names,
        cargo,
        seen,
        def,
    };
    Ok((kind, bp))
}
