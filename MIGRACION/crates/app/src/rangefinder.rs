//! Rangefinder (T): what the crosshair is on and how far (ground or structure, out to 100 km),
//! the nearest ship and the detail it is drawn with. A tool for seeing distances in play and for
//! checking at what range things change (levels of detail, props, lamps).
use crate::{builds::Builds, ships::Ships};
use glam::DVec3;
use lunar_core::body::BodyRegistry;
use lunar_render::View;

/// Farthest it reads (m).
const RANGE: f64 = 100_000.0;

#[derive(Default)]
pub struct Rangefinder {
    pub on: bool,
}

/// One reading.
#[derive(Clone, Debug, PartialEq)]
pub struct Reading {
    /// To what the crosshair is on (m), and what it is.
    pub hit: Option<(f64, String)>,
    /// The nearest ship: its kind, the distance to its nearest point (m) and the detail level it
    /// is shown with.
    pub ship: Option<(String, f64, String)>,
}

/// "123 m", "1.24 km".
pub fn metres(d: f64) -> String {
    if d < 1000.0 {
        format!("{d:.1} m")
    } else {
        format!("{:.2} km", d / 1000.0)
    }
}

impl Rangefinder {
    pub fn read(bodies: &BodyRegistry, builds: &Builds, ships: &Ships, view: &View, r: &lunar_render::Renderer, vis: &crate::visibility::Visibility) -> Reading {
        let (from, dir) = (view.eye, view.forward);
        let ground = bodies.raycast(from, dir, RANGE).map(|(_, p)| p.distance(from));
        let reach = ground.unwrap_or(RANGE);
        let hit = match builds.set.raycast(from, dir, reach) {
            Some((i, h, p)) => {
                let s = &builds.set.list[i];
                let who = s.owner.as_deref().unwrap_or(&s.name).to_string();
                // a ship's parts have names
                let part = ships.by_structure(s.id).and_then(|k| ships.list[k].kind.parts.get(h.part as usize).cloned());
                Some((p.distance(from), match part {
                    Some(part) => format!("{who} · {part}"),
                    None => who,
                }))
            }
            None => ground.map(|d| (d, "suelo".to_string())),
        };
        let ship = ships
            .list
            .iter()
            .filter_map(|sh| {
                let s = builds.set.get(sh.structure)?;
                let d = to_sphere(from, s.to_world(s.center), f64::from(s.radius));
                let lod = match (vis.is_hidden(s.id), r.structure_level(s.id)) {
                    (true, _) => "oculta (no se dibuja)".to_string(),
                    (_, Some(0)) => "nivel 0 de 5 (completa)".to_string(),
                    (_, Some(5)) => "nivel 5 de 5 (silueta lejana)".to_string(),
                    (_, Some(l)) => format!("nivel {l} de 5"),
                    (_, None) => "fuera de la vista".to_string(),
                };
                Some((sh.kind.id.clone(), d, lod))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1));
        Reading { hit, ship }
    }

    /// Its lines for the HUD (the banner at the top).
    pub fn hud(&self, r: &Reading, out: &mut Vec<String>) {
        if !self.on {
            return;
        }
        match &r.hit {
            Some((d, what)) => out.push(format!("TELÉMETRO  {}  ·  {what}", metres(*d))),
            None => out.push(format!("TELÉMETRO  ---  (más de {})", metres(RANGE))),
        }
        if let Some((kind, d, lod)) = &r.ship {
            out.push(format!("nave más cercana: {kind} a {} · detalle {lod}", metres(*d)));
        }
    }
}

/// Distance from `eye` to a sphere at `c` of radius `r`, 0 inside.
pub fn to_sphere(eye: DVec3, c: DVec3, r: f64) -> f64 {
    (eye.distance(c) - r).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_in_metres_then_kilometres() {
        assert_eq!(metres(12.345), "12.3 m");
        assert_eq!(metres(1234.0), "1.23 km");
        assert_eq!(to_sphere(DVec3::ZERO, DVec3::new(10.0, 0.0, 0.0), 4.0), 6.0);
        assert_eq!(to_sphere(DVec3::ZERO, DVec3::new(1.0, 0.0, 0.0), 4.0), 0.0);
    }
}
