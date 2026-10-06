//! What a component holds, from its data (`"contenido"`): a substance of the registry
//! (`assets/defs/sustancias.jsonc`), how much of it at most and how full it is as built. A kind
//! of component says it once; where one is placed it may say otherwise (a half-empty drum,
//! another liquid in the same drum). Nothing else about it is typed anywhere: what its labels
//! write (`{sustancia}`, `{capacidad}`), what a scanner reads and what it weighs all come from
//! this (`lunar_core::structure::contents` keeps how much is left, and weighs it).
//!
//! The room is the pieces': each one's shape as it is defined less its walls (`"hueco"`); what
//! is declared must fit. Several pieces (`"piezas"`: the layers of sacks of a pallet) share it
//! by their room.
//!
//! What a machine counts (a tank's propellant, a bottle's gas) is the same thing: `"nivel"`
//! names the machine's signal that says how many kg are left, and what the component weighs
//! follows it. So that the two can never disagree about how much there is room for, the
//! container and its machine are tied by the names of the machine's own parameters:
//! `"cabida"` (kg at most), `"inicial"` (kg as built) and `"volumen"` (m³ inside). Whatever
//! says it — the machine's data, the container's, or nothing at all: then it is what fits —
//! the other is given the same figure (`Fitted::machine`), and a machine that says more than
//! fits in its tank is refused when the ship is put together, with what does fit.
use crate::{components::ComponentKind, def::ComponentDef};
use lunar_core::structure::{
    catalog::{BurstDef, Catalog, ShapeDef},
    contents::{ContainerDef, SubstanceDef, figure, text},
};
use lunar_signals::{Q, units};
use serde::Deserialize;
use serde_json::{Map, Value};

/// `"contenido"` on a component kind, or on a component placed (which then says only what is
/// different there).
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContentsDef {
    /// The substance, by its id in the registry.
    #[serde(default)]
    pub sustancia: Option<String>,
    /// How much at most, by volume or by mass ("170 L", "600 kg"; a bare number is kg). Without
    /// it, what fits.
    #[serde(default)]
    pub capacidad: Option<Q>,
    /// The share of that it holds as built (1 by default: full).
    #[serde(default)]
    pub lleno: Option<f32>,
    /// The pieces of the component that hold it, by id; by default the first.
    #[serde(default)]
    pub piezas: Option<Vec<String>>,
    /// The signal of the component that says how many kg it holds (`<id>.<nivel>`: a tank's
    /// machine): what it weighs follows it.
    #[serde(default)]
    pub nivel: Option<String>,
    /// The parameter of its machine that says how many kg it holds at most (`"capacidad"` of
    /// a tank, of a bottle): the two are one figure.
    #[serde(default)]
    pub cabida: Option<String>,
    /// The parameter of its machine that says how many kg it starts with (`"masa_inicial"`).
    #[serde(default)]
    pub inicial: Option<String>,
    /// The parameter of its machine that says how many m³ it is inside (`"volumen"` of a
    /// vessel that keeps a gas by its pressure).
    #[serde(default)]
    pub volumen: Option<String>,
}

impl ContentsDef {
    /// What a component placed says, over what its kind says.
    fn over(&self, kind: Option<&ContentsDef>) -> ContentsDef {
        let k = kind.cloned().unwrap_or_default();
        ContentsDef {
            sustancia: self.sustancia.clone().or(k.sustancia),
            capacidad: self.capacidad.clone().or(k.capacidad),
            lleno: self.lleno.or(k.lleno),
            piezas: self.piezas.clone().or(k.piezas),
            nivel: self.nivel.clone().or(k.nivel),
            cabida: self.cabida.clone().or(k.cabida),
            inicial: self.inicial.clone().or(k.inicial),
            volumen: self.volumen.clone().or(k.volumen),
        }
    }
}

