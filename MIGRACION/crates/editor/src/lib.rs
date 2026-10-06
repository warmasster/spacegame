//! Editing ships: a ship's definition as a document (`Doc`) changed by operations (`Op`) — the
//! same ones a person makes in the 3D editor, a model makes through the MCP server, and (later) a
//! crew makes building or repairing in play —, undone and redone, checked by building it and
//! running the diagnostics (`lunar_ship::diag`: leaks, cables, usability), and saved.
//!
//! Operations work on the data, never on code: anything the data can say, an operation can do
//! (`fijar` sets any value by its path).
pub mod mcp;

use glam::Vec3;
use lunar_core::structure::Library;
use lunar_ship::{Ship, Sources, World, def::ShipDef, diag};
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

/// One change to a ship's definition.
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Op {
    /// A component placed (its definition as in `componentes`: id, tipo or forma, en, rot...).
    Poner { componente: Value },
    /// A component moved to `en` (m, ship frame: +x port, +y up, +z nose).
    Mover { id: String, en: [f32; 3] },
    /// A component moved by `delta`.
    Desplazar { id: String, delta: [f32; 3] },
    /// A component turned to `rot` (Euler XYZ, degrees).
    Girar { id: String, rot: [f32; 3] },
    Quitar { id: String },
    /// A copy of a component as `nuevo`, at `en` (else beside it).
    Duplicar { id: String, nuevo: String, #[serde(default)] en: Option<[f32; 3]> },
    /// A component (or panel) wired with its own conduits, or connected without any.
    Cableado { id: String, valor: bool },
    /// Any value of the definition by its path ("/componentes/3/maquina/params/caudal").
    Fijar { ruta: String, valor: Value },
    /// A node of a network.
    Nodo { red: String, nombre: String, en: [f32; 3] },
    /// A run of a network between two of its nodes, by these points (if any).
    Tramo { red: String, de: String, a: String, #[serde(default)] por: Vec<[f32; 3]> },
    /// One more point a run passes by (the editor's manual cable routing).
    PuntoRuta { red: String, tramo: usize, punto: [f32; 3] },
    /// A machine's port on "net:node".
    Conectar { id: String, rol: String, puerto: String },
    /// Where a compartment's own panel goes.
    PanelSala { compartimento: String, en: [f32; 3], normal: [f32; 3] },
    /// Back one operation, forward one.
    Deshacer,
    Rehacer,
}

/// A ship being edited.
pub struct Doc {
    pub id: String,
    pub path: PathBuf,
    pub value: Value,
    undo: Vec<Value>,
    redo: Vec<Value>,
    pub dirty: bool,
}

/// What a check found: build errors, counts, and every diagnostic.
#[derive(Clone, Debug, Default)]
pub struct Report {
    pub error: Option<String>,
    pub parts: usize,
    pub controls: usize,
    pub indicators: usize,
    pub leaks: Vec<String>,
    pub conduits: usize,
    pub exposed: Vec<String>,
    pub buried: Vec<String>,
    pub through: Vec<String>,
    pub usability: Vec<String>,
}

impl Report {
    pub fn ok(&self) -> bool {
        self.error.is_none() && self.leaks.is_empty() && self.usability.is_empty()
    }

    /// As text for a person or a model: the verdict, then what to fix.
    pub fn text(&self) -> String {
        let mut out = String::new();
        if let Some(e) = &self.error {
            out.push_str(&format!("NO SE ARMA: {e}\n"));
            return out;
        }
        out.push_str(&format!(
            "{}: {} piezas, {} mandos, {} indicadores, {} conductos ({} por fuera, {} medio enterrados, {} a través de aparatos)\n",
            if self.ok() { "BIEN" } else { "HAY QUE ARREGLAR" },
            self.parts,
            self.controls,
            self.indicators,
            self.conduits,
            self.exposed.len(),
            self.buried.len(),
            self.through.len()
        ));
        for l in &self.leaks {
            out.push_str(&format!("fuga: {l}\n"));
        }
        for u in &self.usability {
            out.push_str(&format!("uso: {u}\n"));
        }
        for c in self.exposed.iter().chain(self.through.iter()).take(12) {
            out.push_str(&format!("cable: {c}\n"));
        }
        out
    }
}

fn find_component<'a>(v: &'a mut Value, id: &str) -> Result<&'a mut Value, String> {
    // copies ("x#2", mirrored "x.e" or izq/der) are edited through what they copy
    let base = id.split('#').next().unwrap_or(id);
    let list = v.get_mut("componentes").and_then(Value::as_array_mut).ok_or("la nave no tiene componentes")?;
    let k = list.iter().position(|c| c.get("id").and_then(Value::as_str) == Some(base));
    let k = match k {
        Some(k) => k,
        None => {
            let mirrored = lunar_ship::components::mirror_name(base);
            list.iter().position(|c| c.get("id").and_then(Value::as_str) == Some(mirrored.as_str())).ok_or_else(|| format!("no hay componente '{id}'"))?
        }
    };
    Ok(&mut list[k])
}

