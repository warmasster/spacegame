//! What the ships' air does to whoever is in it: the sink and the jets inside a ship, the plumes
//! outside, as a pull on a body (its look is the picture's: `app::air`).
use crate::ships::Ships;
use glam::{DVec3, Vec3};
use lunar_core::structure::set::Structures;
use lunar_ship::atmos::{self, Atmos};

/// Plumes of a ship reach this far past its hull (m).
pub const PLUME_REACH: f64 = 30.0;

/// The pull of every ship's air on a body with drag area per mass `cda` at world point `p`
/// (m/s², world): the sink and the jets inside a ship, the plumes outside.
pub fn wind(ships: &Ships, set: &Structures, p: DVec3, cda: f64) -> DVec3 {
    let mut out = DVec3::ZERO;
    for sh in &ships.list {
        if sh.atmos.vents.is_empty() {
            continue;
        }
        let Some(s) = set.get(sh.structure) else { continue };
        if s.to_world(s.center).distance(p) > f64::from(s.radius) + PLUME_REACH {
            continue;
        }
        let l = s.to_local(p);
        let q = sh.atmos.push(l, atmos::room_of(&sh.kind, l));
        if q != Vec3::ZERO {
            out += (s.rot * Atmos::accel(q, cda)).as_dvec3();
        }
    }
    out
}