/// What a component holds, worked out.
#[derive(Clone, Debug, PartialEq)]
pub struct Fitted {
    /// The pieces that hold it (their place among the component's pieces), each with what it
    /// holds and what that lets go of if the piece is destroyed.
    pub pieces: Vec<(usize, ContainerDef, Option<BurstDef>)>,
    /// The substance's id, and kg of it at most and as built in the whole component.
    pub substance: String,
    pub capacity: f32,
    pub mass: f32,
    /// As its labels write them: the substance in capitals ("AGUA"), how much at most ("170 L").
    pub label: (String, String),
    /// As a scanner reads it as built ("170 L de agua").
    pub text: String,
    /// The signal that says how much it holds, if one does.
    pub level: Option<String>,
    /// What its machine is given so that it says the same (`cabida`, `inicial`, `volumen`): the
    /// parameter and its figure (SI: kg, m³).
    pub machine: Vec<(String, f64)>,
    /// kg that fit in it to the brim (its room by the substance's density).
    pub brim: f32,
}

/// `q` of `sub` in kg: a mass as it is, a volume by the substance's density.
fn kilograms(q: &Q, sub: &SubstanceDef) -> Result<f32, String> {
    match q {
        Q::N(v) => Ok(*v as f32),
        Q::S(s) => {
            let (v, u) = units::parse(s).map_err(|e| e.0)?;
            let of = |name: &str| units::unit(name).is_ok_and(|w| u.compatible(&w));
            if of("kg") {
                Ok(v as f32)
            } else if of("L") {
                Ok(v as f32 * sub.densidad)
            } else {
                Err(format!("'{s}' no es una masa ni un volumen"))
            }
        }
    }
}

