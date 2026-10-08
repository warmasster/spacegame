//! What each tool the suit carries does (`assets/defs/gear.jsonc`, in the order of the number
//! keys): the same for every machine, the server included, which holds each player to it — what
//! they let fly is what their hands carry, at its pace, and what they mend, with a welder at its
//! rate. How each looks, is held and moves in the hands is the window's (`app/src/gear.rs`, the
//! same entries). A new kind of tool is a variant here and what it does there.
use crate::blasts::Blasts;
use lunar_core::defs::{self, DefError};
use serde::{Deserialize, de::IgnoredAny};
use std::path::Path;

/// What a kind of tool does, and its numbers.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(tag = "tipo", rename_all = "snake_case", deny_unknown_fields)]
pub enum ToolKind {
    /// Reach (m); share of a part's hit points mended per second; seconds to put back a part
    /// that is gone; how far round you its view shows integrity (m).
    Soldador { alcance: f64, ritmo: f32, reconstruir: f32, vista: f64 },
    /// The shot it fires (`shots.jsonc`) and the seconds to load the next.
    Lanzador { tiro: String, recarga: f64 },
}

/// One entry of `gear.jsonc` as the game reads it: its id and what it does. The rest is how it
/// looks and is held (`app/src/gear.rs`, `ToolDef`: a field new there is named here too).
#[derive(Deserialize)]
pub struct ToolEntry {
    pub id: String,
    #[serde(flatten)]
    pub kind: ToolKind,
    #[serde(default, rename = "nombre")]
    _name: IgnoredAny,
    #[serde(default, rename = "modelo")]
    _model: IgnoredAny,
    #[serde(default, rename = "sujecion")]
    _held: IgnoredAny,
    #[serde(default, rename = "clic")]
    _click: IgnoredAny,
}

impl ToolEntry {
    /// The tools of `gear.jsonc` in `dir`, in the order of the number keys.
    pub fn load(dir: &Path) -> Result<Vec<(String, ToolKind)>, DefError> {
        let list: Vec<ToolEntry> = defs::load(&defs::file(dir, "gear"))?;
        Ok(list.into_iter().map(|e| (e.id, e.kind)).collect())
    }
}

/// A tool as the game holds a player to it.
#[derive(Clone, Debug, PartialEq)]
pub struct Tool {
    pub id: String,
    pub kind: ToolKind,
    /// The shot it lets fly (`Blasts`' number), a launcher's.
    pub shot: Option<u16>,
}

/// The tools there are, in the order of the number keys.
#[derive(Clone, Debug, Default)]
pub struct Gear {
    pub tools: Vec<Tool>,
}

impl Gear {
    /// The tools of `list` with what each lets fly found among `blasts`' shots: a launcher of a
    /// shot there is not is an error here, not a trigger that does nothing.
    pub fn new(list: &[(String, ToolKind)], blasts: &Blasts) -> Result<Gear, String> {
        let mut tools = Vec::with_capacity(list.len());
        for (id, kind) in list {
            let shot = match kind {
                ToolKind::Lanzador { tiro, .. } => Some(blasts.shot_index(tiro).ok_or_else(|| format!("gear.jsonc: «{id}» dispara «{tiro}», que no está en shots.jsonc"))?),
                ToolKind::Soldador { .. } => None,
            };
            tools.push(Tool { id: id.clone(), kind: kind.clone(), shot });
        }
        Ok(Gear { tools })
    }

    /// The tool in the hands as a command says it (`Cmd::tool`: 0 none, `k` the `k`-th).
    pub fn in_hand(&self, tool: u8) -> Option<&Tool> {
        usize::from(tool).checked_sub(1).and_then(|k| self.tools.get(k))
    }

    /// What tool `tool` (as a command says it) lets fly (its shot) and the seconds between two;
    /// none if it lets nothing fly.
    pub fn fires(&self, tool: u8) -> Option<(u16, f64)> {
        let t = self.in_hand(tool)?;
        match t.kind {
            ToolKind::Lanzador { recarga, .. } => t.shot.map(|s| (s, recarga)),
            ToolKind::Soldador { .. } => None,
        }
    }

    /// What a welder in the hands mends a second (a share of a part) and the seconds it takes to
    /// put back a part that is gone; none if it is not a welder.
    pub fn mends(&self, tool: u8) -> Option<(f32, f32)> {
        match self.in_hand(tool)?.kind {
            ToolKind::Soldador { ritmo, reconstruir, .. } => Some((ritmo, reconstruir)),
            ToolKind::Lanzador { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_gear_of_the_data_is_read_as_the_window_reads_it() {
        let list = ToolEntry::load(&crate::root().join("assets/defs")).unwrap();
        assert!(list.iter().any(|(_, k)| matches!(k, ToolKind::Soldador { .. })) && list.iter().any(|(_, k)| matches!(k, ToolKind::Lanzador { .. })));
        // (a field no one knows is an error, here as there)
        let bad = r#"[{ "id": "x", "nombre": "X", "tipo": "lanzador", "tiro": "cohete", "recarga": 1.0, "recargaa": 2.0 }]"#;
        assert!(defs::parse::<Vec<ToolEntry>>("gear", bad).is_err());
        let fine = r#"[{ "id": "x", "nombre": "X", "modelo": "m", "clic": "herramienta", "sujecion": { "en": [0, 0, 0] }, "tipo": "lanzador", "tiro": "cohete", "recarga": 1.0 }]"#;
        assert!(defs::parse::<Vec<ToolEntry>>("gear", fine).is_ok());
    }
}
