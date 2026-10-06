//! A ship as data (`assets/defs/ships/<id>.jsonc`). Everything physical is a structure part (hull
//! panels, floors, bulkheads, every component, every cable, pipe and duct segment, every light);
//! everything that works is a machine, an actuator or a control on one of them; the networks run
//! through the conduits. See `docs/NAVES.md`.
use lunar_machines::actuator::ActuatorDef;
use lunar_signals::Q;
use serde::Deserialize;
use serde_json::{Map, Value};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShipDef {
    pub nombre: String,
    #[serde(default)]
    pub matricula: Option<String>,
    #[serde(default)]
    pub clase: Option<String>,
    #[serde(default)]
    pub astillero: Option<String>,
    #[serde(default)]
    pub modelo: Option<String>,
    /// For the catalog (G).
    #[serde(default)]
    pub descripcion: Option<String>,
    #[serde(default)]
    pub casco: Option<HullDef>,
    #[serde(default)]
    pub suelos: Vec<FloorDef>,
    #[serde(default)]
    pub mamparos: Vec<BulkheadDef>,
    /// How its conduits are laid (`conduits`): what the ship says instead of what is found.
    #[serde(default)]
    pub canalizaciones: ConduitsDef,
    /// Posters, signs, logos, plates (`assets/textures/calcas`).
    #[serde(default)]
    pub calcas: Vec<DecalDef>,
    /// Stencils and plates: lines of text on its surfaces (ship frame).
    #[serde(default)]
    pub rotulos: Vec<LabelDef>,
    /// Ramps, doors, hatches, lids: everything that shuts an opening (`closures`).
    #[serde(default)]
    pub cierres: Vec<crate::closures::ClosureDef>,
    #[serde(default)]
    pub componentes: Vec<ComponentDef>,
    #[serde(default)]
    pub articulaciones: BTreeMap<String, JointDef>,
    #[serde(default)]
    pub redes: BTreeMap<String, NetDef>,
    #[serde(default)]
    pub maquinas: BTreeMap<String, MachineDef>,
    #[serde(default)]
    pub actuadores: BTreeMap<String, ActuatorDef>,
    #[serde(default)]
    pub compartimentos: Vec<CompartmentDef>,
    #[serde(default)]
    pub paneles: Vec<PanelMount>,
    #[serde(default)]
    pub asientos: Vec<SeatDef>,
    /// Where one gets off: the places to stand at on leaving a seat (`ExitDef`).
    #[serde(default)]
    pub bajadas: Vec<ExitDef>,
    /// Signals with an initial value (and unit): `"nombre": "valor"`.
    #[serde(default)]
    pub senales: BTreeMap<String, Q>,
    #[serde(default)]
    pub derivadas: BTreeMap<String, String>,
    #[serde(default)]
    pub vuelo: Option<FlightDef>,
    /// What whoever it carries weighs aboard it.
    pub gravedad: GravityDef,
    /// Where it stands when placed: metres from its lowest point (gear down).
    #[serde(default)]
    pub alzado: Option<f32>,
    /// Alarms for the black box: `"texto": "condición"`.
    #[serde(default)]
    pub alarmas: BTreeMap<String, String>,
    /// What it must keep whatever one conduit is lost (a trunk, an outlet, a duct):
    /// `"qué": "condición"` that holds with everything running. `diag::single_failures` cuts each
    /// in turn and says which of these stop holding.
    #[serde(default)]
    pub esenciales: BTreeMap<String, String>,
    /// Interlocks: a control that will not move (toward a value) while a condition holds.
    #[serde(default)]
    pub enclavamientos: Vec<InterlockDef>,
    /// Where the panel of circuit priorities goes (one selector per breaker, `rooms`), if any.
    #[serde(default)]
    pub prioridades: Option<RoomPanelDef>,
    /// Ships it carries docked as it is built: each of a kind, in one of its clamps (a cradle).
    #[serde(default)]
    pub lleva: Vec<CarriedDef>,
    /// Air moved on purpose: the hand valves through its walls and where the panel of each
    /// compressor goes (`airworks`, `docs/AIRE.md`).
    #[serde(default)]
    pub trasvase: TransferDef,
    /// What it looks like to whoever looks for it (`tactical`): none, what its size says.
    #[serde(default)]
    pub firma: Option<SignatureDef>,
}