/// What component `cid` holds: `c` as it is placed, `kind` its kind (none: a plain shape).
/// None if it holds nothing.
pub fn fit(cid: &str, c: &ComponentDef, kind: Option<&ComponentKind>, cat: &Catalog) -> Result<Option<Fitted>, String> {
    let def = match (&c.contenido, kind.and_then(|k| k.contenido.as_ref())) {
        (None, None) => return Ok(None),
        (Some(p), k) => p.over(k),
        (None, Some(k)) => k.clone(),
    };
    let err = |e: String| format!("componente {cid}: contenido: {e}");
    let id = def.sustancia.as_deref().ok_or_else(|| err("falta 'sustancia'".into()))?;
    let sub = &cat.substances.iter().find(|(s, _)| s == id).ok_or_else(|| err(format!("sustancia desconocida '{id}' (assets/defs/sustancias.jsonc)")))?.1;
    // its pieces: id, shape, walls
    let own: ShapeDef;
    let pieces: Vec<(&str, &ShapeDef, Option<f32>)> = match (kind, &c.forma) {
        (Some(k), _) => k.piezas.iter().map(|p| (p.id.as_str(), &p.forma, p.hueco.or(c.hueco))).collect(),
        (None, Some(f)) => {
            own = serde_json::from_value(f.clone()).map_err(|e| err(format!("forma: {e}")))?;
            vec![("", &own, c.hueco)]
        }
        (None, None) => return Err(err("sin 'tipo' ni 'forma'".into())),
    };
    let which: Vec<usize> = match &def.piezas {
        Some(names) => names.iter().map(|n| pieces.iter().position(|p| p.0 == n).ok_or_else(|| err(format!("no tiene la pieza '{n}'")))).collect::<Result<_, _>>()?,
        None => vec![0],
    };
    if which.is_empty() {
        return Err(err("'piezas' vacío".into()));
    }
    // the room of each: its shape less its walls
    let mut rooms = Vec::with_capacity(which.len());
    for &i in &which {
        let (name, shape, walls) = pieces[i];
        let walls = walls.ok_or_else(|| err(format!("la pieza '{name}' es maciza: no cabe nada dentro (dale 'hueco')")))?;
        let (volume, area) = shape.measure();
        let room = volume - area * walls;
        if room <= 0.0 {
            return Err(err(format!("la pieza '{name}' no tiene sitio dentro")));
        }
        rooms.push(room);
    }
    // what its machine says of it, where its data ties the two: the placed one's own
    // parameters first, then (after what is said of the contents where it is placed) its kind's
    let (own, base) = (c.contenido.as_ref(), kind.and_then(|k| k.contenido.as_ref()));
    let (m_own, m_base) = (c.maquina.as_ref().map(|m| &m.params), kind.and_then(|k| k.maquina.as_ref()).map(|m| &m.params));
    for (key, name) in [(&def.cabida, "cabida"), (&def.inicial, "inicial"), (&def.volumen, "volumen")] {
        if key.is_some() && m_own.is_none() && m_base.is_none() {
            return Err(err(format!("'{name}' nombra un parámetro de su máquina, y no tiene máquina")));
        }
    }
    let said = |params: Option<&Map<String, Value>>, key: &Option<String>| -> Result<Option<Q>, String> {
        let (Some(p), Some(k)) = (params, key) else { return Ok(None) };
        p.get(k).map(|v| serde_json::from_value::<Q>(v.clone()).map_err(|e| err(format!("su máquina, '{k}': {e}")))).transpose()
    };
    let kg = |q: Option<Q>| q.map(|q| kilograms(&q, sub).map_err(err)).transpose();
    // its inside: its pieces', or less if its machine says so (never more)
    let mut room: f32 = rooms.iter().sum();
    if let Some(q) = said(m_own, &def.volumen)?.or(said(m_base, &def.volumen)?) {
        let inside = q.si_as("m3").map_err(|e| err(e.0))? as f32;
        if inside <= 0.0 || inside > room * 1.001 {
            return Err(err(format!("su máquina dice que tiene {} L dentro ('{}') y en sus piezas hay sitio para {} L", figure(inside * 1000.0), def.volumen.as_deref().unwrap_or(""), figure(room * 1000.0))));
        }
        rooms.iter_mut().for_each(|r| *r *= inside / room);
        room = inside;
    }
    let brim = sub.densidad * room;
    let by_machine = [kg(said(m_own, &def.cabida)?)?, None, kg(said(m_base, &def.cabida)?)?, None];
    let by_contents = [None, kg(own.and_then(|o| o.capacidad.clone()))?, None, kg(base.and_then(|b| b.capacidad.clone()))?];
    let stated = (0..4).find_map(|k| by_machine[k].map(|v| (v, true)).or(by_contents[k].map(|v| (v, false))));
    let (capacity, machines) = stated.unwrap_or((brim, false));
    if capacity <= 0.0 || capacity > brim * 1.001 {
        let who = if machines { format!(" (lo dice su máquina, '{}': quítaselo y llevará lo que cabe, o dale un depósito mayor)", def.cabida.as_deref().unwrap_or("")) } else { String::new() };
        return Err(err(format!("{} de {} no caben: dentro hay sitio para {}{who}", sub.amount(capacity), sub.nombre, sub.amount(brim))));
    }
    for fill in [own.and_then(|o| o.lleno), base.and_then(|b| b.lleno)].into_iter().flatten() {
        if !(0.0..=1.0).contains(&fill) {
            return Err(err(format!("'lleno' va de 0 a 1 (es {fill})")));
        }
    }
    let begins = [kg(said(m_own, &def.inicial)?)?, own.and_then(|o| o.lleno).map(|f| f * capacity), kg(said(m_base, &def.inicial)?)?, base.and_then(|b| b.lleno).map(|f| f * capacity)];
    let mass = begins.into_iter().flatten().next().unwrap_or(capacity);
    if mass < 0.0 || mass > capacity * 1.001 {
        return Err(err(format!("empieza con {} de {} y no lleva más de {}", sub.amount(mass), sub.nombre, sub.amount(capacity))));
    }
    let (mass, fill) = (mass.min(capacity), (mass / capacity).clamp(0.0, 1.0));
    let machine = [(&def.cabida, capacity), (&def.inicial, mass), (&def.volumen, room)].into_iter().filter_map(|(key, v)| key.clone().map(|k| (k, f64::from(v)))).collect();
    let format = kind.and_then(|k| k.recurso.clone());
    // (what a machine counts lets go, when its part is destroyed, of what the machine holds
    // then: `Machine::rupture`. Only what nobody counts bursts by what it was built with)
    let burst = |kg: f32| if def.nivel.is_some() { None } else { sub.burst(kg) };
    let pieces = which.iter().zip(&rooms).map(|(&i, &r)| (i, ContainerDef { substance: id.to_string(), capacity: Some(capacity * r / room), fill, room: Some(r), text: format.clone() }, burst(mass * r / room))).collect();
    Ok(Some(Fitted {
        pieces,
        substance: id.to_string(),
        capacity,
        mass,
        label: (sub.nombre.to_uppercase(), sub.amount(capacity)),
        text: text(format.as_deref(), sub, mass, capacity),
        level: def.nivel.as_ref().map(|n| format!("{cid}.{n}")),
        machine,
        brim,
    }))
}