fn vec3(v: &Value) -> Vec3 {
    let a = v.as_array();
    let f = |i: usize| a.and_then(|a| a.get(i)).and_then(Value::as_f64).unwrap_or(0.0) as f32;
    Vec3::new(f(0), f(1), f(2))
}

fn round(p: Vec3) -> Value {
    // millimetres are enough, and keep the file readable
    json!([(p.x * 1000.0).round() / 1000.0, (p.y * 1000.0).round() / 1000.0, (p.z * 1000.0).round() / 1000.0])
}

impl Doc {
    /// The ship `id` under `defs` (`assets/defs/ships/<id>.jsonc`).
    pub fn open(defs: &Path, id: &str) -> Result<Doc, String> {
        let path = defs.join("ships").join(format!("{id}.jsonc"));
        let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let value: Value = lunar_core::defs::parse(&path.display().to_string(), &text).map_err(|e| e.to_string())?;
        Ok(Doc { id: id.to_string(), path, value, undo: Vec::new(), redo: Vec::new(), dirty: false })
    }

    /// The definition as the game reads it.
    pub fn def(&self) -> Result<ShipDef, String> {
        serde_json::from_value(self.value.clone()).map_err(|e| format!("la definición no vale: {e}"))
    }

    /// Apply `op`: what it did, or why not (then nothing changed).
    pub fn apply(&mut self, op: &Op) -> Result<String, String> {
        match op {
            Op::Deshacer => {
                let v = self.undo.pop().ok_or("nada que deshacer")?;
                self.redo.push(std::mem::replace(&mut self.value, v));
                self.dirty = true;
                return Ok("deshecho".into());
            }
            Op::Rehacer => {
                let v = self.redo.pop().ok_or("nada que rehacer")?;
                self.undo.push(std::mem::replace(&mut self.value, v));
                self.dirty = true;
                return Ok("rehecho".into());
            }
            _ => {}
        }
        let before = self.value.clone();
        let r = self.change(op);
        match r {
            Ok(msg) => {
                // the result must still be a definition the game reads
                if let Err(e) = self.def() {
                    self.value = before;
                    return Err(e);
                }
                self.undo.push(before);
                if self.undo.len() > 200 {
                    self.undo.remove(0);
                }
                self.redo.clear();
                self.dirty = true;
                Ok(msg)
            }
            Err(e) => {
                self.value = before;
                Err(e)
            }
        }
    }