/// The gravity a ship makes for whoever it carries (standing on it or in its rooms): `g` m/s²
/// toward its decks, wherever it is and however it flies. `g` 0: it makes none, and aboard it one
/// weighs what is left of the world's pull as the ship goes (nothing in free fall). `senal`: the
/// signal that says whether it works (none: always). It comes on and goes over `tiempo` s.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GravityDef {
    pub g: f32,
    #[serde(default)]
    pub senal: Option<String>,
    pub tiempo: f32,
}

/// What a ship looks like to a sensor.
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignatureDef {
    /// What it reflects of a radar (m²): a hull shaped and coated for it reflects far less than
    /// its size says.
    pub rcs: f32,
}

/// Air moved on purpose (`airworks`): from one compartment to another by hand, out of one into
/// a tank by a compressor.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransferDef {
    #[serde(default)]
    pub valvulas: Vec<AirValveDef>,
    #[serde(default)]
    pub compresores: Vec<CompressorPanelDef>,
}

/// A valve worked by hand through a wall: as open as its wheel is turned, with a wheel and a
/// gauge of the pressure across it on each side that is a compartment.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AirValveDef {
    pub id: String,
    #[serde(default)]
    pub nombre: Option<String>,
    /// The two places it joins: compartments, or "vacio" for space (the second, if any).
    pub entre: [String; 2],
    /// Where it goes through the wall (ship frame: the middle of the wall), and the way from the
    /// first place to the second across it.
    pub en: [f32; 3],
    pub normal: [f32; 3],
    /// Its bore ("70 mm"): the opening with its wheel all the way.
    #[serde(default)]
    pub paso: Option<Q>,
    /// The component it is (`assets/defs/components`); by default "valvula_igualacion" between
    /// two compartments and "valvula_venteo" out to space.
    #[serde(default)]
    pub tipo: Option<String>,
}

