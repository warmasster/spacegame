//! What every compartment gets without being asked (systems first): the ship's definition is
//! filled in before it is built, so any ship (or station) has them, and the data only says where
//! and what is different.
//!
//! For each compartment (unless `"automatico": false`):
//! - a vent to vacuum (`C.venteo`), and an alarm when it leaks;
//! - for each closure it has: its pressure difference (`D.dp`), an equalising valve to the room
//!   beyond (`D.igualar`, two rooms only; where the ship has a hand valve between the two —
//!   `airworks` — that is the one, and the panel shows how open it is), and the interlock that
//!   keeps it shut across more than `DOOR_DP` (toward vacuum: while the room is pressurised);
//! - make-up gas from the bottles (`C.repres`), if the data names the gas networks
//!   (`represurizar`) and nothing of the ship does it yet: an injector per gas in the room;
//! - its lights worked from inside (`C.luz`: off, as the master switch says, on);
//! - its hull's state (`C.integridad`, the ship publishes it);
//! - and, where the data puts one (`panel`), its own panel made from all that: atmosphere,
//!   pressure, each closure, lights, power, hull. Its controls say what they do.
use crate::def::{CompartmentDef, ComponentDef, InterlockDef, PanelMount, ShipDef, VentDef};
use glam::Vec3;
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// Areas (m²) of a room's vent to vacuum (per m³ of room, at least `VENT_MIN`: any room empties
/// in about fifteen seconds, without its pressure falling fast enough to be a decompression) and
/// of an equalising valve.
pub const VENT_PER_M3: f32 = 0.0015;
pub const VENT_MIN: f32 = 0.03;
pub const EQUALISE_AREA: f32 = 0.004;
/// What a room's make-up injectors give (kg/s per m³ of room, O₂ and N₂): from vacuum, any room
/// is at its pressure in about half a minute (`docs/TIEMPOS.md`: no real bottle regulator
/// gives this; nobody waits minutes for a cabin).
pub const REPRESS_PER_M3: [f32; 2] = [0.009, 0.019];
/// A door does not open across more than this (kPa); a room is "pressurised" for a closure to
/// vacuum over `PRESSURISED`.
pub const DOOR_DP: f64 = 5.0;
pub const PRESSURISED: f64 = 3.0;

fn in_room(c: &CompartmentDef, p: Vec3) -> bool {
    c.cajas.iter().any(|[a, b]| p.cmpge(Vec3::from_array(*a)).all() && p.cmple(Vec3::from_array(*b)).all())
}

/// A control bound to a signal is the same in any panel: its id in a panel is unique there.
fn add_input(derived: &mut BTreeMap<String, String>, order: &str, input: &str) {
    match derived.get_mut(order) {
        Some(e) => {
            // `toggle(a || b)` / `!toggle(a || b)` take one more input; anything else is or'ed
            if let Some(i) = e.find("toggle(") {
                let open = i + "toggle(".len();
                e.insert_str(open, &format!("{input} || "));
            } else {
                *e = format!("({e}) || {input}");
            }
        }
        None => {
            derived.insert(order.to_string(), format!("toggle({input})"));
        }
    }
}

/// `signal` on while `input` is (or'ed with whatever else the data already turns it on with).
fn add_or(derived: &mut BTreeMap<String, String>, signal: &str, input: &str) {
    let e = derived.entry(signal.to_string()).or_default();
    *e = if e.is_empty() { input.to_string() } else { format!("{e} || {input}") };
}

