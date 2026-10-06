//! A star system as data: its sun (a directional light), an optional backdrop planet painted in the
//! sky, and the bodies you can stand on. `system.jsonc` names the bodies; each lives in
//! `bodies/<id>.jsonc`.
use crate::{
    body::{Body, BodyDef, BodyRegistry},
    defs::{self, DefError},
};
use glam::DVec3;
use serde::Deserialize;
use std::{path::Path, sync::Arc};

#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sun {
    /// Degrees, world frame: azimuth from -Z toward +X, elevation over the XZ plane.
    pub azimuth: f32,
    pub elevation: f32,
    /// Linear colour and intensity of the light.
    pub color: [f32; 3],
    pub intensity: f32,
    /// How far (m) sun shadows are swept from their casters.
    pub shadow_sweep: f32,
}

impl Sun {
    /// Unit vector toward the sun.
    pub fn direction(&self) -> DVec3 {
        let (az, el) = (f64::from(self.azimuth).to_radians(), f64::from(self.elevation).to_radians());
        DVec3::new(az.sin() * el.cos(), el.sin(), -az.cos() * el.cos())
    }
}

/// A far planet in the sky (not a body you can reach).
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Backdrop {
    pub dir: [f64; 3],
    /// Angular radius (degrees).
    pub angular_radius: f64,
}

impl Backdrop {
    pub fn direction(&self) -> DVec3 {
        DVec3::from_array(self.dir).normalize()
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SystemDef {
    pub name: String,
    pub sun: Sun,
    #[serde(default)]
    pub backdrop: Option<Backdrop>,
    /// Ids of `bodies/<id>.jsonc`.
    pub bodies: Vec<String>,
}

pub struct System {
    pub name: String,
    pub sun: Sun,
    pub backdrop: Option<Backdrop>,
    pub bodies: Arc<BodyRegistry>,
}

impl System {
    /// `dir`: the definitions folder (holds `system.jsonc` and `bodies/`).
    pub fn load(dir: &Path) -> Result<System, DefError> {
        let def: SystemDef = defs::load(&defs::file(dir, "system"))?;
        let bodies = def.bodies.iter().map(|id| Body::from_def(id, &defs::load::<BodyDef>(&defs::file(&dir.join("bodies"), id))?)).collect::<Result<Vec<_>, _>>()?;
        if bodies.is_empty() {
            return Err(DefError::new("system", "no bodies"));
        }
        Ok(System { name: def.name, sun: def.sun, backdrop: def.backdrop, bodies: Arc::new(BodyRegistry::new(bodies)) })
    }
}
