//! Air moved on purpose (`docs/AIRE.md`), from what the ship's data says of it (`trasvase`) to
//! what the ship is built with. Like `rooms`, the definition is filled in before it is built, so
//! any ship has it by saying where:
//!
//! - **a hand valve** through a wall (`valvulas`): its component (`valvula_igualacion` between two
//!   compartments, `valvula_venteo` out to space; the valve is the piece in the wall, with a plate
//!   on each side that is a compartment), an opening between its two places as wide as its wheel
//!   is turned (`<id>.apertura`, 0..1), the pressure across it (`<id>.dp`), and on each plate a
//!   panel: the wheel and a gauge of that pressure. The wheels of both sides are one mechanism;
//! - **a compressor's panel** (`compresores`), made from what its manifold reaches (`lugares`):
//!   a selector of where it draws from and one of where it delivers, its start button under a
//!   guard, a lamp for each thing it may be doing, the pressures it works between, its flow; the
//!   interlock that will not start it from a place into the same place; and what it sounds with
//!   (`sonido.compresor`).
use crate::{
    components::Components,
    def::{AirValveDef, ComponentDef, InterlockDef, PanelMount, ShipDef, VentDef},
};
use glam::{EulerRot, Mat3, Quat, Vec3};
use lunar_core::structure::catalog::ShapeDef;
use lunar_signals::Q;
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// The component a hand valve is, by where it leads.
pub const VALVE: &str = "valvula_igualacion";
pub const VENT_VALVE: &str = "valvula_venteo";
/// Bore of a hand valve wide open (m), by where it leads, when its data gives none: between
/// compartments as the equalising valves always were; out to space a little more (it works on
/// the last of the air, at next to no pressure).
pub const BORE: f32 = 0.07;
pub const VENT_BORE: f32 = 0.1;
/// A door does not open across more than this (kPa): a gauge's green.
const DOOR_DP: f64 = crate::rooms::DOOR_DP;

fn vacuum(name: &str) -> bool {
    name == "vacio" || name == "vacío"
}

/// The hand valve that joins compartments `a` and `b` (either way round; `b` may be space), if
/// the ship has one.
pub fn valve_between<'a>(def: &'a ShipDef, a: &str, b: &str) -> Option<&'a AirValveDef> {
    let same = |x: &str, y: &str| x == y || (vacuum(x) && vacuum(y));
    def.trasvase.valvulas.iter().find(|v| (same(&v.entre[0], a) && same(&v.entre[1], b)) || (same(&v.entre[0], b) && same(&v.entre[1], a)))
}

/// The signal that says how open valve `id` is (0 shut .. 1 wide open).
pub fn opening(id: &str) -> String {
    format!("{id}.apertura")
}

/// A component turned so that its +z looks along `n`, its +y as far up as that leaves it: its
/// Euler angles (degrees, as `ComponentDef::rot`) and the turn itself.
fn facing(n: Vec3) -> ([f32; 3], Quat) {
    let z = n.normalize_or(Vec3::Z);
    let x = Vec3::Y.cross(z).normalize_or(Vec3::X);
    let y = z.cross(x);
    let q = Quat::from_mat3(&Mat3::from_cols(x, y, z));
    let (a, b, c) = q.to_euler(EulerRot::XYZ);
    ([a.to_degrees(), b.to_degrees(), c.to_degrees()], q)
}

/// A parameter of the machine of component `id`: what the ship says where it places it, else
/// what its kind brings.
fn param<'a>(def: &'a ShipDef, comps: &'a Components, id: &str, key: &str) -> Option<&'a Value> {
    let c = def.componentes.iter().find(|c| c.id == id)?;
    c.maquina.as_ref().and_then(|m| m.params.get(key)).or_else(|| c.tipo.as_ref().and_then(|t| comps.get(t)).and_then(|k| k.maquina.as_ref()).and_then(|m| m.params.get(key)))
}

fn quantity(v: Option<&Value>, unit: &str, or: f64) -> f64 {
    v.and_then(|v| serde_json::from_value::<Q>(v.clone()).ok()).and_then(|q| q.si_as(unit).ok()).unwrap_or(or)
}

/// The model of the machine of component `id`.
fn model<'a>(def: &'a ShipDef, comps: &'a Components, id: &str) -> Option<&'a str> {
    let c = def.componentes.iter().find(|c| c.id == id)?;
    c.maquina.as_ref().map(|m| m.modelo.as_str()).filter(|m| !m.is_empty()).or_else(|| c.tipo.as_ref().and_then(|t| comps.get(t)).and_then(|k| k.maquina.as_ref()).map(|m| m.modelo.as_str()))
}