    fn change(&mut self, op: &Op) -> Result<String, String> {
        let v = &mut self.value;
        match op {
            Op::Poner { componente } => {
                let id = componente.get("id").and_then(Value::as_str).ok_or("el componente necesita 'id'")?.to_string();
                let list = v.get_mut("componentes").and_then(Value::as_array_mut).ok_or("la nave no tiene componentes")?;
                if list.iter().any(|c| c.get("id").and_then(Value::as_str) == Some(id.as_str())) {
                    return Err(format!("ya hay un componente '{id}'"));
                }
                list.push(componente.clone());
                Ok(format!("puesto {id}"))
            }
            Op::Mover { id, en } => {
                find_component(v, id)?["en"] = round(Vec3::from_array(*en));
                Ok(format!("{id} en {en:?}"))
            }
            Op::Desplazar { id, delta } => {
                let c = find_component(v, id)?;
                let p = vec3(&c["en"]) + Vec3::from_array(*delta);
                c["en"] = round(p);
                Ok(format!("{id} en {:?}", p.to_array()))
            }
            Op::Girar { id, rot } => {
                find_component(v, id)?["rot"] = json!(rot);
                Ok(format!("{id} girado {rot:?}"))
            }
            Op::Quitar { id } => {
                let list = v.get_mut("componentes").and_then(Value::as_array_mut).ok_or("la nave no tiene componentes")?;
                let k = list.iter().position(|c| c.get("id").and_then(Value::as_str) == Some(id.as_str())).ok_or_else(|| format!("no hay componente '{id}'"))?;
                list.remove(k);
                Ok(format!("quitado {id}"))
            }
            Op::Duplicar { id, nuevo, en } => {
                let mut c = find_component(v, id)?.clone();
                let at = en.map_or_else(|| vec3(&c["en"]) + Vec3::new(0.0, 0.0, 0.5), Vec3::from_array);
                c["id"] = json!(nuevo);
                c["en"] = round(at);
                if let Some(o) = c.as_object_mut() {
                    o.remove("espejo");
                    o.remove("repetir");
                }
                v["componentes"].as_array_mut().ok_or("sin componentes")?.push(c);
                Ok(format!("{nuevo}: copia de {id}"))
            }
            Op::Cableado { id, valor } => {
                // a component, else a panel
                if let Ok(c) = find_component(v, id) {
                    c["cableado"] = json!(valor);
                } else {
                    let p = v.get_mut("paneles").and_then(Value::as_array_mut).and_then(|l| l.iter_mut().find(|p| p.get("id").and_then(Value::as_str) == Some(id.as_str()))).ok_or_else(|| format!("no hay componente ni panel '{id}'"))?;
                    p["cableado"] = json!(valor);
                }
                Ok(format!("{id}: {}", if *valor { "con su cableado" } else { "sin cableado" }))
            }
            Op::Fijar { ruta, valor } => {
                let (parent, key) = ruta.rsplit_once('/').ok_or("ruta: /a/b/c")?;
                let at = if parent.is_empty() { Some(&mut *v) } else { v.pointer_mut(parent) };
                let at = at.ok_or_else(|| format!("no existe '{parent}'"))?;
                match at {
                    Value::Object(o) => {
                        o.insert(key.to_string(), valor.clone());
                    }
                    Value::Array(a) => {
                        let i: usize = key.parse().map_err(|_| format!("'{key}' no es un índice"))?;
                        *a.get_mut(i).ok_or_else(|| format!("índice {i} fuera"))? = valor.clone();
                    }
                    _ => return Err(format!("'{parent}' no es objeto ni lista")),
                }
                Ok(format!("{ruta} = {valor}"))
            }
            Op::Nodo { red, nombre, en } => {
                let n = v.pointer_mut(&format!("/redes/{red}/nodos")).and_then(Value::as_object_mut).ok_or_else(|| format!("no hay red '{red}'"))?;
                n.insert(nombre.clone(), round(Vec3::from_array(*en)));
                Ok(format!("{red}:{nombre} en {en:?}"))
            }
            Op::Tramo { red, de, a, por } => {
                let net = v.pointer_mut(&format!("/redes/{red}")).ok_or_else(|| format!("no hay red '{red}'"))?;
                for n in [de, a] {
                    if net.pointer(&format!("/nodos/{n}")).is_none() {
                        return Err(format!("la red {red} no tiene el nodo '{n}'"));
                    }
                }
                let mut t = json!({ "de": de, "a": a });
                if !por.is_empty() {
                    t["por"] = json!(por.iter().map(|p| round(Vec3::from_array(*p))).collect::<Vec<_>>());
                }
                let list = net.as_object_mut().ok_or("red rara")?.entry("tramos").or_insert_with(|| json!([]));
                list.as_array_mut().ok_or("tramos raros")?.push(t);
                Ok(format!("{red}: tramo {de} → {a}"))
            }
            Op::PuntoRuta { red, tramo, punto } => {
                let t = v.pointer_mut(&format!("/redes/{red}/tramos/{tramo}")).ok_or_else(|| format!("la red {red} no tiene el tramo {tramo}"))?;
                let list = t.as_object_mut().ok_or("tramo raro")?.entry("por").or_insert_with(|| json!([]));
                list.as_array_mut().ok_or("'por' raro")?.push(round(Vec3::from_array(*punto)));
                Ok(format!("{red} tramo {tramo}: pasa por {punto:?}"))
            }
            Op::Conectar { id, rol, puerto } => {
                let c = find_component(v, id)?;
                let m = c.as_object_mut().ok_or("componente raro")?.entry("maquina").or_insert_with(|| json!({}));
                let p = m.as_object_mut().ok_or("máquina rara")?.entry("puertos").or_insert_with(|| json!({}));
                p[rol.as_str()] = json!(puerto);
                Ok(format!("{id}.{rol} → {puerto}"))
            }
            Op::PanelSala { compartimento, en, normal } => {
                let list = v.get_mut("compartimentos").and_then(Value::as_array_mut).ok_or("sin compartimentos")?;
                let c = list.iter_mut().find(|c| c.get("id").and_then(Value::as_str) == Some(compartimento.as_str())).ok_or_else(|| format!("no hay compartimento '{compartimento}'"))?;
                let mut p = c.get("panel").cloned().unwrap_or_else(|| json!({}));
                p["en"] = round(Vec3::from_array(*en));
                p["normal"] = json!(normal);
                c["panel"] = p;
                Ok(format!("panel de {compartimento} en {en:?}"))
            }
            Op::Deshacer | Op::Rehacer => unreachable!(),
        }
    }

