//! Effect definitions: explosions (`explosions/<id>.jsonc`), shots (`shots.jsonc`), particle styles
//! (`particles.jsonc`) and the detonation laws (`detonacion.jsonc`). An explosion is written by
//! hand (crater, flash, emitters...) or is only a `charge`, and the laws make the rest. A shot
//! with a `charge` gets its explosion made for it (`disparo:<id>`).
use crate::{
    defs::{self, DefError},
    detonation::{ChargeDef, DetonationRules},
    particles::{MAX_STYLES, StyleDef},
};
use serde::Deserialize;
use std::{collections::BTreeMap, path::Path};

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CraterDef {
    pub radius: f32,
    pub depth: f32,
    /// Rim height over the old ground, in depths.
    pub rim: f32,
    /// Dug this long after the blast (s), once the dust hides it.
    #[serde(default)]
    pub delay: f32,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FlashDef {
    /// Linear colour.
    pub color: [f32; 3],
    /// Peak intensity (sun = the system's sun intensity).
    pub intensity: f32,
    /// Distance it reaches (m).
    pub range: f32,
    /// Fades out over (s).
    pub duration: f32,
    /// Over the ground (m).
    #[serde(default)]
    pub lift: f32,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShakeDef {
    /// Camera jitter at the blast (rad).
    pub amplitude: f32,
    /// Felt out to (m).
    pub range: f32,
    pub duration: f32,
}

/// What a blast does to structures (core::structure).
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlastDef {
    /// Energy (J) spread as a flux falling with the square of the distance.
    pub energy: f32,
    /// Reach (m).
    pub radius: f32,
}

/// How a round or missile in flight is drawn: a particle style and its radius (m).
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoundLook {
    pub style: String,
    pub size: f32,
}

/// A shot the menu (Esc) can change while playing: its charge, speed and size, up to these.
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShotMenu {
    /// Heaviest charge (kg of TNT).
    pub max_tnt: f32,
    /// Fastest launch (m/s).
    pub max_speed: f32,
}

/// A projectile fired where the player aims (`shots.jsonc`): it flies (with `speed`) or strikes at
/// once along the aim.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShotDef {
    pub name: String,
    #[serde(default)]
    pub key: Option<String>,
    /// Energy (J) and cross-section (m²): how far it cuts.
    pub energy: f32,
    pub area: f32,
    /// Reach (m).
    pub range: f32,
    /// Muzzle speed (m/s): a round that flies and falls; none, it strikes at once.
    #[serde(default)]
    pub speed: Option<f32>,
    /// How it is drawn in flight.
    #[serde(default)]
    pub look: Option<RoundLook>,
    /// Effect where it lands (an explosion id)...
    #[serde(default)]
    pub impact: Option<String>,
    /// ...or the explosive it carries, whose explosion is made from it.
    #[serde(default)]
    pub charge: Option<ChargeDef>,
    /// Shown in the menu to be changed in play.
    #[serde(default)]
    pub menu: Option<ShotMenu>,
    /// Automatic: rounds per second while its key is held (none: one per press).
    #[serde(default)]
    pub rate: Option<f32>,
    /// Scatter round the aim (mrad, one standard deviation).
    #[serde(default)]
    pub spread: f32,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmitterDef {
    /// Style name in `particles.jsonc`.
    pub style: String,
    pub count: u32,
    /// Launch speed (m/s), min..max.
    pub speed: [f32; 2],
    /// Half-angle of the launch cone round the local vertical (deg): 90 a hemisphere, 180 all round.
    pub cone: f32,
    /// Launch directions bunch toward this angle from the vertical (deg; none: spread evenly).
    #[serde(default)]
    pub tilt: Option<f32>,
    /// Radius at birth (m), min..max.
    pub size: [f32; 2],
    /// Life (s), min..max.
    pub life: [f32; 2],
    /// Born within this distance of the blast (m).
    #[serde(default)]
    pub spread: f32,
    /// Born this high over the ground (m).
    #[serde(default)]
    pub lift: f32,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExplosionDef {
    pub name: String,
    /// Key that sets one off where the player aims ("B", "7"...), if any.
    #[serde(default)]
    pub key: Option<String>,
    /// A conventional explosive: crater, flash, shake, damage and particles from the laws (what is
    /// also written by hand wins; written emitters are thrown as well).
    #[serde(default)]
    pub charge: Option<ChargeDef>,
    #[serde(default)]
    pub crater: Option<CraterDef>,
    #[serde(default)]
    pub flash: Option<FlashDef>,
    #[serde(default)]
    pub shake: Option<ShakeDef>,
    #[serde(default)]
    pub damage: Option<BlastDef>,
    #[serde(default)]
    pub emitters: Vec<EmitterDef>,
}

/// Every effect definition.
pub struct EffectDefs {
    pub styles: Vec<(String, StyleDef)>,
    pub explosions: Vec<(String, ExplosionDef)>,
    pub shots: Vec<(String, ShotDef)>,
    pub rules: DetonationRules,
}

impl EffectDefs {
    /// `particles.jsonc`, `explosions/*.jsonc`, `shots.jsonc` and `detonacion.jsonc` under `dir`.
    pub fn load(dir: &Path) -> Result<EffectDefs, DefError> {
        let styles: BTreeMap<String, StyleDef> = defs::load(&defs::file(dir, "particles"))?;
        let mut explosions: Vec<(String, ExplosionDef)> = defs::load_dir(&dir.join("explosions"))?;
        let shots: BTreeMap<String, ShotDef> = defs::load(&defs::file(dir, "shots"))?;
        let mut shots: Vec<(String, ShotDef)> = shots.into_iter().collect();
        for (id, s) in &mut shots {
            if let (Some(c), None) = (s.charge, &s.impact) {
                let e = format!("disparo:{id}");
                explosions.push((e.clone(), ExplosionDef { name: s.name.clone(), charge: Some(c), ..Default::default() }));
                s.impact = Some(e);
            }
        }
        let rules = defs::load(&defs::file(dir, "detonacion"))?;
        let d = EffectDefs { styles: styles.into_iter().collect(), explosions, shots, rules };
        d.validate().map_err(|e| DefError::new("effects", e))?;
        Ok(d)
    }

    pub fn has_style(&self, name: &str) -> bool {
        self.styles.iter().any(|(s, _)| s == name)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.styles.len() > MAX_STYLES {
            return Err(format!("{} particle styles > {MAX_STYLES}", self.styles.len()));
        }
        self.rules.validate(|s| self.has_style(s))?;
        for (id, e) in &self.explosions {
            if e.charge.is_none() && e.emitters.is_empty() && e.crater.is_none() && e.flash.is_none() {
                return Err(format!("explosion {id}: needs a charge or something to show"));
            }
            for em in &e.emitters {
                if !self.has_style(&em.style) {
                    return Err(format!("explosion {id}: unknown particle style '{}'", em.style));
                }
            }
        }
        for (id, s) in &self.shots {
            if let Some(l) = s.look.as_ref().filter(|l| !self.has_style(&l.style)) {
                return Err(format!("shot {id}: unknown particle style '{}'", l.style));
            }
            if s.speed.is_some_and(|v| v <= 0.0) {
                return Err(format!("shot {id}: speed must be > 0"));
            }
            if s.menu.is_some() && (s.charge.is_none() || s.speed.is_none()) {
                return Err(format!("shot {id}: a shot in the menu needs a charge and a speed"));
            }
            if let Some(i) = &s.impact
                && !self.explosions.iter().any(|(e, _)| e == i)
            {
                return Err(format!("shot {id}: unknown impact '{i}'"));
            }
        }
        Ok(())
    }
}