/// Fill in `def` (see the module); the panels it made, by id, as their text.
pub fn expand(def: &mut ShipDef, comps: &Components) -> Result<BTreeMap<String, String>, String> {
    let mut panels = BTreeMap::new();
    valves(def, comps, &mut panels)?;
    compressors(def, comps, &mut panels)?;
    Ok(panels)
}

fn valves(def: &mut ShipDef, comps: &Components, panels: &mut BTreeMap<String, String>) -> Result<(), String> {
    let list = def.trasvase.valvulas.clone();
    for v in &list {
        let id = &v.id;
        let names: Vec<String> = def.compartimentos.iter().map(|c| c.nombre.clone()).collect();
        let room = |name: &str| def.compartimentos.iter().position(|c| c.id == name);
        let a = room(&v.entre[0]).ok_or_else(|| format!("válvula {id}: '{}' no es un compartimento (el primero de 'entre' lo es siempre)", v.entre[0]))?;
        let b = if vacuum(&v.entre[1]) { None } else { Some(room(&v.entre[1]).ok_or_else(|| format!("válvula {id}: compartimento desconocido '{}'", v.entre[1]))?) };
        let name_of = |c: usize| names[c].clone();
        let kind_id = v.tipo.clone().unwrap_or_else(|| if b.is_some() { VALVE } else { VENT_VALVE }.to_string());
        let kind = comps.get(&kind_id).ok_or_else(|| format!("válvula {id}: no hay componente '{kind_id}'"))?;
        let at = Vec3::from_array(v.en);
        let (rot, turn) = facing(Vec3::from_array(v.normal));
        let n = turn * Vec3::Z;
        // ---- the valve itself, in its wall ----
        let k: ComponentDef = serde_json::from_value(json!({ "id": id, "tipo": kind_id, "en": v.en, "rot": rot })).map_err(|e| e.to_string())?;
        def.componentes.push(k);
        // ---- the opening it is, where its gas comes out ----
        let bore = match &v.paso {
            Some(q) => q.si_as("m").map_err(|e| format!("válvula {id}: paso: {}", e.0))? as f32,
            None if b.is_some() => BORE,
            None => VENT_BORE,
        };
        let mouth = at + turn * kind.anclajes.get("boca").map_or(Vec3::ZERO, |p| Vec3::from_array(*p));
        let signal = opening(id);
        def.compartimentos[a].aberturas.push(VentDef { senal: signal.clone(), area: std::f32::consts::PI * 0.25 * bore * bore, hacia: v.entre[1].clone(), en: Some(mouth.to_array()), normal: Some(n.to_array()) });
        let dp = format!("{id}.dp");
        def.derivadas.entry(dp.clone()).or_insert_with(|| match b {
            Some(_) => format!("abs({}.p - {}.p)", v.entre[0], v.entre[1]),
            None => format!("{}.p", v.entre[0]),
        });
        // ---- a panel on each side that is a compartment: its wheel, the pressure across it ----
        let title = v.nombre.clone().unwrap_or_else(|| match b {
            Some(b) => format!("Válvula de igualación {} – {}", name_of(a), name_of(b)),
            None => format!("Válvula de venteo de {}", name_of(a).to_lowercase()),
        });
        for (side, here, beyond, out) in [("a", Some(a), b, -Vec3::Z), ("b", b, Some(a), Vec3::Z)] {
            let Some(here) = here else { continue };
            let plate_id = format!("placa_{side}");
            let plate = kind.piezas.iter().find(|p| p.id == plate_id).ok_or_else(|| format!("válvula {id}: el componente '{kind_id}' no tiene la pieza '{plate_id}' (la placa de ese lado)"))?;
            let ShapeDef::Box { size } = &plate.forma else { return Err(format!("válvula {id}: la placa '{plate_id}' de '{kind_id}' ha de ser una caja")) };
            // the plate's outer face, where its panel goes
            let face = at + turn * (Vec3::from_array(plate.en) + out * (size[2] * 0.5));
            let panel_id = format!("{id}_{side}");
            let (to, wheel_help, colour) = match beyond {
                Some(c) => (
                    format!("CON {}", name_of(c).to_uppercase()),
                    format!(
                        "Volante de la válvula que comunica {} y {} a través del mamparo. Gíralo con la rueda del ratón (Mayús: un cuarto de golpe; Ctrl: fino). El aire pasa del lado con más presión al otro, más deprisa cuanto más abierta. Tiene un volante a cada lado, en el mismo eje. Ciérrala al terminar.",
                        name_of(here).to_lowercase(),
                        name_of(c).to_lowercase()
                    ),
                    "amarillo",
                ),
                None => (
                    "AL VACÍO".to_string(),
                    format!(
                        "Volante de la válvula que da de {} al exterior: deja salir el aire que el compresor ya no saca. Gíralo con la rueda del ratón (Mayús: un cuarto de golpe; Ctrl: fino). Ciérrala antes de volver a dar presión.",
                        name_of(here).to_lowercase()
                    ),
                    "rojo",
                ),
            };
            let gauge = match beyond {
                Some(_) => json!({ "id": "dp", "kind": "aguja", "senal": dp, "escala": ["0 kPa", "110 kPa"], "unidad": "kPa", "rotulo": "ΔP",
                    "zonas": { "verde": ["0 kPa", format!("{DOOR_DP} kPa")] },
                    "nombre": "Diferencia de presión", "ayuda": format!("Lo que hay de más a un lado de la válvula que al otro. Por debajo de {DOOR_DP} kPa la puerta abre.") }),
                None => json!({ "id": "dp", "kind": "aguja", "senal": dp, "escala": ["0 kPa", "110 kPa"], "unidad": "kPa", "rotulo": "PRESIÓN",
                    "zonas": { "verde": ["0 kPa", format!("{} kPa", crate::rooms::PRESSURISED)] },
                    "nombre": format!("Presión de {}", name_of(here).to_lowercase()), "ayuda": "La que queda dentro, contra el vacío de fuera. En verde ya se puede abrir al exterior." }),
            };
            let wheel = json!({ "id": "volante", "kind": "volante", "rotulo": to, "unidad": "%", "color": colour, "bind": { "senal": signal, "comun": true },
                "nombre": title, "ayuda": wheel_help });
            let room_w = (size[0] - 0.02).max(0.1);
            let room_h = (size[1] - 0.02).max(0.1);
            let panel = json!({ "name": title, "tamano": [format!("{room_w} m"), format!("{room_h} m")], "rejilla": [1, 2], "margen": "8 mm", "separacion": "4 mm", "mandos": [gauge, wheel] });
            panels.insert(format!("auto/{panel_id}"), serde_json::to_string_pretty(&panel).map_err(|e| e.to_string())?);
            def.paneles.push(PanelMount {
                id: panel_id.clone(),
                panel: format!("auto/{panel_id}"),
                cableado: None,
                pieza: Some(format!("{id}.{plate_id}")),
                fondo: None,
                sin: Vec::new(),
                en: face.to_array(),
                normal: (turn * out).to_array(),
                arriba: Some((turn * Vec3::Y).to_array()),
                prefijo: BTreeMap::new(),
                energia: None,
                datos: None,
            });
        }
    }
    Ok(())
}