/// Where the panel of a compressor goes (made from what its manifold reaches).
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompressorPanelDef {
    /// The compressor: a component of this ship, by its id.
    pub id: String,
    pub panel: RoomPanelDef,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CarriedDef {
    /// The ship kind, and the clamp of this ship that holds it.
    pub nave: String,
    pub anclaje: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InterlockDef {
    /// `panel/control`.
    pub mando: String,
    /// Only changes toward this value are refused (any change if absent).
    #[serde(default)]
    pub hacia: Option<f64>,
    pub si: String,
    /// Why, for the HUD ("Peso sobre el tren").
    pub motivo: String,
}

/// The hull as a loft: stations along +Z (the nose is +Z), each a closed convex outline (x, y),
/// joined facet by facet into panels; every panel is a part.
/// Settings of the conduits (`conduits`): all optional.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConduitsDef {
    /// Height of the trunks along the side walls, as a share of each room's height (instead of
    /// the clearest of the usual ones).
    #[serde(default)]
    pub altura: Option<f32>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HullDef {
    /// Finish of the inside face (default "pintura").
    #[serde(default)]
    pub acabado_interior: Option<String>,
    /// Named outlines: closed convex polygons (x, y), counter-clockwise seen from the nose,
    /// starting at the bottom centre. All outlines must have the same number of points.
    pub perfiles: BTreeMap<String, Vec<[f32; 2]>>,
    pub estaciones: Vec<StationDef>,
    /// Panel thickness (m).
    #[serde(default = "skin")]
    pub espesor: f32,
    #[serde(default = "aluminium")]
    pub material: String,
    /// Plates of the shader's plating (m).
    #[serde(default = "plate")]
    pub chapa: f32,
    /// Outline points whose edge is smooth (the rest are chines), by index.
    #[serde(default)]
    pub suaves: Vec<usize>,
    /// Livery: colour bands by outline facet and station ranges (sRGB).
    #[serde(default)]
    pub pintura: Vec<PaintDef>,
    /// Inside colour (sRGB).
    #[serde(default)]
    pub interior: Option<[u8; 3]>,
    /// Facets made glass: (station interval, facet).
    #[serde(default)]
    pub ventanas: Vec<[usize; 2]>,
    /// Facets left open (a door goes there).
    #[serde(default)]
    pub huecos: Vec<[usize; 2]>,
    /// Facets under the floor: they bound no compartment (the floor does).
    #[serde(default)]
    pub bajo_suelo: Vec<usize>,
    /// Close the ends with caps (nose first, tail last): true / false each.
    #[serde(default)]
    pub tapas: [bool; 2],
    #[serde(default = "joint_kind")]
    pub union: String,
}

fn skin() -> f32 {
    0.08
}
fn aluminium() -> String {
    "aluminio".into()
}
fn plate() -> f32 {
    0.9
}
fn joint_kind() -> String {
    "soldadura".into()
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StationDef {
    pub z: f32,
    pub perfil: String,
    /// Scale (x, y) of the outline and a lift of its centre.
    #[serde(default = "unit2")]
    pub escala: [f32; 2],
    #[serde(default)]
    pub y: f32,
    /// The compartment the panels from here to the next station bound.
    #[serde(default)]
    pub compartimento: Option<String>,
    /// Panels per facet along this interval (long intervals split).
    #[serde(default = "one_u")]
    pub divisiones: u32,
}

fn unit2() -> [f32; 2] {
    [1.0, 1.0]
}
fn one_u() -> u32 {
    1
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PaintDef {
    /// Facets (outline edge indices) and station intervals it covers (inclusive).
    pub caras: Vec<usize>,
    #[serde(default)]
    pub tramos: Option<[usize; 2]>,
    pub color: [u8; 3],
    #[serde(default)]
    pub metal: Option<u8>,
    #[serde(default)]
    pub rugosidad: Option<u8>,
}

/// A deck of plates: a rectangle (x0, z0)-(x1, z1) at height y, cut into cols × rows plates.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FloorDef {
    /// Its finish (default "lagrimado": tread plate).
    #[serde(default)]
    pub acabado: Option<String>,
    pub id: String,
    pub desde: [f32; 2],
    pub hasta: [f32; 2],
    #[serde(default)]
    pub y: f32,
    #[serde(default = "floor_t")]
    pub espesor: f32,
    #[serde(default = "two")]
    pub divisiones: [u32; 2],
    #[serde(default = "aluminium")]
    pub material: String,
    #[serde(default)]
    pub color: Option<[u8; 3]>,
    #[serde(default)]
    pub compartimento: Option<String>,
    /// Plates left out (cols, rows indices): hatches, stairs.
    #[serde(default)]
    pub huecos: Vec<[u32; 2]>,
}

fn floor_t() -> f32 {
    0.06
}
fn two() -> [u32; 2] {
    [2, 2]
}

/// A wall across the hull at z (or along it at x): the outline at that station less a doorway,
/// cut into convex pieces.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BulkheadDef {
    pub id: String,
    pub z: f32,
    /// The doorway (x0, x1, height from the floor at y = 0), if any.
    #[serde(default)]
    pub puerta: Option<[f32; 3]>,
    #[serde(default = "floor_t")]
    pub espesor: f32,
    #[serde(default = "aluminium")]
    pub material: String,
    #[serde(default)]
    pub color: Option<[u8; 3]>,
    /// Compartments on each side (−z side, +z side).
    #[serde(default)]
    pub entre: Option<[String; 2]>,
    /// Only the part of the outline above this height (the floor is apart).
    #[serde(default)]
    pub desde_y: f32,
}

/// A decal of the atlas (`assets/textures/calcas`, by name) on a surface: where, facing which
/// way (`normal`, out of the surface), its up, its width (its height follows its aspect).
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DecalDef {
    pub imagen: String,
    pub en: [f32; 3],
    pub normal: [f32; 3],
    #[serde(default)]
    pub arriba: Option<[f32; 3]>,
    pub ancho: f32,
    /// The part it is on (it goes and breaks with it); by default the one under it.
    #[serde(default)]
    pub pieza: Option<String>,
    #[serde(default)]
    pub tinte: Option<[u8; 3]>,
    /// Lit signs (exit signs) glow this much.
    #[serde(default)]
    pub brillo: Option<f32>,
}

/// A label: a line of text in the silkscreen font on a surface — a stencil on a hull, a plate
/// on a machine, what a drum holds —: where its anchor is, facing which way (`normal`, out of the
/// surface), its up, how tall its letters are (m). In its text, `{id}` is the component it is on,
/// `{nombre}` that component's name, `{etiqueta}` what that component is marked as where it is
/// fitted (a bottle's gas: without one, the label is left out), `{matricula}` and `{nave}` the
/// ship's registration and name, `{serie}` a serial number of its own; and on a component that
/// holds something (`contents`), `{sustancia}` what it holds ("AGUA") and `{capacidad}` how much
/// at most ("170 L"), both from its data (on one that holds nothing, the label is left out).
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LabelDef {
    pub texto: String,
    pub en: [f32; 3],
    pub normal: [f32; 3],
    #[serde(default)]
    pub arriba: Option<[f32; 3]>,
    pub alto: f32,
    /// sRGB (black if none).
    #[serde(default)]
    pub color: Option<[u8; 3]>,
    /// 0 painted; more, raised (cast, embossed); less, cut in (engraved, stamped).
    #[serde(default)]
    pub relieve: f32,
    /// Where its anchor is along it: "izq", "centro" (the default), "der".
    #[serde(default)]
    pub alinear: Option<String>,
    /// The part it is on (it goes and breaks with it); by default the one under it.
    #[serde(default)]
    pub pieza: Option<String>,
    /// Lit letters (a sign) glow this much.
    #[serde(default)]
    pub brillo: Option<f32>,
    /// It goes round a cylinder of this radius (m) whose axis is along its up (a drum, a
    /// bottle, a tank): its anchor is on that cylinder's skin.
    #[serde(default)]
    pub radio: Option<f32>,
}

/// A component placed: a kind of the component catalog, or a plain shape.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComponentDef {
    /// Finish of every piece (else each piece's, else its material's).
    #[serde(default)]
    pub acabado: Option<String>,
    pub id: String,
    /// Component kind (`assets/defs/components`), or none with `forma`.
    #[serde(default)]
    pub tipo: Option<String>,
    /// A plain shape: box, cylinder or wedge (the structure catalog's shapes).
    #[serde(default)]
    pub forma: Option<Value>,
    /// What a plain shape is seen as from close by: a style of `tools/modelos/estilos.py` (a wing,
    /// a gear leg, a box girder...), whose model for a shape this size is `components::style_model`.
    #[serde(default)]
    pub estilo: Option<String>,
    #[serde(default)]
    pub material: Option<String>,
    #[serde(default)]
    pub color: Option<[u8; 3]>,
    #[serde(default)]
    pub brillo: Option<u8>,
    #[serde(default)]
    pub en: [f32; 3],
    /// Euler XYZ (degrees).
    #[serde(default)]
    pub rot: [f32; 3],
    /// Copies: count and step (m), named `id#k`.
    #[serde(default)]
    pub repetir: Option<(u32, [f32; 3])>,
    /// A mirrored copy across x = 0, named `id.e` (its machine and ports mirror too).
    #[serde(default)]
    pub espejo: bool,
    /// What its kind's labels mark it as (`{etiqueta}`: a bottle's gas, a tank's fill).
    #[serde(default)]
    pub etiqueta: Option<String>,
    /// The joint that moves it.
    #[serde(default)]
    pub articulacion: Option<String>,
    /// Its machine (inline).
    #[serde(default)]
    pub maquina: Option<MachineDef>,
    /// Its light (fixtures).
    #[serde(default)]
    pub luz: Option<LightDef>,
    #[serde(default)]
    pub compartimento: Option<String>,
    /// Plating on its faces (m).
    #[serde(default)]
    pub chapa: Option<f32>,
    #[serde(default)]
    pub hueco: Option<f32>,
    /// Wired with conduits of its own (default): false, it is connected without any (its wiring
    /// hidden: nothing to see nor to cut). Chosen where it is placed, not by its kind.
    #[serde(default)]
    pub cableado: Option<bool>,
    /// The clamp (a component of this ship, by its id) that holds it: it is cargo, joined to
    /// nothing but its clamp, and comes off whole when that lets go (`cargo`).
    #[serde(default)]
    pub anclaje: Option<String>,
    /// What it holds, where that is not what its kind says (a half-empty drum: `{ "lleno": 0.5 }`),
    /// or all of it for a plain shape (`contents`).
    #[serde(default)]
    pub contenido: Option<crate::contents::ContentsDef>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JointDef {
    /// "bisagra" (hinge) or "corredera" (slide).
    pub tipo: String,
    pub eje: [f32; 3],
    #[serde(default)]
    pub pivote: [f32; 3],
    /// Travel: degrees for hinges, metres for slides.
    pub limites: [f32; 2],
    #[serde(default)]
    pub inicial: f32,
    /// Joint it rides on (gear legs on their bay hinge...).
    #[serde(default)]
    pub padre: Option<String>,
    /// Parts it moves, by id; a trailing `*` takes every part whose id starts so.
    #[serde(default)]
    pub piezas: Vec<String>,
    /// Viscous damping (N·m·s or N·s/m).
    #[serde(default)]
    pub amortiguamiento: Option<f32>,
    #[serde(default)]
    pub nombre: Option<String>,
    /// It is a door, a ramp or a hatch: it opens `area` m² between two compartments ("vacio" for
    /// space) as it moves from its first limit to its second.
    #[serde(default)]
    pub abre: Option<OpeningDef>,
    /// It gives way to what pushes it: lying on the ground it is lifted as the hull comes down,
    /// and it does not hold the hull up (a ramp; by default, a closure hinged at its bottom).
    #[serde(default)]
    pub cede: bool,
    /// It goes where another joint is, times a ratio (`FollowDef`): the middle sections of a
    /// telescopic mast, the second leaf of a door that folds. Nothing drives it.
    #[serde(default)]
    pub sigue: Option<FollowDef>,
    /// A slide that is a sprung leg (a landing gear's strut): nothing drives it, what is under
    /// its foot presses it in against its spring and its damper (`SpringDef`). Its axis is the
    /// way the foot goes as it is pressed in, its limits its stroke.
    #[serde(default)]
    pub muelle: Option<SpringDef>,
}

/// A joint that follows another (`JointDef::sigue`): it is at `razon` times where `de` is.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FollowDef {
    pub de: String,
    pub razon: f32,
}

/// A leg's gas spring and damper (`lunar_core::structure::state::Spring`), by what it is built
/// to carry.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpringDef {
    /// The weight it carries standing ("30 kN"). Without it, its share of what the ship weighs
    /// where it is first set down.
    #[serde(default)]
    pub carga: Option<Q>,
    /// What it pushes with full out and pressed all the way in, as shares of that load (a
    /// quarter and three times by default, an aircraft's oleo: under its load it rests three
    /// quarters of the way in, and comes out when it is lifted off).
    #[serde(default)]
    pub precarga: Option<f32>,
    #[serde(default)]
    pub fin: Option<f32>,
    /// Its damper going in and coming out, as shares of what would just stop its load from
    /// swinging on it (0.45 and 1.1 by default: it gives going down, it does not throw back).
    #[serde(default)]
    pub freno: Option<[f32; 2]>,
    /// Grip of its foot (0.9 by default).
    #[serde(default)]
    pub agarre: Option<f32>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpeningDef {
    pub entre: [String; 2],
    pub area: f32,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NetDef {
    pub medio: String,
    #[serde(default)]
    pub nominal: Option<Q>,
    /// Junctions by name and where they are (ship frame).
    #[serde(default)]
    pub nodos: BTreeMap<String, [f32; 3]>,
    /// Runs of conduit between junctions, through waypoints.
    #[serde(default)]
    pub tramos: Vec<RunDef>,
    /// Breakers, valves, contactors, dampers between two junctions.
    #[serde(default)]
    pub interruptores: Vec<SwitchDef>,
    /// Kind of conduit for its runs and device stubs ("cable", "cable_grueso", "tubo",
    /// "tubo_alta", "conducto"...).
    #[serde(default)]
    pub conducto: Option<String>,
    /// Flow lost per unit of leak at nominal potential (rate).
    #[serde(default)]
    pub fuga: Option<Q>,
    /// Colour of its conduits (sRGB), by convention of the medium if not given.
    #[serde(default)]
    pub color: Option<[u8; 3]>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunDef {
    pub de: String,
    pub a: String,
    #[serde(default)]
    pub por: Vec<[f32; 3]>,
    #[serde(default)]
    pub tipo: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SwitchDef {
    pub id: String,
    pub de: String,
    pub a: String,
    /// Closed while this signal is on (default: `<id>`).
    #[serde(default)]
    pub senal: Option<String>,
    /// Measure the flow through it (breakers trip on it).
    #[serde(default)]
    pub medir: bool,
    /// The part it lives in: destroyed, it opens.
    #[serde(default)]
    pub pieza: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MachineDef {
    /// Model (empty: the component one).
    #[serde(default)]
    pub modelo: String,
    /// The part that is the machine (inline machines: their component).
    #[serde(default)]
    pub pieza: Option<String>,
    /// Ports by role: "net:node" (a stub conduit is laid from the part to the node).
    #[serde(default)]
    pub puertos: BTreeMap<String, String>,
    #[serde(default)]
    pub params: Map<String, Value>,
    #[serde(default)]
    pub ordenes: Map<String, Value>,
    #[serde(default)]
    pub fabricante: Option<String>,
    #[serde(default)]
    pub nombre: Option<String>,
    /// Thrust axis in the part frame (engines, thrusters): default −Z (exhaust +Z... the nozzle
    /// points along +Z, the push is −Z).
    #[serde(default)]
    pub empuje: Option<[f32; 3]>,
    /// The way it looks or fires, in the part frame, for what is no thruster: a radar's
    /// boresight, a cannon's bore, a dispenser's mouth (`tactical`).
    #[serde(default)]
    pub eje: Option<[f32; 3]>,
    /// Its exhaust, if it pushes (`exhaust`): the style it is drawn with and, where the shape of
    /// its parts does not say it, the mouth of its nozzle.
    #[serde(default)]
    pub chorro: Option<crate::exhaust::JetDef>,
    /// How much a violent decompression hurts it: 0 sealed and rugged ... 1 fragile (boiling
    /// liquids, cells, seals); `atmos` has the default.
    #[serde(default)]
    pub descompresion: Option<f32>,
    /// Wired with conduits of its own (default): false, it is connected without any (its wiring
    /// hidden: nothing to see nor to cut). Chosen where it is placed, not by its kind.
    #[serde(default)]
    pub cableado: Option<bool>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LightDef {
    /// sRGB colour and range (m).
    pub color: [u8; 3],
    pub alcance: f32,
    /// Intensity at full brightness.
    #[serde(default = "one_f")]
    pub intensidad: f32,
    /// Where it shines from and, for spots, toward (both in its component's frame; the kind turns
    /// them into its part's) and its cone (deg).
    #[serde(default)]
    pub en: [f32; 3],
    #[serde(default)]
    pub dir: Option<[f32; 3]>,
    #[serde(default)]
    pub cono: Option<f32>,
    /// A glowing lens (m, its radius) drawn when lit.
    #[serde(default)]
    pub lente: Option<f32>,
    /// Blinking (Hz, duty 0..1): beacons and strobes.
    #[serde(default)]
    pub parpadeo: Option<[f32; 2]>,
}

fn one_f() -> f32 {
    1.0
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompartmentDef {
    pub id: String,
    pub nombre: String,
    /// Boxes (min, max) it fills (ship frame).
    pub cajas: Vec<[[f32; 3]; 2]>,
    #[serde(default)]
    pub volumen: Option<f32>,
    /// Initial pressure (kPa as "70 kPa"); vacuum if absent.
    #[serde(default)]
    pub presion: Option<Q>,
    /// Parts that bound it toward vacuum, besides the hull's (ids, `*` prefixes).
    #[serde(default)]
    pub limites: Vec<String>,
    /// Valves and vents: an opening of `area` toward another compartment (or "vacio") while a
    /// signal is on.
    #[serde(default)]
    pub aberturas: Vec<VentDef>,
    /// Its standard systems (vent, equalising valves, interlocks, local lights; `rooms`): on
    /// unless false.
    #[serde(default)]
    pub automatico: Option<bool>,
    /// Where its own panel goes (made from what it has); none without.
    #[serde(default)]
    pub panel: Option<RoomPanelDef>,
    /// Make-up gas (REPRES.) from these networks, if nothing of the ship gives it yet.
    #[serde(default)]
    pub represurizar: Option<RepressDef>,
}

/// Where a compartment's own panel goes (`rooms`).
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoomPanelDef {
    pub en: [f32; 3],
    pub normal: [f32; 3],
    #[serde(default)]
    pub arriba: Option<[f32; 3]>,
    /// Power for its lamps and displays: "net:node".
    #[serde(default)]
    pub energia: Option<String>,
    #[serde(default)]
    pub cableado: Option<bool>,
}

/// The gas networks a compartment's make-up gas comes from.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepressDef {
    pub o2: String,
    pub n2: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VentDef {
    /// Open while this signal is on; a signal between 0 and 1 opens that share of it (a valve
    /// worked by a wheel).
    pub senal: String,
    pub area: f32,
    #[serde(default = "vacuum")]
    pub hacia: String,
    /// Where it lets the gas out (ship frame) and which way: there its rush is seen and felt. None:
    /// a valve with no place.
    #[serde(default)]
    pub en: Option<[f32; 3]>,
    #[serde(default)]
    pub normal: Option<[f32; 3]>,
}

fn vacuum() -> String {
    "vacio".into()
}

/// A panel of `assets/defs/panels` mounted on a part face.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PanelMount {
    pub id: String,
    pub panel: String,
    /// Wired with conduits of its own (default): false, it is connected without any (its wiring
    /// hidden: nothing to see nor to cut). Chosen where it is placed, not by its kind.
    #[serde(default)]
    pub cableado: Option<bool>,

    /// The part it is mounted on (it dies with it): a console's face. None: it brings its own box,
    /// as big as it is laid out, `fondo` m deep behind its face.
    #[serde(default)]
    pub pieza: Option<String>,
    #[serde(default)]
    pub fondo: Option<f32>,
    /// Controls of the panel left out here (and the covers that guarded them): the same panel
    /// outside without what only the inside may work.
    #[serde(default)]
    pub sin: Vec<String>,
    /// Centre of the panel face and its outward normal (ship frame), and its up.
    pub en: [f32; 3],
    pub normal: [f32; 3],
    #[serde(default)]
    pub arriba: Option<[f32; 3]>,
    /// Template variables (`$motor` → "motor_izq").
    #[serde(default)]
    pub prefijo: BTreeMap<String, String>,
    /// Power for its lamps and displays: "net:node".
    #[serde(default)]
    pub energia: Option<String>,
    /// Data for its controls (they command nothing without it): "net:node".
    #[serde(default)]
    pub datos: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SeatDef {
    pub id: String,
    pub nombre: String,
    /// The seat part, where the eyes go (ship frame) and the way they look (yaw deg, 0 = nose).
    pub pieza: String,
    pub ojos: [f32; 3],
    #[serde(default)]
    pub rumbo: f32,
    /// Where you stand when you get up: the first place tried.
    pub salida: [f32; 3],
    /// The ship's places to get off at (`ShipDef::bajadas`, by id) tried after it, in this
    /// order; none named: all of them, the nearest first.
    #[serde(default)]
    pub bajadas: Option<Vec<String>>,
    /// Keys to controls while seated.
    #[serde(default)]
    pub mandos: Vec<BindingDef>,
    /// The panels worked from this seat (their ids): every control of them must be within the
    /// hand's reach and in sight from it, and at arm's length from one of the seats that share
    /// it (`diag::seat_reach`).
    #[serde(default)]
    pub paneles: Vec<String>,
}

/// A place to get off at: where the feet go on leaving a seat. Each ship says its own (a step by
/// an open seat, an aisle, the ground beside it); the first with a floor under it and room for a
/// body standing is taken, so what is blocked (cargo, a rock, another ship) is passed over.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExitDef {
    pub id: String,
    /// Its middle, at the height the feet stand at (ship frame).
    pub en: [f32; 3],
    /// Its size across and along (m, ship x and z): every spot of it is tried; a single spot if
    /// none.
    #[serde(default)]
    pub zona: [f32; 2],
    /// The way one faces on getting off (yaw deg, 0 = nose); none: the way one was looking.
    #[serde(default)]
    pub rumbo: Option<f32>,
    /// How far under it the floor may be (m): a deck or a step is at its height; a place beside
    /// the ship reaches down to the ground it stands on.
    #[serde(default = "exit_drop")]
    pub baja: f32,
}

fn exit_drop() -> f32 {
    0.3
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BindingDef {
    /// A key name ("W", "Espacio", "Flecha arriba", "Raton X"...).
    pub tecla: String,
    /// Control id (`panel/control`) and what the key does: an axis value while held (`eje`,
    /// `valor`), a press, or a turn.
    pub mando: String,
    #[serde(default)]
    pub eje: Option<u8>,
    #[serde(default)]
    pub valor: Option<f64>,
    #[serde(default)]
    pub accion: Option<String>,
    /// What the key does, in words, for the list of controls (keys that say the same share a
    /// line: the two ways of an axis).
    #[serde(default)]
    pub ayuda: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FlightDef {
    /// Signals of the stick (pitch, roll), pedals (yaw), translation (x, y, z), main throttle.
    #[serde(default)]
    pub cabeceo: Option<String>,
    #[serde(default)]
    pub alabeo: Option<String>,
    #[serde(default)]
    pub guinada: Option<String>,
    #[serde(default)]
    pub traslacion: Option<[String; 3]>,
    #[serde(default)]
    pub acelerador: Option<String>,
    /// Modes: stabiliser, velocity hold, hover.
    #[serde(default)]
    pub estabilizador: Option<String>,
    #[serde(default)]
    pub mantener: Option<String>,
    /// Turn rates at full stick (deg/s) and the rate loop gain.
    #[serde(default)]
    pub giro: Option<[f32; 3]>,
    #[serde(default)]
    pub ganancia: Option<f32>,
    /// The machines that are thrusters (RCS) and main engines (ids, `*` prefixes).
    #[serde(default)]
    pub rcs: Vec<String>,
    #[serde(default)]
    pub motores: Vec<String>,
    /// The flight computer it needs (a machine id with a `.on` signal): without it, direct control.
    #[serde(default)]
    pub ordenador: Option<String>,
    /// The machines that are momentum wheels (`giroscopo`): the turn is asked of them first, of
    /// the thrusters what they cannot give. `descarga`: the signal that has the thrusters turn
    /// the hull against the wheels while these unload what they hold.
    #[serde(default)]
    pub ruedas: Vec<String>,
    #[serde(default)]
    pub descarga: Option<String>,
    /// What the pilot sets of the flight computer: the signal of its profile (0 fine: slow turns
    /// for docking and landing; 1 as built; 2 combat: fast turns, a stiffer hold) and the signal
    /// of the most its autopilot may ask of the engines (g; 0: all they give).
    #[serde(default)]
    pub perfil: Option<String>,
    #[serde(default)]
    pub limite: Option<String>,
}
