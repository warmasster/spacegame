//! Where a ship's exhaust leaves it: for every machine that pushes (an engine, a reaction
//! control jet), the mouth of its nozzle and the way the gas goes, found once per kind from the
//! shape of its parts, and what each gives now. One view for whoever shows it, hears it or
//! raises dust with it; how it looks is not the ship's business (`lunar_core::plumes`,
//! `assets/defs/chorros.jsonc`).
//!
//! A machine is a thruster if its data says which way it pushes (`empuje`) or says something
//! of its exhaust (`chorro`). Its nozzle's mouth is the far end of its component along the way
//! the gas leaves (the bell of a nacelle, the rim of a thruster), unless the data gives it. The
//! mouth is kept in the frame of the machine's part: a nacelle that tilts takes it along.
use crate::{def::MachineDef, geom::GenPart, kind::ShipKind, ship::Ship};
use glam::Vec3;
use lunar_core::structure::state::Structure;
use serde::Deserialize;

/// What a machine's data says of its exhaust (`MachineDef::chorro`).
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct JetDef {
    /// Its style (`assets/defs/chorros.jsonc`); without it, its model's.
    #[serde(default)]
    pub estilo: Option<String>,
    /// The middle of its nozzle's mouth in its part's frame (m); without it, the far end of its
    /// component along the way the gas leaves.
    #[serde(default)]
    pub salida: Option<[f32; 3]>,
    /// Radius of the mouth (m).
    #[serde(default)]
    pub radio: Option<f32>,
}

impl JetDef {
    /// What a placed machine says, over what its kind does.
    pub fn over(own: Option<JetDef>, kind: Option<&JetDef>) -> Option<JetDef> {
        match (own, kind) {
            (Some(o), Some(k)) => Some(JetDef { estilo: o.estilo.or_else(|| k.estilo.clone()), salida: o.salida.or(k.salida), radio: o.radio.or(k.radio) }),
            (o, k) => o.or_else(|| k.cloned()),
        }
    }
}

/// A thruster's nozzle, as its kind has it.
#[derive(Clone, Debug, PartialEq)]
pub struct Jet {
    /// The middle of the nozzle's mouth and the way the gas leaves it (unit), in the frame of
    /// the machine's part.
    pub exit: Vec3,
    pub dir: Vec3,
    /// Radius of the mouth (m).
    pub radius: f32,
    /// Rated thrust (N).
    pub rated: f32,
    /// The style its data names, if it names one.
    pub style: Option<String>,
}

/// Rated thrust (N) of a machine that pushes, by its data (`empuje`, or `empuje_vacio`).
pub fn rated(def: &MachineDef) -> f32 {
    let p = &def.params;
    let q = p.get("empuje").or_else(|| p.get("empuje_vacio")).cloned().and_then(|v| serde_json::from_value::<lunar_signals::Q>(v).ok());
    q.and_then(|q| q.si().ok()).unwrap_or(1000.0) as f32
}