    /// Build it and run every diagnostic (with the ramp and doors as the ship starts, then shut
    /// for the leak check).
    pub fn check(&self, defs: &Path) -> Report {
        let mut r = Report::default();
        let def = match self.def() {
            Ok(d) => d,
            Err(e) => {
                r.error = Some(e);
                return r;
            }
        };
        let mut lib = match Library::load(&defs.join("structures")) {
            Ok(l) => l,
            Err(e) => {
                r.error = Some(e.to_string());
                return r;
            }
        };
        let src = match Sources::load(defs) {
            Ok(s) => s,
            Err(e) => {
                r.error = Some(format!("{}: {}", e.file, e.message));
                return r;
            }
        };
        let (kind, bp) = match src.build(&self.id, def, &mut lib.catalog) {
            Ok(x) => x,
            Err(e) => {
                r.error = Some(e);
                return r;
            }
        };
        let kind = std::sync::Arc::new(kind);
        let mut s = lunar_core::structure::state::Structure::new(1, &bp, &lib.catalog, glam::DVec3::ZERO, glam::Quat::IDENTITY);
        let mut ship = match Ship::new(kind.clone(), 1, 7) {
            Ok(x) => x,
            Err(e) => {
                r.error = Some(e);
                return r;
            }
        };
        ship.update(&mut s, &World::default(), 0.0);
        r.parts = s.parts.len();
        r.controls = ship.panels.controls.len();
        r.indicators = ship.panels.indicators.len();
        r.usability = diag::usability(&ship, &s);
        r.usability.extend(diag::panels_cut(&ship, &s));
        r.usability.extend(diag::panel_faces(&kind));
        r.usability.extend(diag::seat_reach(&ship, &s));
        r.usability.extend(diag::seat_fit(&ship.kind, &s));
        let c = diag::cables(&s, &kind, &lib.catalog);
        r.conduits = c.conduits;
        r.exposed = c.exposed;
        r.buried = c.buried;
        r.through = c.through;
        // every closure shut for the leaks
        for cl in &kind.closures {
            for &j in &cl.joints {
                ship.set_joint(&kind.joints[j].id, 0.0);
            }
        }
        ship.update(&mut s, &World::default(), 0.0);
        r.leaks = diag::leaks(&s, &kind)
            .into_iter()
            .map(|l| match l.hole {
                Some(h) => format!("{} sale cerca de ({:.2}, {:.2}, {:.2})", l.room, h.x, h.y, h.z),
                None => format!("{} llega a {:?}", l.room, l.reaches),
            })
            .collect();
        r
    }

    /// Write it (pretty JSON, a header saying it was edited) to `path`, or over its own file
    /// after keeping a copy of that in `backups`.
    pub fn save(&mut self, path: Option<&Path>, backups: &Path) -> Result<PathBuf, String> {
        let to = path.map_or_else(|| self.path.clone(), Path::to_path_buf);
        if to == self.path && self.path.exists() {
            std::fs::create_dir_all(backups).map_err(|e| e.to_string())?;
            let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
            std::fs::copy(&self.path, backups.join(format!("{}-{stamp}.jsonc", self.id))).map_err(|e| e.to_string())?;
        }
        let body = serde_json::to_string_pretty(&self.value).map_err(|e| e.to_string())?;
        let text = format!("// {} — guardada por el editor de naves (lunar-editor). La copia anterior, con sus comentarios,\n// queda en {}.\n{body}\n", self.id, backups.display());
        std::fs::write(&to, text).map_err(|e| format!("{}: {e}", to.display()))?;
        self.dirty = false;
        Ok(to)
    }

