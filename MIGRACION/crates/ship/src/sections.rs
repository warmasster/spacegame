//! Sections of a panel: a titled group of controls written once (`assets/defs/secciones.jsonc`)
//! and carried by any panel that names it on one of its groups (`"seccion": "<id>"`). The
//! group takes the section's title (unless it has its own) and its controls, each with the
//! group's id before its own (`<group>_<control>`), so that a panel may carry a section more
//! than once. A section is data: a new one needs no code, and what it shows is whatever signals
//! every ship has (the "energia" section: `power`).
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SectionDef {
    #[serde(default)]
    pub titulo: Option<String>,
    /// How its controls are set out in the group ("filas", "columnas", "flujo"), if it says.
    #[serde(default)]
    pub orden: Option<String>,
    /// Its controls, as a panel writes them (without `grupo`).
    pub mandos: Vec<Value>,
}

pub type Sections = BTreeMap<String, SectionDef>;

/// The panel `id` (its text) with the sections its groups name laid in. A panel that names none
/// is given back as it is.
pub fn expand(id: &str, text: &str, sections: &Sections) -> Result<String, String> {
    if !text.contains("\"seccion\"") {
        return Ok(text.to_string());
    }
    let mut panel: Value = lunar_core::defs::parse(id, text).map_err(|e| e.to_string())?;
    let mut added = Vec::new();
    for g in panel.get_mut("grupos").and_then(Value::as_array_mut).into_iter().flatten() {
        let Some(group) = g.as_object_mut() else { continue };
        let Some(name) = group.remove("seccion") else { continue };
        let name = name.as_str().ok_or_else(|| format!("panel {id}: \"seccion\" es un nombre"))?.to_string();
        let section = sections.get(&name).ok_or_else(|| format!("panel {id}: no hay sección '{name}' (assets/defs/secciones.jsonc)"))?;
        let gid = group.get("id").and_then(Value::as_str).ok_or_else(|| format!("panel {id}: un grupo sin id"))?.to_string();
        for (key, value) in [("titulo", &section.titulo), ("orden", &section.orden)] {
            if let (false, Some(v)) = (group.contains_key(key), value) {
                group.insert(key.into(), Value::String(v.clone()));
            }
        }
        for c in &section.mandos {
            let mut c = c.clone();
            let o = c.as_object_mut().ok_or_else(|| format!("sección {name}: un mando que no es un objeto"))?;
            let cid = o.get("id").and_then(Value::as_str).ok_or_else(|| format!("sección {name}: un mando sin id"))?;
            o.insert("id".into(), Value::String(format!("{gid}_{cid}")));
            o.insert("grupo".into(), Value::String(gid.clone()));
            // (what it guards is in the same section)
            if let Some(Value::Array(guarded)) = o.get_mut("protege") {
                for x in guarded {
                    if let Some(s) = x.as_str() {
                        *x = Value::String(format!("{gid}_{s}"));
                    }
                }
            }
            added.push(c);
        }
    }
    if !added.is_empty() {
        match panel.get_mut("mandos").and_then(Value::as_array_mut) {
            Some(list) => list.extend(added),
            None => {
                panel.as_object_mut().ok_or_else(|| format!("panel {id}: no es un objeto"))?.insert("mandos".into(), Value::Array(added));
            }
        }
    }
    serde_json::to_string(&panel).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sections() -> Sections {
        lunar_core::defs::parse("secciones", r#"{ "energia": { "titulo": "ENERGÍA", "mandos": [
            { "id": "genera", "kind": "display7", "senal": "energia.generacion" },
            { "id": "tapa", "kind": "tapa", "protege": ["corte"] }, { "id": "corte", "kind": "interruptor" } ] } }"#).unwrap()
    }

    #[test]
    fn a_group_that_names_a_section_gets_its_title_and_its_controls() {
        let text = r#"{ "name": "X", "tamano": ["0.4 m", "0.2 m"], // a comment
            "grupos": [ { "id": "a", "seccion": "energia" }, { "id": "b", "titulo": "OTRA", "seccion": "energia", "celda": [0, 1] } ],
            "mandos": [ { "id": "suelto", "kind": "lampara" } ] }"#;
        let out: Value = serde_json::from_str(&expand("x", text, &sections()).unwrap()).unwrap();
        let groups = out["grupos"].as_array().unwrap();
        assert_eq!(groups[0]["titulo"], "ENERGÍA");
        assert_eq!(groups[1]["titulo"], "OTRA", "a group's own title stays");
        assert!(groups.iter().all(|g| g.get("seccion").is_none()));
        let ids: Vec<&str> = out["mandos"].as_array().unwrap().iter().map(|c| c["id"].as_str().unwrap()).collect();
        assert_eq!(ids, ["suelto", "a_genera", "a_tapa", "a_corte", "b_genera", "b_tapa", "b_corte"]);
        let cover = &out["mandos"][2];
        assert_eq!((cover["grupo"].as_str(), cover["protege"][0].as_str()), (Some("a"), Some("a_corte")));
    }

    #[test]
    fn a_panel_with_no_section_is_left_as_it_is_and_an_unknown_one_says_so() {
        let plain = r#"{ "name": "X", // untouched, comments and all
            "grupos": [] }"#;
        assert_eq!(expand("x", plain, &sections()).unwrap(), plain);
        let bad = r#"{ "name": "X", "grupos": [ { "id": "a", "seccion": "nada" } ] }"#;
        assert!(expand("x", bad, &sections()).unwrap_err().contains("nada"));
    }
}
