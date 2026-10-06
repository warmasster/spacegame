//! Pages of multi-function displays the ship makes from what it has (`"auto": ...` in a page):
//! - "energia": every battery as a tank (charge), every bus's voltage, the generators' power and
//!   a history of the main bus;
//! - "depositos": every propellant tank as a tank (level), every gas bottle;
//! - "masa": what the ship weighs (`mass`): its propellant as one tank, its mass with and
//!   without it, the change of speed it is worth, thrust over weight where it is, how far off
//!   its engines' line its centre of mass lies, and a history of the propellant;
//! - "aire": a plan of its compartments coloured by pressure and each one's pressure; and, where
//!   the ship moves air on purpose (`airworks`), its tank of recovered air, what its compressor is
//!   doing (from where to where, how fast) and how open each hand valve is;
//! - "casco": the same plan coloured by the hull round each, and how much it holds;
//! - "motores": each rocket engine's thrust on a dial, its state and wall temperature;
//! - "puertas": each closure, open or shut, and the pressure across it.
//! A page names what it is about; the rest follows from the ship. Zones colour it: green normal,
//! amber caution, red warning.
use crate::{components::Components, def::ShipDef};
use glam::Vec3;
use lunar_controls::{
    def::{Page, Zones},
    layout::PanelDef,
    mfd::{AreaDef, WidgetDef},
};
use lunar_signals::Q;

/// A machine of the ship as placed: its id and its model.
fn machines(def: &ShipDef, comps: &Components) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for c in &def.componentes {
        let kind = c.tipo.as_ref().and_then(|t| comps.get(t));
        let model = c.maquina.as_ref().map(|m| m.modelo.clone()).filter(|m| !m.is_empty()).or_else(|| kind.and_then(|k| k.maquina.as_ref()).map(|m| m.modelo.clone()));
        let Some(model) = model else { continue };
        let (count, _) = c.repetir.unwrap_or((1, [0.0; 3]));
        for k in 0..count.max(1) {
            let cid = if count > 1 { format!("{}#{k}", c.id) } else { c.id.clone() };
            out.push((cid.clone(), model.clone()));
            if c.espejo {
                let m = crate::components::mirror_name(&cid);
                out.push((if m == cid { format!("{cid}.e") } else { m }, model.clone()));
            }
        }
    }
    for (id, m) in &def.maquinas {
        out.push((id.clone(), m.modelo.clone()));
    }
    out
}

fn q(s: &str) -> Q {
    Q::S(s.to_string())
}

fn zones(red: Option<[&str; 2]>, amber: Option<[&str; 2]>, green: Option<[&str; 2]>) -> Option<Zones> {
    Some(Zones { roja: red.map(|z| z.map(q)), ambar: amber.map(|z| z.map(q)), verde: green.map(|z| z.map(q)) })
}

fn w(tipo: &str, senal: &str, rotulo: &str) -> WidgetDef {
    WidgetDef { tipo: tipo.into(), senal: Some(senal.into()), rotulo: Some(rotulo.into()), ..Default::default() }
}

fn short(id: &str) -> String {
    id.replace('_', " ").to_uppercase()
}

/// The plan of the compartments seen from above (nose up): each one's rectangle in 0..1.
fn plan(def: &ShipDef, signal: &str) -> Vec<AreaDef> {
    let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for c in &def.compartimentos {
        for [a, b] in &c.cajas {
            lo = lo.min(Vec3::from_array(*a));
            hi = hi.max(Vec3::from_array(*b));
        }
    }
    let size = (hi - lo).max(Vec3::splat(0.1));
    let mut out = Vec::new();
    for c in &def.compartimentos {
        for [a, b] in c.cajas.iter().take(1) {
            // x across (port on the left seen from above, nose up: +x to the left), z up the plan
            let (x0, x1) = ((hi.x - b[0]) / size.x, (hi.x - a[0]) / size.x);
            let (y0, y1) = ((a[2] - lo.z) / size.z, (b[2] - lo.z) / size.z);
            out.push(AreaDef { rotulo: c.nombre.to_uppercase(), senal: format!("{}.{signal}", c.id), rect: [x0 + 0.02, y0 + 0.01, (x1 - x0) - 0.04, (y1 - y0) - 0.02] });
        }
    }
    out
}