/// Fill in `def` (see the module); the panels it made, by id, as their text.
pub fn expand(def: &mut ShipDef) -> Result<BTreeMap<String, String>, String> {
    let mut panels = BTreeMap::new();
    let rooms: Vec<CompartmentDef> = def.compartimentos.clone();
    let room_index = |name: &str| rooms.iter().position(|c| c.id == name);
    // signals switches and keys write (not derived): the ones a room's own light switch may stand in for
    let derived_names: Vec<String> = def.derivadas.keys().cloned().collect();
    for (ci, c) in rooms.iter().enumerate() {
        if c.automatico == Some(false) {
            continue;
        }
        let id = c.id.clone();
        let mut controls: Vec<Value> = Vec::new();
        let mut groups: Vec<Value> = Vec::new();
        let panel_id = format!("sala_{id}");
        let ctl = |name: &str| format!("{panel_id}/{name}");
        // ---- vent to vacuum, and its alarm ----
        let vent = format!("{id}.venteo");
        if !def.compartimentos[ci].aberturas.iter().any(|v| v.senal == vent) {
            let area = (c.volumen.unwrap_or(20.0) * VENT_PER_M3).max(VENT_MIN);
            def.compartimentos[ci].aberturas.push(VentDef { senal: vent.clone(), area, hacia: "vacio".into(), en: None, normal: None });
        }
        // (unless the ship's own data already watches that leak: the black box says it once)
        let leak = format!("{id}.fuga > 2 kPa");
        if !def.alarmas.values().any(|cond| *cond == leak) {
            def.alarmas.entry(format!("Descompresión: {}", c.nombre.to_lowercase())).or_insert(leak);
        }
        // ---- make-up gas ----
        let repres = format!("{id}.repres");
        let mut can_repress = def.componentes.iter().any(|k| k.maquina.as_ref().is_some_and(|m| m.ordenes.values().any(|v| v.as_str() == Some(repres.as_str()))));
        if !can_repress && let Some(r) = &c.represurizar {
            // under the ceiling over its panel (out of the hand's way), else over the room's middle
            let top = c.cajas.iter().map(|b| b[1][1]).fold(f32::MIN, f32::max);
            let at = c.panel.as_ref().map_or_else(|| room_centre(c), |p| Vec3::from_array(p.en) + Vec3::from_array(p.normal).normalize_or_zero() * 0.2);
            let at = clamp_in(c, Vec3::new(at.x, top - 0.45, at.z));
            let volume = c.volumen.unwrap_or(20.0);
            for (gas, net, flow, set) in [("o2", &r.o2, format!("{:.3} kg/s", volume * REPRESS_PER_M3[0]), "21 kPa"), ("n2", &r.n2, format!("{:.3} kg/s", volume * REPRESS_PER_M3[1]), "49 kPa")] {
                let Some(n) = def.redes.get_mut(net) else {
                    return Err(format!("{id}: represurizar: no hay red '{net}'"));
                };
                let node = format!("sala_{id}");
                let p = at + Vec3::X * if gas == "o2" { 0.18 } else { -0.18 };
                n.nodos.entry(node.clone()).or_insert(p.to_array());
                // a run from the bottles' network to it, with all the others
                let from = n.nodos.keys().find(|k| **k != node).cloned().ok_or_else(|| format!("red {net} sin nodos"))?;
                n.tramos.push(crate::def::RunDef { de: from, a: node.clone(), por: Vec::new(), tipo: None });
                let m: crate::def::MachineDef = serde_json::from_value(json!({
                    "puertos": { "gas": format!("{net}:{node}") },
                    "ordenes": { "marcha": repres },
                    "params": { "gas": gas, "caudal": flow, "consigna": set, "sensor": format!("{id}.{gas}"), "permiso": format!("{id}.estanco") },
                    "nombre": format!("Represurización ({}) de {}", gas.to_uppercase(), c.nombre.to_lowercase())
                }))
                .map_err(|e| e.to_string())?;
                let mut k: ComponentDef = serde_json::from_value(json!({ "id": format!("repres_{gas}_{id}"), "tipo": "inyector", "en": p.to_array() })).map_err(|e| e.to_string())?;
                k.maquina = Some(m);
                def.componentes.push(k);
            }
            can_repress = true;
        }
        // ---- closures: pressure difference, equalising valve, interlock, a button here ----
        // (its id, its name, its equalising switch if it has one, whether it gives to vacuum, and
        // the hand valve through the same wall if the ship has one)
        let mut doors: Vec<(String, String, Option<String>, bool, Option<String>)> = Vec::new();
        for cl in &def.cierres {
            let Some([a, b]) = &cl.entre else { continue };
            let other = if *a == id {
                b.clone()
            } else if *b == id {
                a.clone()
            } else {
                continue;
            };
            let vacuum = room_index(&other).is_none();
            let dp = format!("{}.dp", cl.id);
            def.derivadas.entry(dp.clone()).or_insert_with(|| if vacuum { format!("{id}.p") } else { format!("abs({a}.p - {b}.p)") });
            // a hand valve through the same wall does the equalising: no switch of its own
            let valve = crate::airworks::valve_between(def, &id, &other).map(|v| v.id.clone());
            let equalise = (!vacuum && valve.is_none()).then(|| format!("{}.igualar", cl.id));
            if let Some(e) = &equalise
                && *a == id
                && !def.compartimentos[ci].aberturas.iter().any(|v| &v.senal == e)
            {
                def.compartimentos[ci].aberturas.push(VentDef { senal: e.clone(), area: EQUALISE_AREA, hacia: other.clone(), en: None, normal: None });
            }
            doors.push((cl.id.clone(), cl.nombre.clone().unwrap_or_else(|| cl.id.clone()), equalise, vacuum, valve));
        }
        // ---- lights inside: off / as the master says / on ----
        let luz = format!("{id}.luz");
        let mut lit = false;
        for k in &mut def.componentes {
            if !in_room(c, Vec3::from_array(k.en)) {
                continue;
            }
            let Some(m) = &mut k.maquina else { continue };
            let Some(Value::String(order)) = m.ordenes.get("encender").cloned() else {
                continue;
            };
            // emergency and other automatic lights keep their own logic
            if derived_names.contains(&order) || order.ends_with(".luces") {
                continue;
            }
            let local = format!("{id}.luces.{}", order.replace('.', "_"));
            def.derivadas.entry(local.clone()).or_insert_with(|| format!("{luz} > 1.5 || ({luz} > 0.5 && {order})"));
            m.ordenes.insert("encender".into(), Value::String(local));
            lit = true;
        }
        // ---- its panel ----
        let Some(place) = c.panel.clone() else {
            continue;
        };
        let power = place.energia.clone();
        let req = json!({ "energia": "1 W" });
        groups.push(json!({ "id": "aire", "titulo": "ATMÓSFERA", "auto": true, "prioridad": 0 }));
        controls.push(json!({ "id": "p", "kind": "aguja", "grupo": "aire", "senal": format!("{id}.p"), "escala": ["0 kPa", "110 kPa"], "unidad": "kPa", "rotulo": "PRESIÓN",
            "zonas": { "roja": ["0 kPa", "50 kPa"], "verde": ["65 kPa", "102 kPa"] }, "requiere": req,
            "nombre": format!("Presión de {}", c.nombre.to_lowercase()), "ayuda": "Presión del aire de este compartimento. Verde: habitable." }));
        controls.push(json!({ "id": "o2", "kind": "display7", "grupo": "aire", "senal": format!("{id}.o2"), "unidad": "kPa", "decimales": 1, "digitos": 4, "rotulo": "O2", "requiere": req,
            "nombre": "Oxígeno (presión parcial)", "ayuda": "Por debajo de 16 kPa falta oxígeno; lo normal son unos 21 kPa." }));
        controls.push(json!({ "id": "co2", "kind": "display7", "grupo": "aire", "senal": format!("{id}.co2"), "unidad": "kPa", "decimales": 2, "digitos": 4, "rotulo": "CO2", "requiere": req,
            "nombre": "Dióxido de carbono (presión parcial)", "ayuda": "Por encima de 1 kPa marea; el depurador lo retira." }));
        controls.push(json!({ "id": "t", "kind": "display7", "grupo": "aire", "senal": format!("{id}.t"), "unidad": "°C", "decimales": 1, "digitos": 4, "rotulo": "TEMP.", "requiere": req,
            "nombre": "Temperatura del aire", "ayuda": "Temperatura del aire de este compartimento." }));
        controls.push(json!({ "id": "fuga", "kind": "lampara", "grupo": "aire", "rotulo": "FUGA", "requiere": req,
            "reglas": [ { "si": format!("{id}.fuga > 0.5 kPa"), "color": "rojo", "parpadeo": 3.0 } ],
            "nombre": "Fuga", "ayuda": "Se enciende mientras el compartimento pierde presión." }));
        controls.push(json!({ "id": "estanco", "kind": "lampara", "grupo": "aire", "rotulo": "ESTANCO", "requiere": req,
            "reglas": [ { "si": format!("{id}.estanco > 0.5"), "color": "verde" } ],
            "nombre": "Estanco", "ayuda": "Verde: el compartimento está cerrado al vacío (puertas cerradas, sin brechas)." }));
        groups.push(json!({ "id": "presion", "titulo": "PRESIÓN", "auto": true, "prioridad": 1 }));
        if can_repress {
            let side = format!("{repres}_{panel_id}");
            add_or(&mut def.derivadas, &repres, &side);
            controls.push(json!({ "id": "repres", "kind": "interruptor", "grupo": "presion", "rotulo": "REPRES.", "posiciones": ["CERRADA", "ABIERTA"], "bind": { "senal": side },
                "nombre": format!("Represurizar {}", c.nombre.to_lowercase()), "ayuda": "Llena el compartimento desde las botellas de O₂ y N₂ mientras está estanco (con una fuga no inyecta)." }));
        }
        controls.push(json!({ "id": "tapa_venteo", "kind": "tapa", "grupo": "presion", "protege": ["venteo"], "precinto": true }));
        let side = format!("{vent}_{panel_id}");
        add_or(&mut def.derivadas, &vent, &side);
        controls.push(json!({ "id": "venteo", "kind": "interruptor", "grupo": "presion", "rotulo": "VENTEO", "posiciones": ["CERRADO", "ABIERTO"], "bind": { "senal": side },
            "nombre": format!("Ventear {}", c.nombre.to_lowercase()), "ayuda": "Vacía el aire del compartimento al exterior: para abrir hacia el vacío, o para igualar con un compartimento sin aire. Tapa precintada." }));
        for (k, (door, name, equalise, vacuum, valve)) in doors.iter().enumerate() {
            let g = format!("puerta{k}");
            groups.push(json!({ "id": g, "titulo": name.to_uppercase(), "auto": true, "prioridad": 2 + k as i32 }));
            let button = format!("{panel_id}.{door}");
            controls.push(json!({ "id": format!("abrir_{door}"), "kind": "pulsador", "grupo": g, "rotulo": "ABRIR/CERRAR", "modo": "momentaneo", "color": "verde",
                "luz": format!("{door}.cerrada < 0.5"), "bind": { "senal": button },
                "nombre": name, "ayuda": format!("Abre o cierra: {}. No abre con más de {DOOR_DP} kPa de diferencia (iguala antes).", name.to_lowercase()) }));
            controls.push(json!({ "id": format!("dp_{door}"), "kind": "display7", "grupo": g, "senal": format!("{door}.dp"), "unidad": "kPa", "decimales": 1, "digitos": 4, "rotulo": "ΔP", "requiere": req,
                "nombre": "Diferencia de presión", "ayuda": "Entre los dos lados de la puerta." }));
            if let Some(e) = equalise {
                // each side its own switch; the valve opens with either
                let side = format!("{e}_{id}");
                add_or(&mut def.derivadas, e, &side);
                controls.push(json!({ "id": format!("igualar_{door}"), "kind": "interruptor", "grupo": g, "rotulo": "IGUALAR", "posiciones": ["CERRADA", "ABIERTA"], "bind": { "senal": side },
                    "nombre": "Válvula de igualar", "ayuda": "Deja pasar el aire entre los dos lados hasta igualar presiones, para poder abrir." }));
            }
            if let Some(v) = valve {
                // the hand valve by the door: how open it is, read from here
                controls.push(json!({ "id": format!("valvula_{door}"), "kind": "display7", "grupo": g, "senal": crate::airworks::opening(v), "unidad": "%", "decimales": 0, "digitos": 3, "rotulo": "VÁLVULA", "requiere": req,
                    "nombre": if *vacuum { "Válvula de venteo" } else { "Válvula de igualación" },
                    "ayuda": if *vacuum { "Lo abierta que está la válvula de mano que da al vacío (su volante está junto a la salida)." } else { "Lo abierta que está la válvula de mano que iguala los dos lados (su volante está en el mamparo, junto a la puerta)." } }));
            }
            // the button opens it too, kept shut by the same interlock as every other one
            let order = def.cierres.iter().find(|x| &x.id == door).and_then(|x| x.orden.clone()).unwrap_or_else(|| format!("{door}.orden"));
            add_input(&mut def.derivadas, &order, &button);
            let vacuum = *vacuum;
            def.enclavamientos.push(InterlockDef {
                mando: ctl(&format!("abrir_{door}")),
                hacia: None,
                si: if vacuum { format!("{id}.p > {PRESSURISED} kPa && {door}.cerrada > 0.5") } else { format!("{door}.dp > {DOOR_DP} kPa && {door}.cerrada > 0.5") },
                motivo: if vacuum { format!("{} presurizada: ventéala antes", c.nombre) } else { "Diferencia de presión: iguala antes".into() },
            });
        }
        if lit {
            groups.push(json!({ "id": "luces", "titulo": "LUCES", "auto": true, "prioridad": 20 }));
            controls.push(json!({ "id": "luz", "kind": "selector", "grupo": "luces", "rotulo": "LUCES", "posiciones": ["APAG.", "AUTO", "ENC."], "defecto": 1, "bind": { "senal": luz },
                "nombre": format!("Luces de {}", c.nombre.to_lowercase()), "ayuda": "APAG.: apagadas aquí. AUTO: como mande el techo del puente. ENC.: encendidas aquí." }));
        }
        groups.push(json!({ "id": "estado", "titulo": "CASCO Y ENERGÍA", "auto": true, "prioridad": 30 }));
        controls.push(json!({ "id": "integridad", "kind": "barra", "grupo": "estado", "senal": format!("{id}.integridad"), "rotulo": "CASCO", "color": "verde", "requiere": req,
            "nombre": "Integridad del casco", "ayuda": "Lo que aguanta la chapa más dañada que encierra el compartimento (100 %: intacta)." }));
        controls.push(json!({ "id": "dano", "kind": "lampara", "grupo": "estado", "rotulo": "DAÑO", "requiere": req,
            "reglas": [ { "si": format!("{id}.integridad < 0.5"), "color": "rojo", "parpadeo": 2.0 }, { "si": format!("{id}.integridad < 0.9"), "color": "ambar" } ],
            "nombre": "Daño en el casco", "ayuda": "Ámbar: chapa dañada. Rojo: a punto de romper." }));
        if let Some(p) = &power
            && let Some((net, node)) = p.split_once(':')
        {
            controls.push(json!({ "id": "v", "kind": "display7", "grupo": "estado", "senal": format!("{net}.{node}"), "unidad": "V", "decimales": 1, "digitos": 4, "rotulo": "TENSIÓN", "requiere": req,
                "nombre": "Tensión de alimentación", "ayuda": "La del circuito que alimenta este panel." }));
        }
        let panel = json!({ "name": format!("Compartimento: {}", c.nombre), "tamano": ["0.95 m", "0.75 m"], "rejilla": [3, 3], "margen": "10 mm", "separacion": "6 mm", "grupos": groups, "mandos": controls });
        panels.insert(format!("auto/{panel_id}"), serde_json::to_string_pretty(&panel).map_err(|e| e.to_string())?);
        def.paneles.push(PanelMount {
            id: panel_id.clone(),
            panel: format!("auto/{panel_id}"),
            cableado: place.cableado,
            pieza: None,
            fondo: None,
            sin: Vec::new(),
            en: place.en,
            normal: place.normal,
            arriba: place.arriba,
            prefijo: BTreeMap::new(),
            energia: power,
            datos: None,
        });
    }
    priorities(def, &mut panels)?;
    Ok(panels)
}