impl Jet {
    /// The nozzle of machine `id` on part `part` of `parts`, pushing along `thrust` (its part's
    /// frame, unit); none if it is no thruster.
    pub fn plan(parts: &[GenPart], id: &str, def: &MachineDef, part: Option<u32>, thrust: Vec3) -> Option<Jet> {
        if def.empuje.is_none() && def.chorro.is_none() {
            return None;
        }
        let own = parts.get(part? as usize)?;
        let dir = -thrust;
        let given = def.chorro.as_ref();
        let (exit, radius) = match given.and_then(|j| j.salida) {
            Some(at) => (Vec3::from_array(at), None),
            None => {
                // the pieces placed with it (`id`, `id.*`) and its own part, in its part's frame
                let to_part = own.at.inverse();
                let sub = format!("{id}.");
                let of_it = |p: &&GenPart| p.id == id || p.id.starts_with(&sub) || std::ptr::eq(*p, own);
                let verts = || parts.iter().filter(of_it).flat_map(|p| p.shape.verts().map(move |v| to_part.transform_point3(p.at.transform_point3(v))));
                let (near, far) = verts().fold((f32::MAX, f32::MIN), |(lo, hi), v| (lo.min(v.dot(dir)), hi.max(v.dot(dir))));
                // the mouth: what of it is at that end
                let rim = (far - near) * 0.01 + 0.002;
                let (sum, n) = verts().filter(|v| v.dot(dir) > far - rim).fold((Vec3::ZERO, 0.0f32), |(s, n), v| (s + v, n + 1.0));
                let mid = sum / n.max(1.0);
                let across = |v: Vec3| (v - mid - dir * (v - mid).dot(dir)).length();
                let r = verts().filter(|v| v.dot(dir) > far - rim).map(across).sum::<f32>() / n.max(1.0);
                (mid, Some(r))
            }
        };
        let radius = given.and_then(|j| j.radio).or(radius).unwrap_or(own.shape.sphere().1 * 0.5).max(0.005);
        Some(Jet { exit, dir, radius, rated: rated(def).max(1.0), style: given.and_then(|j| j.estilo.clone()) })
    }
}

/// A thruster as it is now: where its gas leaves and which way (ship frame), how hard it pushes
/// (N) and what share of its rated thrust that is.
#[derive(Clone, Copy, Debug)]
pub struct Firing {
    pub machine: u32,
    pub at: Vec3,
    pub dir: Vec3,
    /// Radius of its nozzle's mouth (m).
    pub radius: f32,
    pub thrust: f32,
    pub level: f32,
}

/// The machines of `kind` that are thrusters, each with its nozzle.
pub fn thrusters(kind: &ShipKind) -> impl Iterator<Item = (usize, &Jet)> {
    kind.machines.iter().enumerate().filter_map(|(m, plan)| Some((m, plan.jet.as_ref()?)))
}

/// Machine `m` of `ship` if it is a thruster, as it is now (its part gone, it gives nothing).
pub fn thruster(ship: &Ship, s: &Structure, m: usize) -> Option<Firing> {
    let jet = ship.kind.machines.get(m)?.jet.as_ref()?;
    let rt = ship.machines.get(m)?;
    let part = s.parts.get(rt.part? as usize)?;
    let thrust = if part.alive { rt.m.thrust() as f32 } else { 0.0 };
    Some(Firing { machine: m as u32, at: part.local.transform_point3(jet.exit), dir: part.local.transform_vector3(jet.dir).normalize_or_zero(), radius: jet.radius, thrust, level: thrust / jet.rated })
}

/// Every thruster of `ship` that fires now.
pub fn firing(ship: &Ship, s: &Structure, out: &mut Vec<Firing>) {
    out.clear();
    for (m, rt) in ship.machines.iter().enumerate() {
        if rt.m.thrust() > 0.0
            && let Some(f) = thruster(ship, s, m).filter(|f| f.thrust > 0.0)
        {
            out.push(f);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_a_placed_machine_says_of_its_exhaust_goes_over_its_kinds() {
        let kind = JetDef { estilo: Some("motor".into()), salida: Some([0.0, -2.0, 0.0]), radio: None };
        assert_eq!(JetDef::over(None, None), None);
        assert_eq!(JetDef::over(None, Some(&kind)), Some(kind.clone()));
        let own = JetDef { estilo: Some("gas_frio".into()), salida: None, radio: Some(0.1) };
        assert_eq!(JetDef::over(Some(own.clone()), None), Some(own.clone()));
        assert_eq!(JetDef::over(Some(own), Some(&kind)), Some(JetDef { estilo: Some("gas_frio".into()), salida: Some([0.0, -2.0, 0.0]), radio: Some(0.1) }));
        let d: JetDef = lunar_core::defs::parse("chorro", r#"{ "estilo": "rcs" }"#).unwrap();
        assert_eq!(d, JetDef { estilo: Some("rcs".into()), ..JetDef::default() });
        assert!(lunar_core::defs::parse::<JetDef>("chorro", r#"{ "color": 3 }"#).is_err());
    }
}