    /// A short summary for a person or a model: compartments, components, networks, panels.
    pub fn summary(&self) -> String {
        let v = &self.value;
        let mut out = format!("{} ({})\n", v["nombre"].as_str().unwrap_or(&self.id), self.id);
        if let Some(cs) = v["compartimentos"].as_array() {
            for c in cs {
                out.push_str(&format!("compartimento {}: cajas {}\n", c["id"].as_str().unwrap_or("?"), c["cajas"]));
            }
        }
        if let Some(cs) = v["componentes"].as_array() {
            out.push_str(&format!("{} componentes:\n", cs.len()));
            for c in cs {
                let ports = c.pointer("/maquina/puertos").map(|p| format!(" puertos {p}")).unwrap_or_default();
                out.push_str(&format!("  {} {} en {}{}{}\n", c["id"].as_str().unwrap_or("?"), c["tipo"].as_str().unwrap_or("(forma)"), c["en"], if c.get("cableado") == Some(&json!(false)) { " sin cableado" } else { "" }, ports));
            }
        }
        if let Some(rs) = v["redes"].as_object() {
            for (name, n) in rs {
                out.push_str(&format!("red {name} ({}): nodos {}; {} tramos\n", n["medio"].as_str().unwrap_or("?"), n["nodos"].as_object().map_or(0, |o| o.len()), n["tramos"].as_array().map_or(0, Vec::len)));
            }
        }
        if let Some(ps) = v["paneles"].as_array() {
            out.push_str(&format!("{} paneles: {}\n", ps.len(), ps.iter().filter_map(|p| p["id"].as_str()).collect::<Vec<_>>().join(", ")));
        }
        out
    }
}

/// The components and panel templates there are, for a person or a model choosing what to place.
pub fn catalog(defs: &Path, filter: Option<&str>) -> Result<String, String> {
    let src = Sources::load(defs).map_err(|e| format!("{}: {}", e.file, e.message))?;
    let mut out = String::from("componentes:\n");
    for (id, k) in &src.components {
        if filter.is_some_and(|f| !id.contains(f)) {
            continue;
        }
        let m = k.maquina.as_ref().map(|m| format!(" máquina {} (puertos: {})", m.modelo, m.puertos.keys().cloned().collect::<Vec<_>>().join(", "))).unwrap_or_default();
        out.push_str(&format!("  {id}{m}\n"));
    }
    out.push_str("paneles: ");
    out.push_str(&src.panels.keys().cloned().collect::<Vec<_>>().join(", "));
    out.push('\n');
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn defs() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/defs")
    }

    #[test]
    fn operations_change_undo_and_check() {
        let mut d = Doc::open(&defs(), "alcotan").unwrap();
        let before = d.value.clone();
        d.apply(&Op::Desplazar { id: "acumulador".into(), delta: [0.0, 0.0, 0.1] }).unwrap();
        d.apply(&Op::Cableado { id: "baliza_sup".into(), valor: false }).unwrap();
        d.apply(&Op::Duplicar { id: "plafon_bod".into(), nuevo: "plafon_extra".into(), en: Some([0.0, 2.84, -4.0]) }).unwrap();
        assert!(d.apply(&Op::Quitar { id: "no_existe".into() }).is_err());
        // a bad value never gets in
        assert!(d.apply(&Op::Fijar { ruta: "/componentes/0/en".into(), valor: json!("no") }).is_err());
        assert_ne!(d.value, before);
        for _ in 0..3 {
            d.apply(&Op::Deshacer).unwrap();
        }
        assert_eq!(d.value, before);
        d.apply(&Op::Rehacer).unwrap();
        // checked: it still builds, is tight with its doors shut and works from every room
        let r = d.check(&defs());
        eprintln!("{}", r.text());
        assert!(r.ok(), "{}", r.text());
    }

    #[test]
    fn a_broken_ship_says_why() {
        let mut d = Doc::open(&defs(), "alcotan").unwrap();
        d.apply(&Op::Conectar { id: "acumulador".into(), rol: "presion".into(), puerto: "hid:no_hay".into() }).unwrap();
        let r = d.check(&defs());
        assert!(r.error.as_deref().is_some_and(|e| e.contains("no_hay")), "{}", r.text());
    }
}
