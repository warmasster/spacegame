//! A sound as data (`assets/defs/sounds.jsonc`): layers added together.
use serde::Deserialize;
use std::collections::BTreeMap;

/// What a layer starts from.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Wave {
    #[default]
    Seno,
    Triangulo,
    Sierra,
    Cuadrada,
    /// White noise; pink (−3 dB an octave: wind, hiss); brown (−6 dB: rumble).
    Ruido,
    RuidoRosa,
    RuidoMarron,
}

/// What a sound comes through to the ear.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Via {
    /// The air round the head: as loud as that air is dense, silent in a vacuum.
    #[default]
    Aire,
    /// What the body touches (boots on a deck, a hand on a lever, the hull one rides): dull,
    /// whatever the air.
    Contacto,
    /// The suit itself (its fan, one's breath, its radio): always there.
    Traje,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FilterKind {
    Bajo,
    Alto,
    Banda,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FilterDef {
    pub tipo: FilterKind,
    /// Corner (or centre) frequency (Hz).
    pub hz: f32,
    /// Resonance: 0.7 flat, more a narrower band.
    #[serde(default = "flat")]
    pub q: f32,
}

fn flat() -> f32 {
    0.707
}

fn one() -> f32 {
    1.0
}

fn a_note() -> f32 {
    440.0
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LayerDef {
    #[serde(default)]
    pub onda: Wave,
    /// Frequency (Hz) of an oscillator.
    #[serde(default = "a_note")]
    pub hz: f32,
    /// A blow glides to this frequency (Hz) over its length.
    #[serde(default)]
    pub hasta: Option<f32>,
    /// It alternates with this other frequency (Hz) so many times a second: [hz, changes/s].
    #[serde(default)]
    pub alterna: Option<[f32; 2]>,
    #[serde(default)]
    pub filtro: Option<FilterDef>,
    #[serde(default = "one")]
    pub ganancia: f32,
    /// Seconds to full level.
    #[serde(default)]
    pub ataque: f32,
    /// A blow dies away with this time constant (s); without it, it holds to its end.
    #[serde(default)]
    pub caida: Option<f32>,
    /// Its level wavers: [times a second, depth 0..1].
    #[serde(default)]
    pub tremolo: Option<[f32; 2]>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SoundDef {
    pub capas: Vec<LayerDef>,
    /// A blow: how long it lasts (s). Without it, a loop: it sounds while it is held.
    #[serde(default)]
    pub dura: Option<f32>,
    #[serde(default)]
    pub via: Via,
    #[serde(default = "one")]
    pub ganancia: f32,
    /// Each blow is tuned at random within this share (0.06: ±6 %), so that steps are not a
    /// machine gun.
    #[serde(default)]
    pub variacion: f32,
}

/// Every sound by its id.
pub type SoundDefs = BTreeMap<String, SoundDef>;