/// Fill the automatic pages of `p`'s multi-function displays.
pub fn fill(p: &mut PanelDef, def: &ShipDef, comps: &Components) -> Result<(), String> {
    let ms = machines(def, comps);
    let of = |model: &str| ms.iter().filter(|(_, m)| m == model).map(|(id, _)| id.clone()).collect::<Vec<_>>();
    for d in &mut p.mandos {
        if d.kind != "mfd" {
            continue;
        }
        for page in &mut d.paginas {
            let Some(what) = page.auto.clone() else {
                continue;
            };
            let mut el = Vec::new();
            match what.as_str() {
                "energia" => {
                    // the balance: what is made, what is spent, how long the batteries last
                    for (signal, name, decimals) in [("energia.generacion", "GENERA", 1), ("energia.consumo", "CONSUMO", 1), ("energia.autonomia", "QUEDAN", 0)] {
                        let mut v = w("valor", signal, name);
                        v.decimales = Some(decimals);
                        el.push(v);
                    }
                    for b in of("bateria").iter().take(2) {
                        let mut t = w("deposito", &format!("{b}.soc"), &short(b));
                        t.unidad = Some("%".into());
                        t.decimales = Some(0);
                        t.zonas = zones(Some(["0", "0.2"]), Some(["0.2", "0.4"]), Some(["0.4", "1"]));
                        el.push(t);
                    }
                    for (name, n) in &def.redes {
                        if n.medio != "electrico" {
                            continue;
                        }
                        for node in n.nodos.keys().filter(|k| k.starts_with("bus")).take(2) {
                            let mut v = w("valor", &format!("{name}.{node}"), &short(node));
                            v.zonas = zones(Some(["0 V", "22 V"]), Some(["22 V", "25 V"]), Some(["25 V", "32 V"]));
                            el.push(v);
                        }
                    }
                    for g in of("generador").iter().chain(of("apu").iter()).take(1) {
                        el.push(w("valor", &format!("{g}.p"), &short(g)));
                    }
                    if let Some((name, _)) = def.redes.iter().find(|(_, n)| n.medio == "electrico" && n.nodos.contains_key("bus_a")) {
                        let mut g = w("grafica", &format!("{name}.bus_a"), "BUS A");
                        g.escala = Some([q("18 V"), q("32 V")]);
                        g.zonas = zones(Some(["0 V", "22 V"]), None, Some(["24 V", "32 V"]));
                        g.ventana = Some(120.0);
                        el.push(g);
                    }
                }
                "depositos" => {
                    for t in of("deposito").iter().take(4) {
                        let mut d = w("deposito", &format!("{t}.nivel"), &short(t));
                        d.unidad = Some("%".into());
                        d.decimales = Some(0);
                        d.zonas = zones(Some(["0", "0.15"]), Some(["0.15", "0.3"]), Some(["0.3", "1"]));
                        el.push(d);
                    }
                    for b in of("botella").iter().take(4) {
                        let mut v = w("valor", &format!("{b}.masa"), &short(b));
                        v.decimales = Some(1);
                        el.push(v);
                    }
                }
                "masa" => {
                    // the same on any ship: the signals are the ship's own (`mass`)
                    let mut t = w("deposito", "nave.propelente_nivel", "PROPELENTE");
                    t.unidad = Some("%".into());
                    t.decimales = Some(0);
                    t.zonas = zones(Some(["0", "0.15"]), Some(["0.15", "0.3"]), Some(["0.3", "1"]));
                    el.push(t);
                    let mut g = w("grafica", "nave.propelente_nivel", "PROPELENTE");
                    g.unidad = Some("%".into());
                    g.escala = Some([q("0"), q("1")]);
                    g.zonas = zones(Some(["0", "0.15"]), Some(["0.15", "0.3"]), None);
                    g.ventana = Some(300.0);
                    el.push(g);
                    for (signal, name, unit, decimals) in [("nave.masa", "MASA", "t", 2), ("nave.masa_seca", "EN SECO", "t", 2), ("nave.propelente", "PROPELENTE", "kg", 0), ("nave.dv", "DELTA-V", "m/s", 0), ("nave.empuje_peso", "EMPUJE/PESO", "", 2), ("nave.centrado", "DESCENTRADO", "cm", 0)] {
                        let mut v = w("valor", signal, name);
                        v.unidad = Some(unit.into());
                        v.decimales = Some(decimals);
                        // (under its weight it does not lift; with little to spare, barely)
                        if signal == "nave.empuje_peso" {
                            v.zonas = zones(Some(["0", "1"]), Some(["1", "1.2"]), Some(["1.2", "100"]));
                        }
                        el.push(v);
                    }
                }
                "aire" => {
                    el.push(WidgetDef {
                        tipo: "mapa".into(),
                        rotulo: Some("PRESIÓN".into()),
                        unidad: Some("kPa".into()),
                        decimales: Some(0),
                        ocupa: Some([2, 3]),
                        zonas: zones(Some(["0 kPa", "30 kPa"]), Some(["30 kPa", "60 kPa"]), Some(["60 kPa", "110 kPa"])),
                        areas: plan(def, "p"),
                        ..Default::default()
                    });
                    // the air it moves on purpose: the tank, the compressor, the hand valves
                    let (tanks, pumps) = (of("deposito_aire"), of("compresor"));
                    let mut cells = 6;
                    if let Some(t) = tanks.first() {
                        let mut d = w("deposito", &format!("{t}.nivel"), "DEPÓSITO");
                        d.unidad = Some("%".into());
                        d.decimales = Some(0);
                        d.zonas = zones(None, Some(["0.95", "1.2"]), Some(["0", "0.95"]));
                        el.push(d);
                        cells += 2;
                    }
                    let mut lines: Vec<String> = Vec::new();
                    if let Some(c) = pumps.first() {
                        let places = crate::airworks::place_labels(def, comps, c).join("|");
                        lines.push(format!("COMPRESOR {{{c}.estado|PARADO|EN MARCHA|PASO LIBRE|EN SU LÍMITE|DESTINO LLENO|SIN ENERGÍA|DESTINO ABIERTO|MISMO SITIO}}"));
                        lines.push(format!("DE {{{c}.origen|{places}}} A {{{c}.destino|{places}}}"));
                    }
                    for v in def.trasvase.valvulas.iter().take(5 - lines.len()) {
                        let name = |id: &str| def.compartimentos.iter().find(|c| c.id == id).map_or_else(|| "VACÍO".to_string(), |c| c.nombre.to_uppercase());
                        lines.push(format!("V. {}-{} {{{}:0}}", name(&v.entre[0]), name(&v.entre[1]), crate::airworks::opening(&v.id)));
                    }
                    if !lines.is_empty() {
                        el.push(WidgetDef { tipo: "texto".into(), lineas: lines, ocupa: Some([2, 1]), ..Default::default() });
                        cells += 2;
                    }
                    if let Some(c) = pumps.first() {
                        let mut f = w("valor", &format!("{c}.caudal"), "CAUDAL");
                        f.decimales = Some(0);
                        el.push(f);
                        cells += 1;
                        if let Some(t) = tanks.first() {
                            let mut p = w("valor", &format!("{t}.p"), "P. DEPÓSITO");
                            p.decimales = Some(2);
                            el.push(p);
                            cells += 1;
                        }
                    }
                    // each compartment's pressure as a bar, as far as there is room left (the plan
                    // says it too)
                    for c in def.compartimentos.iter().take((12 - cells) / 2) {
                        let mut b = w("barra", &format!("{}.p", c.id), &c.nombre.to_uppercase());
                        b.escala = Some([q("0 kPa"), q("110 kPa")]);
                        b.unidad = Some("kPa".into());
                        b.decimales = Some(1);
                        b.zonas = zones(Some(["0 kPa", "50 kPa"]), Some(["50 kPa", "65 kPa"]), Some(["65 kPa", "110 kPa"]));
                        b.ocupa = Some([2, 1]);
                        el.push(b);
                    }
                }
                "casco" => {
                    el.push(WidgetDef {
                        tipo: "mapa".into(),
                        rotulo: Some("CASCO".into()),
                        unidad: Some("%".into()),
                        decimales: Some(0),
                        ocupa: Some([2, 3]),
                        zonas: zones(Some(["0", "0.5"]), Some(["0.5", "0.9"]), Some(["0.9", "1"])),
                        areas: plan(def, "integridad"),
                        ..Default::default()
                    });
                    for c in def.compartimentos.iter().take(3) {
                        let mut b = w("barra", &format!("{}.integridad", c.id), &c.nombre.to_uppercase());
                        b.unidad = Some("%".into());
                        b.decimales = Some(0);
                        b.zonas = zones(Some(["0", "0.5"]), Some(["0.5", "0.9"]), Some(["0.9", "1"]));
                        b.ocupa = Some([2, 1]);
                        el.push(b);
                    }
                }
                "motores" => {
                    for e in of("motor_cohete").iter().take(2) {
                        let mut r = w("reloj", &format!("{e}.empuje"), &short(e));
                        r.escala = Some([q("0 kN"), q("60 kN")]);
                        r.decimales = Some(1);
                        r.zonas = zones(Some(["50 kN", "60 kN"]), None, Some(["15 kN", "48 kN"]));
                        el.push(r);
                        let mut t = w("barra", &format!("{e}.t_pared"), "T PARED");
                        t.escala = Some([q("0 °C"), q("1000 °C")]);
                        t.decimales = Some(0);
                        t.zonas = zones(Some(["800 °C", "1000 °C"]), Some(["600 °C", "800 °C"]), Some(["0 °C", "600 °C"]));
                        el.push(t);
                    }
                    let lines: Vec<String> = of("motor_cohete").iter().take(4).map(|e| format!("{} {{{e}.estado|APAGADO|PURGA|IGNICIÓN|SUBIDA|EN MARCHA|BAJADA|FALLO}}  PC {{{e}.pc:2}}", short(e))).collect();
                    if !lines.is_empty() {
                        el.push(WidgetDef { tipo: "texto".into(), lineas: lines, ocupa: Some([4, 1]), ..Default::default() });
                    }
                }
                "puertas" => {
                    let lines: Vec<String> = def.cierres.iter().take(6).map(|c| format!("{} {{{}.cerrada|ABIERTA|CERRADA}}  ΔP {{{}.dp:1}}", c.nombre.clone().unwrap_or_else(|| c.id.clone()).to_uppercase(), c.id, c.id)).collect();
                    el.push(WidgetDef { tipo: "texto".into(), lineas: lines, ocupa: Some([4, 2]), ..Default::default() });
                }
                other => {
                    return Err(format!("{}: página automática desconocida '{other}' (energia, depositos, masa, aire, casco, motores, puertas)", d.id));
                }
            }
            // the big ones first (they pack), never more than the grid holds
            let span = |x: &WidgetDef| {
                x.ocupa.unwrap_or(match x.tipo.as_str() {
                    "grafica" | "mapa" => [2, 2],
                    "deposito" => [1, 2],
                    "texto" => [2, 1],
                    _ => [1, 1],
                })
            };
            el.sort_by_key(|x| {
                let [a, b] = span(x);
                std::cmp::Reverse((b, a))
            });
            let mut cells = 0;
            el.retain(|x| {
                let [a, b] = span(x);
                cells += a * b;
                cells <= 12
            });
            page.elementos = el;
        }
    }
    Ok(())
}

/// A page made by the ship.
pub fn auto(title: &str, button: &str, what: &str) -> Page {
    Page { titulo: title.into(), lineas: Vec::new(), elementos: Vec::new(), boton: Some(button.into()), auto: Some(what.into()) }
}