/// A label's text with what its component holds written in: `{sustancia}` and `{capacidad}`.
/// None if it says either and the component holds nothing (the label is then left out).
pub fn lettered(texto: &str, fitted: Option<&Fitted>) -> Option<String> {
    if !texto.contains("{sustancia}") && !texto.contains("{capacidad}") {
        return Some(texto.to_string());
    }
    fitted.map(|f| texto.replace("{sustancia}", &f.label.0).replace("{capacidad}", &f.label.1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lunar_core::structure::contents::{HazardDef, State};

    fn catalog() -> Catalog {
        let sub = |nombre: &str, densidad: f32, estado: State, estalla: Option<HazardDef>| (nombre.to_string(), SubstanceDef { nombre: nombre.to_string(), densidad, estado, medida: None, estalla });
        let subs = vec![sub("agua", 1000.0, State::Liquid, None), sub("propelente", 875.0, State::Liquid, Some(HazardDef { energia: 5000.0, radio: 1.1, efecto: "granada".into() })), sub("regolito", 1750.0, State::Bulk, None)];
        Catalog::with(Vec::new(), Vec::new(), Vec::new(), Default::default(), subs).unwrap()
    }

    fn kind(json: &str) -> ComponentKind {
        lunar_core::defs::parse("tipo", json).unwrap()
    }

    fn placed(json: &str) -> ComponentDef {
        lunar_core::defs::parse("componente", json).unwrap()
    }

    const DRUM: &str = r#"{ "nombre": "Bidón", "contenido": { "sustancia": "agua", "capacidad": "170 L" },
        "piezas": [ { "id": "", "hueco": 0.004, "forma": { "kind": "cylinder", "radius": 0.26, "height": 0.86, "sides": 14 } },
                    { "id": "aro", "forma": { "kind": "cylinder", "radius": 0.275, "height": 0.05 } } ] }"#;

    #[test]
    fn what_a_component_holds_comes_from_its_data() {
        let cat = catalog();
        let k = kind(DRUM);
        let f = fit("b", &placed(r#"{ "id": "b", "tipo": "bidon" }"#), Some(&k), &cat).unwrap().unwrap();
        assert_eq!((f.pieces.len(), f.pieces[0].0), (1, 0), "its first piece holds it");
        assert!((f.capacity - 170.0).abs() < 1e-3 && (f.mass - 170.0).abs() < 1e-3);
        assert_eq!((f.label.0.as_str(), f.label.1.as_str(), f.text.as_str()), ("AGUA", "170 L", "170 L de agua"));
        // the room is the round drum's, less its walls: 175 L
        let room = f.pieces[0].1.room.unwrap();
        assert!((room - 0.1753).abs() < 5e-4, "{room}");
        assert!(f.pieces[0].2.is_none(), "water does not burst");
        // placed half empty, and with something else in it: the same drum says so
        let f = fit("b", &placed(r#"{ "id": "b", "tipo": "bidon", "contenido": { "lleno": 0.5 } }"#), Some(&k), &cat).unwrap().unwrap();
        assert!((f.mass - 85.0).abs() < 1e-3 && f.text == "85 L de agua" && f.label.1 == "170 L");
        let f = fit("b", &placed(r#"{ "id": "b", "tipo": "bidon", "contenido": { "sustancia": "propelente", "capacidad": "100 kg" } }"#), Some(&k), &cat).unwrap().unwrap();
        assert_eq!((f.label.0.as_str(), f.label.1.as_str(), f.text.as_str()), ("PROPELENTE", "114 L", "114 L de propelente"));
        let b = f.pieces[0].2.clone().expect("propellant bursts");
        assert!((b.energy - 500_000.0).abs() < 1.0 && b.effect == "granada");
        // what does not fit is an error that says what does
        let e = fit("b", &placed(r#"{ "id": "b", "tipo": "bidon", "contenido": { "capacidad": "200 L" } }"#), Some(&k), &cat).unwrap_err();
        assert!(e.contains("200 L de agua no caben") && e.contains("175 L"), "{e}");
        assert!(fit("b", &placed(r#"{ "id": "b", "tipo": "bidon", "contenido": { "sustancia": "vino" } }"#), Some(&k), &cat).is_err());
        assert!(fit("b", &placed(r#"{ "id": "b", "tipo": "bidon", "contenido": { "piezas": ["aro"] } }"#), Some(&k), &cat).unwrap_err().contains("maciza"));
        // nothing declared, nothing held
        assert_eq!(fit("b", &placed(r#"{ "id": "b", "tipo": "x" }"#), Some(&kind(r#"{ "nombre": "Caja", "piezas": [] }"#)), &cat).unwrap(), None);
    }

    #[test]
    fn several_pieces_share_what_is_held_by_their_room_and_a_plain_shape_holds_too() {
        let cat = catalog();
        let k = kind(
            r#"{ "nombre": "Palé", "recurso": "{cantidad} de {sustancia} en sacos", "contenido": { "sustancia": "regolito", "capacidad": "600 kg", "piezas": ["s1", "s2"] },
            "piezas": [ { "id": "", "forma": { "kind": "box", "size": [1, 0.1, 1] } },
                        { "id": "s1", "hueco": 0.001, "forma": { "kind": "box", "size": [1, 0.3, 1] } },
                        { "id": "s2", "hueco": 0.001, "forma": { "kind": "box", "size": [1, 0.15, 1] } } ] }"#,
        );
        let f = fit("p", &placed(r#"{ "id": "p", "tipo": "pale" }"#), Some(&k), &cat).unwrap().unwrap();
        assert_eq!(f.text, "600 kg de regolito en sacos");
        assert_eq!(f.pieces.iter().map(|p| p.0).collect::<Vec<_>>(), vec![1, 2]);
        let (a, b) = (f.pieces[0].1.capacity.unwrap(), f.pieces[1].1.capacity.unwrap());
        assert!((a + b - 600.0).abs() < 1e-2 && a > b * 1.9 && a < b * 2.1, "{a} + {b}");
        // a plain shape placed with something in it: what fits, counted from its signal
        let tank = placed(r#"{ "id": "t", "forma": { "kind": "box", "size": [1, 1, 1] }, "hueco": 0.01, "contenido": { "sustancia": "agua", "nivel": "masa" } }"#);
        let f = fit("t", &tank, None, &cat).unwrap().unwrap();
        assert!((f.capacity - 940.0).abs() < 0.5 && f.level.as_deref() == Some("t.masa"), "{f:?}");
        // labels: what they say of what is held, or left out if nothing is
        assert_eq!(lettered("{sustancia} · {capacidad}", Some(&f)).as_deref(), Some("AGUA · 940 L"));
        assert_eq!(lettered("{sustancia}", None), None);
        assert_eq!(lettered("N/S {serie}", None).as_deref(), Some("N/S {serie}"));
    }
}