/// The panel of circuit priorities, where the data puts it: for every breaker of every electrical
/// network ("brk.X"), a selector BAJA / NORMAL / ALTA writing `prio.X` (the ship gives every load on
/// that circuit that priority when power runs short; NORMAL leaves each its own).
fn priorities(def: &mut ShipDef, panels: &mut BTreeMap<String, String>) -> Result<(), String> {
    let Some(place) = def.prioridades.clone() else {
        return Ok(());
    };
    let mut groups: Vec<Value> = Vec::new();
    let mut controls: Vec<Value> = Vec::new();
    for (net, n) in &def.redes {
        if n.medio != "electrico" {
            continue;
        }
        for sw in &n.interruptores {
            let Some(circuit) = sw.id.strip_prefix("brk.") else {
                continue;
            };
            let group = format!("{net}_{}", sw.de);
            if !groups.iter().any(|g| g["id"] == group.as_str()) {
                groups.push(json!({ "id": group, "titulo": sw.de.replace('_', " ").to_uppercase(), "auto": true }));
            }
            controls.push(json!({ "id": circuit, "kind": "selector", "grupo": group, "rotulo": circuit.replace('_', " ").to_uppercase(),
                "posiciones": ["BAJA", "NORMAL", "ALTA"], "defecto": 1, "bind": { "senal": format!("prio.{circuit}") },
                "nombre": format!("Prioridad del circuito {}", circuit.replace('_', " ")),
                "ayuda": "Con poca energía se corta primero lo de prioridad BAJA y lo último lo de ALTA. NORMAL: cada aparato la suya." }));
        }
    }
    if controls.is_empty() {
        return Ok(());
    }
    let panel = json!({ "name": "Prioridades de los circuitos", "tamano": ["0.95 m", "0.7 m"], "rejilla": [3, 2], "margen": "10 mm", "separacion": "6 mm", "grupos": groups, "mandos": controls });
    panels.insert("auto/prioridades".into(), serde_json::to_string_pretty(&panel).map_err(|e| e.to_string())?);
    def.paneles.push(PanelMount {
        id: "prioridades".into(),
        panel: "auto/prioridades".into(),
        cableado: place.cableado,
        pieza: None,
        fondo: None,
        sin: Vec::new(),
        en: place.en,
        normal: place.normal,
        arriba: place.arriba,
        prefijo: BTreeMap::new(),
        energia: place.energia,
        datos: None,
    });
    Ok(())
}

fn room_centre(c: &CompartmentDef) -> Vec3 {
    let (lo, hi) = c.cajas.iter().fold((Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)), |(l, h), [a, b]| (l.min(Vec3::from_array(*a)), h.max(Vec3::from_array(*b))));
    (lo + hi) * 0.5
}

/// `p` kept inside the room's first box (a little in from its sides).
fn clamp_in(c: &CompartmentDef, p: Vec3) -> Vec3 {
    let [a, b] = c.cajas[0];
    p.clamp(Vec3::from_array(a) + 0.2, Vec3::from_array(b) - 0.2)
}