/// How a place reads on a selector: a compartment by its name, a tank as what it is.
fn place_label(def: &ShipDef, name: &str, tanks: usize, nth: &mut usize) -> String {
    match def.compartimentos.iter().find(|c| c.id == name) {
        Some(c) => c.nombre.to_uppercase().chars().take(6).collect(),
        None => {
            *nth += 1;
            if tanks > 1 { format!("DEP. {nth}") } else { "DEPÓS.".to_string() }
        }
    }
}

/// How the places of compressor `id` read on its selectors, in their order (none: it has none).
pub fn place_labels(def: &ShipDef, comps: &Components, id: &str) -> Vec<String> {
    let places: Vec<&str> = match param(def, comps, id, "lugares") {
        Some(Value::Array(a)) => a.iter().filter_map(Value::as_str).collect(),
        _ => Vec::new(),
    };
    let tanks = places.iter().filter(|p| !def.compartimentos.iter().any(|c| c.id == **p)).count();
    let mut nth = 0;
    places.iter().map(|p| place_label(def, p, tanks, &mut nth)).collect()
}

fn compressors(def: &mut ShipDef, comps: &Components, panels: &mut BTreeMap<String, String>) -> Result<(), String> {
    let list = def.trasvase.compresores.clone();
    let mut running: Vec<String> = Vec::new();
    for k in &list {
        let id = &k.id;
        if model(def, comps, id) != Some("compresor") {
            return Err(format!("trasvase: '{id}' no es un compresor de esta nave (un componente con máquina \"compresor\")"));
        }
        let places: Vec<String> = match param(def, comps, id, "lugares") {
            Some(Value::Array(a)) => a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect(),
            _ => return Err(format!("compresor {id}: faltan sus 'lugares' (los compartimentos y depósitos entre los que mueve el aire)")),
        };
        let is_room = |name: &str| def.compartimentos.iter().any(|c| c.id == name);
        for p in &places {
            if !is_room(p) && model(def, comps, p) != Some("deposito_aire") {
                return Err(format!("compresor {id}: '{p}' no es un compartimento ni un depósito de aire"));
            }
        }
        let tanks: Vec<&String> = places.iter().filter(|p| !is_room(p)).collect();
        let labels = place_labels(def, comps, id);
        // it starts set to draw from its first compartment into its first tank (or its second place)
        let from = places.iter().position(|p| is_room(p)).unwrap_or(0);
        let to = places.iter().position(|p| !is_room(p)).unwrap_or(usize::from(from == 0));
        let p_min = quantity(param(def, comps, id, "presion_minima"), "Pa", 5e3);
        let panel_id = format!("panel_{id}");
        let req = json!({ "energia": "1 W" });
        let order = |role: &str| format!("{id}.{role}");
        let mut controls = vec![
            json!({ "id": "origen", "kind": "selector", "grupo": "linea", "rotulo": "ORIGEN", "posiciones": labels, "defecto": from, "bind": { "senal": order("origen") },
                "nombre": "De dónde saca el aire", "ayuda": "El compartimento (o el depósito) del que el compresor toma el aire. Gíralo con la rueda del ratón." }),
            json!({ "id": "destino", "kind": "selector", "grupo": "linea", "rotulo": "DESTINO", "posiciones": labels, "defecto": to, "bind": { "senal": order("destino") },
                "nombre": "Adónde lo manda", "ayuda": "Dónde deja el aire que saca: el depósito de aire recuperado, u otro compartimento. Del depósito a un compartimento el aire baja solo y el motor no gira." }),
            json!({ "id": "tapa_marcha", "kind": "tapa", "grupo": "marcha", "protege": ["marcha"] }),
            json!({ "id": "marcha", "kind": "pulsador", "grupo": "marcha", "modo": "enclavado", "color": "verde", "rotulo": "MARCHA", "posiciones": ["PARADO", "EN MARCHA"], "tamano": [30, 30],
                "luz": format!("{} > 0.5", order("marcha")), "bind": { "senal": order("marcha") },
                "nombre": "Marcha del compresor", "ayuda": "Arranca y para el compresor (levanta la tapa). Saca el aire del ORIGEN y lo mete en el DESTINO; se para solo cuando el origen llega a su límite o el destino se llena, y sigue si deja de estarlo." }),
            json!({ "id": "l_marcha", "kind": "lampara", "grupo": "estado", "rotulo": "EN MARCHA", "requiere": req,
                "reglas": [ { "si": format!("{id}.estado == 1"), "color": "verde" }, { "si": format!("{id}.estado == 2"), "color": "verde", "parpadeo": 2.0 } ],
                "nombre": "En marcha", "ayuda": "Fija: el compresor está sacando aire. Parpadea: el aire pasa solo (del depósito a un compartimento), sin motor." }),
            json!({ "id": "l_limite", "kind": "lampara", "grupo": "estado", "rotulo": "LÍMITE", "requiere": req,
                "reglas": [ { "si": format!("{id}.estado == 3"), "color": "ambar" } ],
                "nombre": "Origen en su límite", "ayuda": format!("El origen ha bajado hasta donde el compresor ya no saca más (unos {:.0} kPa). Lo que queda se ventea por la válvula de venteo.", p_min / 1000.0) }),
            json!({ "id": "l_lleno", "kind": "lampara", "grupo": "estado", "rotulo": "LLENO", "requiere": req,
                "reglas": [ { "si": format!("{id}.estado == 4"), "color": "ambar" } ],
                "nombre": "Destino lleno", "ayuda": "El depósito está a su presión máxima, o el compartimento de destino ha llegado a la suya. Elige otro destino." }),
            json!({ "id": "l_abierto", "kind": "lampara", "grupo": "estado", "rotulo": "ABIERTO", "requiere": req,
                "reglas": [ { "si": format!("{id}.estado == 6"), "color": "rojo", "parpadeo": 2.0 } ],
                "nombre": "Destino abierto", "ayuda": "El compartimento de destino no está estanco (una puerta al vacío, un venteo, una brecha): no se le manda aire." }),
            json!({ "id": "l_energia", "kind": "lampara", "grupo": "estado", "rotulo": "SIN ENERGÍA", "requiere": req,
                "reglas": [ { "si": format!("{id}.estado == 5"), "color": "rojo" } ],
                "nombre": "Sin energía", "ayuda": "Al compresor no le llega corriente: mira su disyuntor y la tensión del bus." }),
            json!({ "id": "p_origen", "kind": "aguja", "grupo": "presiones", "senal": format!("{id}.p_origen"), "escala": ["0 kPa", "110 kPa"], "unidad": "kPa", "rotulo": "ORIGEN", "requiere": req,
                "zonas": { "ambar": ["0 kPa", format!("{} kPa", p_min / 1000.0)] },
                "nombre": "Presión del origen", "ayuda": "La del sitio del que se saca el aire. En ámbar el compresor ya no saca más." }),
        ];
        if let Some(tank) = tanks.first() {
            let p_max = quantity(param(def, comps, tank, "presion_max"), "Pa", 12e6) / 1e6;
            controls.push(json!({ "id": "p_deposito", "kind": "aguja", "grupo": "presiones", "senal": format!("{tank}.p"), "escala": ["0 MPa", format!("{} MPa", p_max * 1.1)], "unidad": "MPa", "rotulo": "DEPÓSITO", "requiere": req,
                "zonas": { "verde": ["0 MPa", format!("{} MPa", p_max * 0.95)], "roja": [format!("{} MPa", p_max * 0.95), format!("{} MPa", p_max * 1.1)] },
                "nombre": "Presión del depósito", "ayuda": format!("La del depósito de aire recuperado. Lleno a {p_max:.0} MPa.") }));
        }
        controls.push(json!({ "id": "p_destino", "kind": "display7", "grupo": "presiones", "senal": format!("{id}.p_destino"), "unidad": "kPa", "decimales": 0, "digitos": 5, "rotulo": "DESTINO", "requiere": req,
            "nombre": "Presión del destino", "ayuda": "La del sitio al que se manda el aire." }));
        controls.push(json!({ "id": "caudal", "kind": "display7", "grupo": "presiones", "senal": format!("{id}.caudal"), "unidad": "g/s", "decimales": 0, "digitos": 4, "rotulo": "CAUDAL", "requiere": req,
            "nombre": "Caudal", "ayuda": "El aire que pasa ahora, en gramos por segundo. Baja según baja la presión del origen y sube la del destino." }));
        let groups = json!([
            { "id": "linea", "titulo": "DE DÓNDE A DÓNDE", "auto": true, "prioridad": 0 },
            { "id": "marcha", "titulo": "COMPRESOR", "auto": true, "prioridad": 1 },
            { "id": "presiones", "titulo": "PRESIONES Y CAUDAL", "auto": true, "prioridad": 2 },
            { "id": "estado", "titulo": "ESTADO", "auto": true, "prioridad": 3 }
        ]);
        let name = def.componentes.iter().find(|c| &c.id == id).and_then(|c| c.tipo.as_ref()).and_then(|t| comps.get(t)).map_or_else(|| "Compresor de aire".to_string(), |kind| kind.nombre.clone());
        let panel = json!({ "name": name, "tamano": ["0.84 m", "0.66 m"], "rejilla": [2, 2], "margen": "10 mm", "separacion": "6 mm", "grupos": groups, "mandos": controls });
        panels.insert(format!("auto/{panel_id}"), serde_json::to_string_pretty(&panel).map_err(|e| e.to_string())?);
        def.paneles.push(PanelMount {
            id: panel_id.clone(),
            panel: format!("auto/{panel_id}"),
            cableado: k.panel.cableado,
            pieza: None,
            fondo: None,
            sin: Vec::new(),
            en: k.panel.en,
            normal: k.panel.normal,
            arriba: k.panel.arriba,
            prefijo: BTreeMap::new(),
            energia: k.panel.energia.clone(),
            datos: None,
        });
        def.enclavamientos.push(InterlockDef { mando: format!("{panel_id}/marcha"), hacia: Some(1.0), si: format!("{} == {}", order("origen"), order("destino")), motivo: "Origen y destino son el mismo: elige otro".into() });
        running.push(format!("{id}.giro"));
    }
    // what they sound with: the loop `compresor`, as loud as the fastest of them turns
    if !running.is_empty() {
        def.derivadas.entry("sonido.compresor".into()).or_insert_with(|| if running.len() == 1 { running[0].clone() } else { format!("max({})", running.join(", ")) });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_valve_faces_the_way_its_wall_does() {
        for n in [Vec3::Z, Vec3::NEG_Z, Vec3::X, Vec3::NEG_X, Vec3::new(0.6, 0.0, 0.8)] {
            let (rot, turn) = facing(n);
            assert!((turn * Vec3::Z - n).length() < 1e-5 && (turn * Vec3::Y - Vec3::Y).length() < 1e-5, "{n:?}: {rot:?}");
            // (as the kind builder turns a component: Euler XYZ, degrees)
            let again = crate::components::euler(rot);
            assert!((again * Vec3::Z - n).length() < 1e-4, "{n:?}: {rot:?}");
        }
    }
}
